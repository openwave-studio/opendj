//! Settings › Appearance › List text: how big the text is in the track
//! list, the lists panel and the side panel's lists, for DJs who find the
//! default hard to read. Rows grow with it (`ListText` in library.slint).

use serde_json::json;
use slint::ComponentHandle;

use crate::{settings, AppWindow, ListText};

pub const KEY: &str = "list_text";
/// In Settings order; the index is what's saved.
pub const LABELS: [&str; 4] = ["Small", "Normal", "Large", "Extra large"];
const DEFAULT: usize = 1;

/// The saved choice, as an index into [`LABELS`].
pub fn size() -> usize {
    (settings::number(KEY, DEFAULT as f64).max(0.0) as usize).min(LABELS.len() - 1)
}

/// How much larger than Normal the lists' text is (as `ListText.scale`).
pub fn scale(size: usize) -> f32 {
    match size {
        0 => 0.87,
        2 => 1.14,
        3 => 1.33,
        _ => 1.0,
    }
}

pub fn install(app: &AppWindow) {
    app.global::<ListText>().set_size(size() as i32);
}

/// Picked in Settings.
pub fn choose(app: &AppWindow, index: usize) {
    let size = index.min(LABELS.len() - 1);
    settings::set(KEY, json!(size));
    app.global::<ListText>().set_size(size as i32);
}
