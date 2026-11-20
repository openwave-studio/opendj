//! The browser's lists panel and side panel: their widths and whether the
//! DJ folded the lists panel, per layout (library.slint, `BrowserPanels`).
//!
//! Kept in settings.json under `browser_panels`, one entry per layout:
//! `{"lists": 270, "side": 340, "folded": false}`.

use std::rc::Rc;

use serde_json::{json, Value};
use slint::{ComponentHandle, Model, ModelRc, VecModel};

use crate::{AppWindow, BrowserPanels};

/// 2-Deck, Player, 4-Deck, Prep and 2-Deck Basic.
const LAYOUTS: usize = 5;
const LISTS: (f32, f32, f32) = (270.0, 200.0, 360.0);
const SIDE: (f32, f32, f32) = (340.0, 280.0, 480.0);

/// The saved value for `layout`, kept within its range, or the default.
fn width(saved: &Value, layout: usize, key: &str, (default, min, max): (f32, f32, f32)) -> f32 {
    saved
        .get(layout)
        .and_then(|e| e[key].as_f64())
        .map_or(default, |w| (w as f32).clamp(min, max))
}

fn folded(saved: &Value, layout: usize) -> bool {
    saved
        .get(layout)
        .and_then(|e| e["folded"].as_bool())
        .unwrap_or(false)
}

/// The settings entry for the widths and folds showing.
fn entry(lists: &[f32], side: &[f32], folded: &[bool]) -> Value {
    Value::Array(
        (0..LAYOUTS)
            .map(|i| json!({ "lists": lists[i].round(), "side": side[i].round(), "folded": folded[i] }))
            .collect(),
    )
}

pub fn install(app: &AppWindow) {
    let saved = crate::settings::load()["browser_panels"].clone();
    let ui = app.global::<BrowserPanels>();
    let lists: Vec<f32> = (0..LAYOUTS)
        .map(|i| width(&saved, i, "lists", LISTS))
        .collect();
    let side: Vec<f32> = (0..LAYOUTS)
        .map(|i| width(&saved, i, "side", SIDE))
        .collect();
    let fold: Vec<bool> = (0..LAYOUTS).map(|i| folded(&saved, i)).collect();
    ui.set_lists_widths(ModelRc::from(Rc::new(VecModel::from(lists))));
    ui.set_side_widths(ModelRc::from(Rc::new(VecModel::from(side))));
    ui.set_lists_folded(ModelRc::from(Rc::new(VecModel::from(fold))));
    let app_weak = app.as_weak();
    ui.on_changed(move || {
        let Some(app) = app_weak.upgrade() else {
            return;
        };
        let ui = app.global::<BrowserPanels>();
        let lists: Vec<f32> = ui.get_lists_widths().iter().collect();
        let side: Vec<f32> = ui.get_side_widths().iter().collect();
        let fold: Vec<bool> = ui.get_lists_folded().iter().collect();
        if lists.len() == LAYOUTS && side.len() == LAYOUTS && fold.len() == LAYOUTS {
            crate::settings::set("browser_panels", entry(&lists, &side, &fold));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn saved_widths_stay_in_range() {
        let saved = json!([{ "lists": 900, "side": 100, "folded": true }]);
        assert_eq!(width(&saved, 0, "lists", LISTS), 360.0);
        assert_eq!(width(&saved, 0, "side", SIDE), 280.0);
        assert!(folded(&saved, 0));
        // A layout with nothing saved gets the defaults.
        assert_eq!(width(&saved, 3, "lists", LISTS), 270.0);
        assert!(!folded(&saved, 3));
    }

    #[test]
    fn entry_keeps_every_layout() {
        let e = entry(
            &[250.0; 5],
            &[300.0; 5],
            &[false, true, false, false, false],
        );
        assert_eq!(e.as_array().map(Vec::len), Some(LAYOUTS));
        assert_eq!(e[1]["folded"], json!(true));
        assert_eq!(e[0]["lists"], json!(250.0));
    }
}
