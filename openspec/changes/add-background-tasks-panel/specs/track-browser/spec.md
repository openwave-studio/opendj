## ADDED Requirements

### Requirement: Stems column
The track list SHALL offer a Stems column showing the state of a track's high-quality stems: Ready, the percent done while separating, Waiting, Paused or Failed; empty when none were asked for.

#### Scenario: Ready to load
- **WHEN** a track's HQ stems finish
- **THEN** its Stems cell shows "Ready"
