#!/usr/bin/env python3
"""Writes a docs page for every controller mapping in res/controllers.

Each page lists the mapping's controls from the descriptions in its XML file,
with SHIFT actions in their own column. Controls that drive something openDJ
doesn't have yet are marked, using website/docs/supported-controls.txt (the
output of `opendj --list-controls`). Also writes the controllers' index page
and src/controllers.json, their entries in the docs' sidebar.

Usage: controllers.py <res/controllers> <website/docs>
"""

import html
import json
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parent))
from controller_figure import numbered  # noqa: E402

MIXXX_MANUAL = "https://manual.mixxx.org/2.4/en/hardware/controllers/{}"
ISSUE_URL = "https://github.com/openwave-studio/opendj/issues/new?template=controller.yml"
STATUS = {
    "supported": "**Supported.** Tried on the hardware.",
    "beta": "**Beta.** [Let us know]({}) if you run into any issues.",
    "planned": "**Coming soon.** openDJ doesn't have a mapping for this controller yet.",
}

SHIFT = re.compile(r"\s*\+\s*shift\b|\bshift\s*\+\s*|\s*\(shift(ed)?\)", re.I)
DECK = re.compile(r"\s*\((?:deck|ch|channel)\s*\d\)", re.I)
SECTION = re.compile(r"^\s*(.+?)\s+section\s+start\b", re.I)
TURN = re.compile(r"^(rotate|turn|turned|twist)$", re.I)
GESTURE = re.compile(r"^(slider?|slide|rotate|turn|knob|press|fader|encoder)$", re.I)

# What a control does, for descriptions that only name the gesture
# ("CROSSFADER - slider").
KEY_LABELS = {
    "volume": "Channel volume",
    "crossfader": "Crossfade between the decks",
    "pregain": "Gain",
    "parameter1": "EQ low",
    "parameter2": "EQ mid",
    "parameter3": "EQ high",
    "super1": "Filter",
    "rate": "Tempo",
    "headMix": "Mix of cue and main in the headphones",
    "headGain": "Headphone volume",
    "gain": "Main volume",
    "balance": "Balance",
    "play": "Play/pause",
    "cue_default": "Cue",
    "cue_gotoandplay": "Jump to the cue point and play",
    "cue_gotoandstop": "Jump to the cue point and stop",
    "start_play": "Play from the start",
    "start_stop": "Stop and go back to the start",
    "stop": "Stop",
    "sync_enabled": "Sync",
    "beatsync": "Sync once",
    "pfl": "Headphone cue",
    "keylock": "Key lock",
    "quantize": "Quantize",
    "reverse": "Play backwards",
    "eject": "Eject the track",
    "LoadSelectedTrack": "Load the selected track",
    "MoveVertical": "Scroll the library",
    "ScrollVertical": "Scroll the library by pages",
    "MoveFocus": "Move between the library and its sidebar",
    "MoveFocusForward": "Move between the library and its sidebar",
    "GoToItem": "Open the selected item",
    "SelectNextTrack": "Select the next track",
    "SelectPrevTrack": "Select the previous track",
    "enabled": "Turn on or off",
    "fwd": "Fast forward",
    "back": "Rewind",
    "rate_temp_up": "Nudge faster",
    "rate_temp_down": "Nudge slower",
    "rate_perm_up": "Tempo up a step",
    "rate_perm_down": "Tempo down a step",
    "loop_in": "Loop in",
    "loop_out": "Loop out",
    "reloop_exit": "Loop on or off",
    "reloop_toggle": "Loop on or off",
    "loop_halve": "Halve the loop",
    "loop_double": "Double the loop",
    "bpm_tap": "Tap the tempo",
    "filterLowKill": "Kill the lows",
    "filterMidKill": "Kill the mids",
    "filterHighKill": "Kill the highs",
    "beats_translate_curpos": "Move the beat grid to here",
}
NUMBERED_LABELS = [
    (re.compile(r"hotcue_(\d+)_activate"), "Hot cue {}"),
    (re.compile(r"hotcue_(\d+)_clear"), "Clear hot cue {}"),
    (re.compile(r"beatloop_([\d.]+)_toggle"), "Loop {} beats"),
    (re.compile(r"beatjump_([\d.]+)_forward"), "Jump {} beats forward"),
    (re.compile(r"beatjump_([\d.]+)_backward"), "Jump {} beats back"),
]


def key_label(group, key):
    if group == "[AutoDJ]" and key == "enabled":
        return "Automix on or off"
    if key in KEY_LABELS and not (key == "enabled" and "Effect" not in group):
        return KEY_LABELS[key]
    for pattern, label in NUMBERED_LABELS:
        m = pattern.fullmatch(key)
        if m:
            return label.format(m.group(1))
    return None


def group_section(group):
    """A section for a control without section comments in its file."""
    if re.match(r"\[(Channel|Deck)\d", group):
        return "Decks"
    if re.match(r"\[(Sampler|PreviewDeck)", group):
        return "Samplers"
    if "Effect" in group:
        return "Effects"
    if group in ("[Library]", "[Playlist]", "[AutoDJ]"):
        return "Browser"
    if group in ("[Master]", "[Main]", "[Mixer]", "[Mixer Profile]"):
        return "Mixer"
    return "Other"


BADGES = {"supported": {"text": "Supported", "variant": "success"},
          "beta": {"text": "Beta", "variant": "tip"}}


def front_matter(title, status=None):
    """Starlight's front matter: the title, a sidebar badge for the controllers
    openDJ looks after, and no edit link, since the page is generated."""
    out = ["---", f"title: {json.dumps(title)}", "editUrl: false"]
    if status in BADGES:
        out.append("sidebar:\n  badge: " + json.dumps(BADGES[status]))
    return out + ["---", ""]


def md(text):
    return html.escape(" ".join(text.split()), quote=False).replace("|", "\\|")


# Makers as the popular list groups them: Pioneer DJ became AlphaTheta, so
# their gear sits together, and Traktor gear comes under Native Instruments.
MAKERS = [("Pioneer", "Pioneer DJ and AlphaTheta"), ("AlphaTheta", "Pioneer DJ and AlphaTheta"),
          ("Native Instruments", "Native Instruments"), ("Allen & Heath", "Allen & Heath"),
          ("DJ TechTools", "DJ TechTools")]


def maker(name):
    for prefix, label in MAKERS:
        if name.startswith(prefix):
            return label
    return name.split()[0]


def natural(text):
    return [int(t) if t.isdigit() else t.lower() for t in re.split(r"(\d+)", text)]


def slug(path):
    name = (path.name.replace(".midi.xml", "").replace(".hid.xml", "")
            .replace(".opendj-mapping", "").replace(".xml", ""))
    return re.sub(r"[^a-z0-9]+", "-", name.lower()).strip("-")


def parse_description(text):
    """Splits "NAME +SHIFT (DECK1) - press - action" into its parts."""
    parts = [p.strip() for p in text.split(" - ")]
    if len(parts) >= 3:
        name, gesture, action = parts[0], parts[1], " - ".join(parts[2:])
    elif len(parts) == 2:
        name, gesture, action = parts[0], "", parts[1]
    elif len(text.split()) <= 4 and not text.endswith("."):
        name, gesture, action = text, "", ""
    else:
        name, gesture, action = "", "", text
    shift = bool(SHIFT.search(name))
    unshifted = SHIFT.sub(" ", name).strip()
    if DECK.sub("", unshifted).strip():
        name = unshifted
    else:
        shift = False  # the SHIFT button itself
    if TURN.match(gesture):
        name = f"{name}, turn" if name else "Turn"
    return name, shift, action


class Mapping:
    def __init__(self, path, supported):
        self.path = path
        builder = ET.TreeBuilder(insert_comments=True)
        root = ET.parse(path, ET.XMLParser(target=builder)).getroot()
        info = root.find("info")
        text = lambda tag: (info.findtext(tag) or "").strip() if info is not None else ""
        self.name = text("name") or path.stem
        self.author = text("author")
        self.description = text("description")
        self.manual = text("manual")
        self.forums = text("forums")
        self.hid = root.find(".//controller") is not None and path.name.endswith(".hid.xml")

        # Rows in file order: (section, name, shift, action, unsupported).
        self.rows = []
        self.undescribed = 0
        self.mapped = 0
        self.missing = 0
        self.scripted = 0
        controls = root.find(".//controls")
        children = list(controls) if controls is not None else []
        sections = [c for c in children if c.tag is ET.Comment and SECTION.match(c.text or "")]
        section = None
        for child in children:
            if child.tag is ET.Comment:
                m = SECTION.match(child.text or "")
                if m:
                    section = m.group(1).strip().title()
                continue
            if child.tag != "control":
                continue
            group = (child.findtext("group") or "").strip()
            key = (child.findtext("key") or "").strip()
            scripted = "." in key or child.find("options/Script-Binding") is not None
            unsupported = False
            self.scripted += scripted
            if not scripted and supported is not None:
                self.mapped += 1
                if f"{group},{key}" not in supported:
                    self.missing += 1
                    unsupported = True
            desc = (child.findtext("description") or "").strip()
            if not desc:
                self.undescribed += 1
                continue
            name, shift, action = parse_description(desc)
            # 14-bit controls come as an MSB and an LSB line; keep one.
            if re.search(r"\bLSB\b", action):
                continue
            action = re.sub(r"\s*\bMSB\b", "", action).strip()
            if GESTURE.match(action) or not action or action == key:
                if re.match(r"shift\b", DECK.sub("", name).strip(), re.I):
                    action = "Hold for the actions in the SHIFT column"
                elif key_label(group, key):
                    action = key_label(group, key)
                elif not scripted:
                    action = f"`{group} {key}`"
            if not action:
                self.undescribed += 1
                continue
            where = section if sections else group_section(group)
            self.rows.append((where, name, shift, action, unsupported))

    @property
    def documented(self):
        total = len(self.rows) + self.undescribed
        return total > 0 and len(self.rows) / total >= 0.4

    def table_rows(self):
        """One line per control and section, with the SHIFT action beside the
        normal one, and one line for both decks when they do the same."""
        records = {}
        for section, name, shift, action, unsupported in self.rows:
            base = DECK.sub("", name).strip()
            k = (section, base, shift, DECK.sub("", action))
            r = records.setdefault(k, {"names": set(), "action": action, "missing": False})
            r["names"].add(name)
            r["missing"] |= unsupported
        lines = []
        for (section, base, shift, _), r in records.items():
            name = base if len(r["names"]) > 1 else next(iter(r["names"]))
            slot = "shift" if shift else "normal"
            same = [l for l in lines if l["section"] == section and l["name"] == name]
            if any(r["action"] in (l["normal"], l["shift"]) for l in same):
                continue
            line = next((l for l in lines if l["section"] == section and l["name"] == name
                         and not l[slot]), None)
            if line is None:
                line = {"section": section, "name": name, "normal": "", "shift": "", "missing": False}
                lines.append(line)
            line[slot] = r["action"]
            line["missing"] |= r["missing"]
        # Keep each section's lines together, sections in first-seen order.
        sections = list(dict.fromkeys(l["section"] for l in lines))
        return [(l["section"], l) for s in sections for l in lines if l["section"] == s]

    def page(self, status=None, guide=None, figure=None):
        out = front_matter(self.name, status)
        if status:
            out += [STATUS[status].format(ISSUE_URL), ""]
        if guide:
            out += [guide.strip(), ""]
        if figure:
            out += figure_section(self.name, figure)
        meta = []
        if self.author:
            meta.append(f"Mapping by {md(self.author)}, from Mixxx")
        if self.manual:
            meta.append(f"[Mixxx manual page]({MIXXX_MANUAL.format(self.manual.lower())})")
        if self.forums.startswith("http"):
            meta.append(f"[Forum thread]({self.forums})")
        rest = []
        if self.description and self.description != self.name:
            rest += [md(self.description), ""]
        if self.mapped:
            works = self.mapped - self.missing
            line = (f"openDJ supports **{works} of the {self.mapped}** controls this mapping "
                    "sets directly.")
            if self.scripted:
                line += " The rest of the mapping runs as a script, which isn't counted here."
            rest += [line, ""]
            if self.missing and self.documented:
                rest += ["Lines marked *Not in openDJ yet* don't do anything yet.", ""]
        if self.documented:
            rows = self.table_rows()
            # Mappings that describe controls in sentences, without naming the
            # hardware, get a table without the Control column.
            named = sum(1 for _, r in rows if r["name"]) >= len(rows) / 2
            current = None
            # Under a figure the full list is folded away, with small headings
            # that stay out of the page outline.
            level = "####" if figure else "##"
            for section, row in rows:
                if section != current:
                    current = section
                    head = "| Control | Action | With SHIFT |" if named else "| Action | With SHIFT |"
                    rest += ["", f"{level} {md(section)}", "", head, "|---" * head.count("|", 1) + "|"]
                action = md(row["normal"]) or "–"
                if row["missing"]:
                    action += " *Not in openDJ yet*"
                cells = [md(row["name"]) or "–"] if named else []
                if not named and row["name"] and not row["normal"]:
                    action = md(row["name"])
                cells += [action, md(row["shift"]) or "–"]
                rest.append("| " + " | ".join(cells) + " |")
            if self.undescribed:
                rest += ["", f"{self.undescribed} more controls in this mapping have no "
                         "description, so they aren't listed."]
            if figure:
                out += ["<details>", "<summary>Every control, as the mapping describes it</summary>",
                        ""] + rest + ["", "</details>", ""]
            else:
                out += (["## Every control", ""] if guide else []) + rest
        elif not figure:
            out += rest
            if self.hid:
                out += ["This is a USB HID mapping: it runs as a script, with no list of controls "
                        "to show here."]
            else:
                out += ["This mapping doesn't describe its controls one by one, so there's no "
                        "list here yet."]
            if self.manual:
                out[-1] += " The Mixxx manual page linked below shows the layout."
            out += [""]
        out += ["", "---", ""]
        if meta:
            out += [" · ".join(meta), ""]
        out += [f"Generated from `res/controllers/{self.path.name}`. If something here is "
                f"wrong on your controller, [tell us]({ISSUE_URL}).", ""]
        return "\n".join(out)


def figure_section(name, figure):
    """The controller's picture with a number on each control, and what each
    number does."""
    image, rows = figure
    out = ["## What the controls do", "",
           f"![The {html.escape(name)} with a number on each control](figures/{image})", ""]
    if any(r[0] for r in rows):
        out += ["Grey numbers do the same as the lilac one with that number, on the other deck.", "",
                "| | Control | Does | With SHIFT |", "|--:|---|---|---|"]
        out += [f"| {n} | {md(c)} | {md(d) or '–'} | {md(s) or '–'} |" for n, c, d, s in rows]
        out.append("")
    return out


class OwnMapping:
    """One of openDJ's own mappings (`.opendj-mapping`): its guide lists the
    controls, so the page is the guide."""

    hid = False
    documented = True

    def __init__(self, path):
        self.path = path
        data = json.loads(path.read_text())
        self.name = data.get("name") or path.stem
        self.author = data.get("author", "")

    def page(self, status=None, guide=None, figure=None):
        out = front_matter(self.name, status)
        if status:
            out += [STATUS[status].format(ISSUE_URL), ""]
        if guide:
            out += [guide.strip(), ""]
        if figure:
            out += figure_section(self.name, figure)
        out += ["---", "",
                f"openDJ's own mapping, `res/controllers/{self.path.name}`. If something here "
                f"is wrong on your controller, [tell us]({ISSUE_URL}).", ""]
        return "\n".join(out)


def main():
    controllers, docs = Path(sys.argv[1]), Path(sys.argv[2])
    listing = docs / "supported-controls.txt"
    supported = set(listing.read_text().split()) if listing.exists() else None
    out = docs / "src" / "content" / "docs" / "controllers"
    out.mkdir(parents=True, exist_ok=True)
    for old in out.glob("*.md"):
        old.unlink()
    figures_out = out / "figures"
    figures_out.mkdir(exist_ok=True)
    for old in figures_out.glob("*.svg"):
        old.unlink()

    mappings = []
    paths = list(controllers.glob("*.midi.xml")) + list(controllers.glob("*.hid.xml"))
    for path in sorted(paths):
        try:
            mappings.append(Mapping(path, supported))
        except ET.ParseError as e:
            print(f"skipping {path.name}: {e}", file=sys.stderr)
    for path in sorted(controllers.glob("*.opendj-mapping")):
        mappings.append(OwnMapping(path))
    mappings.sort(key=lambda m: m.name.lower())

    # The controllers openDJ looks after itself, and their hand-written guides.
    support = json.loads((controllers.parent.parent / "crates/controllers/support.json").read_text())
    looked_after = support["controllers"]
    status = {c["file"]: c["status"] for c in looked_after if c["file"]}
    guides = docs / "controller-guides"
    # The Controller Editor's pictures, and what each control does in them
    # (written by `cargo run -p opendj-controllers --example docs_controls`).
    layouts = {}
    for f in sorted((controllers.parent.parent / "crates/controllers/layouts").glob("*.json")):
        layout = json.loads(f.read_text())
        for file in [layout.get("file")] + layout.get("also", []):
            if file:
                layouts[file] = layout
    functions = docs / "figures" / "functions"

    index = front_matter("Supported controllers") + [
             "openDJ uses Mixxx's controller mappings, so these MIDI and USB HID controllers "
             "load their mapping as soon as they're plugged in.", "",
             "## Popular controllers", "",
             "openDJ looks after these itself: each has a picture in the Controller Editor and "
             f"a guide. **Beta** ones are new: [let us know]({ISSUE_URL}) if you run into any "
             "issues.", ""]
    by_file = {m.path.name: m for m in mappings}
    labels = {"supported": "Supported", "beta": "Beta", "planned": "Coming soon"}
    by_maker = {}
    for c in looked_after:
        by_maker.setdefault(maker(c["name"]), []).append(c)
    for name_of_maker in sorted(by_maker, key=str.lower):
        index += [f"### {md(name_of_maker)}", "",
                  "| Controller | Status | Notes |", "|---|---|---|"]
        # Coming soon last, the rest by name with numbers in order (DDJ-200 before DDJ-1000).
        for c in sorted(by_maker[name_of_maker],
                        key=lambda c: (c["status"] == "planned", natural(c["name"]))):
            m = by_file.get(c["file"] or "")
            name = f"[{md(c['name'])}]({slug(m.path)}.md)" if m else md(c["name"])
            index.append(f"| {name} | {labels[c['status']]} | {md(c['note']) or '–'} |")
        index.append("")
    index += ["## Every mapping", "",
              "| Controller | Mapping by | Controls listed |", "|---|---|---|"]
    entries = []
    for m in mappings:
        s = slug(m.path)
        guide = guides / f"{s}.md"
        figure = None
        if m.path.name in layouts and (functions / f"{m.path.name}.json").exists():
            svg, rows = numbered(layouts[m.path.name],
                                 json.loads((functions / f"{m.path.name}.json").read_text()))
            (figures_out / f"{s}.svg").write_text(svg)
            figure = (f"{s}.svg", rows)
        page = m.page(status.get(m.path.name), guide.read_text() if guide.exists() else None,
                      figure)
        (out / f"{s}.md").write_text(page)
        listed = "Yes" if m.documented else "Not yet"
        index.append(f"| [{md(m.name)}]({s}.md) | {md(m.author) or '–'} | {listed} |")
        entries.append(f"controllers/{s}")
    index += ["", f"Don't see yours, or does it misbehave? [Tell us which one]({ISSUE_URL}).", ""]
    (out / "index.md").write_text("\n".join(index))
    (docs / "src" / "controllers.json").write_text(json.dumps(entries, indent=1) + "\n")
    documented = sum(m.documented for m in mappings)
    print(f"Wrote {len(mappings)} controller pages ({documented} with control lists)")


if __name__ == "__main__":
    main()
