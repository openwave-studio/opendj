# cyclone, ELSE and pd-lua for openDJ's Pure Data support

The two Pure Data object libraries plugdata ships with, built into openDJ's
plugin helper when it has the `pd` feature (OpenSpec change `add-pd-rack`):

- **cyclone 0.9.3** (`cyclone/`, git tag `cyclone_0.9.3`, commit 5102d2b),
  Max/MSP-style objects. BSD-3-Clause, see `cyclone/LICENSE.txt`. 0.9.3 is
  the last release that runs on Pd 0.55, which libpd-sys 0.3.4 bundles.
  Its abstractions are left out.
- **ELSE 1.0-rc14** (`else/`, git tag `v.1.0-rc14`, commit bd3fbb5), about
  320 objects. Mostly WTFPL; some objects are BSD, MIT or GPL (for example
  `giga.rev~` GPL-2.0-or-later, `conv~` GPL-3.0-or-later): see
  `else/LICENSE/` and each file's header. Left out: `else` (the library
  loader), `beat~` (aubio), `sfinfo`, `sfload`, `play.file~` (ffmpeg),
  `pdlink`, `pdlink~` (networking, Ableton Link), `openfile`, `del.out~`
  (Pd 0.56 API), the objects in sub-folders (`circuit~`, `plaits~`,
  `sfont~`, `sfz~`), `lua` and the abstractions.
- **pd-lua 0.12.23** (`pdlua/`, git tag `0.12.23`): `pdlua.c`, `pdlua.h`,
  `pdlua_gfx.h`, `pd.lua` and `pdx.lua` (from `pdlua/tutorial/examples`).
  GPL-2.0-or-later, see `pdlua/COPYING`. Built with `PLUGDATA` defined, as
  plugdata builds it, so a Lua object's drawing goes to a callback instead
  of Pd's Tk editor.
- **Lua 5.4.7** (`lua/`, from github.com/lua/lua tag `v5.4.7`, without
  `lua.c`, `ltests.*` and the test suite), built as one file through
  `onelua.c`. MIT, see the end of `lua/lua.h`.
- **Pd 0.55.2 headers** (`pd-src/`, BSD, from libpd-sys 0.3.4) to build
  against.
- **pthreads4w headers** (`pthreads-win/`, Apache-2.0, from libpd-sys 0.3.4's
  Windows build of pthreads-win32 v3): Windows only, for the objects that
  use threads, against the library libpd-sys links.

One vendored line is changed: ELSE's `Source/Shared/blep.h` did
`#define t_float float`, which breaks the `bl.*~` oscillators in a
double-precision Pd like this one (they read and wrote the wrong width and
gave silence). The same file also takes `I`, `creal` and `cexp` from the
compiler on Windows, where cyclone and ELSE build with clang-cl (MSVC's C
has no complex numbers or variable-length arrays). What openDJ adds:

- `gui_compat.h`: stands in for the Pd 0.56 theme colours a few GUI objects
  (`knob`, `button`, `note`, `pad`, ELSE; cyclone's `file`) read when they
  draw in Pd's editor, which libpd never does.
- `generated/` (from `scripts/pd_libs.py`): the files built, a header per
  library that prefixes every global name with `cyclone_` or `else_`
  (cyclone, ELSE and libpd share names such as `mifiread_*`, `u8_*`,
  `pink_tilde_setup`), and each library's object setups.
- `glue.c`: calls the setups, adds `cyclone/<name>` and `else/<name>` for
  every object (plain names go to ELSE where both libraries have one), and
  answers `[declare -lib cyclone]` / `[declare -lib else]`.

- `pdlua_glue.c`: starts pd-lua, finds a patch's Lua objects that draw,
  and passes their drawing calls (`lua_fill_rect`, `lua_draw_text` ...) to
  the helper, which draws them itself (`crates/plugin-host/src/pd_display.rs`);
  mouse events go back through pd-lua's own handlers. It also lists the
  text of a canvas's objects (`opendj_pd_object_texts`), so the helper can
  save the arguments Lua objects set with `self:set_args` in a slot's sound.

To update a library: replace its folder, run `python3 scripts/pd_libs.py`,
then `cargo test -p opendj-plugin-host --features pd`.
