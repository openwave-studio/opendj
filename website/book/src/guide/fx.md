# Effects

openDJ has four kinds of effects:

- **Deck effects**: three on each deck, in the FX bar.
- **Beat FX**: one effect for the main mix or a deck, under the mixer.
- **Color FX**: one on each channel's filter knob.
- **Pad FX**: played from the pads while you hold them.

All of them follow the tempo of the track, so echoes and gates stay on the
beat. Gates, flangers, phasers, the chorus and rolls also lock to the track's
beat grid: a gate opens on the beat, and a roll starts on it.

## The FX bar

The **FX** button in the top bar, beside **WAVEFORMS**, shows or hides the
FX bar. It runs the width of the window, under the decks and above the
browser: each deck's three effects across that deck's width, and the Beat
FX in the middle, under the mixer.

Each effect has:

- **Its name.** Click it for the list of effects, or step through them with
  the arrows either side. The arrows, and a controller's FX SELECT, go
  through the effects in the list's order, then your plugins, and wrap
  round at the end.
- **ON** to switch it on and off.
- **Its length in beats**, with ◀ and ▶ to halve and double it, up to 64
  beats. Some effects have their own range: flanger and phaser 1/2 to 64
  beats, gate 1/16 to 16. Echo and filter echo go up to 4 beats on a deck
  and 16 on the Beat FX; rolls up to a beat on a deck and 16 on the Beat FX.
  Switching to another effect with a
  length keeps it as near as that effect allows. Effects
  that have no length show a slider instead, for their main setting:
  reverb's decay, the filter's resonance, the bitcrusher's bit depth, the
  noise's sweep, the drive's tone, the ladder's resonance, the hall's decay.
- **D/W**, how much of the effect you hear over the dry track.

A deck's effects start as **Reverb**, **Echo** and **Gate**. They play one
after another, so you can stack them.

In 4-Deck each deck's three effects are stacked in three short rows under
that deck.

### The effects

| Effect | What it does |
|---|---|
| Echo | Repeats the sound on the beat, fading out |
| Reverb | Puts the sound in a room or a hall |
| Filter | Low-pass and high-pass, with resonance |
| Flanger | A sweeping jet sound |
| Bitcrusher | Lo-fi: fewer bits and a lower sample rate |
| Phaser | A softer sweep than the flanger |
| Gate | Cuts the sound in and out on the beat |
| Roll | Repeats a short slice, for a stutter |
| Echo Out | Lets the track go, leaving its echo |
| Brake | Slows the track to a stop, like a turntable |
| Noise | A white noise sweep, for build-ups |
| Spiral | An echo that rises in pitch |
| Drive | Warm overdrive, from a little grit to full crunch |
| Chorus | Thickens the sound with slowly drifting copies, wide in stereo |
| Ladder | A resonant analogue-style low-pass that closes as you turn it up |
| Hall | A big, smooth concert hall, with pre-delay |
| Rev Roll | Repeats a short slice backwards |
| Filter Echo | An echo whose repeats get thinner (or darker) |

Switching an effect off fades it out quickly. On a DDJ-FLX4,
**SHIFT** + **BEAT FX ON/OFF** releases the Beat FX instead: it stops
taking in the track, and its echo or reverb rings out for a few beats.

## Beat FX

The **BEAT FX** cell under the mixer is one effect on its own. Pick it by
name, choose where it plays with **1**, **2**, **1&2** or **MST** (deck 1,
deck 2, both, or the main mix), then use **ON**, its beats and **D/W** as on
a deck effect. In 4-Deck the row reads **1 · 2 · 3 · 4 · MST**.

A Pioneer controller's BEAT FX section plays it: DDJ-FLX4, DDJ-FLX6,
DDJ-FLX10, DDJ-400, DDJ-800, XDJ-RX3 and XDJ-AZ. CH SELECT picks the deck,
and on **MASTER** (or **MST**) the main mix.

When an effect changes from anywhere but its own list (a controller's FX
SELECT, the arrows, OSC), its name glows for a moment. On the Beat FX a
strip also shows the effect before and after it in the list for about two
seconds, so you can see where you are without looking at the list.

Its **LEVEL/DEPTH** works as on a Pioneer mixer: echoes are fully wet at
the centre and turning further adds feedback; flanger, phaser and chorus
are dry only fully left. Deck effects keep a plain dry/wet.

### Pioneer DDJ-400

The controller’s **BEAT FX** section controls the Beat FX under the mixer:

- **FX SELECT** steps through the effects; hold **SHIFT** to step backwards.
- **LEVEL/DEPTH** adjusts the effect’s depth. As with other controller
  knobs, move through the on-screen value to pick it up.
- **BEAT ◀ / ▶** halve or double the effect’s beat length, within its range.
- **CH SELECT** routes it to deck **1**, deck **2** or **MASTER** (the main
  mix, **MST** on screen).
- **ON/OFF** toggles the effect; **SHIFT + ON/OFF** releases its tail.
- **SHIFT + BEAT ◀** returns to automatic tempo; **SHIFT + BEAT ▶** taps
  the effect tempo.

## Color FX

Each channel's filter knob plays a **Color FX**. The small **▾** beside the
knob picks which. Each deck keeps its own. The knob does nothing in the
middle and more the further you turn it either way.

- **Filter**: a low-pass to the left, a high-pass to the right.
- **Space** and **Dub Echo**: the same filter, gentler, sent into a reverb
  or an echo on the beat. The tail rings on for a few seconds after the knob
  is back in the middle.
- **Noise**: adds filtered white noise.
- **Crush**: lowers the bit depth and sample rate.

On a DDJ-FLX4, **SHIFT** + **SMART CFX** steps through the Color FX.

## Pad FX

The **PAD FX 1** and **PAD FX 2** pad pages play an effect while you hold
the pad, and stop it when you let go: quick throws without touching the FX
bar. See [Pads](pads.md) for how to open them.

| PAD FX 1 | PAD FX 2 |
|---|---|
| Echo | Roll |
| Reverb | Gate |
| Low pass | Echo Out |
| High pass | Brake |
| Flanger | Noise |
| Phaser | Spiral |
| Crush | Gate Fast |
| Echo Max | Reverb Max |

## When you load a track

Loading a track switches its deck's effects off, so the last track's echo
doesn't carry over. Their beats and dry/wet stay as you set them. Turn this
off in **Settings › Decks** under **Auto reset on load** (see
[Decks and mixer](decks.md#auto-reset-on-load)).

## Plugins

openDJ runs CLAP and VST3 plugins, and Audio Units on a Mac. Nothing is
loaded until you ask:

1. Open **Settings › Plugins**, check the folders on the **Folders** tab
   and press **Scan**. Each file is opened once in a separate helper, a few
   at a time, so a broken plugin only fails its own row.
2. On the **Effects** tab, tick the effects you want. Plugins are grouped by
   maker: tick a maker to take all of theirs, or open it with the arrow.
   The **CLAP**, **VST3** and **AU** chips narrow the list to those formats
   (none lit shows all), and the search box finds a plugin or a maker.
   Only ticked effects show in the effect lists,
   under a line after the built-in effects, in every slot: decks, Beat FX,
   Mic and Aux. A plugin's first four settings follow the slot's
   parameter knobs.

### The plugin's own window

When a slot plays a plugin, an **EDIT** button shows beside its **ON**
(in the FX bar, the Beat FX and the Mic FX). Press it to open the plugin's
own window, with its presets and every setting; press it again, or close
the window, to put it away. The window floats above openDJ and is titled
after the plugin and where it plays, for example "Pro-Q 3 · Deck 1 · FX 1".
Closing it leaves the plugin playing.

When an FX unit plays on more than one deck, each deck has its own copy of
the plugin and **EDIT** opens the first one's window. Changes made in the
window last while the plugin stays loaded; openDJ doesn't keep them when it
restarts the plugin or quits yet. On Linux the windows need X11 (XWayland
on a Wayland desktop).

Each plugin runs in its own helper process. If it crashes, openDJ restarts
it once with the slot switched off; if it crashes again, or keeps the
audio waiting, it is switched off and a message says so. The music keeps
playing either way.

### Instruments

Tick an instrument on the **Instruments** tab of **Settings › Plugins**,
then click **Jam** in the rail and drag it from the list onto a deck (see
[Jam on a deck](decks.md#jam-on-a-deck)). It plays through that deck's
channel, with its EQ, filter, effects and fader.

In 2-Deck the deck has a slim title, like the Mixer and Groove Box decks:
the lilac **☰** menu, the plugin's name and format (**CLAP**, **VST3** or
**AU**), its preset with **‹** and **›** to step through the instrument's
presets in the order the browser lists them, and a heart. Once you turn a
knob the preset's name gets an asterisk (**Squelch Bass\***) and the heart
empties: click it to save over your preset. For one of the plugin's own
presets, or a sound with no preset yet, the heart asks a name and saves a
copy. **☰** has **Save preset**, **Save preset as…**, **Rename…** and
**Tags…** (what the preset is and how it sounds, for the browser's chips),
**Keys on pads**, **Open plugin window** and **Unload deck**. openDJ keeps
the plugin's state, so the sound comes back next time.

In 2-Deck the deck shows:

- A box with a scope of what the instrument plays, its level, **TRIM** and
  **PAN**. A safety limiter keeps the deck from clipping whatever the trim.
- Eight knobs for the plugin's settings, on a backdrop in the instrument's
  color. When it has more than eight, tabs above them show the next pages,
  named after the plugin's own pages where a CLAP plugin names them.
  Right-click a knob to pick the setting it turns; **The plugin's own
  choice** puts it back.
- **OCT ‹ ›** and the **SCALE**, **CHORD**, **ARP** and **HOLD** pills. Click
  a pill's light to switch it on or off, and its name for its settings.
- A keyboard the width of the deck that lights the notes held in lilac, with
  pitch bend and mod wheel strips on its left.

**☰ › Play with** shows the keyboard (**Keys**) or, in its place, the step
sequencer (**Steps**). **☰ › Color**
picks the instrument's color; each plugin starts with its own.

A 4-Deck strip shows the scope, **OPEN**, the first four knobs and
**TRIM**, with the preset in its header.

Each of the three opens a small editor with the same light at its top:
switched off, notes play as they are.

**Scale** moves each note you play to the nearest note of a scale, so
every key sounds in key with the mix. **Key › Auto** takes the master
deck's key; **Pick** uses the **Root** (C to B) and **Scale** (**Maj**,
**Min**, **Pent** or **mPent**) you choose. The keyboard on the deck marks
the scale's keys and dims the rest.

**Chord** plays a chord built on the scale from each key: **Triad**,
**7th**, or **Power** (root, fifth and octave). For a chord of your own,
press **Learn a chord**, hold its keys and let go; **Mine** then plays it
from any key.

**Arp** plays the notes you hold one at a time, on the mix's beat. Pick its
**Order** (**Up**, **Down**, **Up/Dn**, **Rand** or **Played**), step the
**Rate** with **‹ ›** (1/4 to 1/32, with triplets), and set **Octaves**
(1 to 4) and **Gate** (how long each note sounds, 25% to 100%). **Hold**
keeps it playing after you let go, until you play new keys. It plays the
chords from **Chord** too.

**HOLD** keeps the notes you play sounding after you let go of the keys,
until you play new ones or switch it off. With **ARP** on it is the
arpeggiator's **Hold**.

### Step sequencer

With **☰ › Play with › Steps** the keyboard makes room for a riff of up to
16 steps that plays by itself, on the mix's beat and in key, while your
hands are on the mixer. Its notes go in where the keys' do, so **SCALE**,
**CHORD**, **ARP** and **HOLD** work on them too.

- **▶** starts it on the next bar of the playing deck's beat (or at once
  when no deck plays; the button is outlined while it waits) and **■**
  stops it. **A B C D** are four patterns; a new one starts on the next
  bar.
- **FWD**, **BACK**, **P-PONG** (forward and back) or **RAND** is the order the
  steps play in; the rate (**1/4** to **1/32**, with triplets), **SW**
  (swing, every second step late) and **LEN** (1 to 16 steps) sit under it.
- **PITCH** is the tall lane. Click a step to put a note at that height,
  click its bar to take it off, and drag to move it. Heights are notes of
  the deck's scale, or C minor while **SCALE** is off, from the root in the
  middle up and down an octave.
- The lane under it is picked by the tabs: **CHANCE** (how likely the step
  plays), **VEL** (how hard; the top is the accent, drawn brighter),
  **OCT** (click for +1, −1 or 0 octaves), **GATE** (how long the note
  sounds; full ties it into the next step) and **SLIDE** (the note runs
  into the next one, so a mono synth glides).
- Each lane has **⚄** (fill it at random, in key), **✕** (clear it) and
  **‹ n ›**, its own length. A lane shorter than the pattern repeats sooner
  and drifts against the others; it is dim while it follows the pattern.

**OCT ‹** and **›** move the **KEYBOARD** pads up or down an
octave, three at most each way. The pads play the white keys from middle C.

Any MIDI keyboard without a mapping plays the instrument. The keyboard
button in the deck's header picks which one (**Any keyboard** or one by
name) and which **CHANNEL** (**Any channel** or 1 to 16), and shows which
keyboard played it last. See
[MIDI keyboards](controllers.md#midi-keyboards-and-decks) for which
devices play instruments and which deck each one plays.
