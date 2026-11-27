# Proposal

Planned for: **v1.0 · Ready for any DJ switching over**

## Why

An assistant that can read what's playing and set up the next deck is useful during a set, but one wrong call is heard by the whole room. This change adds deck control to the MCP server from `add-mcp-server`, with rules that keep the deck on air out of reach.

## What Changes

- An MCP endpoint inside the running app, off by default: streamable HTTP on 127.0.0.1 with a random token, Host and Origin checks, no CORS.
- Tools: now playing, deck state, load a track on a deck that isn't on air, one openDJscript line, FX, Automix queue, recording.
- On-air lock: a deck playing and audible on master can't be changed by the assistant at all; there is no confirm to override it. Master and booth levels are never tool-settable.
- An AI light in the top bar while a client is connected, with one click to cut it off.
- Deck actions quantize to the next beat or bar.

## Capabilities

### New Capabilities

- `mcp-live`: deck tools for the local MCP server, with the on-air lock

### Modified Capabilities

None.

## Out of Scope

- Connections from other computers.
- Voice control.

## Impact

App module for the HTTP endpoint; uses the control registry and openDJscript; top-bar light.
