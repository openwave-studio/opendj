## ADDED Requirements

### Requirement: Fake lossless
A lossless file (FLAC, WAV, AIFF, ALAC) at 44.1 kHz or more whose spectrum is cut off below 19.5 kHz SHALL be flagged as once lossy: checked when its download finishes and when it is analysed, tagged Lossy, marked on the Downloads list, and listed in Health with a way to find another copy.

#### Scenario: An MP3 saved as FLAC
- **WHEN** a downloaded FLAC has nothing above 16 kHz
- **THEN** its Downloads row shows "Not real lossless" and Health lists it

#### Scenario: A real CD rip
- **WHEN** a FLAC's spectrum reaches 20 kHz
- **THEN** it is not flagged
