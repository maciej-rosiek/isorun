//! The message pane on the grid.
//!
//! messages.js keeps `#messages` scrolled to its bottom inside a container `msg_height` lines
//! tall, each message a `pre-wrap` div with a prefix glyph (`_` in lightgray for a new turn,
//! darkgray for a new command, a blank otherwise) and the formatted text; `#more` is the line
//! under the container, shown while the server waits on a `--more--`, with the server's
//! `more_text` when it sent one. The text cursor is appended to the last message while the game
//! reads a line there.
//!
//! On the grid the pane is `rows` rows of messages and the more row: the log's lines wrapped to
//! the pane's width, the last `rows` of them shown, scrolled back `offset` lines when the player
//! is reading the log. Pure: a log in, rows out.

use super::rows::{from_formatted, wrap_row, Row, Span};
use isorun_webtiles::{Marker, MessageLog};

/// style.css `.prefix_glyph.turn_marker`: lightgray; `.command_marker`: darkgray
const TURN_MARKER: u8 = 7;
const COMMAND_MARKER: u8 = 8;

#[derive(Clone, Debug, PartialEq)]
pub struct PaneRows {
    /// the message rows, oldest first, exactly `rows` long (blank rows at the top when the log is short)
    pub lines: Vec<Row>,
    /// the `--more--` row: the server's text while a more is pending, else empty
    pub more: Row,
    /// lines scrolled back past the end of the log, clamped to what the log holds
    pub offset: usize,
}

/// every message of the log as wrapped rows, the marker on the first row of each
fn wrapped_log(log: &MessageLog, width: usize) -> Vec<Row> {
    let mut out = Vec::new();
    let text = width.saturating_sub(1).max(1);
    for l in &log.lines {
        let marker = Span {
            text: if l.marker.is_some() { "_" } else { " " }.into(),
            fg: Some(if l.marker == Some(Marker::Turn) { TURN_MARKER } else { COMMAND_MARKER }),
            ..Default::default()
        };
        for (i, row) in wrap_row(&from_formatted(&l.text, None), text).into_iter().enumerate() {
            let mut r = vec![if i == 0 { marker.clone() } else { Span::new(" ") }];
            r.extend(row);
            out.push(r);
        }
    }
    out
}

/// The pane's rows: the last `rows` wrapped lines of the log, less `offset` scrolled back, the
/// text cursor on the last line while the game reads there, and the more row.
pub fn pane_rows(log: &MessageLog, rows: usize, width: usize, offset: usize) -> PaneRows {
    let all = wrapped_log(log, width);
    let offset = offset.min(all.len().saturating_sub(rows));
    let end = all.len() - offset;
    let mut shown: Vec<Row> = all[end.saturating_sub(rows)..end].to_vec();
    if log.text_cursor && offset == 0 {
        if let Some(last) = shown.last_mut() {
            last.push(Span { text: "_".into(), cls: Some("text-cursor".into()), ..Default::default() });
        }
    }
    while shown.len() < rows {
        shown.insert(0, vec![]);
    }
    let more = if log.more { vec![Span::new(if log.more_text.is_empty() { "--more--" } else { &log.more_text })] } else { vec![] };
    PaneRows { lines: shown, more, offset }
}

#[cfg(test)]
mod tests {
    use super::*;
    use isorun_webtiles::GameMessage;

    fn log(lines: &[(&str, Option<Marker>)]) -> MessageLog {
        MessageLog { lines: lines.iter().map(|(t, m)| GameMessage { text: t.to_string(), marker: *m, ..Default::default() }).collect(), ..Default::default() }
    }

    #[test]
    fn last_rows_with_markers() {
        let l = log(&[("one", Some(Marker::Turn)), ("two", None), ("three", Some(Marker::Command))]);
        let p = pane_rows(&l, 2, 20, 0);
        assert_eq!(p.lines.len(), 2);
        assert_eq!(p.lines[0][0].text, " ");
        assert_eq!(p.lines[1][0], Span { text: "_".into(), fg: Some(COMMAND_MARKER), ..Default::default() });
        let back = pane_rows(&l, 2, 20, 5);
        assert_eq!(back.offset, 1);
        assert_eq!(back.lines[0][1].text, "one");
        let short = pane_rows(&l, 5, 20, 0);
        assert!(short.lines[0].is_empty() && short.lines[1].is_empty());
        assert!(short.more.is_empty());
    }
}
