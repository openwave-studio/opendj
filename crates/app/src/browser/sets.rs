//! Crates in the browser (see [`opendj_library::sets`]): the crates each
//! track is in, for the Crates column and `crate:`, and tidying the lists
//! from Collection Health.

use opendj_library::sets::{ListIssue, ListIssueKind};
use slint::{ComponentHandle, ModelRc, VecModel};

use super::filters::CrateLook;
use super::{Browser, Contents};
use crate::{ListIssueRow, ListsUi};

impl Browser {
    /// Reads again which crates each track is in, and what Collection Health
    /// finds in the lists.
    pub(super) fn refresh_crates(&mut self) {
        let Some(db) = &self.db else { return };
        self.crates = db.crates_by_track().unwrap_or_default();
        self.list_report = db.list_report().unwrap_or_default();
        // How each crate looks, for the Crates chip's list.
        self.filter.crate_looks = db
            .playlists()
            .unwrap_or_default()
            .into_iter()
            .filter(opendj_library::sets::is_crate)
            .map(|p| {
                let icon = p
                    .icon
                    .as_deref()
                    .and_then(|name| opendj_library::LIST_ICONS.iter().position(|&i| i == name))
                    .unwrap_or(0);
                let look = CrateLook {
                    icon,
                    color: p.color,
                };
                (p.name.to_lowercase(), look)
            })
            .collect();
    }

    /// What Collection Health says about the lists on the Overview; empty
    /// when they're fine.
    pub(super) fn list_health_text(&self) -> String {
        let r = &self.list_report;
        let bad = r.lists - r.tidy();
        if bad == 0 {
            return String::new();
        }
        format!("{bad} of {} need tidying", lists_word(r.lists))
    }

    pub(super) fn install_lists(
        ui: &ListsUi,
        with: &(impl Fn(&dyn Fn(&mut Browser)) + Clone + 'static),
    ) {
        let w = with.clone();
        ui.on_tidy(move || w(&|b| b.tidy_sure_lists()));
        let w = with.clone();
        ui.on_fix(move |row| w(&|b| b.fix_list_row(row.max(0) as usize)));
        let w = with.clone();
        ui.on_show(move |row| w(&|b| b.open_list_row(row.max(0) as usize)));
    }

    /// Whether the Lists page is shown.
    pub(super) fn shows_lists(&self) -> bool {
        self.saved()
            && self.tree == super::COLLECTION
            && self
                .nodes
                .get(self.current_node)
                .is_some_and(|n| matches!(n.contents, Contents::Lists))
    }

    /// Shows the Lists page.
    pub(super) fn open_lists(&mut self) {
        self.show_health(true);
        let Some(n) = self
            .nodes
            .iter()
            .position(|n| matches!(n.contents, Contents::Lists))
        else {
            return;
        };
        self.review_at = None;
        self.dupe_tab = None;
        self.current_node = n;
        self.folders_focused = false;
        self.refresh_tree();
        self.refresh_rows();
    }

    /// Shows the Lists page as things are now.
    pub(super) fn refresh_lists_ui(&self) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let ui = app.global::<ListsUi>();
        let shown = self.shows_lists();
        ui.set_open(shown);
        if !shown {
            return;
        }
        let r = &self.list_report;
        ui.set_percent((r.tidy() * 100).checked_div(r.lists).unwrap_or(100) as i32);
        ui.set_summary(format!("{} of {} tidy.", r.tidy(), lists_word(r.lists)).into());
        let rows: Vec<ListIssueRow> = r
            .issues
            .iter()
            .map(|i| {
                let (problem, fix) = issue_text(i);
                ListIssueRow {
                    name: i.name.clone().into(),
                    problem: problem.into(),
                    fix: fix.into(),
                    sure: i.sure,
                }
            })
            .collect();
        ui.set_rows(ModelRc::new(VecModel::from(rows)));
        ui.set_sure(r.sure() as i32);
    }

    /// Fixes every sure problem in the lists.
    pub(super) fn tidy_sure_lists(&mut self) {
        let Some(db) = &mut self.db else { return };
        self.health_status = match db.tidy_sure_lists() {
            Ok((lists, entries)) => {
                let mut s = Vec::new();
                if lists > 0 {
                    s.push(format!("deleted {}", lists_word(lists)));
                }
                if entries > 0 {
                    s.push(format!(
                        "took {} out",
                        if entries == 1 {
                            "1 entry".into()
                        } else {
                            format!("{entries} entries")
                        }
                    ));
                }
                if s.is_empty() {
                    "The lists were already tidy.".into()
                } else {
                    capitalized(&s.join(" and ")) + "."
                }
            }
            Err(e) => format!("Couldn't tidy the lists: {e}"),
        };
        self.after_lists_changed();
    }

    /// The fix on a row of the Lists page; a name used twice opens the list
    /// to rename it.
    pub(super) fn fix_list_row(&mut self, row: usize) {
        let Some(issue) = self.list_report.issues.get(row).cloned() else {
            return;
        };
        if issue.kind == ListIssueKind::SameName {
            self.show_list(issue.list);
            return;
        }
        let Some(db) = &mut self.db else { return };
        self.health_status = match db.fix_list(issue.list, issue.kind) {
            Ok(_) => match issue.kind {
                ListIssueKind::Empty => format!("Deleted \"{}\".", issue.name),
                _ => format!("Tidied \"{}\".", issue.name),
            },
            Err(e) => format!("Couldn't tidy \"{}\": {e}", issue.name),
        };
        self.after_lists_changed();
    }

    /// Opens a row's list.
    pub(super) fn open_list_row(&mut self, row: usize) {
        if let Some(id) = self.list_report.issues.get(row).map(|i| i.list) {
            self.show_list(id);
        }
    }

    fn after_lists_changed(&mut self) {
        let on_page = self.shows_lists();
        self.reload_tree(None);
        // The tree was read again: stay on the page.
        if let Some(n) = self
            .nodes
            .iter()
            .position(|n| matches!(n.contents, Contents::Lists))
            .filter(|_| on_page)
        {
            self.current_node = n;
        }
        self.refresh_tree();
        self.refresh_health_ui();
    }
}

/// "1 list" or "3 lists".
fn lists_word(n: usize) -> String {
    if n == 1 {
        "1 list".into()
    } else {
        format!("{n} lists")
    }
}

fn capitalized(s: &str) -> String {
    let mut c = s.chars();
    c.next()
        .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
        .unwrap_or_default()
}

/// A row's problem and the fix's button.
fn issue_text(i: &ListIssue) -> (String, &'static str) {
    let n = i.count;
    match i.kind {
        ListIssueKind::Removed => (
            format!(
                "{} for tracks taken out of your collection",
                if n == 1 {
                    "1 entry".into()
                } else {
                    format!("{n} entries")
                }
            ),
            "Take out",
        ),
        ListIssueKind::Twice => (
            format!(
                "{} listed more than once",
                if n == 1 {
                    "1 track".into()
                } else {
                    format!("{n} tracks")
                }
            ),
            "Keep the first",
        ),
        ListIssueKind::Empty if i.sure => ("Empty since it was brought in".into(), "Delete"),
        ListIssueKind::Empty => ("Empty".into(), "Delete"),
        ListIssueKind::SameName => (
            "Another list or folder beside it has this name".into(),
            "Rename",
        ),
    }
}
