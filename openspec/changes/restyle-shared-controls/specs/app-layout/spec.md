## ADDED Requirements

### Requirement: Shared controls
The window SHALL use one set of controls: a round power button only for turning a whole unit on (an FX slot, an instrument deck, Automix); a pill pressed to switch a setting on or off or to step through a few choices; a `‹ value ›` stepper whose value opens a titled list; a value pill with ▾ for long lists; and arc knobs. Menus SHALL carry a title naming what is chosen and show the current choice in lilac. Section titles SHALL use a condensed face. Their colours and sizes SHALL come from `Theme`.

#### Scenario: Switching a setting
- **WHEN** the DJ presses the Slip pill on a deck
- **THEN** it turns lilac and Slip is on; pressing again turns it grey and Slip is off

#### Scenario: Stepping a choice
- **WHEN** the DJ presses › on the pad mode stepper showing Hot Cues
- **THEN** the pads switch to the next page, and pressing the words "Hot Cues" instead opens the titled Pads list

#### Scenario: Effect menu
- **WHEN** the DJ opens an FX slot's effect menu
- **THEN** it is titled with the slot, offers None, and shows the effects in two columns of kinds, each with its kind's icon and the current one in lilac
