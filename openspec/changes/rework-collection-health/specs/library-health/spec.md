## MODIFIED Requirements

### Requirement: Area lists
The Health sidebar SHALL list the Overview, then one row per check under six sections in this order: Files (Missing files, Low bitrate), Location (Not in collection, Outside folders, Long tracks, Strays, Not music), Duplicates, Metadata (Junk in names, Missing information, No cover, Album tags differ), Prep (Not analyzed, Unrated) and Organization (Tags, Lists), then Ignored and Quarantine. Each row SHALL show how many tracks it finds and its state, with a tick when it finds none. Selecting a row SHALL show that check's page, closing any open review card.

#### Scenario: Switch check during a review
- **WHEN** the review card is open on Missing files and the DJ selects Duplicates in the sidebar
- **THEN** the card closes and the Duplicates page shows

### Requirement: Health overview
The Overview SHALL show one bar of the collection split by stage (Inbox, In Prep, Play Ready) adding up to 100%, with Wanted as a separate bar on the same scale; the share of clean tracks with, for each check that finds tracks, how many it finds and how much the share would rise if they were fixed; then one tile per check grouped by section. Prep, Organization, Ignored and Quarantine SHALL NOT count against the clean share.

#### Scenario: What holds the score back
- **WHEN** 566 tracks have junk in their names and 300 of them have no other problem
- **THEN** the Overview lists Junk in names with 566 and the gain those 300 tracks would give

## ADDED Requirements

### Requirement: Check pages
A check's page SHALL show its name, one line on what is listed, its fixes as numbered steps in the order to do them, and tabs that only filter the list. A fix that needs answers on the way SHALL open a wizard.

#### Scenario: Filters don't fix
- **WHEN** the DJ clicks the Likely tab on Duplicates
- **THEN** only likely groups are listed and nothing is merged

### Requirement: Duplicate groups
Duplicates SHALL list one row per group with how its copies were matched and how sure the match is (sure, likely, versions). Opening a row SHALL show its copies with the one kept and what differs. Only sure groups SHALL be merged without review. A local file SHALL be kept over a streamed copy.

#### Scenario: Renamed copy
- **WHEN** "Track 03.mp3" has the same audio fingerprint as a tagged FLAC
- **THEN** they form a sure group and the FLAC is kept

### Requirement: Tags health
Collection Health SHALL have a Tags page showing, per tag group and for genres, how many tracks are tagged, the most used tags, tags spelled like another and tags used once, and SHALL merge tags into a chosen one, writing the result to the files.

#### Scenario: Spelling twins
- **WHEN** genres "Deep House", "Deep-House" and "deep house" exist
- **THEN** they are offered as one roll-up keeping the most used spelling

### Requirement: Lists health
Collection Health SHALL have a Lists page for tracks listed twice in a list, empty lists, a folder and a list with the same name, and entries for removed tracks, with the share of tidy lists. Only empty lists that came from an import and were never changed SHALL be deleted without review.

#### Scenario: Imported empty list
- **WHEN** an imported list is empty and was never changed
- **THEN** Tidy removes it; an empty list the DJ made goes to review

### Requirement: Files not in the collection
Collection Health SHALL list audio files in the music and stage folders that aren't in the collection, offering to bring them into the Inbox or move them to quarantine.

#### Scenario: Copied in Finder
- **WHEN** a FLAC was copied into the Inbox folder outside openDJ
- **THEN** it is listed under Not in collection with Bring in as the sure fix
