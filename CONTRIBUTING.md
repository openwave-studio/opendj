# Contributing to openDJ

Thanks for helping. A few rules keep the issue tracker and the releases in step.

## Building while you work

`cargo run --release` builds what we ship, but every change recompiles the whole app with full optimisation, which takes minutes. A plain `cargo run` compiles quickly but runs too slowly to judge audio or the UI. For day-to-day work use the `fast` profile:

```sh
cargo run --profile fast --bin opendj
```

The first build takes about as long as a release build. After that, dependencies stay cached at full speed and openDJ's own crates rebuild incrementally, so a change to one file is ready in a fraction of the time and still plays smoothly. Builds land in `target/fast/`. Check performance-sensitive changes (audio, waveforms) with `--release` before you open the PR.

## Pull requests

- **Link the issue.** Every pull request body names the issue it works on:
  - `Fixes #N` when the PR finishes the issue. GitHub closes the issue when the PR merges.
  - `Part of #N` when it is one step of a bigger issue. Leave a short comment on the issue when the last step lands, saying what shipped and in which release, and close it.
  - If there is no issue yet, open one first (or say in the PR why it doesn't need one, for example a typo fix or a release-notes PR).
- **Set a milestone** on every PR and issue (v0.4, v0.5, …), following `openspec/roadmap.md`.
- **Technical details only.** Issues, PR bodies, commit messages and comments describe the change, not who asked for it. Don't name people or write "X asked for this"; say "a user request" if the origin matters.
- Keep a PR to one change. Pull requests are squash-merged.
- **Enable auto-merge** (squash) when you open a PR that is ready, so it merges as soon as its required checks pass; leave it off only for a PR that waits on a decision or a test by hand. Keep an open PR current by merging `main` into it when `main` moves, and avoid stacking PRs on each other unless one really needs the other. (CI also runs on `merge_group`, ready for a merge queue if the repository moves to an organization, where GitHub offers one.)
- **A PR built on another PR** (only when it really needs it): once the first one is squash-merged, its commits are no longer in `main`, so merging `main` into the second branch conflicts on every line the first one touched. Replay only your own commits instead: `git rebase --onto origin/main <last commit of the first PR's branch> && git push --force-with-lease`.
- **Lists everyone appends to** (`openspec/roadmap.md`, `supported-controls.txt`) merge with git's union driver (see `.gitattributes`), so a local `git merge origin/main` keeps both sides. Settings code goes in its section's file under `crates/app/src/prefs/`.
- Before pushing, run `cargo fmt --check`, `cargo clippy --workspace --all-targets` and the tests for the crates you changed.
- If the change alters what a DJ sees, update the matching page in `website/docs/src/content/docs/guide`. If it adds controls, regenerate `website/docs/supported-controls.txt`.
- Docs pages are Markdown with a `title:` front matter in place of a `#` heading. Link to other pages by file (`[Looping](looping.md#loop-rolls)`); the build turns that into the page's address and fails on a broken link. A new page also goes in the sidebar list in `website/docs/astro.config.mjs`.

## UI

- **Use the shared controls.** Power buttons, ‹ › stepper pills, value pills with titled lists, arc knobs and section titles live in `crates/app/ui/pills.slint` and `controls.slint`. Use them rather than drawing a new one in a screen's file; if a screen needs a variant, add it to the shared component.
- **Colours and control styles come from `Theme`.** Add no hex colour literals outside `crates/app/ui/theme.slint`; when you touch a screen, move the literals in the part you change onto tokens. This keeps Light mode and color schemes (`openspec/changes/add-color-schemes`) reaching every screen.

## Issues

- Bugs and feature ideas: use the issue templates.
- Tester calls (for example a controller we don't own) stay open until someone has tried the hardware.
