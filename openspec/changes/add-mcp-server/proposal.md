# Proposal

Planned for: **v0.9.6 · More test releases before 1.0**

## Why

Library work (finding tracks, building lists, tagging, overnight prep) is slow by hand and easy to describe in words. A command line and a local Model Context Protocol server let scripts and AI assistants do it, without touching the decks. Live deck control is its own change, `add-live-mcp`, because it needs on-air safety rules first.

## What Changes

- `opendj <command>`: headless commands on the same binary, for search, track details, lists, ratings and tags, with `--json` output for scripts. They open the library directly and never start audio, the window or controllers.
- `opendj mcp`: the same commands as MCP tools over stdio. The assistant launches it; it opens no network port and can't reach the decks.
- A running openDJ reloads the library when a command changed it (a change marker file it watches).
- Bulk writes take `dry_run` and are capped per call. Track text (comments, tags, names) comes back as plain data fields, cut to a length, with control characters removed.
- Later steps: prep batches (analysis, stems) that share a heavy-job lock with the app and always wait while a deck plays; set planning tools.

## Capabilities

### New Capabilities

- `mcp-server`: a command line and a local MCP server for library work

### Modified Capabilities

None.

## Out of Scope

- Deck control (see `add-live-mcp`).
- Network transports and cloud access.
- Deleting files.

## Impact

New crate `crates/mcp` (opendj-mcp) on the library crate; the app binary hands its first argument to it; the browser reloads on the change marker.
