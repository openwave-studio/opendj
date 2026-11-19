#[derive(Clone, Copy, Debug)]
pub struct Row {
    pub depth: i32,
    /// 0: leaf, 1: collapsed, 2: expanded.
    pub children: i32,
}

#[derive(Clone, Copy, Debug)]
pub enum Direction {
    Up,
    Down,
    Left,
    Right,
}

impl Direction {
    pub fn from_code(code: i32) -> Option<Self> {
        match code {
            0 => Some(Self::Up),
            1 => Some(Self::Down),
            2 => Some(Self::Left),
            3 => Some(Self::Right),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Select(usize),
    Toggle(usize),
    EnterContent,
    ExitToSources,
}

/// Moves a selection by signed encoder steps, clamped to the visible rows.
pub fn step_row(current: Option<usize>, len: usize, steps: i32) -> Option<usize> {
    if len == 0 || steps == 0 {
        return None;
    }
    let target = match current.filter(|&at| at < len) {
        Some(at) => at as i64 + i64::from(steps),
        None => i64::from(steps.saturating_sub(1)).max(0),
    };
    Some(target.clamp(0, len as i64 - 1) as usize)
}

pub fn navigate(rows: &[Row], current: Option<usize>, direction: Direction) -> Option<Action> {
    if rows.is_empty() {
        return matches!(direction, Direction::Left).then_some(Action::ExitToSources);
    }
    let Some(at) = current.filter(|&i| i < rows.len()) else {
        return Some(Action::Select(0));
    };

    let Row { depth, children } = rows[at];

    match direction {
        Direction::Up => (at > 0).then(|| Action::Select(at - 1)),
        Direction::Down => (at + 1 < rows.len()).then(|| Action::Select(at + 1)),
        Direction::Right if children == 1 => Some(Action::Toggle(at)),
        Direction::Right if children == 2 => rows
            .get(at + 1)
            .filter(|row| row.depth > depth)
            .map(|_| Action::Select(at + 1)),
        Direction::Right => Some(Action::EnterContent),
        Direction::Left if children == 2 => Some(Action::Toggle(at)),
        Direction::Left => Some(
            (0..at)
                .rev()
                .find(|&i| rows[i].depth < depth)
                .map(Action::Select)
                .unwrap_or(Action::ExitToSources),
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(depth: i32, children: i32) -> Row {
        Row { depth, children }
    }

    #[test]
    fn encoder_steps_and_keyboard_steps_share_boundaries() {
        assert_eq!(step_row(None, 0, 1), None);
        assert_eq!(step_row(None, 5, 1), Some(0));
        assert_eq!(step_row(None, 5, -1), Some(0));
        assert_eq!(step_row(None, 5, 3), Some(2));
        assert_eq!(step_row(Some(1), 5, 3), Some(4));
        assert_eq!(step_row(Some(1), 5, -3), Some(0));
        assert_eq!(step_row(Some(4), 5, i32::MAX), Some(4));
        assert_eq!(step_row(Some(2), 5, 0), None);
    }

    #[test]
    fn empty_and_missing_selection() {
        assert_eq!(navigate(&[], None, Direction::Down), None);
        assert_eq!(
            navigate(&[row(0, 0)], None, Direction::Down),
            Some(Action::Select(0))
        );
        assert_eq!(
            navigate(&[row(0, 0)], Some(8), Direction::Up),
            Some(Action::Select(0))
        );
    }

    #[test]
    fn empty_tree_and_nonzero_depth_root_can_return_to_sources() {
        assert_eq!(
            navigate(&[], None, Direction::Left),
            Some(Action::ExitToSources)
        );
        assert_eq!(
            navigate(&[row(2, 0)], Some(0), Direction::Left),
            Some(Action::ExitToSources)
        );
        assert_eq!(
            navigate(&[row(2, 0)], Some(0), Direction::Right),
            Some(Action::EnterContent)
        );
    }

    #[test]
    fn up_and_down_use_visible_rows_and_stop_at_edges() {
        let rows = [row(0, 2), row(1, 0), row(0, 0)];
        assert_eq!(navigate(&rows, Some(0), Direction::Up), None);
        assert_eq!(
            navigate(&rows, Some(0), Direction::Down),
            Some(Action::Select(1))
        );
        assert_eq!(
            navigate(&rows, Some(2), Direction::Up),
            Some(Action::Select(1))
        );
        assert_eq!(navigate(&rows, Some(2), Direction::Down), None);
    }

    #[test]
    fn right_opens_a_collapsed_or_lazy_folder() {
        assert_eq!(
            navigate(&[row(0, 1)], Some(0), Direction::Right),
            Some(Action::Toggle(0))
        );
    }

    #[test]
    fn right_enters_the_first_child_but_not_a_sibling() {
        let rows = [row(0, 2), row(1, 0), row(0, 0)];
        assert_eq!(
            navigate(&rows, Some(0), Direction::Right),
            Some(Action::Select(1))
        );
        assert_eq!(
            navigate(&rows, Some(1), Direction::Right),
            Some(Action::EnterContent)
        );
        assert_eq!(
            navigate(&[row(0, 2), row(0, 0)], Some(0), Direction::Right),
            None
        );
    }

    #[test]
    fn left_closes_before_visiting_parent() {
        let rows = [row(0, 2), row(1, 2), row(2, 0)];
        assert_eq!(
            navigate(&rows, Some(1), Direction::Left),
            Some(Action::Toggle(1))
        );
        assert_eq!(
            navigate(&rows, Some(2), Direction::Left),
            Some(Action::Select(1))
        );
    }

    #[test]
    fn left_skips_siblings_and_stops_at_root() {
        let rows = [row(0, 2), row(1, 0), row(1, 1), row(0, 0)];
        assert_eq!(
            navigate(&rows, Some(2), Direction::Left),
            Some(Action::Select(0))
        );
        assert_eq!(
            navigate(&rows, Some(3), Direction::Left),
            Some(Action::ExitToSources)
        );
    }
}
