//! Crawl formatted strings: `<red>text<lightgrey>`, `<bg:blue>`, `<<` escapes. Ported from
//! the official client's util.js so the output matches it.

use once_cell::sync::Lazy;
use regex::{Captures, Regex};

/// The tag names crawl writes, to their terminal colour index.
pub fn colour_index(name: &str) -> Option<u8> {
    Some(match name {
        "black" => 0,
        "blue" => 1,
        "green" => 2,
        "cyan" => 3,
        "red" => 4,
        "magenta" => 5,
        "brown" => 6,
        "lightgrey" | "lightgray" => 7,
        "darkgrey" | "darkgray" => 8,
        "lightblue" => 9,
        "lightgreen" => 10,
        "lightcyan" => 11,
        "lightred" => 12,
        "lightmagenta" => 13,
        "yellow" | "h" => 14,
        "white" | "w" => 15,
        _ => return None,
    })
}

/// The canonical tag name of each of the sixteen terminal colours, by index.
pub const COLOUR_TAGS: [&str; 16] = [
    "black",
    "blue",
    "green",
    "cyan",
    "red",
    "magenta",
    "brown",
    "lightgrey",
    "darkgrey",
    "lightblue",
    "lightgreen",
    "lightcyan",
    "lightred",
    "lightmagenta",
    "yellow",
    "white",
];

/// The sixteen terminal colours as the client draws them (`--color-0` .. `--color-15` in
/// apps/isorun/src/styles.css: the Tango palette, with crawl's own blues and reds).
pub const PALETTE: [[u8; 3]; 16] = [
    [0x00, 0x00, 0x00],
    [0x00, 0x5a, 0xfa],
    [0x4e, 0x9a, 0x06],
    [0x06, 0x98, 0x9a],
    [0xb3, 0x00, 0x09],
    [0xcd, 0x21, 0xcb],
    [0x8f, 0x59, 0x02],
    [0xba, 0xbd, 0xb6],
    [0x55, 0x57, 0x53],
    [0x5e, 0x78, 0xff],
    [0x8a, 0xe2, 0x34],
    [0x34, 0xe2, 0xe2],
    [0xef, 0x29, 0x29],
    [0xfd, 0x59, 0xfa],
    [0xfc, 0xe9, 0x4f],
    [0xee, 0xee, 0xec],
];

/// Text as a formatted string in one colour: `<red>text</red>`. Plain text has its `<` doubled
/// so it stays a literal ([`formatted_string_to_text`] undoes it); text that is `formatted`
/// already keeps its own tags, which then override the colour inside it. A colour outside the
/// sixteen leaves the text as it was.
pub fn coloured_text(text: &str, fg: i64, formatted: bool) -> String {
    let inner = if formatted { text.to_owned() } else { text.replace('<', "<<") };
    match usize::try_from(fg).ok().and_then(|i| COLOUR_TAGS.get(i)) {
        Some(name) => format!("<{name}>{inner}</{name}>"),
        None => inner,
    }
}

fn escape_html(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

/// A tag split as util.js splits it: closing, background, and the name left.
fn split_tag(raw: &str) -> (bool, bool, &str) {
    let (closing, rest) = match raw.strip_prefix('/') {
        Some(r) => (true, r),
        None => (false, raw),
    };
    match rest.strip_prefix("bg:") {
        Some(r) => (closing, true, r),
        None => (closing, false, rest),
    }
}

static HTML_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<?<(/?(bg:)?[a-z]*)>?|>|&").unwrap());
static SPAN_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)<?<(/?(bg:)?[a-z]*)>?").unwrap());
static ESCAPES_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"<<+").unwrap());
static TAG_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(?i)</?(bg:)?[a-z]+>").unwrap());

/// A formatted string as HTML: `<span class='fgN'>` and `<span class='bgN'>`, as util.js
/// `formatted_string_to_html` writes it (a closing bg tag with no background open pops a
/// foreground, as there).
pub fn formatted_string_to_html(s: &str) -> String {
    let mut cur_fg: Vec<u8> = Vec::new();
    let mut bg_open = false;
    let mut out = HTML_RE
        .replace_all(s, |c: &Captures| {
            let whole = &c[0];
            let (closing, bg, name) = split_tag(c.get(1).map_or("", |m| m.as_str()));
            let colour = colour_index(name).filter(|_| !whole.starts_with("<<") && whole.ends_with('>'));
            let Some(colour) = colour else {
                return escape_html(whole.strip_prefix('<').filter(|_| whole.starts_with("<<")).unwrap_or(whole));
            };
            if closing {
                if bg && bg_open {
                    bg_open = false;
                    "</span>".into()
                } else if !cur_fg.is_empty() {
                    cur_fg.pop();
                    match cur_fg.last() {
                        Some(f) => format!("</span><span class='fg{f}'>"),
                        None => "</span>".into(),
                    }
                } else {
                    String::new()
                }
            } else if bg {
                let mut text = format!("<span class='bg{colour}'>");
                if bg_open {
                    text = format!("</span>{text}");
                }
                if let Some(f) = cur_fg.last() {
                    text = format!("</span>{text}<span class='fg{f}'>");
                }
                bg_open = true;
                text
            } else {
                let mut text = format!("<span class='fg{colour}'>");
                if !cur_fg.is_empty() {
                    text = format!("</span>{text}");
                }
                cur_fg.push(colour);
                text
            }
        })
        .into_owned();
    if !cur_fg.is_empty() {
        out.push_str("</span>");
    }
    if bg_open {
        out.push_str("</span>");
    }
    out
}

/// Colour tags off and `<<` unescaped, for plain-text uses (labels, parsing). A tag that names
/// no colour stays.
pub fn formatted_string_to_text(s: &str) -> String {
    let unescaped = ESCAPES_RE.replace_all(s, |c: &Captures| c[0][1..].to_owned());
    TAG_RE
        .replace_all(&unescaped, |c: &Captures| {
            let m = &c[0];
            let name: String = m.chars().filter(|&ch| ch != '<' && ch != '>' && ch != '/').collect();
            let name = name.strip_prefix("bg:").unwrap_or(&name);
            if colour_index(name).is_some() {
                String::new()
            } else {
                m.to_owned()
            }
        })
        .into_owned()
}

/// A run of a formatted string in one colour: `fg` and `bg` are terminal colour indexes, `None`
/// for the default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FormattedSpan {
    pub text: String,
    pub fg: Option<u8>,
    pub bg: Option<u8>,
}

/// A formatted string as runs of one colour, for drawing on a grid of cells rather than as HTML.
/// The same state machine as [`formatted_string_to_html`]: a colour tag pushes a foreground, its
/// closing tag pops back to the one before, `<bg:...>` opens a background until its closing tag,
/// `<<` is a literal `<`, and a tag that names no colour is text.
pub fn formatted_string_to_spans(s: &str) -> Vec<FormattedSpan> {
    let mut out = Vec::new();
    let mut fg_stack: Vec<u8> = Vec::new();
    let mut bg: Option<u8> = None;
    let mut text = String::new();
    let flush = |text: &mut String, out: &mut Vec<FormattedSpan>, fg: Option<u8>, bg: Option<u8>| {
        if !text.is_empty() {
            out.push(FormattedSpan { text: std::mem::take(text), fg, bg });
        }
    };
    let mut last = 0;
    for c in SPAN_RE.captures_iter(s) {
        let m = c.get(0).expect("group 0");
        text.push_str(&s[last..m.start()]);
        last = m.end();
        let whole = m.as_str();
        let (closing, is_bg, name) = split_tag(c.get(1).map_or("", |g| g.as_str()));
        match colour_index(name).filter(|_| !whole.starts_with("<<") && whole.ends_with('>')) {
            Some(colour) => {
                flush(&mut text, &mut out, fg_stack.last().copied(), bg);
                if closing {
                    if is_bg && bg.is_some() {
                        bg = None;
                    } else {
                        fg_stack.pop();
                    }
                } else if is_bg {
                    bg = Some(colour);
                } else {
                    fg_stack.push(colour);
                }
            }
            None => text.push_str(if whole.starts_with("<<") { &whole[1..] } else { whole }),
        }
    }
    text.push_str(&s[last..]);
    flush(&mut text, &mut out, fg_stack.last().copied(), bg);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn span(text: &str, fg: Option<u8>, bg: Option<u8>) -> FormattedSpan {
        FormattedSpan { text: text.into(), fg, bg }
    }

    #[test]
    fn html_like_the_official_client() {
        assert_eq!(formatted_string_to_html("<red>hi<lightgrey> there"), "<span class='fg4'>hi</span><span class='fg7'> there</span>");
        assert_eq!(formatted_string_to_html("a << b"), "a &lt; b");
        assert_eq!(formatted_string_to_html("x > y"), "x &gt; y");
        assert_eq!(formatted_string_to_html("<bg:blue>a</bg:blue>&"), "<span class='bg1'>a</span>&amp;");
        assert_eq!(formatted_string_to_html("<red>a</red>b"), "<span class='fg4'>a</span>b");
        assert_eq!(formatted_string_to_html("<foo>"), "&lt;foo&gt;");
    }

    #[test]
    fn text_strips_colours_only() {
        assert_eq!(formatted_string_to_text("<red>a <<b</red> <foo>"), "a <b <foo>");
        assert_eq!(formatted_string_to_text("<bg:blue>x</bg:blue>"), "x");
    }

    #[test]
    fn spans() {
        assert_eq!(
            formatted_string_to_spans("a<red>b<bg:blue>c</red>d</bg:blue><<e"),
            vec![span("a", None, None), span("b", Some(4), None), span("c", Some(4), Some(1)), span("d", None, Some(1)), span("<e", None, None)]
        );
        assert_eq!(formatted_string_to_spans("<nope>"), vec![span("<nope>", None, None)]);
    }

    #[test]
    fn coloured() {
        assert_eq!(coloured_text("a<b", 4, false), "<red>a<<b</red>");
        assert_eq!(coloured_text("<w>a", 2, true), "<green><w>a</green>");
        assert_eq!(coloured_text("a", 16, false), "a");
        assert_eq!(formatted_string_to_text(&coloured_text("a<b", 4, false)), "a<b");
    }
}
