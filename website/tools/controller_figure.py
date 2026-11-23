#!/usr/bin/env python3
"""Draws a controller from its Controller Editor layout (crates/controllers/layouts)
as an SVG for the docs, with numbered callouts on groups of controls.

Usage: controller_figure.py <layout.json> <callouts.json> <out.svg>

callouts.json has "groups", a list of {"name": "Loop", "at": [x, y]}: a lilac
tag naming each area, and "labels", {control id: [what it does, what it does
with SHIFT, "inside"?]}, written under each control (or inside a big one), and
"notes", [{"text": ..., "at": [x, y]}], plain grey text.
"""
import json
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
