## ADDED Requirements

### Requirement: Stem waveform layouts
With the Stems style the waveforms SHALL draw each stem in its colour without stems overlapping, in one of two layouts picked in the Wave panel's Layout row, shown only with Stems: Stacked (the default) and 3 Lanes. Stacked SHALL cut the track's waveform shape into a layer per stem, from the line out: kick (or all drums), bass, instruments, vocals, hi-hats, each as thick as its stem's share of the stems' levels. 3 Lanes SHALL split the large waveforms into three equal lanes, top to bottom drums, music (bass and instruments) and vocals, each the lane's stems stacked; overviews SHALL stay Stacked. A stem's role SHALL come from its name, an unknown name counting as instruments.

#### Scenario: Vocals in their own lane
- **WHEN** the layout is 3 Lanes and a track's vocal stem is silent for the first 16 bars
- **THEN** the bottom lane of its large waveform is empty for those bars while the drums lane shows every kick

#### Scenario: Stacked keeps the track's outline
- **WHEN** the layout is Stacked
- **THEN** the waveform's outer edge is the same as the RGB waveform's and the kick layer sits along the centre line
