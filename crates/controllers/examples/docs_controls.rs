//! Writes what each pictured control does, for the docs' controller figures.
//!
//! `cargo run -p opendj-controllers --example docs_controls -- <out dir>`
//! writes `<out dir>/<mapping file>.json` for every mapping with a layout:
//! for each part of the picture, what it does on its own and with SHIFT,
//! as the Controller Editor shows it.

use std::collections::BTreeMap;
use std::path::PathBuf;

use opendj_control::ControlRegistry;
use opendj_controllers::default_mappings_dir;
use opendj_controllers::layout::Layout;
use opendj_controllers::view::build;

fn main() {
    let out = PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: docs_controls <out dir>"),
    );
    std::fs::create_dir_all(&out).unwrap();
    let dir = default_mappings_dir().unwrap();
    for file in Layout::files() {
        let path = dir.join(&file);
        if !path.exists() {
            continue;
        }
        let reg = ControlRegistry::new();
        let _engine = opendj_engine::engine::Engine::new(&reg, 44_100.0);
        let view = match build(&path, reg) {
            Ok(v) => v,
            Err(e) => {
                eprintln!("{file}: {e}");
                continue;
            }
        };
        // part -> layer -> function names, first-seen order, no repeats.
        let mut parts: BTreeMap<String, BTreeMap<String, Vec<String>>> = BTreeMap::new();
        for r in &view.rows {
            let Some(part) = &r.part else { continue };
            let layer = if r.layer.is_empty() {
                "normal"
            } else {
                &r.layer
            };
            // "FX 1 Effect 2 on", "Sampler 3 Play from start": the target
            // says which one; decks, the mixer and the rest go without.
            let t = &r.function.target;
            let name = if ["FX", "Sampler", "Mic", "Aux"]
                .iter()
                .any(|p| t.starts_with(p))
            {
                format!("{t} {}", r.function.name)
            } else {
                r.function.name.clone()
            };
            let names = parts
                .entry(part.clone())
                .or_default()
                .entry(layer.to_string())
                .or_default();
            if !names.contains(&name) {
                names.push(name);
            }
        }
        let json = serde_json::json!({ "file": file, "parts": parts });
        let name = out.join(format!("{file}.json"));
        std::fs::write(&name, serde_json::to_string_pretty(&json).unwrap() + "\n").unwrap();
    }
}
