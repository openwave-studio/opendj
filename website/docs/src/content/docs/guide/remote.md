---
title: Phone remote
---

Control Automix from your phone while you're away from the laptop: see
what's playing, change what comes next, skip a track or turn the music
down. Or put an iPad beside your controller and play it as an extra deck
(see [Use an iPad as a deck](#use-an-ipad-as-a-deck)). Your phone or tablet
opens a page openDJ serves, so there's no app to install.

## Pair your phone

1. Make sure your phone and the computer are on the same Wi-Fi.
2. Choose **File › Remote…** (the ☰ menu on Windows and Linux) and switch
   the remote on.
3. Scan the QR code with your phone's camera and open the link.

That's it: the phone shows the remote. While the remote is on, a **Remote**
pill shows in the top bar, left of **Mic**; it turns green while a phone is
connected, and a click on it opens the panel again. Next time, just open the same page again (add it to your home
screen to make that one tap); the phone stays paired.

The QR code works once, for two minutes; a fresh one appears each time you
open the panel. If your phone can't scan, open the address shown under the
QR code in its browser and type the **PIN**.

The first time, macOS may ask whether openDJ may accept incoming network
connections. Click **Allow**.

## What the phone can do

The remote is built around Automix: it shows what's playing and lets you
manage what comes next.

### Now playing

The phone opens on the track you hear, with its cover, title, artist,
tempo and key. The progress bar shades where the next mix happens, and the
card under the controls shows the next track and how long until it mixes
in. Tap the card to open the queue.

- **Play / pause**: pause stops the music and Automix; play picks up where
  it left off. When Automix is off, play starts it.
- **Mix now** (⏭) starts the next mix straight away.
- **Order** puts the queue in order (Shuffle, BPM low to high, Harmonic
  path and so on), as in the Automix panel (see [Automix](automix.md)),
  with **Undo** afterwards.
- **Mix settings** (left of play) sets the mix **Length** and **Style**,
  and has **Skip the next track**.
- **Timer** (far right) sets the sleep timer.
- **Volume**, under the controls, sets the master level with one drag.
- The speaker button at the top opens **Levels** (booth level and Fade out); **⋯** has the queue,
  Add tracks, the sleep timer and Stop Automix.

### Queue

Tap the queue button at the bottom right. The track playing is at the top,
then everything up next, each with its cover.

- Drag a track by its handle to move it, or tap it for **Play next**,
  **Move to the end** and **Remove from the queue**.
- You can swap the track waiting on a deck for another until 15 seconds
  before its mix. After that, and during the mix, it shows a lock and
  stays put.
- **Edit** lets you pick several tracks at once, then **Play next**, **To
  end** or **Remove** them all.
- **+** opens search (see below).
- The bar at the bottom has **Mix**, **Order**, **Levels** and **Timer**.

### Add tracks

Tap **+** in the queue to see your whole collection, sorted by title; keep
scrolling and more tracks load. Type to search it, with the same search as
the browser (try `bpm:120-124`, `key:8A` or `crate:Warm-up`). Tap **+** on
a result to add it to the end of the queue, or **Next** to play it next.
Tracks already queued show **Added**. Tap **Done** to go back to the queue;
the search clears itself for next time.

### Levels and the sleep timer

- **Levels**: the master level, and the booth level when a booth output is
  set up. **Fade out** turns the master level down to silence over eight
  seconds; **Bring the sound back** puts it back.
- **Timer**: pick 15, 30, 45, 60 or 90 minutes. When time is up the music
  fades out over 20 seconds and Automix stops. The Timer button shows the
  time left.

Everything the phone does shows in openDJ straight away, and the other way
round.

## Use an iPad as a deck

The **Deck** tab turns a tablet or phone into one of openDJ's decks: for
example deck 3 beside a two-channel controller such as the DDJ-FLX4. It
talks to openDJ directly, so there's nothing to map and no MIDI to set up.

On an iPad, or a phone on its side, a top bar like openDJ's sits along the
top: the page picker on the left (Automix and Deck), the logo in the
middle, and on the Deck page the deck buttons, **LOAD** and the time on the
right. On a phone upright, the pages are tabs along the bottom. A tablet
opens on **Deck**, a phone on **Automix**; the page remembers the one you
used last.

- **1 2 3 4** pick the deck. It starts on deck 3. A green line under a
  number means that deck is playing.
- The header shows the deck as openDJ does: cover, title and artist, bar
  and beat, time left, tempo and key. **‹ ›** beside the key shift it a
  semitone, **MATCH** matches it to the other deck and **RESET** puts it
  back. The key turns green when it mixes with the deck playing.
- **LOAD** lists your collection. Tap a track to load it onto the deck. If
  the deck is playing with its fader up, tap it a second time to load it
  anyway. On an iPad the list shows tempo, key and time beside each title;
  keys that mix with the deck playing are green. The buttons over the list
  narrow it down: **Mixes with deck**, the playing track's **Key** or
  **BPM**, **★★★ and up** and **Not played**.
- The waveforms use the colours picked in openDJ (RGB or 3-Band). The big
  one scrolls with the music, with beats and bars, hot cues and the loop
  shaded; the overview under the title shows the whole track. Its play
  position drifts by 10 to 30 ms over Wi-Fi, fine for buttons and pads;
  there's no scratching from the page.
- The tempo fader is on the left, down for faster as on a player, with
  **KEY** (key lock) and **RESET**. On a phone it's a slider: right is
  faster.
- **SYNC**, **Q** (quantize), **SLIP**, **REV** (held), **NUDGE** (hold ◀
  or ▶ to slow down or speed up a moment), **▶❙❙** and **CUE**.
- **Pads**: **Hot Cue** (tap an empty pad to set one, hold a set one for a
  second to clear it), **Loop** (1/4 to 32 beats) and **Beat Jump** (back on
  the top row, forward on the bottom).
- Under the pads: loop **IN** and **OUT**, the loop size (‹ › halve and
  double, the middle starts a loop), **RELOOP**, and beat jump (the middle
  changes the jump size).
- The channel strip on the right is one side of the 2-Deck mixer: **HI**,
  **MID**, **LOW** and **FILTER** (double tap a knob for the middle),
  **GAIN**, the volume fader, the level meter and 🎧 for headphones. On a
  phone it's a row at the bottom.

## Phones and safety

- The remote is off until you switch it on, and only works on your local
  network.
- Only paired phones can control openDJ. Under **Paired phones** in the
  REMOTE panel you can see which are connected and **Remove** any of them.
- After five wrong codes or PINs in a row, pairing stops for a minute.
- The remote uses plain HTTP on your network. On a public network, anyone
  watching the traffic could see it; use a network you trust (see below).

## If your phone can't connect

Hotel, venue and café Wi-Fi often keeps devices apart, so the phone can't
reach the computer even on the same network. Either:

- share the computer's connection as its own Wi-Fi network (on a Mac:
  **System Settings › General › Sharing › Internet Sharing**) and join it
  with your phone, or
- turn on your phone's hotspot and join the computer to it.

If the computer is on more than one network, the REMOTE panel lists the
other addresses under **Other networks**.

A phone that locks its screen drops the connection; the page reconnects as
soon as you look at it again.
