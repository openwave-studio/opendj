## Getting started

1. Connect the DDJ-FLX10 to the computer with a USB cable.
2. In **Settings › Audio**, choose the DDJ-FLX10 as the sound card, with the
   main output on channels 1-2 and the headphones on 3-4.
3. openDJ picks this mapping by itself.

Use the **4-Deck** layout to see all four decks on screen.

This mapping is openDJ's own, written from AlphaTheta's list of the
DDJ-FLX10's MIDI messages and built on the [DDJ-FLX6 mapping](pioneer-ddj-flx6.md),
so it works the same way where they share controls.

- **DECK 1/3** and **DECK 2/4** pick the deck each side plays; each deck
  sends on its own MIDI channel, and the mixer always has all four channels.
- **Pads**: the pad mode buttons open the deck's pad pages (HOT CUE, PAD FX,
  BEAT JUMP, SAMPLER, and with SHIFT KEYBOARD, PAD FX 2, BEAT LOOP and KEY
  SHIFT). The pads play whatever page the deck shows on screen.
- **BEAT FX** runs openDJ's Beat FX: **BEAT FX SELECT** picks the nearest
  openDJ effect, **CH SELECT** sends it to one channel or, on MASTER, all
  four, **LEVEL/DEPTH** sets the amount, **ON/OFF** turns it on (SHIFT:
  lets the tail ring out) and **BEAT ◀ ▶** halve and double its length
  (SHIFT: AUTO and TAP tempo).
- **SOUND COLOR FX**: SPACE, D.ECHO, CRUSH, NOISE and FILTER pick what the
  COLOR knobs play.
- **ACTIVE PART** DRUMS, VOCAL and INST mute and unmute those stems of the
  deck: lit while they play, dimmed when muted.
- The **pads** light in colour: hot cues in the colours they have on
  screen, loops orange, loop rolls purple, stems pads in the stems'
  colours.
- The **jogs** are scaled for the FLX10's big platters, so a turn scratches,
  bends and searches as far as on the smaller FLX controllers. Pitch bend
  and Scratch in the Controller Editor's Options fine-tune them.
- **SHIFT + BEAT SYNC** makes the deck the one the others sync to.
  **KEY SYNC** matches the key of the other deck (SHIFT: back to the
  track's key), **TEMPO RESET** puts the tempo back to 0 (SHIFT: next
  tempo range).
- **4 BEAT JUMP ◀ ▶** jump 4 beats; **4 BEAT/EXIT** sets a 4-beat loop or
  ends one; **CUE/LOOP CALL** halves and doubles a loop; SHIFT + LOOP IN or
  LOOP OUT lets the jog move that end of the loop.
- **SLIP**, **SLIP REVERSE** and **QUANTIZE** do what they say.
- **CROSSFADER ASSIGN** puts each channel on side A, THRU or B.
- The jog displays show the playhead going round, the cue point, the BPM,
  the time played, the tempo and the key with its shift, and light MASTER, SYNC and
  MASTER TEMPO.
- **Waveforms on the jog screens** (new, not yet tried on the hardware):
  the FLX10 takes them over USB, not MIDI, so openDJ runs a second mapping
  for that, "Pioneer DDJ-FLX10 jog screens". It shows in Settings ›
  Controllers beside the FLX10 and starts on its own. While it runs, the
  screens show each deck's waveform, scrolling as it plays, with the time,
  BPM, tempo and key. Turn it off there to go back to the plain displays.
  On Linux, install openDJ's udev rule first (the .deb and .rpm do it).

Not mapped yet: MIX POINT, CUE/LOOP MEMORY, MASTER CUE, INPUT SELECT, the
mic, SAMPLER CUE and VOL, FX PART SELECT, the PITCH colour FX, the pad PAGE
buttons and the fader start messages.

## When you try it

- Load a track on each deck, switch each side between its two decks, and
  check PLAY, CUE, the pads and the jog wheel follow.
- Move every mixer knob and fader on all four channels, and the crossfader
  assign switches.
- Try every pad mode, with and without SHIFT, and check the pad colours.
- Turn on the Beat FX, turn **BEAT FX SELECT** and **CH SELECT** through
  every position, and press each SOUND COLOR FX button.
- Watch the jog displays while a track plays: the playhead, cue point, BPM,
  time, tempo and key.
- The waveforms: check "Pioneer DDJ-FLX10 jog screens" is on in Settings ›
  Controllers, load a track on each deck, and tell us whether each jog
  screen shows the waveform, whether it scrolls in time with the track
  (also after a seek, a scratch and a tempo change), whether the time, BPM
  and key are right, and whether it stays up for a whole track. If it
  doesn't show, say which system you're on and what Settings › Controllers
  says about the jog screens. Your sound and controls should keep working
  through it all.
- Scratch and nudge with the platter and the rim: a full turn of the
  platter should move the track about 1.8 seconds, as a record would. If
  it's still far off, tell us how far one turn moves it.
- With a track that has stems, press each ACTIVE PART button.
