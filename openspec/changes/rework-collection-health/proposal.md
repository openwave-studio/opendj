# Proposal

Planned for: **v0.8 · From the booth to the studio**

## Why

Collection Health groups its checks into five areas, but the areas hide what is wrong: "Files" mixes missing files with low bitrate, "Info" and "Prep" don't say what they find, and the bar over an area's tracks shows check chips (which filter the list) next to fix buttons (which change files) in the same pill style, with no order to do things in. The Overview shows three stage cards that don't add up, and "% clean" doesn't say what holds it back. Clicking another area in the sidebar keeps an open review card. Duplicates list tracks, not groups, with no reason or confidence. Lists, tags and genres are handled by footer buttons, and some problems aren't looked for at all: duplicates that only sound alike, spelling twins in tags, list problems, and audio files in the music folders that aren't in the collection.

A gap analysis against Lexicon, rekordbox-library-fixer, rekordbox Migration Helper, RCT, Crate Cleaner and dupeGuru found the same few patterns: a duplicate group is one kept copy plus the others with how they were matched, sure fixes run at once and the rest are reviewed, and tag and genre spelling twins are rolled up in one screen.

## What Changes

- **One sidebar row per check**, under white one-word sections: Files (Missing files, Low bitrate), Location (Not in collection, Outside folders, Long tracks, Strays, Not music), Duplicates, Metadata (Junk in names, Missing information, No cover, Album tags differ), Prep (Not analyzed, Unrated) and Organization (Tags, Lists). Each row's dot shows its state and turns into a green tick when the check finds nothing. Ignored and Quarantine are the last rows.
- **Check pages**: the check's name, one line on what is listed, numbered steps (the fixes in the order to do them, each one button), then Show tabs that only filter the list, and per-row fixes. Fixes that need answers on the way open a short wizard (Find missing files, Look up online, Roll up tags, Bring in files).
- **Overview**: one stage bar that adds up to 100% of the collection (Inbox, In Prep, Play Ready in the stage colors), Wanted as a grey bar on the same scale under it, the clean score with what holds it back (tracks per check and how much fixing it would raise the score), and one tile per check grouped like the sidebar.
- **Duplicates**: one row per group with how it was matched and how sure (sure, likely, versions), opened to compare the copies with differences from the kept copy marked; audio fingerprints find copies whose names differ; a local file is kept over a streamed copy.
- **Tags**: a page with a card per tag group (Genre first): tracks tagged, top tags, spelling twins and tags used once, and a Roll up list; Merge into… on a tag in Settings › Tags. Merges write to the files.
- **Lists**: a page for tracks twice in a list, empty lists, a folder and a list with one name, and entries for removed tracks, with its own % tidy; only empty lists that came from an import and were never changed are deleted without review. Lists don't count in the clean score.
- **Not in collection**: audio files in the music and stage folders that aren't in the collection, brought into the Inbox or sent to quarantine; old waveform and stem caches cleared from the Overview.
- **Missing files** that can't be found can move to Wanted, keeping their cues and lists.
- Selecting a row in the sidebar always shows that check (an open review card closes).

## Capabilities

### Modified Capabilities

- `library-health`: per-check rows and pages, steps and Show tabs, the stage bar and explained score, grouped duplicates, Tags, Lists and Not in collection

## Out of Scope

- iCloud offloaded files (add-cloud-storage-care), odd values, no cues, clickable stats beyond the Tags page, a Recent fixes log.

## Impact

library (`health::Check`, `health::Area` sections, fingerprints, tag merges, list checks), app browser (health tree, pages, dashboard data), app ui (`HealthPanel`, `HealthBar`, review card, new pages). Mockups: claude.ai artifact "Collection Health Upgrade" (project files health-upgrade/).
