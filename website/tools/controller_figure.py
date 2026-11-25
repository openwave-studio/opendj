#!/usr/bin/env python3
"""Draws a controller from its Controller Editor layout (crates/controllers/layouts)
as an SVG for the docs, with numbered callouts on groups of controls.

Usage: controller_figure.py <layout.json> <callouts.json> <out.svg>

Without a hand-written callouts file, `numbered()` puts a numbered lilac
circle on each control instead and returns the matching list, from what the
Controller Editor says each control does (website/docs/figures/functions/).

callouts.json has "groups", a list of {"name": "Loop", "at": [x, y]}: a lilac
tag naming each area, and "labels", {control id: [what it does, what it does
with SHIFT, "inside"?]}, written under each control (or inside a big one), and
"notes", [{"text": ..., "at": [x, y]}], plain grey text.
"""
import json
import re
import sys
from pathlib import Path

LILAC, BODY, PANEL, EDGE, CTRL, LABEL = "#B69CFF", "#1a1922", "#14131b", "#2f2c40", "#3a374f", "#8f8ca6"


def control(c):
    x, y, w, h = c["x"], c["y"], c["w"], c["h"]
    kind = c["kind"]
    if kind == "jog":
        cx, cy, r = x + w / 2, y + h / 2, w / 2
        return (f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="#121118" stroke="{EDGE}" stroke-width="6"/>'
                f'<circle cx="{cx}" cy="{cy}" r="{r * 0.72}" fill="#1d1b27" stroke="{EDGE}" stroke-width="2"/>'
                f'<circle cx="{cx}" cy="{cy}" r="{r * 0.12}" fill="{CTRL}"/>')
    if kind in ("knob", "encoder"):
        cx, cy, r = x + w / 2, y + h / 2, w / 2
        return (f'<circle cx="{cx}" cy="{cy}" r="{r}" fill="{CTRL}"/>'
                f'<line x1="{cx}" y1="{cy}" x2="{cx}" y2="{y + 4}" stroke="#d6d4e4" stroke-width="3" stroke-linecap="round"/>')
    if kind == "fader":
        vertical = h > w
        slot = (f'<rect x="{x + w / 2 - 3}" y="{y}" width="6" height="{h}" rx="3" fill="#0c0b11"/>' if vertical
                else f'<rect x="{x}" y="{y + h / 2 - 3}" width="{w}" height="6" rx="3" fill="#0c0b11"/>')
        cap = (f'<rect x="{x}" y="{y + h / 2 - 12}" width="{w}" height="24" rx="4" fill="{CTRL}"/>' if vertical
               else f'<rect x="{x + w / 2 - 14}" y="{y}" width="28" height="{h}" rx="4" fill="{CTRL}"/>')
        return slot + cap
    if c.get("round"):
        return f'<circle cx="{x + w / 2}" cy="{y + h / 2}" r="{w / 2}" fill="{CTRL}"/>'
    rx = 8 if kind == "pad" else 4
    return f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{rx}" fill="{CTRL}"/>'


def mirror(cid, ids):
    """The left-hand twin of a right-hand control, if the layout has one."""
    for a, b in (("r-", "l-"), ("-r", "-l"), ("right", "left")):
        if a in cid and cid.replace(a, b, 1) in ids:
            return cid.replace(a, b, 1)
    return None


# Names the Controller Editor falls back to when a script's own function
# name is all it has ("Vinyl button", "Pad select buttons 3", "Input msb"):
# they say nothing to a reader, so they're left out.
NOISE = re.compile(r"^mode button|^recv\b|\b(input|msb|lsb|callback|handler|leds?)\b|^pad select|^[a-z]+\d+\b", re.I)
BUTTON = re.compile(r"\s*\bbuttons?\b\s*\d*$", re.I)
GESTURES = ("press", "turn", "touch", "side", "top", "push", "switch", "curve", "hold", "normal",
            "long", "double", "tap")
# Effect controls, said the way the effects panel says them.
EFFECT = [(r"^FX \d+ ", ""), (r"^Effect \d+ on$", "Effect on"), (r"^Effect \d+ next$", "Next effect"),
          (r"^Effect \d+ previous$", "Previous effect"), (r"^Effect \d+ choose$", "Choose effect"),
          (r"^Effect \d+ knob$", "Effect amount"), (r"^Effect \d+ beats halve$", "Halve effect beats"),
          (r"^Effect \d+ beats double$", "Double effect beats"), (r"^Effect \d+ (.*)$", r"Effect \1"),
          (r"^Release$", "Effect release"), (r"^Bpm auto$", "Effect beats from the track")]


# Names from mapping scripts, said the way the app says them.
RENAME = [(r"^Sampler (\d+) Channel fader$", r"Sampler \1 volume"),
          (r"^(Jog|Wheel) touch$", "Scratch (platter touch)"), (r"^Jog platter tick.*$", "Jog nudge"),
          (r"^Lfo (\w+)$", r"LFO \1"), (r"^Effect loaded effect$", "Choose effect"),
          (r"^Rate set zero$", "Reset tempo"), (r"^Head mix$", "Headphone mix"),
          (r"^(Vinylmode|Vinyl mode toggle|Wheel toggle|Vinyl button)$", "Vinyl mode"),
          (r"^Scratch toggle$", "Vinyl mode"), (r"^Toggle beatloop roll mode$", "Loop roll pad mode"),
          (r"^Toggle beatloop mode$", "Beat loop pad mode"), (r"^Toggle (.+) mode$", r"\1 pad mode"),
          (r"^(Pad mode pressed|Pad mode button pressed|Change mode)$", "Pad mode"),
          (r"^(Toggle deck|Deck switch)$", "Switch deck"), (r"^Four deck mode$", "Four-deck mode"),
          (r"^(Position|Needle search strip position)$", "Needle search"),
          (r"^Auto ?djtoggle sync (\w+)$", r"Automix sync \1"), (r"^Autodj skip next$", "Automix skip"),
          (r"^enable \(on touch\).*Pitch bend$", "Fast search"), (r"^Hotcue", "Hot cue"),
          (r"^Trans mode$", "Transform pad mode"), (r"\s*\(openDJ\)$", "")]
NOISE_EXACT = re.compile(r"^(shift( toggle| state)?|route|rotary selector click|needle search touch|"
                         r"browse encoder|.* encoder)$", re.I)


def clean(names):
    out = []
    for n in names:
        n = BUTTON.sub("", n or "")
        for a, b in EFFECT + RENAME:
            n = re.sub(a, b, n)
        n = n[:1].upper() + n[1:]
        if n and not NOISE.search(n) and not NOISE_EXACT.match(n) and n not in out:
            out.append(n)
    # "Sampler 1 volume · Sampler 2 volume · …" as "Sampler 1-4 volume".
    groups = {}
    for n in out:
        groups.setdefault(re.sub(r"\d+", "#", n, count=1), []).append(n)
    merged = []
    for n in out:
        key = re.sub(r"\d+", "#", n, count=1)
        same = groups[key]
        if len(same) > 2 and "#" in key:
            if same[0] == n:
                nums = [int(re.search(r"\d+", x).group()) for x in same]
                merged.append(key.replace("#", f"{min(nums)}-{max(nums)}"))
        else:
            merged.append(n)
    return merged


def short(names, most=3):
    names = clean(names)
    text = " · ".join(names[:most])
    return text + (" …" if len(names) > most else "")


def does(layers, label="", kind=""):
    """What a control does without and with SHIFT, from its layers."""
    # "Deck 3" layers repeat the control for the deck it switches to.
    other = {l: n for l, n in layers.items() if not re.match(r"deck [2-4b-d]\b", l, re.I)}
    layers = other or layers
    plain = {l: n for l, n in layers.items() if "shift" not in l.lower()}
    shifted = {l: n for l, n in layers.items() if "shift" in l.lower()}

    def say(group):
        if not group:
            return ""
        def bare(l):
            l = re.sub(r"deck [\da-d]\b\s*\+?\s*", "", l, flags=re.I)
            return l.replace("SHIFT", "").replace(":", "").strip(" +,").lower()
        gesture = all(not bare(l) or any(g in bare(l) for g in GESTURES) for l in group)
        named = [l for l in group if bare(l) not in ("", "normal") and not gesture]
        # A mode button: its layers are the modes it picks.
        if named and not any(clean(n) for n in group.values()):
            picks = [l.replace("SHIFT:", "").strip().capitalize() for l in named]
            if kind == "switch":
                return "Picks " + ", ".join(picks)
            if any(bare(l) in label.lower() for l in named):
                return "Pad mode: " + ", ".join(picks)
            return ""
        if len(group) == 1:
            return short(next(iter(group.values())))
        # A jog, an encoder or a control with modes: "Turn: Down · Up; Press: Next panel".
        parts = []
        for l, n in group.items():
            text = short(n, 2)
            if text and text not in [p.split(": ", 1)[-1] for p in parts]:
                how = bare(l).capitalize()
                parts.append(f"{how}: {text}" if how and how != "Normal" else text)
        return "; ".join(parts[:3])

    normal, shift = say(plain), say(shifted)
    if shift.startswith("Pad mode: "):
        shift = shift[len("Pad mode: "):]
    return normal, shift if shift != normal else ""


def numbered(layout, functions):
    """An SVG with a number on each control the mapping uses, and the list
    that goes with it: [(number, control, what it does, with SHIFT)]. Controls
    on the right deck that mirror the left one, and every pad but the first,
    share their twin's number."""
    by_id = {c["id"]: c for c in layout["controls"]}
    parts = functions.get("parts", {})
    numbers, rows = {}, []
    pads_done = set()
    for c in layout["controls"]:
        cid = c["id"]
        twin = mirror(cid, by_id)
        if twin in numbers:
            numbers[cid] = numbers[twin]
            continue
        side = cid.split("-")[0] if "-" in cid else ""
        if c["kind"] == "pad":
            if side in pads_done:
                numbers[cid] = next(n for k, n in numbers.items()
                                    if by_id[k]["kind"] == "pad" and k.split("-")[0] == side)
                continue
            layers = {}
            for k, cc in by_id.items():
                if cc["kind"] == "pad" and k.split("-")[0] == side:
                    for layer, names in parts.get(k, {}).items():
                        layers.setdefault(layer, []).extend(names)
            if not layers:
                continue
            pads_done.add(side)
            modes = [l for l in layers if "shift" not in l.lower() and l != "normal"
                     and not re.match(r"deck \d", l, re.I)]
            if modes:
                text = "What the pad mode picks: " + ", ".join(m.lower() for m in modes[:8])
                # With SHIFT, say what hot cue pads do: the one every pad mode set has.
                hot = [n for l, n in layers.items() if "shift" in l.lower() and "hot" in l.lower()]
                shift = (re.sub(r"\s*[\d-]+$", "", clean(hot[0])[0]) + " (hot cue mode)"
                         if hot and clean(hot[0]) else "")
            else:
                text, shift = does(layers)
            rows.append([len(rows) + 1, "Pads", text, shift])
            numbers[cid] = len(rows)
            continue
        layers = parts.get(cid)
        if not layers:
            continue
        normal, shift = does(layers, c["label"], c["kind"])
        if not normal and not shift or normal == "SHIFT":
            continue
        label = c["label"]
        # A mixer channel that works like another (LEVEL 3 and LEVEL 1)
        # shares its number.
        plain_label = re.sub(r"\s*(\(\d\)|\d|CH ?\d)$", "", label)
        same = next((r for r in rows if r[2:] == [normal, shift]
                     and re.sub(r"\s*(\(\d\)|\d|CH ?\d)$", "", r[1]) == plain_label), None)
        twin_row = next((r for r in rows if r[1:] == [label, normal, shift]), None)
        if twin_row:
            numbers[cid] = twin_row[0]
            continue
        if same and plain_label != label:
            same[1] = plain_label
            numbers[cid] = same[0]
            continue
        # A row of buttons that do the same (FX 1 to FX 4) shares a line.
        if rows and rows[-1][2:] == [normal, shift] and rows[-1][1] != "Pads":
            rows[-1][1] += f", {label}"
            numbers[cid] = len(rows)
            continue
        rows.append([len(rows) + 1, label, normal, shift])
        numbers[cid] = len(rows)
    W, H = layout["width"], layout["height"]
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="-30 -30 {W + 60} {H + 60}" '
           f'font-family="Inter Tabular, -apple-system, sans-serif">']
    for i, (x, y, w, h, r) in enumerate(layout["panels"]):
        out.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" '
                   f'fill="{BODY if i == 0 else PANEL}" stroke="{EDGE}" stroke-width="2"/>')
    out += [control(c) for c in layout["controls"]]
    # Twins get the same number in grey, so the right deck reads as a copy.
    first = {}
    for cid, n in numbers.items():
        first.setdefault(n, cid)
    for cid, n in numbers.items():
        c = by_id[cid]
        if c["kind"] == "pad" and first[n] != cid:
            continue
        x, y, w, h = c["x"], c["y"], c["w"], c["h"]
        if c["kind"] == "jog":
            cx, cy = x + w / 2, y + h / 2 + w * 0.22
        else:
            cx, cy = x + w / 2, y + h / 2
        main = first[n] == cid
        fill, ink = (LILAC, "#0e0d17") if main else ("#5b5770", "#ecebf5")
        # Small, packed controls get smaller numbers so they don't overlap.
        r = max(11, min(18, min(w, h) * 0.5 + 2)) if c["kind"] != "jog" else 18
        size = r * (1.05 if n < 10 else 0.95 if n < 100 else 0.8)
        out.append(f'<circle cx="{cx}" cy="{cy}" r="{r:g}" fill="{fill}" stroke="#0e0d17" stroke-width="2"/>'
                   f'<text x="{cx}" y="{cy + size * 0.36:g}" text-anchor="middle" font-size="{size:.1f}" '
                   f'font-weight="700" fill="{ink}">{n}</text>')
    out.append("</svg>")
    return "\n".join(out) + "\n", rows


def main():
    layout = json.loads(Path(sys.argv[1]).read_text())
    callouts = json.loads(Path(sys.argv[2]).read_text())
    by_id = {c["id"]: c for c in layout["controls"]}
    W, H = layout["width"], layout["height"]
    out = [f'<svg xmlns="http://www.w3.org/2000/svg" viewBox="-30 -30 {W + 60} {H + 60}" '
           f'font-family="Inter Tabular, -apple-system, sans-serif">']
    for i, (x, y, w, h, r) in enumerate(layout["panels"]):
        out.append(f'<rect x="{x}" y="{y}" width="{w}" height="{h}" rx="{r}" '
                   f'fill="{BODY if i == 0 else PANEL}" stroke="{EDGE}" stroke-width="2"/>')
    out += [control(c) for c in layout["controls"]]
    labels = callouts.get("labels", {})
    for cid, (text, *rest) in labels.items():
        c = by_id[cid]
        shift = rest[:1]
        x, y, w, h = c["x"], c["y"], c["w"], c["h"]
        if "inside" in rest:
            tx, ty = x + w / 2, y + h / 2 + (0 if shift and shift[0] else 6)
        elif c["kind"] == "jog":
            tx, ty = x + w / 2, y + h / 2 + 70
        elif c["kind"] == "fader" and h > w:
            tx, ty = x + w / 2, y + h + 22
        else:
            tx, ty = x + w / 2, y + h + 18
        out.append(f'<text x="{tx}" y="{ty}" text-anchor="middle" font-size="16" font-weight="600" fill="#ecebf5">{text}</text>')
        if shift and shift[0]:
            out.append(f'<text x="{tx}" y="{ty + 18}" text-anchor="middle" font-size="14" fill="{LILAC}">⇧ {shift[0]}</text>')
    for g in callouts.get("groups", []):
        bx, by = g["at"]
        tw = 13 * len(g["name"]) + 24
        out.append(f'<rect x="{bx}" y="{by - 16}" width="{tw}" height="32" rx="16" fill="{LILAC}"/>'
                   f'<text x="{bx + tw / 2}" y="{by + 7}" text-anchor="middle" font-size="18" font-weight="700" '
                   f'fill="#0e0d17">{g["name"]}</text>')
    for n in callouts.get("notes", []):
        out.append(f'<text x="{n["at"][0]}" y="{n["at"][1]}" text-anchor="middle" font-size="20" '
                   f'fill="{LABEL}">{n["text"]}</text>')
    out.append("</svg>")
    Path(sys.argv[3]).write_text("\n".join(out) + "\n")


if __name__ == "__main__":
    main()
