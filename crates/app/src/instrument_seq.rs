//! The instrument's step sequencer's lane tools (`SeqPanel` in
//! `ui/deck.slint`): RANDOM fills a lane of the pattern being edited, RESET
//! clears it. The steps themselves are `[Instrument1],seqS_LANE_N` controls
//! the panel sets directly (see `opendj_engine::seq`).

use std::sync::Arc;

use opendj_control::ControlRegistry;
use opendj_engine::seq::{lane_key, random_lane, reset_value, LANES, ON, PITCH, SLOTS, STEPS};
use slint::ComponentHandle;

use crate::{AppWindow, InstrumentsUi};

const GROUP: &str = "[Instrument1]";

pub fn install(app: &AppWindow, registry: Arc<ControlRegistry>) {
    let mut next = crate::keep_going::random();
    app.global::<InstrumentsUi>()
        .on_seq_lane(move |slot, lane, random| {
            let (Ok(slot), Ok(lane)) = (usize::try_from(slot), usize::try_from(lane)) else {
                return;
            };
            if slot >= SLOTS || lane >= LANES.len() {
                return;
            }
            // PITCH's tools take the steps on and off too: on/off is
            // folded into it.
            let lanes: &[usize] = if lane == PITCH { &[PITCH, ON] } else { &[lane] };
            for &lane in lanes {
                let values = if random {
                    random_lane(lane, &mut next)
                } else {
                    [reset_value(lane); STEPS]
                };
                for (step, v) in values.into_iter().enumerate() {
                    if let Some(c) = registry.get(GROUP, &lane_key(slot, lane, step)) {
                        c.set(v);
                    }
                }
            }
        });
}
