# Tasks

## 1. Library commands and MCP server

- [x] 1.1 `crates/mcp` (opendj-mcp): library tools (search, track, lists, list, create list, add to list, rate, tag) with dry run, caps and cleaned text; tests against an in-memory library
- [x] 1.2 `opendj mcp`: JSON-RPC 2.0 over stdio (initialize, tools/list, tools/call, ping), tool annotations; tests over in-memory pipes
- [x] 1.3 CLI: `opendj <command>` argument parsing onto the tools, `--json`, text output, help; app binary dispatch before the window opens (console attached on Windows)
- [x] 1.4 Change marker: commands write `.library-changed`, the app reloads tracks, tags and the tree when it moves
- [x] 1.5 User guide page (setup for an assistant, commands, safety notes)

## 2. Prep batches

- [ ] 2.1 `analyze` and `stems` commands and tools with progress and cancel, sharing a heavy-job lock with the app; always wait while a deck plays (the app keeps a playing marker fresh)

## 3. Set planning

- [ ] 3.1 `plan_set` and `suggest_next` tools on Set Builder and Automix ordering (tempo path, key, energy, played marks)

## Workflow follow-up

- Archive the change once its PR is merged, so the main specs take its requirements.
