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
}

pub fn navigate(rows: &[Row], current: Option<usize>, direction: Direction) -> Option<Action> {
    if rows.is_empty() {
        return None;
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
        Direction::Left if children == 2 => Some(Action::Toggle(at)),
        Direction::Left => (0..at)
            .rev()
            .find(|&i| rows[i].depth < depth)
            .map(Action::Select),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(depth: i32, children: i32) -> Row {
        Row { depth, children }
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
        assert_eq!(navigate(&rows, Some(1), Direction::Right), None);
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
        assert_eq!(navigate(&rows, Some(3), Direction::Left), None);
    }
}
