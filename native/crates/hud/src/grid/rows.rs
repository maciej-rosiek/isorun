//! Text on the grid: a row is runs of one colour, one character per cell. The stats pane, the
//! message pane and a room's text all reduce to rows, and the host paints a row as a line of
//! spans in the grid font. Nothing here draws.

use isorun_webtiles::formatted_string_to_spans;

/// A cell's background: a terminal colour, a plain colour the official stylesheet names (the
/// noise bar), or a bar's hard stops across the span as fractions of it (stats.rs `paint_bar`).
#[derive(Clone, Debug, PartialEq)]
pub enum Bg {
    Term(u8),
    Rgb([u8; 3]),
    Stripes(Vec<(f32, f32, BarPaint)>),
}

/// One stop of a bar: a terminal colour, or the bar's own dark track.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BarPaint {
    Term(u8),
    Track,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Span {
    pub text: String,
    /// foreground: a terminal colour index (`--color-N`); None for the region's default
    pub fg: Option<u8>,
    pub bg: Option<Bg>,
    /// a class the official stylesheet names (`colour_yellow`, `boosted_stat`), when the colour
    /// is not one of the sixteen
    pub cls: Option<String>,
    /// a tooltip: the status light's `desc`, the doom's `doom_desc`
    pub title: Option<String>,
}

impl Span {
    pub fn new(text: impl Into<String>) -> Span {
        Span { text: text.into(), ..Default::default() }
    }
    pub fn fg(text: impl Into<String>, fg: u8) -> Span {
        Span { text: text.into(), fg: Some(fg), ..Default::default() }
    }
}

pub type Row = Vec<Span>;

/// how many cells the row takes
pub fn row_length(row: &[Span]) -> usize {
    row.iter().map(|s| s.text.chars().count()).sum()
}

/// a run of `n` blank cells, painted `bg` when given
pub fn blank(n: usize, bg: Option<Bg>) -> Span {
    Span { text: " ".repeat(n), bg, ..Default::default() }
}

/// The row cut to `width` cells. With `ellipsis`, a row that was cut ends in `…` as
/// `text-overflow: ellipsis` ends the weapon and quiver lines.
pub fn cut_row(row: Row, width: usize, ellipsis: bool) -> Row {
    if row_length(&row) <= width {
        return row;
    }
    let mut out: Row = Vec::new();
    let mut left = if ellipsis { width.saturating_sub(1) } else { width };
    for s in row {
        if left == 0 {
            break;
        }
        let n = s.text.chars().count();
        if n <= left {
            left -= n;
            out.push(s);
        } else {
            out.push(Span { text: s.text.chars().take(left).collect(), ..s });
            left = 0;
        }
    }
    if ellipsis && width > 0 {
        let fg = out.last().and_then(|s| s.fg);
        out.push(Span { text: "…".into(), fg, ..Default::default() });
    }
    out
}

/// a row padded with blank cells to `width`, so what follows lands on a column
pub fn pad_row(mut row: Row, width: usize) -> Row {
    let n = width.saturating_sub(row_length(&row));
    if n > 0 {
        row.push(blank(n, None));
    }
    row
}

/// two rows side by side: `left` cut and padded to `at` cells, then `right`
pub fn join_rows(left: Row, at: usize, right: Row) -> Row {
    let mut out = pad_row(cut_row(left, at, false), at);
    out.extend(right);
    out
}

/// a crawl formatted string (`<red>text<lightgrey>`) as a row; `fg` is the colour of untagged text
pub fn from_formatted(s: &str, fg: Option<u8>) -> Row {
    formatted_string_to_spans(s).into_iter().map(|f| Span { text: f.text, fg: f.fg.or(fg), bg: f.bg.map(Bg::Term), ..Default::default() }).collect()
}

/// The row wrapped to `width` cells as `white-space: pre-wrap` wraps a message: at the last space
/// that fits, else hard at the width. A row that fits is one line; an empty row is one empty line.
pub fn wrap_row(row: &[Span], width: usize) -> Vec<Row> {
    if width == 0 {
        return vec![row.to_vec()];
    }
    // flatten to characters carrying their span, so a break can fall anywhere
    let chars: Vec<(char, usize)> = row.iter().enumerate().flat_map(|(i, s)| s.text.chars().map(move |c| (c, i))).collect();
    let mut lines = Vec::new();
    let mut start = 0;
    while start < chars.len() {
        let mut end = (start + width).min(chars.len());
        if end < chars.len() {
            if let Some(brk) = (start + 1..=end).rev().find(|&i| chars[i - 1].0 == ' ') {
                end = brk;
            }
        }
        // the space the line broke at stays at the end of the line, as pre-wrap hangs it there
        lines.push(rebuild(row, &chars[start..end]));
        start = end;
    }
    if lines.is_empty() {
        lines.push(vec![]);
    }
    lines
}

fn rebuild(row: &[Span], chars: &[(char, usize)]) -> Row {
    let mut out: Row = Vec::new();
    let mut src: Option<usize> = None;
    for &(c, i) in chars {
        if src == Some(i) {
            out.last_mut().unwrap().text.push(c);
        } else {
            out.push(Span { text: c.to_string(), ..row[i].clone() });
            src = Some(i);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(r: &Row) -> String {
        r.iter().map(|s| s.text.as_str()).collect()
    }

    #[test]
    fn wraps_at_spaces() {
        let row = from_formatted("<red>You hit</red> the goblin hard.", None);
        let lines = wrap_row(&row, 12);
        assert_eq!(lines.iter().map(text).collect::<Vec<_>>(), ["You hit the ", "goblin hard."]);
        assert_eq!(lines[0][0].fg, Some(4));
        assert_eq!(wrap_row(&from_formatted("abcdefghij", None), 4).iter().map(text).collect::<Vec<_>>(), ["abcd", "efgh", "ij"]);
        assert_eq!(wrap_row(&[], 4), vec![Vec::<Span>::new()]);
    }

    #[test]
    fn cuts_and_joins() {
        let r = cut_row(vec![Span::fg("a long weapon name", 3)], 8, true);
        assert_eq!(text(&r), "a long …");
        assert_eq!(r[1].fg, Some(3));
        assert_eq!(text(&join_rows(vec![Span::new("AC: 3")], 8, vec![Span::new("Str: 9")])), "AC: 3   Str: 9");
    }
}
