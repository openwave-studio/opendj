# Tasks

## 1. Groundwork

- [ ] 1.1 Token table in Rust with every `Theme` colour's Dark and Light values; `Theme` colours become `in` properties set from it; Appearance applies Dark or Light through the table; screenshots show Dark and Light unchanged
- [ ] 1.2 Remaining colour literals in `.slint` files onto tokens (deck, library, mixed set, preferences, main and the rest), apart from the ones kept on purpose
- [ ] 1.3 Style values for the shared controls in `Theme` (radius, power ring width, arc width, pill fill), used by the kit in `pills.slint`

## 2. Color schemes

- [ ] 2.1 Scheme files: `Color Schemes/*.toml` in the data folder, `base = "dark"` or `"light"` and `[colors]` / `[style]` overrides by token name, unknown names reported; read at start and when the window regains focus
- [ ] 2.2 Settings › Layouts › Appearance: Color scheme choice with the schemes found, Make a copy (writes every token, shows the file), saved and restored
- [ ] 2.3 Contrast check: text tokens under 4.5:1 on their background named in the Color scheme row
- [ ] 2.4 User guide: Color schemes in website/book/src/guide/layouts.md, with a sample scheme

## Workflow follow-up

- Archive the change once its last PR is merged, so the main specs take its requirements.
