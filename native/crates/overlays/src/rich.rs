//! Text as the overlays draw it: crawl's formatted strings (`<red>...`), the server's own HTML
//! lines (the CRT screens' `<span class='fg7 bg0'>`), and plain text, as egui layout jobs in the
//! page's font, with `white-space: pre-wrap` (spaces kept, lines wrapped at the box's width) and
//! the line box Chrome gives `line-height: normal`.

use egui::text::{LayoutJob, LayoutSection, TextWrapping};
use egui::{Color32, FontFamily, FontId, TextFormat};
use isorun_ui::text::TERM_COLOURS;
use isorun_webtiles::formatted_string_to_spans;
use once_cell::sync::Lazy;
use regex::Regex;
use std::sync::Arc;

/// A run of text's look.
#[derive(Clone, Debug, PartialEq)]
pub struct TextStyle {
    pub size: f32,
    pub line_height: f32,
    pub colour: Color32,
    pub family: FontFamily,
    /// `font-weight: bold` whatever the screen (headers, level-1 menu rows)
    pub bold: bool,
}

impl TextStyle {
    pub fn new(ctx: &egui::Context, size: f32, colour: Color32) -> Self {
        TextStyle { size, line_height: isorun_ui::widgets::normal_line_height(size), colour, family: isorun_ui::widgets::text_family(ctx), bold: false }
    }
    pub fn bold(mut self) -> Self {
        self.bold = true;
        self.family = isorun_ui::fonts::bold();
        self
    }
    pub fn line(mut self, h: f32) -> Self {
        self.line_height = h;
        self
    }
    pub fn colour(mut self, c: Color32) -> Self {
        self.colour = c;
        self
    }
    pub fn font(&self) -> FontId {
        FontId::new(self.size, self.family.clone())
    }
    fn format(&self, colour: Color32, background: Color32) -> TextFormat {
        TextFormat { font_id: self.font(), color: colour, background, line_height: Some(self.line_height), valign: egui::Align::Center, ..Default::default() }
    }
}

fn job(wrap: f32) -> LayoutJob {
    let mut j = LayoutJob::default();
    j.wrap = TextWrapping { max_width: if wrap > 0.0 { wrap } else { f32::INFINITY }, ..Default::default() };
    j.break_on_newline = true;
    j
}

fn term(c: u8) -> Color32 {
    TERM_COLOURS[c as usize & 15]
}

/// A crawl formatted string; untagged text in the style's colour.
pub fn formatted(text: &str, st: &TextStyle, wrap: f32) -> LayoutJob {
    let mut j = job(wrap);
    for span in formatted_string_to_spans(text) {
        let fg = span.fg.map(term).unwrap_or(st.colour);
        let bg = span.bg.map(term).unwrap_or(Color32::TRANSPARENT);
        j.append(&span.text, 0.0, st.format(fg, bg));
    }
    j
}

/// Plain text in one colour.
pub fn plain(text: &str, st: &TextStyle, wrap: f32) -> LayoutJob {
    let mut j = job(wrap);
    j.append(text, 0.0, st.format(st.colour, Color32::TRANSPARENT));
    j
}

/// Runs of plain text, each in its own colour (the god's powers, a footer with lit words).
pub fn runs(parts: &[(&str, Color32)], st: &TextStyle, wrap: f32) -> LayoutJob {
    let mut j = job(wrap);
    for (t, c) in parts {
        j.append(t, 0.0, st.format(*c, Color32::TRANSPARENT));
    }
    j
}

static HTML_TOKEN: Lazy<Regex> = Lazy::new(|| Regex::new(r#"(?is)<(/?)([a-z]+)([^>]*)>|([^<]+)"#).unwrap());
static CLASS_ATTR: Lazy<Regex> = Lazy::new(|| Regex::new(r#"class\s*=\s*["']([^"']*)["']"#).unwrap());

/// The entities the server escapes, restored.
pub fn unescape(s: &str) -> String {
    s.replace("&lt;", "<").replace("&gt;", ">").replace("&quot;", "\"").replace("&#39;", "'").replace("&nbsp;", "\u{a0}").replace("&amp;", "&")
}

/// A colour run of server HTML: its text and the `fgN` / `bgN` classes over it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct HtmlSpan {
    pub text: String,
    pub fg: Option<u8>,
    pub bg: Option<u8>,
}

/// Server HTML (`<span class='fg7 bg0'>`, `<br>`, entities) as colour runs; other tags are dropped
/// and their text kept. `bg0` is the page's black, which the overlays leave transparent
/// (styles.css `.crt .bg0`).
pub fn html_spans(html: &str) -> Vec<HtmlSpan> {
    let mut out = Vec::new();
    let mut stack: Vec<(Option<u8>, Option<u8>)> = Vec::new();
    let cur = |stack: &Vec<(Option<u8>, Option<u8>)>| {
        let fg = stack.iter().rev().find_map(|s| s.0);
        let bg = stack.iter().rev().find_map(|s| s.1);
        (fg, bg)
    };
    for c in HTML_TOKEN.captures_iter(html) {
        if let Some(t) = c.get(4) {
            let (fg, bg) = cur(&stack);
            out.push(HtmlSpan { text: unescape(t.as_str()), fg, bg });
            continue;
        }
        let closing = &c[1] == "/";
        let tag = c[2].to_ascii_lowercase();
        if tag == "br" {
            let (fg, bg) = cur(&stack);
            out.push(HtmlSpan { text: "\n".into(), fg, bg });
            continue;
        }
        if closing {
            stack.pop();
            continue;
        }
        let mut fg = None;
        let mut bg = None;
        if let Some(cl) = CLASS_ATTR.captures(&c[3]) {
            for class in cl[1].split_whitespace() {
                if let Some(n) = class.strip_prefix("fg").and_then(|n| n.parse::<u8>().ok()) {
                    fg = Some(n);
                } else if let Some(n) = class.strip_prefix("bg").and_then(|n| n.parse::<u8>().ok()) {
                    bg = Some(n);
                }
            }
        }
        if !c[3].trim_end().ends_with('/') {
            stack.push((fg, bg));
        }
    }
    out
}

/// Server HTML as a layout job.
pub fn html(html: &str, st: &TextStyle, wrap: f32) -> LayoutJob {
    let mut j = job(wrap);
    for s in html_spans(html) {
        let fg = s.fg.map(term).unwrap_or(st.colour);
        let bg = s.bg.filter(|&b| b != 0).map(term).unwrap_or(Color32::TRANSPARENT);
        j.append(&s.text, 0.0, st.format(fg, bg));
    }
    j
}

/// Lay a job out.
pub fn galley(ctx: &egui::Context, job: LayoutJob) -> Arc<egui::Galley> {
    let job = prewrap(job);
    ctx.fonts_mut(|f| f.layout_job(job))
}

/// `white-space: pre-wrap` as the browser does it, for text in DejaVu Sans Mono (one size): a line
/// breaks after a run of spaces, which hangs at its end, so the next line starts with the word;
/// a word longer than the line overflows it. (egui's own wrapping carries the spaces over to the
/// next line, which indents it.) Other text is left to egui.
pub fn prewrap(mut job: LayoutJob) -> LayoutJob {
    let width = job.wrap.max_width;
    if !width.is_finite() || job.sections.is_empty() {
        return job;
    }
    let size = job.sections[0].format.font_id.size;
    let mono = |f: &egui::FontFamily| matches!(f, egui::FontFamily::Monospace) || *f == isorun_ui::fonts::bold();
    if job.sections.iter().any(|s| s.format.font_id.size != size || !mono(&s.format.font_id.family) || s.leading_space != 0.0) {
        return job;
    }
    let cols = ((width + 0.01) / isorun_ui::widgets::ch(size)).floor().max(1.0) as usize;
    // where to insert breaks, as byte offsets into the text
    let mut breaks = Vec::new();
    for (line_start, line) in line_spans(&job.text) {
        let chars: Vec<(usize, char)> = line.char_indices().collect();
        let mut start = 0; // char index where the current visual line starts
        let mut i = 0;
        let mut last_break: Option<usize> = None; // char index after a run of spaces
        while i < chars.len() {
            if chars[i].1 == ' ' {
                while i < chars.len() && chars[i].1 == ' ' {
                    i += 1;
                }
                last_break = Some(i);
                continue;
            }
            // a character that does not fit: break at the last run of spaces on this line
            if i - start >= cols {
                if let Some(b) = last_break.filter(|&b| b > start) {
                    breaks.push(line_start + chars.get(b).map_or(line.len(), |c| c.0));
                    start = b;
                    last_break = None;
                    continue;
                }
            }
            i += 1;
        }
    }
    if breaks.is_empty() {
        job.wrap.max_width = f32::INFINITY;
        return job;
    }
    let mut text = String::with_capacity(job.text.len() + breaks.len());
    let mut last = 0;
    for &b in &breaks {
        text.push_str(&job.text[last..b]);
        text.push('\n');
        last = b;
    }
    text.push_str(&job.text[last..]);
    let shift = |x: usize| x + breaks.iter().filter(|&&b| b < x || (b == x && x > 0 && false)).count();
    for s in job.sections.iter_mut() {
        let (a, b) = (s.byte_range.start.0, s.byte_range.end.0);
        // a break at a section's start belongs before it
        let na = a + breaks.iter().filter(|&&k| k <= a).count();
        let nb = b + breaks.iter().filter(|&&k| k < b).count();
        let _ = shift;
        s.byte_range = na.min(nb).into()..nb.into();
    }
    // breaks at a section boundary land in no section when counted `<=` for the start and `<` for
    // the end: give the newline to the section before
    job.text = text;
    fix_gaps(&mut job);
    job.wrap.max_width = f32::INFINITY;
    job
}

/// Lines of a text with the byte offset each starts at.
fn line_spans(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut at = 0;
    for l in text.split('\n') {
        out.push((at, l));
        at += l.len() + 1;
    }
    out
}

/// Close any gap between consecutive sections (a newline inserted at their boundary).
fn fix_gaps(job: &mut LayoutJob) {
    let n = job.sections.len();
    for k in 0..n {
        let next = if k + 1 < n { job.sections[k + 1].byte_range.start.0 } else { job.text.len() };
        if job.sections[k].byte_range.end.0 < next {
            job.sections[k].byte_range.end = next.into();
        }
    }
    if let Some(first) = job.sections.first_mut() {
        first.byte_range.start = 0.into();
    }
}

/// The width a job takes unwrapped: its longest line (`max-content`).
pub fn natural_width(ctx: &egui::Context, mut job: LayoutJob) -> f32 {
    job.wrap.max_width = f32::INFINITY;
    galley(ctx, job).size().x
}

/// Change the colour of every section (a row lit by the cursor, a heading in white).
pub fn recolour(mut job: LayoutJob, f: impl Fn(Color32) -> Color32) -> LayoutJob {
    for LayoutSection { format, .. } in job.sections.iter_mut() {
        format.color = f(format.color);
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prewrap_hangs_spaces() {
        let st = TextStyle { size: 10.0, line_height: 12.0, colour: Color32::WHITE, family: egui::FontFamily::Monospace, bold: false };
        let w = isorun_ui::widgets::ch(10.0) * 10.0;
        let j = prewrap(formatted("<red>abc def</red> ghijkl mno", &st, w));
        assert_eq!(j.text, "abc def \nghijkl mno");
        assert_eq!(j.sections.last().unwrap().byte_range.end.0, j.text.len());
        let long = prewrap(plain("abcdefghijklmn x", &st, w));
        assert_eq!(long.text, "abcdefghijklmn \nx");
    }

    #[test]
    fn server_html_spans() {
        let s = html_spans("  <span class=\"fg8 bg0\">b - Maces &amp; Flails </span><span class='fg3'>0.8</span>");
        assert_eq!(s, vec![HtmlSpan { text: "  ".into(), fg: None, bg: None }, HtmlSpan { text: "b - Maces & Flails ".into(), fg: Some(8), bg: Some(0) }, HtmlSpan { text: "0.8".into(), fg: Some(3), bg: None }]);
        assert_eq!(html_spans("a<br>b").iter().map(|s| s.text.as_str()).collect::<String>(), "a\nb");
    }
}
