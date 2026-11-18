# Design

## A separate small app, not a gated desktop app

The desktop app crate is 55,000 lines of Rust and a large Slint UI made for mouse and keyboard; most of it (library database, imports, streaming, plugin windows, settings) has no place in a very small touch app, and gating each part with `cfg` would spread iOS conditions through the whole crate. The tablet app is its own crate on the shared engine, control and controller crates, so desktop-only parts are simply not linked. What the shared crates can't do on iPadOS is gated there, at the smallest point:

| Part | iPadOS |
|---|---|
| Whisper (whisper.cpp, C++) | engine feature `word-timing`, default on; the tablet turns default features off |
| HID controllers (hidapi) | not a dependency on iOS; `Hid::new` returns an error, as when USB HID is unavailable on a desktop |
| QuickJS bindings | `bindgen` feature on iOS, since rquickjs ships none for it |
| Stems (burn) | plain wgpu (WGSL on Metal); cubecl's MSL backend only builds for macOS |
| Plugin host, add-on programs | not started: the helper isn't beside the app, and the tablet never starts add-ons |
| Ableton Link (C++) | builds with CMake; the app links `libc++` itself, as the crate only does that for macOS |

## Rendering

Slint's iOS support uses the winit backend with Skia. The desktop app draws waveforms with wgpu into textures through Slint's `unstable-wgpu` interop and the femtovg-wgpu renderer, which isn't documented for iOS. The tablet draws its waveforms on the CPU into two images per frame (window width × 104 px each, plus overviews redrawn only when a track's analysis grows). At 2,400 px wide that's about 0.5 Mpx a frame, well within an A12Z's budget.

## Audio

cpal's iOS host plays through RemoteIO and leaves the session category alone. The app sets Playback, asks for 5 ms buffers, activates the session and asks for all of the route's output channels. With four (a controller's own sound card), a headphones route on pair 1 is added to the default master route. The route is fixed at start, so the controller should be plugged in before the app opens.

## Files

The app's Documents folder is shown in the Files app (`UIFileSharingEnabled`, `LSSupportsOpeningDocumentsInPlace`), so music is copied in rather than picked. Folders elsewhere need a document picker and security-scoped bookmarks; that is a later step.

## Checking without a Mac

Linking for iOS needs Xcode, but a cloud container can type-check the Rust for `aarch64-apple-ios` with C and C++ compilation stubbed out. That found the iOS-only build failures above before any Mac build.

## Android later

Nothing in the tablet crate is iOS-specific except `session.rs` and the Xcode project. Android needs an `android_main` entry point and an APK build with the NDK, MIDI through Android's MIDI API (midir has no Android backend), and the Storage Access Framework for folders.
