## Purpose

Letting an AI assistant read and prepare the decks during a set, without ever changing the deck the room hears.

## ADDED Requirements

### Requirement: Off by default, local only
The live MCP endpoint SHALL be off until turned on in Settings, SHALL listen on 127.0.0.1 only, and SHALL refuse requests without its token or with a foreign Origin.

#### Scenario: Web page refused
- **WHEN** a web page in a browser sends a request to the endpoint
- **THEN** it is refused

### Requirement: On-air lock
A deck that is playing and audible on master SHALL refuse every tool that would change it, with no way to override.

#### Scenario: Load on the playing deck
- **WHEN** an assistant asks to load a track on the deck playing out
- **THEN** the call is refused and the deck plays on

### Requirement: Deck tools
The endpoint SHALL offer tools to read deck state and to load, cue, sync, set loops and FX on decks not on air, through the same controls as controllers.

#### Scenario: Prepare the next deck
- **WHEN** deck 1 plays out and an assistant loads a track on deck 2 and syncs it
- **THEN** deck 2 has the track, synced, and deck 1 is untouched

### Requirement: Connection shown
The top bar SHALL show while a client is connected, and one click SHALL cut it off.

#### Scenario: Cut off
- **WHEN** the DJ clicks the AI light
- **THEN** the client is disconnected and later calls are refused
