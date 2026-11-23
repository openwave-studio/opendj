//! Builds the files listed in generated/sources.txt, each library with its
//! name-prefixing header, plus glue.c (see scripts/pd_libs.py).

use std::path::Path;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    println!("cargo:rerun-if-changed=generated");
    println!("cargo:rerun-if-changed=glue.c");
    println!("cargo:rerun-if-changed=gui_compat.h");
    println!("cargo:rerun-if-changed=pdlua_glue.c");
    for dir in ["pdlua", "lua", "cyclone", "else", "pd-src"] {
        println!("cargo:rerun-if-changed={dir}");
    }
    let list = std::fs::read_to_string(root.join("generated/sources.txt")).unwrap();
    let msvc = std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc");
    let force = |b: &mut cc::Build, header: &Path| {
        if msvc {
            b.flag(format!("/FI{}", header.display()));
        } else {
            b.flag("-include").flag(header.to_str().unwrap());
        }
    };
    let base = || {
        let mut b = cc::Build::new();
        b.define("PD_FLOATSIZE", "64")
            .define("PDINSTANCE", "1")
            .define("PDTHREADS", "1")
            .include(root.join("pd-src"))
            .include(root)
            .warnings(false)
            .flag_if_supported("-w")
            .flag_if_supported("-Wno-everything");
        if msvc {
            b.define("PD_LONGINTTYPE", "long long")
                .define("HAVE_STRUCT_TIMESPEC", None)
                .define("_CRT_SECURE_NO_WARNINGS", None);
        } else {
            b.define("HAVE_UNISTD_H", "1");
        }
        b
    };
    // The glue first: it calls the libraries' setups, and a static library
    // only resolves names for the ones linked after it.
    base().file(root.join("glue.c")).compile("opendj_pd_glue");
    // pd-lua, as plugdata builds it (drawing goes to a callback, not to
    // Pd's Tk editor), then Lua itself.
    let target_os = std::env::var("CARGO_CFG_TARGET_OS").unwrap_or_default();
    let lua_os = match target_os.as_str() {
        "macos" => "LUA_USE_MACOSX",
        "windows" => "LUA_USE_WINDOWS",
        _ => "LUA_USE_LINUX",
    };
    let pdlua = || {
        let mut b = base();
        b.define("PLUGDATA", "1")
            .define("PDLUA_VERSION", "0.12.23")
            .define(lua_os, None)
            .include(root.join("lua"));
        b
    };
    pdlua()
        .file(root.join("pdlua_glue.c"))
        .compile("opendj_pdlua_glue");
    pdlua()
        .file(root.join("pdlua/pdlua.c"))
        .compile("opendj_pdlua");
    pdlua()
        .define("MAKE_LIB", None)
        .file(root.join("lua/onelua.c"))
        .compile("opendj_lua");
    if target_os == "linux" {
        println!("cargo:rustc-link-lib=dl");
    }
    for (lib, includes) in [
        (
            "cyclone",
            &["cyclone/shared", "cyclone/cyclone_objects/binaries"][..],
        ),
        ("else", &["else/Source/Shared", "else/Source/Extra"][..]),
    ] {
        let config = || {
            let mut b = base();
            for inc in includes {
                b.include(root.join(inc));
            }
            force(&mut b, &root.join(format!("generated/names_{lib}.h")));
            force(&mut b, &root.join("gui_compat.h"));
            b
        };
        let mut all = config();
        for f in list
            .lines()
            .filter(|l| !l.starts_with('#'))
            .filter_map(|l| l.split_once('\t'))
            .filter(|(l, _)| *l == lib)
            .map(|(_, f)| f)
        {
            // Files that each define their own `def_font`: one build each,
            // so the name can differ per file.
            let text = std::fs::read_to_string(root.join(f)).unwrap_or_default();
            if text.contains("def_font") {
                let id: String = Path::new(f)
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .chars()
                    .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
                    .collect();
                config()
                    .define("def_font", Some(format!("{lib}_def_font_{id}").as_str()))
                    .file(root.join(f))
                    .compile(&format!("opendj_pd_{lib}_{id}"));
            } else {
                all.file(root.join(f));
            }
        }
        all.compile(&format!("opendj_pd_{lib}"));
    }
}
