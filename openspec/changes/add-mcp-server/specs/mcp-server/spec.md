## Purpose

Letting scripts and AI assistants search and organise the library through a command line and a local Model Context Protocol server, without touching the decks.

## ADDED Requirements

### Requirement: Headless commands
The app binary SHALL run library commands given as arguments (`opendj search`, `opendj track`, `opendj lists`, `opendj list`, `opendj rate`, `opendj tag`) without opening a window, audio or controllers, and SHALL print JSON with `--json`.

#### Scenario: Search from a terminal
- **WHEN** `opendj search 'bpm:120-124 genre:house' --json` runs
- **THEN** it prints the matching tracks as JSON and exits

#### Scenario: No library
- **WHEN** a command runs and there is no library yet
- **THEN** it says so and exits without creating one

### Requirement: Local MCP server over stdio
`opendj mcp` SHALL serve the same commands as MCP tools over stdin and stdout, and SHALL open no network port.

#### Scenario: Assistant lists tools
- **WHEN** an assistant starts `opendj mcp` and asks for its tools
- **THEN** it gets the library tools with their input schemas and read-only or destructive hints

### Requirement: Library tools
The server SHALL offer tools to search tracks, read a track, list lists, read a list, create a list, add tracks to a list, set ratings and add or remove tags.

#### Scenario: Warm-up list
- **WHEN** an assistant searches for 118-122 BPM house tracks and adds them to a new list
- **THEN** the list appears in a running openDJ's tree within a few seconds

### Requirement: No deck control
The commands and the MCP server SHALL NOT load, play or change anything on a deck.

#### Scenario: Deck untouched
- **WHEN** any tool is called while a deck plays
- **THEN** the deck plays on unchanged

### Requirement: Safe writes
A write SHALL take at most 500 tracks per call, SHALL accept `dry_run` to report the change without making it, and no tool SHALL delete tracks, files or lists.

#### Scenario: Dry run
- **WHEN** `opendj tag add Mood Peak --where 'rating:5' --dry-run` runs
- **THEN** it reports how many tracks would be tagged and changes none

### Requirement: Track text is data
Results SHALL return track text in named fields, cut to a set length and stripped of control characters.

#### Scenario: Long comment
- **WHEN** a track's comment is 5,000 characters of instructions
- **THEN** the result carries its first 300 characters in the `comment` field

### Requirement: App reloads
A running openDJ SHALL reload its library when a command changed it.

#### Scenario: Rating from the terminal
- **WHEN** `opendj rate 1234 5` runs while openDJ is open
- **THEN** the browser shows five stars on that track within a few seconds

### Requirement: Prep never during a set
Analysis and stem separation started from a command or the MCP server SHALL NOT run while a deck plays in openDJ: a command SHALL wait for the decks to stop, an MCP call SHALL stop and report how far it got, and a separation in progress SHALL give up and start again later. Only one stem separation SHALL run at a time across openDJ and its commands.

#### Scenario: Deck starts during a batch
- **WHEN** `opendj stems --list Friday` runs and the DJ presses play
- **THEN** the separation stops at once and resumes after the decks stop

### Requirement: Set planning
The server SHALL put a list or tracks in mixing order (flow, harmonic, tempo up or down) with the key steps and tempo change between each pair, optionally saved as a new list, and SHALL suggest tracks that mix well after a given one.

#### Scenario: Plan a list
- **WHEN** `opendj plan Friday --save 'Friday order'` runs
- **THEN** a new list holds Friday's tracks in the smoothest key and tempo order
