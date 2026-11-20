## ADDED Requirements

### Requirement: Color schemes
openDJ SHALL load color schemes from files in its data folder. A scheme SHALL start from Dark or Light and MAY override any Theme colour or control style value by name; values it leaves out SHALL keep the base value and unknown names SHALL be reported without failing. Settings SHALL offer each scheme found next to Dark, Light and Match system, SHALL save the choice, and SHALL offer a copy of the current scheme as a new file with every value in it. A changed scheme file SHALL take effect when the window regains focus, without a restart. Text whose contrast against its background falls under 4.5:1 SHALL be named in Settings. The large waveforms SHALL keep their own colour setting.

#### Scenario: A DJ's scheme
- **WHEN** the DJ saves `Color Schemes/Sunset.toml` with `base = "dark"` and a new accent, then picks Sunset in Settings › Layouts
- **THEN** the accent changes on every pill, power button, knob arc and menu at once, everything else stays Dark, and openDJ opens in Sunset next time

#### Scenario: Editing while open
- **WHEN** the DJ edits the active scheme in a text editor and switches back to openDJ
- **THEN** the new colours show without a restart

#### Scenario: Low contrast
- **WHEN** a scheme sets text and panel colours that read under 4.5:1
- **THEN** the Color scheme row names the tokens that fail, and the scheme still applies
