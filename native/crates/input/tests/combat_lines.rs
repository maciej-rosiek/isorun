use isorun_input::combat::{lines_since, LastLine};
use isorun_webtiles::{GameMessage, Marker};

fn line(text: &str, turn: i64) -> GameMessage {
    GameMessage { text: text.into(), turn: Some(turn), channel: Some(0), marker: None }
}

fn texts(v: Vec<&GameMessage>) -> Vec<&str> {
    v.into_iter().map(|l| l.text.as_str()).collect()
}

#[test]
fn a_marker_set_after_reading_keeps_the_place() {
    let mut lines = vec![line("You see a bat.", 5)];
    let last = LastLine::newest(&lines);
    // the reducer marks the line read last when the next command starts, then the fight comes
    lines[0].marker = Some(Marker::Command);
    lines.push(line("You hit the bat for 3.", 5));
    assert_eq!(texts(lines_since(&lines, last.as_ref(), 6)), ["You hit the bat for 3."]);
}

#[test]
fn a_repeated_line_is_found_where_it_stood() {
    let mut lines = vec![line("There are monsters nearby!", 5), line("There are monsters nearby!", 5)];
    let last = LastLine::newest(&lines);
    lines[1].marker = Some(Marker::Command);
    lines.push(line("There are monsters nearby!", 5));
    lines.push(line("You hit the bat for 3.", 5));
    assert_eq!(texts(lines_since(&lines, last.as_ref(), 6)), ["There are monsters nearby!", "You hit the bat for 3."]);
}

#[test]
fn after_the_log_moved_and_with_nothing_read() {
    let lines = vec![line("a", 1), line("b", 2), line("c", 3)];
    // the line read last now stands one place earlier (the cap dropped the oldest)
    let last = LastLine { line: line("b", 2), at: 2 };
    assert_eq!(texts(lines_since(&lines, Some(&last), 3)), ["c"]);
    // nothing read yet: the lines of this turn
    assert_eq!(texts(lines_since(&lines, None, 3)), ["c"]);
}
