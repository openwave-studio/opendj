---
title: Command line and AI assistants
---

openDJ's library can be searched and organised from a terminal, a script, or
an AI assistant that speaks the Model Context Protocol (MCP), such as Claude
Desktop, Claude Code or Cursor. These commands work with the library only:
they never load, play or change anything on a deck, and nothing can be
deleted through them.

They run from the openDJ program itself, without opening its window, so they
work while openDJ is open. openDJ shows their changes within a second or two.

## Finding the program

| System | Command |
| --- | --- |
| macOS | `/Applications/openDJ.app/Contents/MacOS/opendj` |
| Windows | `"C:\Program Files\openDJ\opendj.exe"` |
| Linux | `opendj` |

On macOS, add `alias opendj=/Applications/openDJ.app/Contents/MacOS/opendj`
to `~/.zshrc` to type `opendj` alone.

## Commands

| Command | What it does |
| --- | --- |
| `opendj search bpm:120-124 genre:house` | Finds tracks, with the same search words as the browser. `--limit 200` shows more than 50. |
| `opendj track 1234` | Everything about one track: tags, comment, lists it is in. |
| `opendj lists` | Your lists, crates, smart lists and folders with their ids. `--history` adds history sessions. |
| `opendj list Friday` | The tracks in a list, by name or id. |
| `opendj list create "Friday warm-up"` | Makes a list. `--crate` makes a crate, `--in 12` puts it in a folder. |
| `opendj list add Friday 1234 1240` | Adds tracks at the end of a list. |
| `opendj rate 5 1234 1240` | Sets star ratings. |
| `opendj tag add Mood Peak 1234` | Adds a tag in a tag group; `tag remove` takes it off. |

Instead of track ids, `list add`, `rate` and `tag` take `--where` and a
search: `opendj tag add Mood Peak --where "rating:5 genre:techno"`. Add
`--dry-run` to see how many tracks would change first. One command changes
at most 500 tracks.

Add `--json` to any command for output a script can read.

## AI assistants

`opendj mcp` is an MCP server on standard input and output. The assistant
starts it itself; it opens no network port, so nothing else on the network
or in a web browser can reach it.

In Claude Desktop, add it to `claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "opendj": {
      "command": "/Applications/openDJ.app/Contents/MacOS/opendj",
      "args": ["mcp"]
    }
  }
}
```

In Claude Code: `claude mcp add opendj -- /Applications/openDJ.app/Contents/MacOS/opendj mcp`.

The assistant can then search, read tracks, make lists and crates, add
tracks to them, set ratings and tag tracks. Ask it things like "make a crate
of my 4- and 5-star house tracks between 120 and 124 BPM" or "which tracks
in Friday have no Mood tag?".

## Things to know

- **Your library goes to the assistant.** Track names, tags, comments and
  file paths are sent to whichever AI service the assistant uses.
- **Tags are not instructions.** A comment or tag from a downloaded file can
  contain text written to steer an AI. openDJ shortens long text and tells
  the assistant these fields are data, but read what it plans before saying
  yes to a big change.
- **Check bulk changes.** Ask the assistant for a dry run first; it sees
  how many tracks would change.
- **Decks are out of reach.** Nothing here can load, play or stop a deck,
  even during a set.
