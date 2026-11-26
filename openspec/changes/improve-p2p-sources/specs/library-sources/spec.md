## ADDED Requirements

### Requirement: Readiness ranking
A peer-to-peer source SHALL rank results of the same quality by each user's estimated wait before sending, then by their speed. The wait SHALL be none for a user with a free upload slot, else their queue at the speed to expect; the speed to expect SHALL be the one measured on recent downloads from that user when there is one, else the one they advertise. Each recent failed download from a user SHALL add to their wait. Results SHALL show the readiness as a line ("Now · 2.1 MB/s", "~4 min wait").

#### Scenario: A user who advertises more than they send
- **WHEN** two users both have queues of 2 and advertise 10 MB/s, and downloads from one were measured at 100 KB/s in the last hour
- **THEN** the other user's copy ranks first

#### Scenario: A free slot
- **WHEN** a user has a free upload slot
- **THEN** their copy reads "Now" with their speed

### Requirement: Unwanted copies
Karaoke, nightcore, sped-up, slowed, 8D, cover, tribute, lullaby and parody copies SHALL rank after every other result and show in a closed "Other versions" group, unless the search names them. Remixes, edits and extended mixes SHALL stay among the results, after the original.

#### Scenario: Searching a song
- **WHEN** a search for "Bonobo Kerala" finds "Kerala", "Kerala (Remix)" and "Kerala (Karaoke)"
- **THEN** they rank in that order, and the karaoke copy is in the closed Other versions group

#### Scenario: Asking for it
- **WHEN** the search is "Kerala nightcore"
- **THEN** nightcore copies rank as the song itself

### Requirement: Sign-out pause
While the peer-to-peer client is signed out of its network, downloads whose transfers fail SHALL be held as waiting, not failed, and SHALL NOT count against the other user; once signed in again each SHALL be asked for again from the same user. The Downloads list SHALL show a banner while any are held.

#### Scenario: A sign-out mid-download
- **WHEN** slskd loses its Soulseek sign-in while a song is downloading
- **THEN** the song reads "Paused · Soulseek signed out", and when slskd signs in again it is asked for from the same user

### Requirement: Music server similarity
A music-server source SHALL offer "Similar artists' songs" on a song; when the server lists audio similarity among its OpenSubsonic extensions it SHALL also offer "Sounds like this" and "Path to another song", the latter listing the songs the server finds between the two.

#### Scenario: A server without audio similarity
- **WHEN** the server does not list `sonicSimilarity`
- **THEN** only "Similar artists' songs" shows
