#!/usr/bin/env python3
"""Builds the Core ML version of Spleeter 4-stem from openDJ's safetensors file.

    pip install coremltools==9.0 numpy safetensors
    python3 convert_spleeter_coreml.py spleeter-4stems.safetensors spleeter-4stems.mlpackage

The input is the file `convert_spleeter.py` makes (and the `stem-models`
release has), so both back ends run the same weights. The network is written
out op by op with Core ML's MIL builder, mirroring
crates/separation/src/spleeter/gpu.rs, so no PyTorch or TensorFlow is
needed and the conversion runs on Linux too.

One fixed-shape program holds all four U-Nets: input `magnitudes`
[1, 2, 512, 1024] (channel, frame, bin), output `estimates`
[4, 2, 512, 1024] in vocals, drums, bass, other order, as `spleeter::Net`
expects. It computes in fp16 so macOS can run it on the Neural Engine;
inputs and outputs stay fp32. Zip the .mlpackage folder to publish it
(the app unzips and compiles it on first use).
"""
import sys

import numpy as np
from safetensors.numpy import load_file

import coremltools as ct
from coremltools.converters.mil import Builder as mb
from coremltools.converters.mil.mil import types

STEMS = ["vocals", "drums", "bass", "other"]
T, F = 512, 1024


def unet(x, w, stem):
    """One stem's U-Net on x [1, 2, T, F]; returns its estimated magnitudes."""

    def p(name):
        return w[f"{stem}.{name}"].astype(np.float32)

    def norm(h, layer):
        scale = p(f"{layer}.bn_scale").reshape(1, -1, 1, 1)
        shift = p(f"{layer}.bn_shift").reshape(1, -1, 1, 1)
        return mb.add(x=mb.mul(x=h, y=scale), y=shift)

    convs = []
    h = x
    for k in range(6):
        # 5x5, stride 2, TensorFlow "same" on an even size: 1 before, 2 after.
        c = mb.conv(
            x=h, weight=p(f"enc{k}.weight"), bias=p(f"enc{k}.bias"),
            strides=[2, 2], pad_type="custom", pad=[1, 2, 1, 2],
        )
        h = mb.elu(x=norm(c, f"enc{k}"), alpha=1.0)
        convs.append(c)
    # The decoder starts from the last convolution before its batch norm and
    # activation; skips connect pre-norm outputs.
    h = convs.pop()
    size_t, size_f = T // 64, F // 64
    for k in range(6):
        weight = p(f"dec{k}.weight")  # [in, out, 5, 5]
        out_c = weight.shape[1]
        y = mb.conv_transpose(
            x=h, weight=weight, bias=p(f"dec{k}.bias"),
            strides=[2, 2], pad_type="valid",
        )
        # Full output is 2n + 3; "same" keeps 2n starting at 1.
        size_t, size_f = size_t * 2, size_f * 2
        y = mb.slice_by_index(
            x=y, begin=[0, 0, 1, 1], end=[1, out_c, 1 + size_t, 1 + size_f]
        )
        u = norm(mb.elu(x=y, alpha=1.0), f"dec{k}")
        h = mb.concat(values=[convs.pop(), u], axis=1) if convs else u
    mask = mb.sigmoid(
        x=mb.conv(
            x=h, weight=p("out.weight"), bias=p("out.bias"),
            strides=[1, 1], dilations=[2, 2], pad_type="custom", pad=[3, 3, 3, 3],
        )
    )
    return mb.mul(x=mask, y=x)


def main(weights, out):
    w = load_file(weights)

    @mb.program(
        input_specs=[mb.TensorSpec(shape=(1, 2, T, F), dtype=types.fp32)],
        opset_version=ct.target.macOS13,
    )
    def prog(magnitudes):
        return mb.concat(
            values=[unet(magnitudes, w, s) for s in STEMS], axis=0, name="estimates"
        )

    model = ct.convert(
        prog,
        convert_to="mlprogram",
        compute_precision=ct.precision.FLOAT16,
        minimum_deployment_target=ct.target.macOS13,
    )
    model.short_description = "Spleeter 4-stem U-Nets (Deezer, MIT) for openDJ's instant stems"
    model.user_defined_metadata["stems"] = ",".join(STEMS)
    model.save(out)
    print(f"{out}: saved")


if __name__ == "__main__":
    main(sys.argv[1], sys.argv[2])
