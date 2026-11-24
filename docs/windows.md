# openDJ on Windows

Windows ships as a preview from openDJ 0.4: 64-bit Windows 10 and 11 on x64
PCs. Windows on ARM runs the x64 build through emulation (fine for testing,
for example in UTM on an Apple Silicon Mac); a native ARM64 build can come
later.

## Downloads

Each release has two Windows files, made by `scripts/bundle-windows.ps1`:

- `openDJ-VERSION-windows-x64-setup.exe`: installs for the current user into
  `%LOCALAPPDATA%\Programs\openDJ`, with a Start menu entry and an
  uninstaller. No administrator needed. openDJ installed this way updates
  itself: it downloads the next setup, checks its SHA-256 against GitHub's,
  and runs it silently when the decks stop.
- `opendj-VERSION-windows-x64.zip`: portable. Unzip anywhere and run
  `openDJ.exe`. It doesn't update itself; the banner links to the release.

The setup is built with Inno Setup 6 from `res/windows/opendj.iss`. Its
`AppId` must never change, or a new setup installs a second openDJ instead
of updating the first.

## Where openDJ keeps things

- Settings, library and logs: `%APPDATA%\openDJ` (logs in `logs\opendj.log`,
  the previous run in `opendj.1.log`, crash reports as `crash-*.txt`).
- Analysis cache and stems: `%LOCALAPPDATA%\openDJ`.

Paste `%APPDATA%\openDJ\logs` into the File Explorer address bar to open the
logs. A crash inside a driver (graphics or audio) writes a `crash-*.txt`
too, naming the DLL it happened in.

## If openDJ doesn't open

openDJ draws with DirectX 12 on Windows. Some graphics drivers (older
Intel drivers, laptops with both Intel and NVIDIA graphics) can still close
openDJ while it sets up the GPU, before the window shows. openDJ notices at
the next start and tries another graphics mode: DirectX 12, then Vulkan,
then OpenGL.
The one that works is kept in `%APPDATA%\openDJ\graphics.txt`, so opening
openDJ two or three times gets past a driver that crashes.

To pick one by hand, set the environment variable `OPENDJ_GRAPHICS` to
`dx12`, `vulkan` or `gl` (in Command Prompt:
`set OPENDJ_GRAPHICS=dx12 && "%LOCALAPPDATA%\Programs\openDJ\openDJ.exe"`).
`OPENDJ_GRAPHICS=auto` goes back to the default and forgets the kept one.
Updating the graphics driver from Intel's or the laptop maker's site
often fixes the crash itself.

## Audio

The Settings > Audio device lists show Windows Audio (WASAPI, shared mode)
devices and, after them, ASIO drivers as `ASIO: <driver name>`. Pick an
ASIO driver for the lowest latency with a DJ controller or audio interface
that ships one; the buffer size is usually set in the driver's own control
panel. ASIO4ALL works for devices without an ASIO driver.

openDJ looks for devices once, at start-up. Listing ASIO drivers loads each
installed one in turn, which takes a few seconds when several are installed
for hardware that isn't plugged in, and can briefly interrupt a Windows
Audio stream on the same interface. So opening Settings > Audio looks again
only for Windows Audio devices; press **Refresh devices** there for an ASIO
device plugged in after openDJ started. While a deck plays, Refresh devices
asks for a second press first.

The build downloads Steinberg's ASIO SDK (GPLv3) through cpal's `asio`
feature, which also needs LLVM (libclang) for bindgen; the GitHub Windows
runners have it. Built copies of openDJ were already GPL-3.0 because of
Slint, so this changes nothing about the licence (see NOTICE).

## Controllers

Windows lets only one app at a time open a MIDI device. If VirtualDJ,
rekordbox or another DJ app is running and holding the controller, openDJ
can't open it; the log says so. Quit the other app and press Refresh in
Settings > Controllers.

## Code signing

openDJ is accepted into SignPath Foundation's free code signing for open
source projects (2026-11-24). The `windows` job in
`.github/workflows/release.yml` signs tagged releases and nightlies once the
`SIGNPATH_API_TOKEN` secret is set; without it, and on pull requests, the
files stay unsigned and CI passes as before. Until the production
certificate is in place, Windows SmartScreen shows "Windows protected your
PC" on first run (More info > Run anyway).

Signing takes two requests per build: first `openDJ.exe` and
`opendj-plugin-host.exe` (artifact configuration `binaries`), so the
portable zip and the setup carry signed copies; then the setup itself
(artifact configuration `setup`). Both configurations are in
`.signpath/artifact-configurations/`.

One-time setup:

1. Create a SignPath account, accept the invitation to the openDJ
   organization, then confirm the CI user's email address (in that order).
2. In SignPath, Trusted Build Systems > Add predefined > GitHub.com.
3. Create a project with the slug `opendj` and link the GitHub.com trusted
   build system to it (project > Trusted build systems > Link).
4. In the project, add two artifact configurations with the slugs
   `binaries` and `setup`, pasting in `binaries.xml` and `setup.xml`.
5. Install the SignPath GitHub App (https://github.com/apps/signpath) on
   the repository.
6. Give the CI user submitter rights on the `test-signing` policy (and
   later `release-signing`) and create an API token for it.
7. In the repository's Settings > Secrets and variables > Actions, add the
   secret `SIGNPATH_API_TOKEN` (the token) and the variable
   `SIGNPATH_ORGANIZATION_ID` (the organization id from SignPath's URLs).
   Optional variables: `SIGNPATH_PROJECT_SLUG` (default `opendj`) and
   `SIGNPATH_SIGNING_POLICY` (default `test-signing`).
8. Run Actions > Release > Run workflow on main to publish a nightly and
   check that the job's last signing step lists both files as signed.

The first builds use SignPath's self-signed test certificate. Once SignPath
has reviewed the setup and imported the production certificate, set
`SIGNPATH_SIGNING_POLICY` to `release-signing`. Release signing may wait for
an approval in SignPath; the job waits up to an hour per request.

The in-app updater requires each new setup to carry a valid signature from
the same publisher as the running copy, but only when the running
`openDJ.exe` has a valid (trusted) signature. Test-signed builds don't, so
they update like unsigned ones, and the first production-signed release
installs over them normally.

`scripts/bundle-windows.ps1` also still runs `SIGN_COMMAND` on `openDJ.exe`
and the setup when that variable is set, for signing with another service
such as Azure Trusted Signing.

## Testing

CI builds and tests every pull request on `windows-latest`, and the
`windows-app` job attaches the setup and zip to the run. To try a build:
open the pull request's Checks tab, pick the CI run, and download
`opendj-windows` from its artifacts.
