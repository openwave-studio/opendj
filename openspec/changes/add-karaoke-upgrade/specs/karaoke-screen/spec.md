## ADDED Requirements

### Requirement: Highlight color
Settings SHALL offer a Sung words color for the karaoke screen, from Lilac, Ocean, Mint, Amber, Rose and White, Lilac by default. The word fill and count-in dots SHALL use it on the window and the page, and a change SHALL apply while the screen is open.

#### Scenario: Choosing Amber
- **WHEN** the DJ picks Amber while the karaoke screen is open
- **THEN** the sung words fill with amber on the window and the shared page without reopening either

### Requirement: Player karaoke
Player mode SHALL offer one lyrics display: a microphone button on the Player bar SHALL show, in the main window, the album art with title, artist, album and year beside the lyrics drawn as on the karaoke screen (the same column, highlight color and Light up setting), and SHALL hide the side panel while it shows. Lyrics not timed SHALL show as a list. A Karaoke Screen button SHALL open the karaoke window. Other layouts SHALL keep opening the karaoke window from the Lyrics tab without switching layout.

#### Scenario: Singing along in Player
- **WHEN** the listener presses the microphone on the Player bar while a track with timed lyrics plays
- **THEN** the art and track details show on the left, the lyrics glide and fill on the right, and the side panel is hidden until it closes

### Requirement: Lines and light up
Settings SHALL offer Lines (Few, the default four lines around the line to sing, or Many, a column of the whole song that scrolls as it is sung) and Light up (Word by word, the default, or Whole line, which lights the line being sung all at once). Both SHALL apply to the window and the page while the screen is open. Beside the cover the lyrics SHALL always be a column.

#### Scenario: Whole line
- **WHEN** Light up is Whole line and a line starts
- **THEN** the whole line takes the highlight color at once instead of filling word by word

#### Scenario: A long gap between lines
- **WHEN** a line has been sung and the next one starts more than a second and a half later
- **THEN** the screen moves on to count in the next line once and never steps back to the line already sung

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
