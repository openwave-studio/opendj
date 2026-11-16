## ADDED Requirements

### Requirement: Highlight color
Settings SHALL offer a Sung words color for the karaoke screen, from Lilac, Ocean, Mint, Amber, Rose and White, Lilac by default. The word fill and count-in dots SHALL use it on the window and the page, and a change SHALL apply while the screen is open.

#### Scenario: Choosing Amber
- **WHEN** the DJ picks Amber while the karaoke screen is open
- **THEN** the sung words fill with amber on the window and the shared page without reopening either

### Requirement: On-screen buttons
The karaoke window and page SHALL show Cover and Full screen buttons only while the pointer moves over them or after a tap, hiding them after about three seconds without input. The window SHALL toggle them with C and F. The lyrics SHALL always show. The window's choice SHALL be remembered and the page SHALL start from it; a viewer's own picks on the page SHALL change only that page.

#### Scenario: Buttons stay out of the audience's way
- **WHEN** nobody has moved the mouse over the karaoke window for three seconds
- **THEN** no buttons are visible

#### Scenario: Hiding the cover
- **WHEN** Cover is on and the DJ presses C
- **THEN** the cover goes away and the lyrics show on their own, as before this change

### Requirement: Cover layout
With Cover on, the screen SHALL show the cover with title and artist on one side and the lyrics beside it, the line to sing filling with the highlight color and count-in dots before a line that follows a break. The background SHALL be one flat color from the cover, dark enough for white text, or the usual dark background when the track has no cover.

#### Scenario: A track without a cover
- **WHEN** Cover is on and the track playing has no cover
- **THEN** the cover space shows a neutral placeholder on the usual dark background and the lyrics show as normal

#### Scenario: Changing tracks quickly
- **WHEN** the followed track changes before the previous track's cover is ready
- **THEN** the previous cover is never shown for the new track

#### Scenario: A viewer changes the page
- **WHEN** someone watching the karaoke page on a TV turns Cover off there
- **THEN** that page shows the lyrics only, and the karaoke window and other viewers keep the cover
