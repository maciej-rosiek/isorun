//! The repository's own documents (ABOUT.md, CHANGELOG.md, STEAM.md) as the app's About & credits,
//! What's new and Add to Steam screens have them (the port of `markdown.ts`).
//!
//! A small reader for what those files use, not a Markdown implementation: headings, paragraphs,
//! bullet lists (with wrapped lines), fenced code, tables, and inline code, emphasis, links and
//! autolinks. Raw HTML and comments are skipped, as is anything that has no place on the page (the
//! logo, screenshots). Links to other files of the repository point at them on GitHub; links
//! within the document are left as words.

/// Where a relative link (`LICENSE`, `ATTRIBUTION.md`) is found.
const REPO_BLOB: &str = "https://github.com/maciej-rosiek/isorun/blob/main/";

#[derive(Clone, Debug, PartialEq)]
pub enum Inline {
    Text(String),
    Code(String),
    Strong(Vec<Inline>),
    Em(Vec<Inline>),
    Link { text: Vec<Inline>, href: String },
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    /// A heading, 2 to 6 (a `#` is an h2: the screen's own name is the h1).
    Heading(u8, Vec<Inline>),
    Para(Vec<Inline>),
    List(Vec<Vec<Inline>>),
    Code(String),
    Table {
        head: Vec<Vec<Inline>>,
        rows: Vec<Vec<Vec<Inline>>>,
    },
}

fn is_autolink_line(line: &str) -> bool {
    // `<https://…>`, `<someone@example.com>`: words, not html
    let Some(rest) = line.strip_prefix('<') else { return false };
    let rest = rest.strip_prefix("https://").or_else(|| rest.strip_prefix("http://")).or_else(|| rest.strip_prefix("mailto:")).unwrap_or(rest);
    let end = rest.find(|c: char| c.is_whitespace() || c == '>').unwrap_or(rest.len());
    let word = &rest[..end];
    rest[end..].starts_with('>') && (word.contains('@') || word.contains(':') || line[1..].starts_with("http"))
}

fn is_bullet(line: &str) -> Option<&str> {
    let t = line.trim_start();
    let rest = t.strip_prefix("- ").or_else(|| t.strip_prefix("* ")).or_else(|| t.strip_prefix("-\t")).or_else(|| t.strip_prefix("*\t"))?;
    Some(rest.trim_start())
}

fn is_table_rule(line: &str) -> bool {
    let t = line.trim_end();
    t.starts_with('|') && t.ends_with('|') && t.len() > 1 && t.chars().all(|c| matches!(c, '|' | ':' | '-' | ' ' | '\t'))
}

/// Read `src`; `drop_title` leaves out the document's own first heading, as the screen it stands
/// on already says what it is.
pub fn parse(src: &str, drop_title: bool) -> Vec<Block> {
    let text = src.replace("\r\n", "\n").replace('\r', "\n");
    let lines: Vec<&str> = text.split('\n').collect();
    let mut out = Vec::new();
    let mut para: Vec<String> = Vec::new();
    let flush = |para: &mut Vec<String>, out: &mut Vec<Block>| {
        if !para.is_empty() {
            out.push(Block::Para(inline(&para.join(" "))));
            para.clear();
        }
    };
    let mut i = 0;
    while i < lines.len() {
        let line = lines[i];
        // a comment, on one line or many
        if line.trim_start().starts_with("<!--") {
            flush(&mut para, &mut out);
            while i < lines.len() && !lines[i].trim_end().ends_with("-->") {
                i += 1;
            }
            i += 1;
            continue;
        }
        // a block of raw html (the logo, the screenshots): nothing of it is drawn here
        if line.starts_with('<') && line[1..].starts_with(|c: char| c.is_ascii_alphabetic() || c == '/') && !is_autolink_line(line) {
            flush(&mut para, &mut out);
            while i < lines.len() && !lines[i].trim().is_empty() {
                i += 1;
            }
            continue;
        }
        if line.trim().is_empty() {
            flush(&mut para, &mut out);
            i += 1;
            continue;
        }
        if line.starts_with("```") {
            flush(&mut para, &mut out);
            let mut code = Vec::new();
            i += 1;
            while i < lines.len() && !lines[i].starts_with("```") {
                code.push(lines[i]);
                i += 1;
            }
            i += 1;
            out.push(Block::Code(code.join("\n")));
            continue;
        }
        let hashes = line.chars().take_while(|c| *c == '#').count();
        if (1..=6).contains(&hashes) && line[hashes..].starts_with(|c: char| c.is_whitespace()) {
            flush(&mut para, &mut out);
            let body = line[hashes..].trim().trim_end_matches('#').trim_end();
            out.push(Block::Heading((hashes as u8 + 1).min(6), inline(body)));
            i += 1;
            continue;
        }
        if is_bullet(line).is_some() {
            flush(&mut para, &mut out);
            let mut items = Vec::new();
            while i < lines.len() {
                let Some(first) = is_bullet(lines[i]) else { break };
                let mut item = vec![first.to_string()];
                i += 1;
                // a wrapped item continues on indented lines up to the next bullet or blank
                while i < lines.len() && lines[i].starts_with(char::is_whitespace) && !lines[i].trim().is_empty() && is_bullet(lines[i]).is_none() {
                    item.push(lines[i].trim().to_string());
                    i += 1;
                }
                items.push(inline(&item.join(" ")));
                // an empty line between items keeps the list together
                if i + 1 < lines.len() && lines[i].trim().is_empty() && is_bullet(lines[i + 1]).is_some() {
                    i += 1;
                }
            }
            out.push(Block::List(items));
            continue;
        }
        if line.starts_with('|') && i + 1 < lines.len() && is_table_rule(lines[i + 1]) {
            flush(&mut para, &mut out);
            let cells = |l: &str| -> Vec<Vec<Inline>> {
                let t = l.trim_end();
                let t = t.strip_prefix('|').unwrap_or(t);
                let t = t.strip_suffix('|').unwrap_or(t);
                t.split('|').map(|c| inline(c.trim())).collect()
            };
            let head = cells(line);
            i += 2;
            let mut rows = Vec::new();
            while i < lines.len() && lines[i].starts_with('|') {
                rows.push(cells(lines[i]));
                i += 1;
            }
            out.push(Block::Table { head, rows });
            continue;
        }
        para.push(line.trim().to_string());
        i += 1;
    }
    flush(&mut para, &mut out);
    if drop_title && matches!(out.first(), Some(Block::Heading(2, _))) {
        out.remove(0);
    }
    out
}

/// A link out of the document: another site or another file of the repository. A link within
/// the document has nowhere to go on this screen: its words stand alone.
fn link(text: Vec<Inline>, href: &str) -> Vec<Inline> {
    if href.starts_with('#') {
        return text;
    }
    let scheme = href.find(':').is_some_and(|c| c > 0 && href[..c].chars().all(|ch| ch.is_ascii_alphabetic()));
    let abs = if scheme { href.to_string() } else { format!("{REPO_BLOB}{}", href.trim_start_matches("./").trim_start_matches('/')) };
    vec![Inline::Link { text, href: abs }]
}

fn is_word(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Inline marks: code, bold, emphasis, links, autolinks and mail; an image is its alt text. Tried
/// at each place in the order the web build's one regular expression tries them.
pub fn inline(text: &str) -> Vec<Inline> {
    let chars: Vec<char> = text.chars().collect();
    let s = |a: usize, b: usize| chars[a..b].iter().collect::<String>();
    let mut out = Vec::new();
    let mut last = 0;
    let mut at = 0;
    while at < chars.len() {
        if let Some((end, made)) = mark_at(&chars, at) {
            if at > last {
                out.push(Inline::Text(s(last, at)));
            }
            out.extend(made);
            last = end;
            at = end;
        } else {
            at += 1;
        }
    }
    if last < chars.len() {
        out.push(Inline::Text(s(last, chars.len())));
    }
    out
}

/// The mark that starts at `at`, if one does: where it ends, and what it makes.
fn mark_at(c: &[char], at: usize) -> Option<(usize, Vec<Inline>)> {
    let s = |a: usize, b: usize| c[a..b].iter().collect::<String>();
    let n = c.len();
    // `code`, any run of backticks
    if c[at] == '`' {
        let run = c[at..].iter().take_while(|x| **x == '`').count();
        let from = at + run;
        let mut e = from + 1;
        while e + run <= n {
            if c[e..e + run].iter().all(|x| *x == '`') {
                return Some((e + run, vec![Inline::Code(s(from, e))]));
            }
            e += 1;
        }
    }
    // ![alt](src): the alt text
    if c[at] == '!' && at + 1 < n && c[at + 1] == '[' {
        if let Some(close) = (at + 2..n).find(|&k| c[k] == ']') {
            if close + 1 < n && c[close + 1] == '(' {
                if let Some(end) = (close + 2..n).find(|&k| c[k] == ')') {
                    return Some((end + 1, vec![Inline::Text(s(at + 2, close))]));
                }
            }
        }
    }
    // [text](href)
    if c[at] == '[' {
        if let Some(close) = (at + 1..n).find(|&k| c[k] == ']') {
            if close > at + 1 && close + 1 < n && c[close + 1] == '(' {
                let mut e = close + 2;
                while e < n && c[e] != ')' && !c[e].is_whitespace() {
                    e += 1;
                }
                if e < n && c[e] == ')' && e > close + 2 {
                    return Some((e + 1, link(inline(&s(at + 1, close)), &s(close + 2, e))));
                }
            }
        }
    }
    if c[at] == '<' {
        let mut e = at + 1;
        while e < n && c[e] != '>' && !c[e].is_whitespace() {
            e += 1;
        }
        if e < n && c[e] == '>' && e > at + 1 {
            let inner = s(at + 1, e);
            // <https://…>, <mailto:…>
            if inner.starts_with("http://") || inner.starts_with("https://") || inner.starts_with("mailto:") {
                let shown = inner.trim_start_matches("mailto:").to_string();
                return Some((e + 1, vec![Inline::Link { text: vec![Inline::Text(shown)], href: inner }]));
            }
            // <someone@example.com>
            if let Some(atsign) = inner.find('@') {
                if atsign > 0 && atsign + 1 < inner.len() && inner.matches('@').count() == 1 {
                    return Some((e + 1, vec![Inline::Link { text: vec![Inline::Text(inner.clone())], href: format!("mailto:{inner}") }]));
                }
            }
        }
    }
    // **strong**
    if c[at] == '*' && at + 1 < n && c[at + 1] == '*' {
        let mut e = at + 3;
        while e + 1 < n {
            if c[e] == '*' && c[e + 1] == '*' {
                return Some((e + 2, vec![Inline::Strong(inline(&s(at + 2, e)))]));
            }
            e += 1;
        }
    }
    // *em* and _em_: not inside a word, not against a space
    for mark in ['*', '_'] {
        if c[at] != mark {
            continue;
        }
        let before_ok = at == 0 || !(is_word(c[at - 1]) || c[at - 1] == mark);
        if !before_ok || at + 1 >= n || c[at + 1].is_whitespace() {
            continue;
        }
        let mut e = at + 2;
        while e < n {
            if c[e] == mark && !c[e - 1].is_whitespace() && (e + 1 >= n || !(is_word(c[e + 1]) || c[e + 1] == mark)) {
                return Some((e + 1, vec![Inline::Em(inline(&s(at + 1, e)))]));
            }
            e += 1;
        }
    }
    None
}

/// The words of a run of inlines, without their marks.
pub fn plain(spans: &[Inline]) -> String {
    spans
        .iter()
        .map(|i| match i {
            Inline::Text(t) | Inline::Code(t) => t.clone(),
            Inline::Strong(x) | Inline::Em(x) => plain(x),
            Inline::Link { text, .. } => plain(text),
        })
        .collect()
}

// ------------------------------------------------------------------ drawing

/// How a document is set: the sizes and colours of `.doc` in styles.css.
#[derive(Clone, Copy)]
pub struct DocStyle {
    /// 1rem in points.
    pub rem: f32,
    pub ink: egui::Color32,
    pub dim: egui::Color32,
    pub code: egui::Color32,
    pub bold: bool,
}

/// Lay out a run of inlines; the link ranges (in chars) come back with it.
pub fn job(spans: &[Inline], size: f32, colour: egui::Color32, style: &DocStyle, wrap: f32) -> (egui::text::LayoutJob, Vec<(std::ops::Range<usize>, String)>) {
    let mut job = egui::text::LayoutJob::default();
    job.wrap.max_width = wrap;
    let mut links = Vec::new();
    let mut chars = 0;
    fn walk(
        spans: &[Inline],
        fmt: egui::TextFormat,
        style: &DocStyle,
        job: &mut egui::text::LayoutJob,
        links: &mut Vec<(std::ops::Range<usize>, String)>,
        chars: &mut usize,
        link: Option<&str>,
    ) {
        for s in spans {
            match s {
                Inline::Text(t) => {
                    let start = *chars;
                    *chars += t.chars().count();
                    if let Some(h) = link {
                        links.push((start..*chars, h.to_string()));
                    }
                    job.append(t, 0.0, fmt.clone());
                }
                Inline::Code(t) => {
                    let mut f = fmt.clone();
                    f.color = style.code;
                    f.font_id.size *= 0.92;
                    let start = *chars;
                    *chars += t.chars().count();
                    if let Some(h) = link {
                        links.push((start..*chars, h.to_string()));
                    }
                    job.append(t, 0.0, f);
                }
                Inline::Strong(x) => {
                    let mut f = fmt.clone();
                    f.font_id.family = isorun_ui::fonts::bold();
                    f.color = style.ink;
                    walk(x, f, style, job, links, chars, link);
                }
                Inline::Em(x) => {
                    let mut f = fmt.clone();
                    f.italics = true;
                    walk(x, f, style, job, links, chars, link);
                }
                Inline::Link { text, href } => {
                    let mut f = fmt.clone();
                    f.color = style.code;
                    f.underline = egui::Stroke::new(1.0, style.code);
                    walk(text, f, style, job, links, chars, Some(href));
                }
            }
        }
    }
    let family = if style.bold { isorun_ui::fonts::bold() } else { egui::FontFamily::Monospace };
    let fmt = egui::TextFormat { font_id: egui::FontId::new(size, family), color: colour, line_height: Some(size * 1.55), ..Default::default() };
    walk(spans, fmt, style, &mut job, &mut links, &mut chars, None);
    (job, links)
}

/// Draw a run of inlines; a click on a link opens it.
pub fn inline_ui(ui: &mut egui::Ui, spans: &[Inline], size: f32, colour: egui::Color32, style: &DocStyle) -> egui::Response {
    let (job, links) = job(spans, size, colour, style, ui.available_width());
    let galley = ui.fonts_mut(|f| f.layout_job(job));
    let (rect, response) = ui.allocate_exact_size(galley.size(), egui::Sense::click());
    let mut over = None;
    if let Some(p) = response.hover_pos() {
        let cursor = galley.cursor_from_pos(p - rect.min);
        over = links.iter().find(|(r, _)| r.contains(&cursor.index.0)).map(|(_, h)| h.clone());
    }
    if over.is_some() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    if response.clicked() {
        if let Some(h) = &over {
            ui.ctx().open_url(egui::OpenUrl::new_tab(h));
        }
    }
    ui.painter().galley(rect.min, galley, colour);
    response
}

/// Draw a document (`.doc`): its blocks down the width it is given, at most 70 characters wide.
pub fn doc_ui(ui: &mut egui::Ui, blocks: &[Block], style: &DocStyle) {
    let rem = style.rem;
    let max = (70.0 * 0.602 * rem).min(ui.available_width());
    ui.set_max_width(max);
    ui.spacing_mut().item_spacing.y = 0.0;
    for (i, b) in blocks.iter().enumerate() {
        match b {
            Block::Heading(level, spans) => {
                let (size, colour) = match level {
                    2 => (1.2 * rem, isorun_ui::text::TERM_COLOURS[14]),
                    3 => (1.05 * rem, style.ink),
                    _ => (rem, style.dim),
                };
                if i > 0 {
                    ui.add_space(1.4 * size);
                }
                let st = DocStyle { bold: true, ..*style };
                inline_ui(ui, spans, size, colour, &st);
                ui.add_space(0.5 * size);
            }
            Block::Para(spans) => {
                inline_ui(ui, spans, rem, style.ink, style);
                ui.add_space(0.9 * rem);
            }
            Block::List(items) => {
                for item in items {
                    ui.add_space(0.25 * rem);
                    ui.horizontal_top(|ui| {
                        ui.spacing_mut().item_spacing.x = 0.0;
                        let w = 1.3 * rem;
                        let (r, _) = ui.allocate_exact_size(egui::vec2(w, rem * 1.55), egui::Sense::hover());
                        ui.painter().text(r.min + egui::vec2(0.35 * rem, rem * 0.78), egui::Align2::LEFT_CENTER, "•", egui::FontId::monospace(rem), style.dim);
                        ui.vertical(|ui| {
                            inline_ui(ui, item, rem, style.ink, style);
                        });
                    });
                    ui.add_space(0.25 * rem);
                }
                ui.add_space(0.9 * rem);
            }
            Block::Code(text) => {
                let size = 0.92 * rem;
                egui::Frame::NONE.fill(egui::Color32::from_white_alpha(13)).inner_margin(egui::Margin::symmetric((0.8 * rem) as i8, (0.6 * rem) as i8)).show(
                    ui,
                    |ui| {
                        ui.set_width(ui.available_width());
                        let mut job = egui::text::LayoutJob::single_section(
                            text.clone(),
                            egui::TextFormat { font_id: egui::FontId::monospace(size), color: style.code, ..Default::default() },
                        );
                        job.wrap.max_width = ui.available_width();
                        ui.label(job);
                    },
                );
                ui.add_space(0.9 * rem);
            }
            Block::Table { head, rows } => {
                let size = 0.92 * rem;
                egui::Grid::new(("doc-table", i)).spacing([0.9 * rem, 0.5 * rem]).show(ui, |ui| {
                    for h in head {
                        let st = DocStyle { bold: true, ..*style };
                        inline_ui(ui, h, size, style.dim, &st);
                    }
                    ui.end_row();
                    for r in rows {
                        for c in r {
                            inline_ui(ui, c, size, style.ink, style);
                        }
                        ui.end_row();
                    }
                });
                ui.add_space(0.9 * rem);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn t(s: &str) -> Inline {
        Inline::Text(s.into())
    }

    #[test]
    fn inline_marks() {
        assert_eq!(inline("a `b` c"), vec![t("a "), Inline::Code("b".into()), t(" c")]);
        assert_eq!(inline("**bold** and *em*"), vec![Inline::Strong(vec![t("bold")]), t(" and "), Inline::Em(vec![t("em")])]);
        assert_eq!(inline("snake_case_name"), vec![t("snake_case_name")]);
        assert_eq!(inline("2 * 3 * 4"), vec![t("2 * 3 * 4")]);
        assert_eq!(inline("see [the guide](docs/x.md)"), vec![t("see "), Inline::Link { text: vec![t("the guide")], href: format!("{REPO_BLOB}docs/x.md") }]);
        assert_eq!(inline("[here](#top)"), vec![t("here")]);
        assert_eq!(inline("<https://a.org/b>"), vec![Inline::Link { text: vec![t("https://a.org/b")], href: "https://a.org/b".into() }]);
        assert_eq!(inline("<me@x.org>"), vec![Inline::Link { text: vec![t("me@x.org")], href: "mailto:me@x.org".into() }]);
        assert_eq!(inline("![logo](a.png) Isorun"), vec![t("logo"), t(" Isorun")]);
    }

    #[test]
    fn blocks() {
        let md = "# Title\n\nIntro line\nwraps.\n\n<p align=center>\n<img src=x>\n</p>\n\n<!-- a\ncomment -->\n## Part\n\n- one\n  more\n\n- two\n\n```\ncode\n```\n\n| a | b |\n|---|---|\n| 1 | 2 |\n";
        let b = parse(md, true);
        assert_eq!(b[0], Block::Para(vec![t("Intro line wraps.")]));
        assert_eq!(b[1], Block::Heading(3, vec![t("Part")]));
        assert_eq!(b[2], Block::List(vec![vec![t("one more")], vec![t("two")]]));
        assert_eq!(b[3], Block::Code("code".into()));
        assert!(matches!(&b[4], Block::Table { head, rows } if head.len() == 2 && rows.len() == 1));
        assert_eq!(b.len(), 5);
    }

    #[test]
    fn the_repository_documents_read() {
        for doc in [crate::docs::ABOUT, crate::docs::STEAM, crate::news::CHANGELOG] {
            let b = parse(doc, true);
            assert!(b.len() > 3);
        }
    }
}
