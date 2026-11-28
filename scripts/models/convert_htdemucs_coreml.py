#!/usr/bin/env python3
"""Builds the Core ML version of HTDemucs 4-stem from openDJ's safetensors file.

    pip install coremltools==9.0 numpy safetensors
    python3 convert_htdemucs_coreml.py htdemucs.safetensors htdemucs.mlpackage [--fp32]

The input is the `htdemucs.safetensors` the `stem-models` release has, so
both back ends run the same weights. The network is written out op by op,
mirroring third_party/demucs-core/src/model (htdemucs.rs, conv.rs,
transformer.rs), so no PyTorch is needed and the conversion runs on Linux.

Only the network is in the model: the spectrograms, and normalizing the
inputs and outputs, stay in Rust in fp32 (`demucs_core::separate_segment_with`),
since fp16 loses too much there. One fixed-shape program, for one training
segment (343,980 samples at 44.1 kHz):

    inputs   freq      [1, 4, 2048, 336]   normalized CaC spectrogram
             time      [1, 2, 343980]      normalized waveform
    outputs  freq_out  [1, 16, 2048, 336]  4 stems x CaC, before denormalizing
             time_out  [1, 8, 343980]      4 stems x stereo

in drums, bass, other, vocals order. It computes in fp16 (unless --fp32) so
macOS can run it on the Neural Engine. Zip the .mlpackage folder to publish it.

`--check DIR` runs the same graph in numpy instead, on the inputs and
outputs `the_network_matches_its_dump` in crates/separation wrote to DIR, and
prints how close it gets: a check of the graph that needs no Mac.
"""
import argparse
import math
import re

import numpy as np
from safetensors.numpy import load_file

SEGMENT = 343980
N_FFT, HOP = 4096, 1024
FRAMES = math.ceil(SEGMENT / HOP)  # 336
BINS = N_FFT // 2
STEMS = 4
EPS = 1e-5


# ── Back ends: the same graph as numpy (to check it) or Core ML MIL ─────────


class Numpy:
    """Runs each op at once in float32."""

    def const(self, a):
        return np.asarray(a, dtype=np.float32)

    def shape(self, x):
        return tuple(x.shape)

    def add(self, x, y):
        return x + y

    def mul(self, x, y):
        return x * y

    def sigmoid(self, x):
        return 1.0 / (1.0 + np.exp(-x))

    def gelu(self, x):
        # Exact GELU, with erf to 1.5e-7 (Abramowitz and Stegun 7.1.26).
        z = x / math.sqrt(2.0)
        s = np.sign(z)
        a = np.abs(z)
        t = 1.0 / (1.0 + 0.3275911 * a)
        poly = t * (0.254829592 + t * (-0.284496736 + t * (1.421413741 + t * (-1.453152027 + t * 1.061405429))))
        erf = s * (1.0 - poly * np.exp(-a * a))
        return (0.5 * x * (1.0 + erf)).astype(np.float32)

    def reshape(self, x, shape):
        return x.reshape(shape)

    def transpose(self, x, perm):
        return np.ascontiguousarray(x.transpose(perm))

    def slice(self, x, begin, end):
        return x[tuple(slice(b, e) for b, e in zip(begin, end))]

    def layer_norm(self, x, axes, gamma=None, beta=None):
        axes = tuple(a % x.ndim for a in axes)
        m = x.mean(axis=axes, keepdims=True, dtype=np.float64)
        v = ((x - m) ** 2).mean(axis=axes, keepdims=True, dtype=np.float64)
        y = ((x - m) / np.sqrt(v + EPS)).astype(np.float32)
        if gamma is not None:
            y = y * gamma + beta
        return y

    def conv(self, x, w, b, strides, pad, dilations=None):
        """x [N, C, *S], w [O, C, *K]; pad = [before, after] per spatial axis."""
        nd = x.ndim - 2
        dil = dilations or [1] * nd
        widths = [(0, 0), (0, 0)] + [(pad[2 * i], pad[2 * i + 1]) for i in range(nd)]
        xp = np.pad(x, widths)
        ks = w.shape[2:]
        out_sz = [(xp.shape[2 + i] - dil[i] * (ks[i] - 1) - 1) // strides[i] + 1 for i in range(nd)]
        out = np.zeros((x.shape[0], w.shape[0], *out_sz), dtype=np.float32)
        for k in np.ndindex(*ks):
            idx = [slice(None), slice(None)] + [
                slice(k[i] * dil[i], k[i] * dil[i] + strides[i] * (out_sz[i] - 1) + 1, strides[i])
                for i in range(nd)
            ]
            xs = xp[tuple(idx)]
            wk = w[(slice(None), slice(None)) + tuple(k)]  # [O, C]
            out += np.tensordot(wk, xs, axes=([1], [1])).swapaxes(0, 1)
        return out + b.reshape((1, -1) + (1,) * nd)

    def conv_tr(self, x, w, b, strides):
        """x [N, C, *S], w [C, O, *K]; "valid": the full (S - 1) * s + K."""
        nd = x.ndim - 2
        ks = w.shape[2:]
        sz = x.shape[2:]
        out_sz = [(sz[i] - 1) * strides[i] + ks[i] for i in range(nd)]
        out = np.zeros((x.shape[0], w.shape[1], *out_sz), dtype=np.float32)
        for k in np.ndindex(*ks):
            idx = [slice(None), slice(None)] + [
                slice(k[i], k[i] + strides[i] * (sz[i] - 1) + 1, strides[i]) for i in range(nd)
            ]
            wk = w[(slice(None), slice(None)) + tuple(k)]  # [C, O]
            out[tuple(idx)] += np.tensordot(wk, x, axes=([0], [1])).swapaxes(0, 1)
        return out + b.reshape((1, -1) + (1,) * nd)

    def linear(self, x, w, b):
        return x @ w.T + b

    def matmul(self, a, b, transpose_y=False):
        return a @ (np.swapaxes(b, -1, -2) if transpose_y else b)

    def softmax(self, x, axis):
        e = np.exp(x - x.max(axis=axis, keepdims=True))
        return e / e.sum(axis=axis, keepdims=True)


class Mil:
    """Adds each op to the Core ML program being built."""

    def __init__(self, mb):
        self.mb = mb

    def const(self, a):
        return np.asarray(a, dtype=np.float32)

    def shape(self, x):
        return tuple(int(d) for d in x.shape)

    def add(self, x, y):
        return self.mb.add(x=x, y=y)

    def mul(self, x, y):
        return self.mb.mul(x=x, y=y)

    def sigmoid(self, x):
        return self.mb.sigmoid(x=x)

    def gelu(self, x):
        return self.mb.gelu(x=x, mode="EXACT")

    def reshape(self, x, shape):
        return self.mb.reshape(x=x, shape=list(shape))

    def transpose(self, x, perm):
        return self.mb.transpose(x=x, perm=list(perm))

    def slice(self, x, begin, end):
        return self.mb.slice_by_index(x=x, begin=list(begin), end=list(end))

    def layer_norm(self, x, axes, gamma=None, beta=None):
        kw = {}
        if gamma is not None:
            kw = dict(gamma=gamma, beta=beta)
        return self.mb.layer_norm(x=x, axes=list(axes), epsilon=EPS, **kw)

    def conv(self, x, w, b, strides, pad, dilations=None):
        nd = len(strides)
        return self.mb.conv(
            x=x, weight=w, bias=b, strides=list(strides), pad_type="custom", pad=list(pad),
            dilations=list(dilations or [1] * nd),
        )

    def conv_tr(self, x, w, b, strides):
        return self.mb.conv_transpose(x=x, weight=w, bias=b, strides=list(strides), pad_type="valid")

    def linear(self, x, w, b):
        return self.mb.linear(x=x, weight=w, bias=b)

    def matmul(self, a, b, transpose_y=False):
        return self.mb.matmul(x=a, y=b, transpose_y=transpose_y)

    def softmax(self, x, axis):
        return self.mb.softmax(x=x, axis=axis)


# ── The network, as demucs-core has it ──────────────────────────────────────


class Net:
    def __init__(self, w, ops):
        self.w = w
        self.o = ops

    def p(self, name):
        return self.w[name]

    def glu(self, x):
        s = self.o.shape(x)
        h = s[1] // 2
        a = self.o.slice(x, [0] * len(s), [s[0], h, *s[2:]])
        g = self.o.slice(x, [0, h] + [0] * (len(s) - 2), list(s))
        return self.o.mul(a, self.o.sigmoid(g))

    def group_norm1(self, x, name):
        """GroupNorm with one group over [N, C, L], affine per channel."""
        o = self.o
        y = o.layer_norm(x, [1, 2])
        g = self.p(f"{name}.weight").reshape(1, -1, 1)
        b = self.p(f"{name}.bias").reshape(1, -1, 1)
        return o.add(o.mul(y, g), b)

    def dconv(self, x, name):
        """DConv on [N, C, L]: two residual layers, dilation 1 then 2."""
        o = self.o
        for j in range(2):
            pre = f"{name}.layers.{j}"
            d = 1 << j
            y = o.conv(x, self.p(f"{pre}.0.weight"), self.p(f"{pre}.0.bias"), [1], [d, d], [d])
            y = o.gelu(self.group_norm1(y, f"{pre}.1"))
            y = o.conv(y, self.p(f"{pre}.3.weight"), self.p(f"{pre}.3.bias"), [1], [0, 0])
            y = self.glu(self.group_norm1(y, f"{pre}.4"))
            y = o.mul(y, self.p(f"{pre}.6.scale").reshape(1, -1, 1))
            x = o.add(x, y)
        return x

    def dconv_freq(self, x, name):
        """DConv per frequency row of [1, C, Fr, T]."""
        o = self.o
        _, c, fr, t = o.shape(x)
        y = o.reshape(o.transpose(x, [0, 2, 1, 3]), [fr, c, t])
        y = self.dconv(y, name)
        return o.transpose(o.reshape(y, [1, fr, c, t]), [0, 2, 1, 3])

    def henc(self, x, i):
        o, pre = self.o, f"encoder.{i}"
        x = o.conv(x, self.p(f"{pre}.conv.weight"), self.p(f"{pre}.conv.bias"), [4, 1], [2, 2, 0, 0])
        x = self.dconv_freq(o.gelu(x), f"{pre}.dconv")
        x = o.conv(x, self.p(f"{pre}.rewrite.weight"), self.p(f"{pre}.rewrite.bias"), [1, 1], [0, 0, 0, 0])
        return self.glu(x)

    def tenc(self, x, i):
        o, pre = self.o, f"tencoder.{i}"
        extra = (-o.shape(x)[2]) % 4  # right-pad to a multiple of the stride
        x = o.conv(x, self.p(f"{pre}.conv.weight"), self.p(f"{pre}.conv.bias"), [4], [2, 2 + extra])
        x = self.dconv(o.gelu(x), f"{pre}.dconv")
        x = o.conv(x, self.p(f"{pre}.rewrite.weight"), self.p(f"{pre}.rewrite.bias"), [1], [0, 0])
        return self.glu(x)

    def hdec(self, x, skip, i, target, last):
        o, pre = self.o, f"decoder.{i}"
        x = o.add(x, skip)
        x = o.conv(x, self.p(f"{pre}.rewrite.weight"), self.p(f"{pre}.rewrite.bias"), [1, 1], [1, 1, 1, 1])
        x = self.dconv_freq(self.glu(x), f"{pre}.dconv")
        x = o.conv_tr(x, self.p(f"{pre}.conv_tr.weight"), self.p(f"{pre}.conv_tr.bias"), [4, 1])
        s = o.shape(x)
        # Padding 2: the full output less 2 at each end, then trimmed.
        x = o.slice(x, [0, 0, 2, 0], [1, s[1], 2 + min(s[2] - 4, target), s[3]])
        return x if last else o.gelu(x)

    def tdec(self, x, skip, i, target, last):
        o, pre = self.o, f"tdecoder.{i}"
        n = o.shape(x)[2]
        if o.shape(skip)[2] > n:
            skip = o.slice(skip, [0, 0, 0], [1, o.shape(skip)[1], n])
        x = o.add(x, skip)
        x = o.conv(x, self.p(f"{pre}.rewrite.weight"), self.p(f"{pre}.rewrite.bias"), [1], [1, 1])
        x = self.dconv(self.glu(x), f"{pre}.dconv")
        x = o.conv_tr(x, self.p(f"{pre}.conv_tr.weight"), self.p(f"{pre}.conv_tr.bias"), [4])
        s = o.shape(x)
        x = o.slice(x, [0, 0, 2], [1, s[1], 2 + min(s[2] - 4, target)])
        return x if last else o.gelu(x)

    def attention(self, q_in, kv_in, name):
        o = self.o
        w = self.p(f"{name}.in_proj_weight")
        b = self.p(f"{name}.in_proj_bias")
        d = w.shape[1]
        heads, dh = 8, d // 8

        def heads_of(x, k):
            y = o.linear(x, w[k * d:(k + 1) * d], b[k * d:(k + 1) * d])
            s = o.shape(y)[1]
            return o.transpose(o.reshape(y, [1, s, heads, dh]), [0, 2, 1, 3])

        q, k, v = heads_of(q_in, 0), heads_of(kv_in, 1), heads_of(kv_in, 2)
        att = o.softmax(o.mul(o.matmul(q, k, transpose_y=True), np.float32(1.0 / math.sqrt(dh))), -1)
        ctx = o.matmul(att, v)
        s = o.shape(ctx)[2]
        ctx = o.reshape(o.transpose(ctx, [0, 2, 1, 3]), [1, s, d])
        return o.linear(ctx, self.p(f"{name}.out_proj.weight"), self.p(f"{name}.out_proj.bias"))

    def ln(self, x, name):
        return self.o.layer_norm(x, [-1], self.p(f"{name}.weight"), self.p(f"{name}.bias"))

    def tlayer(self, x, cross, name):
        o = self.o
        if cross is None:
            h = self.ln(x, f"{name}.norm1")
            a = self.attention(h, h, f"{name}.self_attn")
            ffn_norm = f"{name}.norm2"
        else:
            a = self.attention(self.ln(x, f"{name}.norm1"), self.ln(cross, f"{name}.norm2"), f"{name}.cross_attn")
            ffn_norm = f"{name}.norm3"
        x = o.add(x, o.mul(a, self.p(f"{name}.gamma_1.scale")))
        y = o.gelu(o.linear(self.ln(x, ffn_norm), self.p(f"{name}.linear1.weight"), self.p(f"{name}.linear1.bias")))
        y = o.linear(y, self.p(f"{name}.linear2.weight"), self.p(f"{name}.linear2.bias"))
        x = o.add(x, o.mul(y, self.p(f"{name}.gamma_2.scale")))
        # GroupNorm(1) over the whole [S, D], affine per D.
        x = o.layer_norm(x, [1, 2])
        return o.add(o.mul(x, self.p(f"{name}.norm_out.weight")), self.p(f"{name}.norm_out.bias"))

    def transformer(self, freq, time):
        o = self.o
        _, ch, fr, tf = o.shape(freq)
        up = self.p("channel_upsampler.weight")
        freq = o.conv(freq, up.reshape(*up.shape, 1), self.p("channel_upsampler.bias"), [1, 1], [0, 0, 0, 0])
        time = o.conv(time, self.p("channel_upsampler_t.weight"), self.p("channel_upsampler_t.bias"), [1], [0, 0])
        d = up.shape[0]
        tt = o.shape(time)[2]
        freq = o.reshape(o.transpose(freq, [0, 3, 2, 1]), [1, tf * fr, d])
        time = o.transpose(time, [0, 2, 1])
        freq = o.add(self.ln(freq, "crosstransformer.norm_in"), sin_2d(d, fr, tf))
        time = o.add(self.ln(time, "crosstransformer.norm_in_t"), sin_1d(tt, d))
        for i in range(5):
            f, t = f"crosstransformer.layers.{i}", f"crosstransformer.layers_t.{i}"
            if i % 2 == 0:
                freq, time = self.tlayer(freq, None, f), self.tlayer(time, None, t)
            else:
                freq, time = self.tlayer(freq, time, f), self.tlayer(time, freq, t)
        freq = o.transpose(o.reshape(freq, [1, tf, fr, d]), [0, 3, 2, 1])
        time = o.transpose(time, [0, 2, 1])
        down = self.p("channel_downsampler.weight")
        freq = o.conv(freq, down.reshape(*down.shape, 1), self.p("channel_downsampler.bias"), [1, 1], [0, 0, 0, 0])
        time = o.conv(time, self.p("channel_downsampler_t.weight"), self.p("channel_downsampler_t.bias"), [1], [0, 0])
        return freq, time

    def forward(self, freq, time):
        o = self.o
        skips = []
        freq = self.henc(freq, 0)
        fr = o.shape(freq)[2]
        emb = (self.p("freq_emb.embedding.weight")[:fr] * 10.0 * 0.2).T.reshape(1, -1, fr, 1)
        freq = o.add(freq, emb)
        skips.append(freq)
        for i in range(1, 4):
            freq = self.henc(freq, i)
            skips.append(freq)
        tskips, lengths = [], []
        for i in range(4):
            lengths.append(o.shape(time)[2])
            time = self.tenc(time, i)
            tskips.append(time)
        freq, time = self.transformer(freq, time)
        fdims = [o.shape(s)[2] for s in skips]
        for i in range(4):
            target = fdims[len(fdims) - 2 - i] if i + 1 < len(fdims) else BINS
            freq = self.hdec(freq, skips.pop(), i, target, i == 3)
        for i in range(4):
            time = self.tdec(time, tskips.pop(), i, lengths[len(lengths) - 1 - i], i == 3)
        return freq, time


def sin_1d(length, d):
    half = d // 2
    pos = np.arange(length, dtype=np.float32)[:, None]
    i = np.arange(half, dtype=np.float32)[None, :]
    angle = pos / np.power(np.float32(10000.0), i / np.float32(half - 1))
    return np.concatenate([np.cos(angle), np.sin(angle)], axis=1)[None].astype(np.float32)


def sin_2d(d, height, width):
    half = d // 2
    quarter = half // 2
    k = np.arange(quarter, dtype=np.float32)
    div = np.exp(-2.0 * k * np.float32(math.log(10000.0)) / np.float32(half)).astype(np.float32)
    out = np.zeros((width * height, d), dtype=np.float32)
    t = np.repeat(np.arange(width, dtype=np.float32), height)[:, None]
    f = np.tile(np.arange(height, dtype=np.float32), width)[:, None]
    out[:, 0:half:2] = np.sin(t * div)
    out[:, 1:half:2] = np.cos(t * div)
    out[:, half::2] = np.sin(f * div)
    out[:, half + 1::2] = np.cos(f * div)
    return out[None]


def weights(path):
    """The 4-stem model's tensors as float32, without their signature prefix."""
    raw = load_file(path)
    sigs = {k.split(".", 1)[0] for k in raw}
    if len(sigs) != 1:
        raise SystemExit(f"expected one model in {path}, found {sorted(sigs)}")
    return {k.split(".", 1)[1]: v.astype(np.float32) for k, v in raw.items()}


def check(w, folder):
    """Runs the graph in numpy on a dump of the network's inputs and outputs."""
    read = lambda name, shape: np.fromfile(f"{folder}/{name}.f32", dtype="<f4").reshape(shape)
    freq = read("freq", (1, 4, BINS, FRAMES))
    time = read("time", (1, 2, SEGMENT))
    f, t = Net(w, Numpy()).forward(freq, time)
    for name, got in [("freq_out", f), ("time_out", t)]:
        want = read(name, got.shape)
        snr = 10 * math.log10((want.astype(np.float64) ** 2).sum() / max(((want - got).astype(np.float64) ** 2).sum(), 1e-30))
        print(f"{name}: {snr:.1f} dB against burn")
        assert snr > 60, name


def convert(w, out, fp32):
    import coremltools as ct
    from coremltools.converters.mil import Builder as mb
    from coremltools.converters.mil.mil import types

    @mb.program(
        input_specs=[
            mb.TensorSpec(shape=(1, 4, BINS, FRAMES), dtype=types.fp32),
            mb.TensorSpec(shape=(1, 2, SEGMENT), dtype=types.fp32),
        ],
        opset_version=ct.target.macOS13,
    )
    def prog(freq, time):
        f, t = Net(w, Mil(mb)).forward(freq, time)
        return mb.identity(x=f, name="freq_out"), mb.identity(x=t, name="time_out")

    model = ct.convert(
        prog,
        convert_to="mlprogram",
        compute_precision=ct.precision.FLOAT32 if fp32 else ct.precision.FLOAT16,
        minimum_deployment_target=ct.target.macOS13,
    )
    model.short_description = "HTDemucs 4-stem network (Meta) for openDJ's high-quality stems"
    model.user_defined_metadata["stems"] = "drums,bass,other,vocals"
    model.save(out)
    print(f"{out}: saved")


if __name__ == "__main__":
    a = argparse.ArgumentParser()
    a.add_argument("weights")
    a.add_argument("out", nargs="?")
    a.add_argument("--fp32", action="store_true")
    a.add_argument("--check")
    args = a.parse_args()
    w = weights(args.weights)
    if args.check:
        check(w, args.check)
    else:
        convert(w, args.out, args.fp32)
