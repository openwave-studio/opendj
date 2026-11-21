# Proposal

Planned for: **v1.0 · Ready for any DJ switching over**

## Why

People switching need to find their features: a full manual, a shortcuts page, and "coming from" guides. The guide today is long and has few pictures: 31 pages, about 40,000 words and 7 screenshots, with "Your collection" alone at 8,700 words. A comparison of the djay Pro, rekordbox, VirtualDJ, Serato, Traktor and Mixxx manuals found the clearest ones are short and task-first: djay for page shape, rekordbox for screenshots with numbered callouts.

## What Changes

- A style guide and page template in `website/README.md`. Each page opens with one line on what it's for, then the quickest way in 2–4 numbered steps and what you see when it worked. After that comes one section per task, with task headings, shortcuts inline and a one-line controller note. Pages aim for about 800 words.
- Detail tucked away: settings in a collapsed "More options" block or the Settings reference, every-option tables on reference pages (Effects reference, Pad modes), and layout differences as one line or tabs.
- `guide/music.md` split into about five pages, and every other page brought to the template.
- Marked-up screenshots: cropped panels at 2x with numbered lilac callouts, made by a script on the cloud screenshot setup so they can be regenerated when the UI changes.
- Short silent clips (5–10 s, looping) for gestures a still can't show, at most one per page.
- Each page links its in-app tutorial and, where one exists, its tutorial video.
- A complete keyboard shortcuts page, and guides for DJs coming from rekordbox, Serato, Traktor, VirtualDJ and djay.

## Capabilities

### New Capabilities

None.

### Modified Capabilities

None.

## Out of Scope

- Making the tutorial videos themselves (planned separately); pages only link them.
- Versioned docs per release.

## Impact

`website/docs` only (pages, images, the screenshot script and `website/README.md`).
