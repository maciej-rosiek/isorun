//! The cursor over a screen's rows (the pure part of `focus.ts`): each item stands in a row and
//! a column; up and down go to the nearest row above or below, keeping the column as close as
//! possible; left and right stay on the row. Past an edge the cursor wraps to the far end.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FocusSlot {
    pub row: i32,
    pub col: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FocusDir {
    Up,
    Down,
    Left,
    Right,
}

/// Resolve the row / col of each item: rows default to registration order, cols to order within the row.
pub fn focus_slots(items: &[(Option<i32>, Option<i32>)]) -> Vec<FocusSlot> {
    let mut per_row = std::collections::HashMap::new();
    items
        .iter()
        .enumerate()
        .map(|(i, (row, col))| {
            let row = row.unwrap_or(i as i32);
            let n = per_row.entry(row).or_insert(0);
            let slot = FocusSlot { row, col: col.unwrap_or(*n) };
            *n += 1;
            slot
        })
        .collect()
}

/// Where the cursor goes from `index` in `dir`; none at an edge.
pub fn focus_step(slots: &[FocusSlot], index: usize, dir: FocusDir) -> Option<usize> {
    let Some(cur) = slots.get(index) else { return if slots.is_empty() { None } else { Some(0) } };
    if matches!(dir, FocusDir::Left | FocusDir::Right) {
        let forward = dir == FocusDir::Right;
        let mut best: Option<usize> = None;
        for (i, s) in slots.iter().enumerate() {
            if i == index || s.row != cur.row {
                continue;
            }
            if if forward { s.col <= cur.col } else { s.col >= cur.col } {
                continue;
            }
            match best {
                None => best = Some(i),
                Some(b) => {
                    let b = slots[b];
                    if if forward { s.col < b.col } else { s.col > b.col } {
                        best = Some(i);
                    }
                }
            }
        }
        return best;
    }
    let down = dir == FocusDir::Down;
    // the nearest row in that direction with an item in the same column, else the nearest row at all
    let nearest_row = |same_col: bool| -> Option<i32> {
        let mut row: Option<i32> = None;
        for s in slots {
            if same_col && s.col != cur.col {
                continue;
            }
            if if down { s.row > cur.row } else { s.row < cur.row } {
                row = Some(match row {
                    None => s.row,
                    Some(r) => {
                        if down {
                            r.min(s.row)
                        } else {
                            r.max(s.row)
                        }
                    }
                });
            }
        }
        row
    };
    let row = nearest_row(true).or_else(|| nearest_row(false))?;
    let mut best: Option<usize> = None;
    for (i, s) in slots.iter().enumerate() {
        if s.row != row {
            continue;
        }
        if best.is_none_or(|b| (s.col - cur.col).abs() < (slots[b].col - cur.col).abs()) {
            best = Some(i);
        }
    }
    best
}

/// Past an edge: the far end of the row sideways; going down the first row (the nearest column with
/// `keep_column`, else its start), going up the last row (likewise, else its end).
pub fn focus_wrap(slots: &[FocusSlot], index: usize, dir: FocusDir, keep_column: bool) -> Option<usize> {
    if slots.is_empty() {
        return None;
    }
    let Some(cur) = slots.get(index) else { return Some(0) };
    if matches!(dir, FocusDir::Left | FocusDir::Right) {
        let mut best: Option<usize> = None;
        for (i, s) in slots.iter().enumerate() {
            if s.row != cur.row {
                continue;
            }
            match best {
                None => best = Some(i),
                Some(b) => {
                    let b = slots[b];
                    if s.col != b.col && if dir == FocusDir::Right { s.col < b.col } else { s.col > b.col } {
                        best = Some(i);
                    }
                }
            }
        }
        return match best {
            Some(b) if b != index && slots[b].col != cur.col => Some(b),
            _ => None,
        };
    }
    let down = dir == FocusDir::Down;
    let row = slots.iter().map(|s| s.row).reduce(|a, b| if down { a.min(b) } else { a.max(b) })?;
    if row == cur.row {
        return None;
    }
    let mut best: Option<usize> = None;
    for (i, s) in slots.iter().enumerate() {
        if s.row != row {
            continue;
        }
        let better = match best {
            None => true,
            Some(b) => {
                if keep_column {
                    (s.col - cur.col).abs() < (slots[b].col - cur.col).abs()
                } else if down {
                    s.col < slots[b].col
                } else {
                    s.col > slots[b].col
                }
            }
        };
        if better {
            best = Some(i);
        }
    }
    best
}

/// One move of the cursor, wrapping past an edge.
pub fn focus_move(slots: &[FocusSlot], index: usize, dir: FocusDir, keep_column: bool) -> Option<usize> {
    focus_step(slots, index, dir).or_else(|| focus_wrap(slots, index, dir, keep_column))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn slots(v: &[(i32, i32)]) -> Vec<FocusSlot> {
        v.iter().map(|&(row, col)| FocusSlot { row, col }).collect()
    }

    #[test]
    fn rows_and_columns() {
        let s = focus_slots(&[(None, None), (Some(1), Some(0)), (Some(1), Some(1)), (None, None)]);
        assert_eq!(s, slots(&[(0, 0), (1, 0), (1, 1), (3, 0)]));
    }

    #[test]
    fn steps() {
        // Back; an account with its rc and delete beside it; Add
        let s = slots(&[(0, 0), (1, 0), (1, 1), (1, 2), (2, 0)]);
        assert_eq!(focus_step(&s, 0, FocusDir::Down), Some(1));
        assert_eq!(focus_step(&s, 1, FocusDir::Right), Some(2));
        assert_eq!(focus_step(&s, 2, FocusDir::Right), Some(3));
        assert_eq!(focus_step(&s, 3, FocusDir::Right), None);
        assert_eq!(focus_step(&s, 3, FocusDir::Down), Some(4));
        assert_eq!(focus_step(&s, 4, FocusDir::Up), Some(1));
        assert_eq!(focus_step(&s, 0, FocusDir::Up), None);
    }

    #[test]
    fn wraps() {
        let s = slots(&[(0, 0), (1, 0), (200, 0), (200, 1), (200, 2)]);
        // up from the top of the title screen lands on the footer's nearest column: the account
        assert_eq!(focus_move(&s, 0, FocusDir::Up, true), Some(2));
        assert_eq!(focus_move(&s, 4, FocusDir::Down, true), Some(0));
        assert_eq!(focus_move(&s, 4, FocusDir::Right, true), Some(2));
        assert_eq!(focus_move(&s, 2, FocusDir::Left, true), Some(4));
        assert_eq!(focus_move(&s, 0, FocusDir::Left, true), None);
    }
}
