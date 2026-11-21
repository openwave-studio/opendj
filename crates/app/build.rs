fn main() {
    // The default skin is compiled in; user skins will load at runtime through slint-interpreter.
    slint_build::compile_with_config(
        "ui/main.slint",
        slint_build::CompilerConfiguration::new().with_style("fluent-dark".into()),
    )
    .expect("compile default skin");

    // Windows gives a program's main thread 1 MB of stack; macOS and Linux
    // give it 8 MB. Starting the audio engine alone takes more than 1 MB
    // there (AudioEngine::start and Engine::new build the engine on the
    // stack before moving it to the audio thread), so openDJ crashed with a
    // stack overflow before its window opened. Reserve 8 MB on Windows too.
    // scripts/bundle-windows.ps1 checks the built exe keeps it.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let flag = match std::env::var("CARGO_CFG_TARGET_ENV").as_deref() {
            Ok("msvc") => "/STACK:8388608",
            _ => "-Wl,--stack,8388608",
        };
        println!("cargo:rustc-link-arg-bin=opendj={flag}");
    }

    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon("../../res/windows/opendj.ico")
            .set("ProductName", "openDJ")
            .set("FileDescription", "openDJ");
        res.compile().expect("embed the Windows icon");
    }
}
