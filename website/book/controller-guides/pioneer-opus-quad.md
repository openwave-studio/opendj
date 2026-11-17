## Getting started

The OPUS-QUAD is an all-in-one system, so openDJ drives it in its **PC
control** (MIDI) mode:

1. Connect the OPUS-QUAD's rear USB-C port to the computer. On Windows,
   install AlphaTheta's OPUS-QUAD driver first; macOS needs none.
2. Switch it to PC control: press **SOURCE**, pick **SOFTWARE CONTROL**,
   then press **LOAD 1/3** and **LOAD 2/4**.
3. In **Settings › Audio**, choose the OPUS-QUAD as the sound card.
4. openDJ picks this mapping by itself.

Use the **4-Deck** layout to see all four decks on screen.

This mapping is openDJ's own, written from AlphaTheta's list of the
OPUS-QUAD's MIDI messages and built on the [XDJ-AZ mapping](pioneer-xdj-az.md),
so it works the same way where they share controls.

- **DECK SELECT** picks the deck each side plays; each deck sends on its own
  MIDI channel, so the mixer follows.
- **Pads** play hot cues A to H; SHIFT and a pad clears that hot cue.
- **BEAT LOOP**: turn it to halve or double the loop size (and the loop
  playing), press it for a loop of that size or to leave the loop; SHIFT
  and press goes back into the last loop.
- **LOOP IN** and **LOOP OUT** set a loop; holding **LOOP IN** lets the jog
  move the loop start.
- **BEAT JUMP ◀ ▶** jump by the deck's beat jump size (SHIFT: halve and
  double it). **TRACK SEARCH** jumps 32 beats. **KEY SYNC** matches the key
  of the other deck and **TEMPO RESET** puts the tempo back to 0.
- **CROSSFADER ASSIGN** (on the screen) puts each channel on side A, THRU
  or B.
- **BEAT FX** runs openDJ's Beat FX: **EFFECT SELECT** picks the nearest
  openDJ effect (PITCH has none and keeps the one playing), **BEAT ◀ ▶**
  halve and double its length, **TAP** taps its tempo and **LEVEL/DEPTH**
  sets the amount.
- **SOUND COLOR FX**: SPACE, DUB ECHO, NOISE, CRUSH and FILTER pick what the
  COLOR knobs play; SWEEP plays the filter.
- **MASTER** makes the deck the one the others sync to. **JOG MODE** swaps
  between scratching and bending.
- The jog ring shows the playhead going round and where the cue point sits.
- Touching a deck's waveform on the screen jumps there while the deck is
  paused.
- **SMART ROTARY SELECTOR** moves through the list (SHIFT: zooms the
  waveforms) and tilts to move up, down and between the list and the
  sidebar. **BACK** moves between the list and the sidebar and **TAG
  TRACK** adds the track to Automix.

Not mapped yet: VINYL SPEED ADJUST, BEAT FX TIME, PARAMETER, SMOOTH ECHO,
ZONE LEVEL, INPUT SELECT, SYNC long press, the 2/4 deck waveform switch and
hot cue colours. The touch screen keeps showing the OPUS-QUAD's own PC-mode
screen: it doesn't show openDJ's decks or library.

## When you try it

- The OPUS-QUAD has no MIDI message that hands its mixer to the computer,
  and other apps leave the mixing and the Beat FX to its own mixer. Check
  whether its mixer still mixes what openDJ sends. If it does, the EQs and
  faders act twice: tell us, then try **Settings › Audio › Mixing ›
  External mixer** with each deck sent to its own OPUS-QUAD channel, and
  tell us how many outputs the OPUS-QUAD offers.
- Load a track on each deck with **LOAD**, switch each side between its two
  decks, and check PLAY, CUE, the hot cues and the jog wheel follow.
- Move every mixer knob and fader on all four channels, and the crossfader
  assign on the screen.
- Turn on the Beat FX and turn **EFFECT SELECT** through every position.
- Watch the jog ring and the LOAD lights while a track plays.
