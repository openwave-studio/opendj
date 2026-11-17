# Proposal

Planned for: **v0.8**. Issue #827, a user request (2026-11-16).

## Why

Mid-set it is easy to forget a track was already played. openDJ already writes every play into the night's History session, but the browser shows nothing, so the DJ scrolls past tracks they played an hour ago. A survey of other apps (Serato, rekordbox, Traktor, VirtualDJ, djay Pro, Engine DJ, Mixxx, OpenMix) found that small icons alone get missed, marking a track the moment it is heard in the headphones is the top complaint, and not being able to reset is the second.

## What Changes

- **Played marks**: a track played in the current History session shows dimmed in the track list with a small lilac check before its title. Hovering the check shows when it was played ("Played 23:41"). Tracks are not hidden and there is no filter.
- **Heard in the master only**: a play counts after 30 s heard in the master (playing, channel fader up, crossfader open on its side). Pre-listening in the headphones never counts. With an external mixer, a playing deck counts. History and play counts follow the same rule.
- **Session length**: marks last for the History session (a new one after 60 min with nothing played, or at 6:00). The app carries on in the last session when it restarts within that time, so the marks survive a restart or crash. Recording does not start a new session.
- **Deck note**: loading a played track shows a lilac "Played 23:41" pill beside the deck's title for 3 s.
- **Start new session**: a small + at the right end of History's Sessions heading, tooltip "Start new session", ends the session and clears the marks.
- **Mark as Not Played** in the track menu takes the selected tracks out of the session.

## Out of scope

- A Hide played filter, a played count chip, and Mark as Played.
- Marks in the side panel lists (Automix, Up Next).

## Capabilities

### Modified Capabilities

- `music-library`: plays count only when heard in the master; the session carries over a restart and can be ended by hand.
- `track-browser`: played tracks dimmed with a check; deck note; Mark as Not Played.
