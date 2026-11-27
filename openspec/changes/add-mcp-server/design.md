# Design

## Decisions

- **One binary.** `opendj search …` and `opendj mcp` run from the app binary, which checks its first argument before the single-instance check, the log, audio or the window. The code lives in `crates/mcp`, which depends only on `opendj-library` and `serde_json`, so it builds and tests without the UI.
- **Stdio only.** The assistant starts `opendj mcp` and talks JSON-RPC 2.0 over stdin and stdout (MCP 2025-06-18). Nothing listens on a port, so no other program or web page can reach it. Hand-rolled like ClubCraft's server: the protocol part is small and needs no async runtime.
- **The CLI is the tools.** Each command maps onto one tool and its arguments; `--json` prints the tool's result as is, otherwise a short text rendering. One code path, one set of tests.
- **Library access.** The library is SQLite in WAL mode with a 5 s busy timeout and immediate transactions, so a command and the running app can both write. Commands keep transactions short. A command refuses to run when there is no library yet, rather than creating an empty one.
- **Telling the app.** After a write, the command writes `<config>/.library-changed`. The app checks its modification time once a second and reloads tracks, tags and the tree, so it doesn't save over the change from its own copy.
- **Search.** The same search syntax as the browser (`bpm:120-124 key:8A genre:house`), with the DJ's tag group and grouping names, so what works in the search bar works here.
- **Prompt injection.** Track text is written by whoever tagged the file. Results return it in named fields, cut to 300 characters (comments) and stripped of control characters; tool descriptions say these fields are data. No tool deletes tracks, files or lists, and every write names its tracks by id.
- **Bulk limits.** A write takes at most 500 tracks per call, and `dry_run: true` reports what would change without changing it.
- **Heavy jobs** (later step): a file lock `<config>/heavy.lock` shared with the app's analysis and stems work, and a "deck playing" marker the app keeps fresh; a heavy command waits while any deck plays, with no override.
