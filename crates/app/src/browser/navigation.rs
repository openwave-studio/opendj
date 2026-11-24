//! Semantic browser navigation shared by keyboard and controller input.
use super::*;
use std::time::{Duration, Instant};

/// How long the rail's highlight rests before its source opens.
/// RailOrder's `cursor` when nothing is pending.
const NO_RAIL_CURSOR: i32 = -2;

pub(super) const RAIL_SETTLE: Duration = Duration::from_millis(250);

impl Browser {
    /// Opens the rail entry the knob or ↑ ↓ moved to, once it has rested
    /// for `RAIL_SETTLE` (or at once with `now`). A click on the rail in
    /// the meantime clears the cursor and wins.
    pub(super) fn settle_rail(&mut self, now: bool) {
        let Some((next, at)) = self.rail_pending else {
            return;
        };
        if !now && at.elapsed() < RAIL_SETTLE {
            return;
        }
        self.rail_pending = None;
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let order = app.global::<crate::RailOrder>();
        if order.get_cursor() != next {
            return;
        }
        order.set_cursor(NO_RAIL_CURSOR);
        if next != COLLECTION && self.history_tab || next < 0 {
            self.show_history(false);
            app.global::<LibraryEdit>().set_history_tab(false);
        }
        if next < 0 {
            self.show_health(true);
        } else {
            app.global::<StoreUi>().set_source(next);
            self.select_source(next);
        }
    }

    pub(super) fn navigate_browser(&mut self, code: i32, steps: i32) {
        let (Some(direction), Some(app)) = (Direction::from_code(code), self.app.upgrade()) else {
            return;
        };
        if steps <= 0 {
            return;
        }
        let focus = app.global::<BrowserFocus>();
        let pane = focus.get_pane();
        match pane {
            BrowserPane::Sources => match direction {
                Direction::Up | Direction::Down => {
                    // Only the highlight moves now; the source opens once
                    // the moves stop, so a fast turn doesn't wait for each
                    // source (Collection Health above all) to load.
                    let (source, health) = match self.rail_pending {
                        Some((entry, _)) => (entry.max(0), entry < 0),
                        None => (app.global::<StoreUi>().get_source(), self.health_view),
                    };
                    let next = app.global::<crate::RailOrder>().invoke_adjacent(
                        source,
                        health,
                        if matches!(direction, Direction::Up) {
                            -steps
                        } else {
                            steps
                        },
                    );
                    app.global::<crate::RailOrder>().set_cursor(next);
                    self.rail_pending = Some((next, Instant::now()));
                    focus.invoke_focus(BrowserPane::Sources);
                }
                Direction::Right => {
                    self.settle_rail(true);
                    focus.invoke_focus(BrowserPane::Folders);
                }
                Direction::Left => {}
            },
            BrowserPane::Folders => {
                if app.global::<StoreUi>().get_source() == INSTRUMENTS {
                    self.navigate_instruments(&app, direction, code, steps);
                } else if matches!(direction, Direction::Up | Direction::Down) {
                    let delta = if matches!(direction, Direction::Up) {
                        -steps
                    } else {
                        steps
                    };
                    let row = if self.health_view && self.tree == COLLECTION {
                        // Headings are click shortcuts, not distinct pages.
                        // Including them would trap Up at the first check.
                        let rows: Vec<_> = self
                            .visible_nodes
                            .iter()
                            .enumerate()
                            .filter(|(_, n)| self.nodes[**n].kind != KIND_HEALTH_SECTION)
                            .collect();
                        let current = rows.iter().position(|&(_, &n)| n == self.current_node);
                        tree_navigation::step_row(current, rows.len(), delta).map(|i| rows[i].0)
                    } else {
                        let current = self
                            .visible_nodes
                            .iter()
                            .position(|&n| n == self.current_node);
                        tree_navigation::step_row(current, self.visible_nodes.len(), delta)
                    };
                    if let Some(row) = row {
                        self.select_folder_row(row, false);
                        focus.invoke_focus(BrowserPane::Folders);
                    }
                } else {
                    self.navigate_folders(code);
                }
            }
            BrowserPane::Content => match direction {
                Direction::Left => focus.invoke_focus(BrowserPane::Folders),
                Direction::Right => {}
                Direction::Up | Direction::Down => {
                    if !focus.get_content_available() {
                        return;
                    }
                    let delta = if matches!(direction, Direction::Up) {
                        -steps
                    } else {
                        steps
                    };
                    if app.global::<StoreUi>().get_source() == INSTRUMENTS {
                        let ui = app.global::<crate::InstrumentsUi>();
                        if let Some(row) = tree_navigation::step_row(
                            usize::try_from(ui.get_current_item()).ok(),
                            ui.get_rows().row_count(),
                            delta,
                        ) {
                            ui.set_current_item(row as i32);
                        }
                    } else if let Some(row) = tree_navigation::step_row(
                        usize::try_from(app.get_current_track()).ok(),
                        self.rows.len(),
                        delta,
                    ) {
                        self.pick_only(row as i32);
                        app.set_current_track(row as i32);
                    }
                    focus.invoke_focus(BrowserPane::Content);
                }
            },
        }
    }

    fn navigate_instruments(&self, app: &AppWindow, direction: Direction, code: i32, steps: i32) {
        if matches!(direction, Direction::Up | Direction::Down) {
            let ui = app.global::<crate::InstrumentsUi>();
            let delta = if matches!(direction, Direction::Up) {
                -steps
            } else {
                steps
            };
            if let Some(row) = tree_navigation::step_row(
                usize::try_from(ui.get_current_node()).ok(),
                ui.get_nodes().row_count(),
                delta,
            ) {
                ui.invoke_select_node(row as i32);
                app.global::<BrowserFocus>()
                    .invoke_focus(BrowserPane::Folders);
            }
        } else {
            crate::instruments::navigate_nodes(app, code);
        }
    }
}
