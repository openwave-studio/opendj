# Mixed Set Editor

The Mixed Set Editor opens a set you recorded as a timeline, so you can
listen back, see how each transition went, check the mix for problems,
make quick fixes and send it on with its tracklist.

To open a set, go to **Mixed Sets** in the sources on the left,
right-click a recording and choose **Open in Mixed Set Editor**. Press
**Esc** or **×** to close it.

## The timeline

The timeline shows the whole set with its waveform and its tracks above
it. Transitions, where two decks are heard at once, are shaded.

- **Click** the waveform to play from there. The play button in the top
  left plays and pauses. Playback goes to your main output.
- **Drag** over the waveform to select a stretch, for **Trim** and
  **Cut**.
- **Scroll**, or use **−** and **+**, to zoom. Drag the box in the small
  overview above the timeline to move around the set.
- **Show decks** puts each deck on its own row, with its tracks, loops
  and scratches. **Show moves** then adds lanes for the faders, EQs,
  filters and effects, as you moved them.

Every recording from openDJ 0.8 on comes with a control log that the
editor reads all this from. An older recording, or one with its log
missing, shows only its waveform and the tracklist from its cue sheet.

## Transitions

The **Transitions** tab lists each transition with when it started, how
long it lasted in bars and the style it was mixed in: **Cut**, **Bass
swap**, **Filter**, **Echo out** or **Smooth**. Click one to hear it from
4 bars before; the card above the list shows both tracks, where each was
in the track, and whether the beats stayed in time. **‹** and **›** step
through them.

### Pair mixes

Tick a transition and press **Save pair mixes** to keep it as a pair mix:
the next time Automix plays those two tracks in a row, it mixes them the
way you did, from the same points and over the same length. Transitions
of 4 bars or more start ticked; cuts start unticked. A transition saved
before shows when; saving again replaces it.

## Check my mix

The **Check my mix** tab listens through the set and lists what's worth
another listen:

- **Clipping**: the mix went over full scale.
- **Silence**: the sound dropped out.
- **Level jump**: one track plays noticeably louder or quieter than the
  ones around it.
- **Beats drifting**: the beats of two decks pulled apart during a
  transition.
- **Effect left on**: an effect stayed on after its track was gone.
- **Jump mid-phrase**: a jump or hot cue landed in the middle of a
  phrase.

Red dots are problems most listeners will hear; amber ones are worth a
listen. Click one to hear it from 2 bars before.

## Fixes

The **Fixes** tab makes a few quick fixes. Your recording is never
changed: the fixes are kept beside it and only made into a new file when
you press **Save edited mix**.

- **Trim start and end**: drag over what to keep and press **Trim to
  selection**. The start fades in over 2 seconds and the end fades out
  over 5.
- **Track level**: click a track on the timeline, set how many dB up or
  down and press **Apply**. The recording is the whole mix, so this moves
  everything heard then.
- **Cut a section**: drag over what to take out and press **Cut
  selection**. The join gets a short crossfade.
- **Loudness for**: brings the whole set to the loudness a place
  expects, with its peaks kept under −1 dBTP: **Streaming and Mixcloud**
  (−14 LUFS), **Apple Music and podcasts** (−16 LUFS) or **Club and
  radio** (−9 LUFS). The tab says how far the mix has to move, and warns
  when getting there takes more than 3 dB of limiting.

Each fix shows under **Your fixes**, with **Undo**. **Save edited mix**
writes `<name> (edited).wav` (or `.flac`, as you record) beside the
recording, with its own cue sheet and tracklist, and adds it to Mixed
Sets.

## Export

**Export ▾** in the top bar gives:

- **Mixcloud tracklist**: a text file with each track's start time.
- **YouTube chapters**: a text file to paste into a video's description.
  YouTube shows chapters only when there are three or more, the first at
  0:00, each at least 10 seconds long; shorter ones are joined to the one
  before.
- **Cue sheet (.cue)**: for players and burning tools that read them.
- **Export for Mixcloud…**: makes the 320 kbps MP3 with the tracklist as
  chapters, as **File › Export › Mixcloud…** does. Once you've saved an
  edited mix, it's made from that.

The text files are written beside the recording and shown in your file
manager. Their times follow your fixes: a trim or cut moves every track
after it earlier.
