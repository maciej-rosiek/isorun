//! What crawl is asking, read from the game state (the part of context.ts the overlays need):
//! the mode the screen is in ([`derive_mode`]), and the prompt up now with its answers
//! ([`parse_prompt`]), which the prompt card draws as chips.

use crate::menu_nav::char_at;
use isorun_webtiles::{formatted_string_to_spans, formatted_string_to_text, GameState, MenuState, Msgch, MouseMode, Phase, UiState};
use once_cell::sync::Lazy;
use regex::Regex;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Command,
    /// inputMode MACRO: the server is capturing a macro, keys go through raw
    Macro,
    More,
    Menu,
    Popup,
    Newgame,
    Targeting,
    Levelmap,
    Text,
    Yesno,
    Prompt,
    Crt,
    Dialog,
    Lobby,
    Spectating,
    Ended,
}

impl Mode {
    /// Modes whose top overlay is driven by the focus layer rather than by mode-specific keys.
    /// A crt menu (the skills screen) has no server hover: the focus layer's cursor is the only one.
    pub fn is_focus(self, crt_menu: bool) -> bool {
        matches!(self, Mode::Popup | Mode::Newgame | Mode::Crt | Mode::Yesno | Mode::Prompt | Mode::Dialog | Mode::Ended) || (self == Mode::Menu && crt_menu)
    }
}

/// One answer of a prompt. `held`: what the letter holds now (a letter picker over the
/// inventory). `colour`: the terminal colour the message pane painted the option's word in.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PromptOption {
    pub hotkey: String,
    pub label: String,
    pub held: Option<String>,
    pub colour: Option<u8>,
}

impl PromptOption {
    pub fn new(hotkey: &str, label: &str) -> Self {
        PromptOption { hotkey: hotkey.into(), label: label.into(), ..Default::default() }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Letters {
    Item,
    Spell,
    Ability,
}

#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct ParsedPrompt {
    pub text: String,
    pub options: Vec<PromptOption>,
    pub yesno: bool,
    /// the answer crawl's own Enter gives (a yes/no's default), when it has one
    pub default: Option<String>,
    /// A letter picker: the prompt wants one letter, and the options are every inventory letter
    /// (adjust.cc: "Adjust to which letter?").
    pub letters: Option<Letters>,
    /// the letter being moved, named on the line before the prompt: the cursor starts there
    pub from: Option<String>,
    /// Escape means something to this prompt; false only for the stat-gain prompt, whose loop
    /// ignores Escape and Enter outright (player-stats.cc `attribute_increase`).
    pub cancel: bool,
    pub stat_gain: bool,
}

/// A yes/no's answers when nothing names them: prompt.cc `yesno` takes Y and N, whatever the rc.
pub fn default_yesno() -> ParsedPrompt {
    ParsedPrompt { options: vec![PromptOption::new("Y", "Yes"), PromptOption::new("N", "No")], yesno: true, cancel: true, ..Default::default() }
}

/// the message pane's untagged text is the page's white (styles.css `.messages`)
pub const LOG_DEFAULT_COLOUR: u8 = 15;

/// The colour the message pane paints the character at `at` (in characters) of a log line's
/// plain text in: the innermost tag over it, else the pane's white.
pub fn log_colour_at(formatted: &str, at: usize) -> u8 {
    let mut n = 0;
    for s in formatted_string_to_spans(formatted) {
        n += s.text.chars().count();
        if at < n {
            return s.fg.unwrap_or(LOG_DEFAULT_COLOUR);
        }
    }
    LOG_DEFAULT_COLOUR
}

pub fn derive_mode(state: &GameState) -> Mode {
    match state.phase {
        Phase::Lobby => return Mode::Lobby,
        Phase::Ended => return Mode::Ended,
        Phase::Watching => return Mode::Spectating,
        _ => {}
    }
    if state.messages.more {
        return Mode::More;
    }
    if state.text_input.is_some() {
        return Mode::Text;
    }
    if state.dialog.is_some() {
        return Mode::Dialog;
    }
    if state.input_mode == MouseMode::MORE {
        return Mode::More;
    }
    // prompt.cc `yesno`, however crawl asks it: one card answers them all
    if state.input_mode == MouseMode::YESNO {
        return Mode::Yesno;
    }
    // an overlay a `ui_cutoff` hid is not up
    if let Some(p) = state.ui.last() {
        if !p.hidden {
            return if p.kind == "newgame-choice" { Mode::Newgame } else { Mode::Popup };
        }
    }
    if state.menus.iter().any(|m| !m.hidden) {
        return Mode::Menu;
    }
    if state.ui_state == UiState::CRT {
        return Mode::Crt;
    }
    if state.ui_state == UiState::VIEW_MAP {
        return Mode::Levelmap;
    }
    if state.input_mode == MouseMode::PROMPT {
        return Mode::Prompt;
    }
    if matches!(state.input_mode, MouseMode::TARGET | MouseMode::TARGET_DIR | MouseMode::TARGET_PATH) {
        return Mode::Targeting;
    }
    if state.input_mode == MouseMode::MACRO {
        return Mode::Macro;
    }
    if state.input_mode == MouseMode::NORMAL && raw_key_prompt(state) {
        return Mode::Prompt;
    }
    Mode::Command
}

/// A prompt read with a bare `getch_ck`, outside any `mouse_control` (the faded altar): its
/// prompt-channel line is the last thing printed, and nothing else has changed the mode since.
fn raw_key_prompt(state: &GameState) -> bool {
    state.messages.lines.last().is_some_and(|l| l.channel == Some(Msgch::PROMPT))
}

fn re(s: &str) -> Regex {
    Regex::new(s).unwrap()
}

static HOTKEY_RE: Lazy<Regex> = Lazy::new(|| re(r"\(([A-Za-z0-9?*!.,])\)"));
/// items.cc `pickup`: "Pick up X? ((y)es/(n)o/(a)ll/(m)enu/*?g,/q)" reads its own keys, lowercase
static PICKUP_YESNO_RE: Lazy<Regex> = Lazy::new(|| re(r"\(y\)es/\(n\)o"));
/// prompt.cc `yesno`: what it prints after a wrong key; the question is the line before
static YESNO_NAG_RE: Lazy<Regex> = Lazy::new(|| re(r"\[Y\]es or \[N\]o(, or \[A\]lways)? only, please\.$"));
static LETTER_RE: Lazy<Regex> = Lazy::new(|| re(r"^Adjust to which letter\?"));
static LETTER_KIND_RE: Lazy<Regex> = Lazy::new(|| re(r"^Adjust which (spell|ability)\?"));
static LETTER_FROM_RE: Lazy<Regex> = Lazy::new(|| re(r"^([A-Za-z]) - "));
static DIGITS_RE: Lazy<Regex> = Lazy::new(|| re(r"\((\d)-(\d)[,)]"));
static LISTED_RE: Lazy<Regex> = Lazy::new(|| re(r"\(([A-Za-z0-9])\) (\S+)"));
static TRAVEL_PROMPT_RE: Lazy<Regex> = Lazy::new(|| re(r"^\s*Where to\?"));
static ECU_ALTAR_RE: Lazy<Regex> = Lazy::new(|| re(r"^This altar belongs to "));
static STAT_GAIN_RE: Lazy<Regex> = Lazy::new(|| re(r"^Increase \([Ss]\)trength, \([Ii]\)ntelligence, or \([Dd]\)exterity\?"));
static YESNO_DEFAULT_RE: Lazy<Regex> = Lazy::new(|| re(r"\((?:(Y)/n|y/(N))\)"));
static LEAD_WORD_RE: Lazy<Regex> = Lazy::new(|| re(r"\S*\([A-Za-z0-9?*!.,]\)\S*"));
static ONLY_HINT_RE: Lazy<Regex> = Lazy::new(|| re(r"^\(.*\)$"));

fn lists_choices(text: &str) -> bool {
    LISTED_RE.is_match(text)
}

/// The named keys a prompt spells out, as the key to send: Tab and Enter are not text.
fn named_key(k: &str) -> &str {
    match k {
        "Tab/Enter" | "Tab" => "\t",
        "Enter" => "\r",
        other => other,
    }
}

const KEYED_SINGLE: &str = "!?*.<>_^~&%/#:";

/// One `k - label` choice (context.ts KEYED_RE, which needs a lookahead the regex crate lacks):
/// `(?:^|[\s(,])(KEY) - ([^,()]+?)(?: \([^()]*\))?(?=\s{2,}|[,)]|\s*$)`, run globally. Returns
/// (match start, match end, key, label), offsets in characters.
fn keyed_matches(text: &str) -> Vec<(usize, usize, String, String)> {
    let c: Vec<char> = text.chars().collect();
    let n = c.len();
    let lookahead = |p: usize| -> bool {
        // \s{2,} | [,)] | \s*$
        if p + 1 < n + 1 && p < n && (c[p] == ',' || c[p] == ')') {
            return true;
        }
        let mut q = p;
        while q < n && c[q].is_whitespace() {
            q += 1;
        }
        q - p >= 2 || q == n
    };
    // the optional " (...)" after the label, returning where it ends
    let paren = |p: usize| -> Option<usize> {
        if p + 1 < n && c[p] == ' ' && c[p + 1] == '(' {
            let mut q = p + 2;
            while q < n && c[q] != '(' && c[q] != ')' {
                q += 1;
            }
            if q < n && c[q] == ')' {
                return Some(q + 1);
            }
        }
        None
    };
    let try_at = |s: usize| -> Option<(usize, String, String)> {
        // the prefix: start of text, or one of \s ( ,
        let (key_start, prefix_ok) = if s == 0 { (0, true) } else { (s, false) };
        let _ = prefix_ok;
        let mut keys: Vec<String> = Vec::new();
        if key_start < n {
            let k = c[key_start];
            if k.is_ascii_alphanumeric() || KEYED_SINGLE.contains(k) {
                keys.push(k.to_string());
            }
        }
        for w in ["Tab/Enter", "Enter", "Tab"] {
            let wc: Vec<char> = w.chars().collect();
            if c.len() >= key_start + wc.len() && c[key_start..key_start + wc.len()] == wc[..] {
                keys.push(w.to_string());
            }
        }
        for key in keys {
            let after = key_start + key.chars().count();
            if !(after + 3 <= n && c[after] == ' ' && c[after + 1] == '-' && c[after + 2] == ' ') {
                continue;
            }
            let ls = after + 3;
            // lazy label: the shortest that lets the rest match
            let mut e = ls + 1;
            while e <= n && !matches!(c[e - 1], ',' | '(' | ')') {
                if let Some(pe) = paren(e) {
                    if lookahead(pe) {
                        return Some((pe, key, c[ls..e].iter().collect()));
                    }
                }
                if lookahead(e) {
                    return Some((e, key, c[ls..e].iter().collect()));
                }
                e += 1;
            }
        }
        None
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < n {
        // a match starts at 0 with no prefix, or at a separator char followed by the key
        if i == 0 {
            if let Some((end, key, label)) = try_at(0) {
                out.push((0, end, key, label));
                i = end;
                continue;
            }
        }
        if c[i].is_whitespace() || c[i] == '(' || c[i] == ',' {
            if let Some((end, key, label)) = try_at(i + 1) {
                out.push((i, end, key, label));
                i = end;
                continue;
            }
        }
        i += 1;
    }
    out
}

fn add(options: &mut Vec<PromptOption>, hotkey: &str, label: &str, colour: Option<u8>) {
    let hotkey = named_key(hotkey);
    if options.iter().any(|o| o.hotkey == hotkey) {
        return;
    }
    let label = label.trim().trim_end_matches(['.', ',']);
    let mut lc = label.chars();
    let label = match lc.next() {
        Some(f) => f.to_uppercase().collect::<String>() + lc.as_str(),
        None => hotkey.to_string(),
    };
    options.push(PromptOption { hotkey: hotkey.into(), label, held: None, colour });
}

/// A prompt whose choices are not on its line as `(k)ey` words: they are spelled `k - label` in
/// its parenthesis or on the lines after it, or listed as `(K) Name` there, or on the
/// prompt-channel lines just before it (a message-pane menu).
fn listed_prompt(lines: &[(String, Option<i64>)]) -> Option<ParsedPrompt> {
    let at = (lines.len().saturating_sub(8)..lines.len()).rev().find(|&i| lines[i].1 == Some(Msgch::PROMPT))?;
    let mut text = formatted_string_to_text(&lines[at].0);
    // a prompt line that is only its hint ("(? for menu, Esc to cancel)") asks nothing: the question is the line above
    if ONLY_HINT_RE.is_match(text.trim()) && at > 0 && lines[at - 1].1 == Some(Msgch::PROMPT) {
        text = formatted_string_to_text(&lines[at - 1].0);
    }
    let mut options = Vec::new();
    let keyed = |options: &mut Vec<PromptOption>, line: &str| {
        let t = formatted_string_to_text(line);
        for (start, end, key, label) in keyed_matches(&t) {
            let _ = start;
            add(options, &key, &label, Some(log_colour_at(line, end - label.chars().count())));
        }
    };
    let listed = |options: &mut Vec<PromptOption>, line: &str| {
        let t = formatted_string_to_text(line);
        for c in LISTED_RE.captures_iter(&t) {
            let end = char_at(&t, c.get(0).unwrap().end());
            add(options, &c[1], &c[2], Some(log_colour_at(line, end - c[2].chars().count())));
        }
    };
    // a menu in the message pane: its rows are the prompt-channel lines just before the prompt
    let mut first = at;
    while first > 0 && lines[first - 1].1 == Some(Msgch::PROMPT) && lists_choices(&formatted_string_to_text(&lines[first - 1].0)) {
        first -= 1;
    }
    for l in &lines[first..at] {
        listed(&mut options, &l.0);
    }
    listed(&mut options, &lines[at].0);
    if TRAVEL_PROMPT_RE.is_match(&text) {
        add(&mut options, "<", "Up the stairs", None);
        add(&mut options, ">", "Down the stairs", None);
    }
    if ECU_ALTAR_RE.is_match(&text) {
        add(&mut options, "Enter", "Convert", None);
    }
    if let Some(d) = DIGITS_RE.captures(&text) {
        let (a, b): (u32, u32) = (d[1].parse().unwrap(), d[2].parse().unwrap());
        for n in a..=b {
            add(&mut options, &n.to_string(), &n.to_string(), None);
        }
    }
    keyed(&mut options, &lines[at].0);
    for l in &lines[at + 1..] {
        keyed(&mut options, &l.0);
        listed(&mut options, &l.0);
    }
    (!options.is_empty()).then(|| ParsedPrompt { text, options, cancel: true, ..Default::default() })
}

/// Every inventory letter in slot order (invent.h `index_to_letter`): a-z, then A-Z.
const INVENTORY_LETTERS: &str = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";

/// The letter picker for a prompt that wants one letter.
fn letter_prompt(state: &GameState, i: usize, text: &str) -> ParsedPrompt {
    let lines = &state.messages.lines;
    let mut kind = text.contains("for menu").then_some(Letters::Item);
    let mut from = None;
    for j in (i.saturating_sub(6)..i).rev() {
        let t = formatted_string_to_text(&lines[j].text);
        if from.is_none() {
            if let Some(f) = LETTER_FROM_RE.captures(&t) {
                from = Some(f[1].to_string());
            }
        }
        if let Some(k) = LETTER_KIND_RE.captures(&t) {
            if kind.is_none() {
                kind = Some(if &k[1] == "spell" { Letters::Spell } else { Letters::Ability });
            }
            break;
        }
    }
    let kind = kind.unwrap_or(Letters::Item);
    let options = INVENTORY_LETTERS
        .chars()
        .enumerate()
        .map(|(slot, ch)| {
            let held = if kind == Letters::Item { state.player.inv.get(&(slot as i64)).and_then(|it| it.name.clone()).filter(|n| !n.is_empty()) } else { None };
            PromptOption { hotkey: ch.to_string(), label: ch.to_string(), held, colour: None }
        })
        .collect();
    ParsedPrompt { text: text.into(), options, cancel: true, letters: Some(kind), from, ..Default::default() }
}

/// When a choice prompt is nothing but its options ("Increase (S)trength, (I)ntelligence, or
/// (D)exterity?"), the answer chips are the question. This returns what is left of the text once
/// the option words, the punctuation and the "or"/"and" between them go: at most one word, the
/// verb the chips complete ("Increase"), or "" when nothing is. None when more remains.
pub fn prompt_lead(text: &str) -> Option<String> {
    let stripped = LEAD_WORD_RE.replace_all(text, " ");
    let stripped: String = stripped.chars().map(|c| if ",.?!:;".contains(c) { ' ' } else { c }).collect();
    let rest: Vec<&str> = stripped.split_whitespace().filter(|w| !w.eq_ignore_ascii_case("or") && !w.eq_ignore_ascii_case("and")).collect();
    (rest.len() <= 1).then(|| rest.concat())
}

/// A yes/no crawl asks with a menu of its own (tag `prompt`), or a question in the footer of the
/// menu that is up (the shop's "Purchase items for 40 gold? (y/N)").
fn menu_yesno(menu: &MenuState) -> Option<ParsedPrompt> {
    if menu.tag == "prompt" {
        let mut options = Vec::new();
        let mut dflt = None;
        static LEAD: Lazy<Regex> = Lazy::new(|| re(r"^\s*\S\s+-\s+"));
        for (i, it) in menu.items.iter().enumerate() {
            let Some(it) = it else { continue };
            let Some(&hk) = it.hotkeys.as_ref().and_then(|h| h.first()) else { continue };
            let k = char::from_u32(hk as u32).map(String::from).unwrap_or_default();
            let label = LEAD.replace(&formatted_string_to_text(&it.text), "").trim().to_string();
            options.push(PromptOption { hotkey: k.clone(), label: if label.is_empty() { k.clone() } else { label }, ..Default::default() });
            if i as i64 == menu.last_hovered {
                dflt = Some(k);
            }
        }
        if options.is_empty() {
            return None;
        }
        return Some(ParsedPrompt { text: formatted_string_to_text(&menu.title.text).trim().into(), options, yesno: true, cancel: true, default: dflt, ..Default::default() });
    }
    let text = formatted_string_to_text(&menu.more).trim().to_string();
    let d = YESNO_DEFAULT_RE.captures(&text)?;
    let dflt = if d.get(1).is_some() { "Y" } else { "N" };
    Some(ParsedPrompt { text, default: Some(dflt.into()), ..default_yesno() })
}

/// The prompt up now, when the server says one is (input mode YESNO or PROMPT): message text
/// alone never makes a prompt. Only the prompt line itself; a line with no `(k)ey` words may still
/// spell its choices another way (`listed_prompt`), or want a bare letter (`letter_prompt`).
pub fn parse_prompt(state: &GameState) -> Option<ParsedPrompt> {
    let yesno_mode = state.input_mode == MouseMode::YESNO;
    if yesno_mode {
        if let Some(p) = state.menus.last().and_then(menu_yesno) {
            return Some(p);
        }
    }
    let prompt_mode = state.input_mode == MouseMode::PROMPT || (state.input_mode == MouseMode::NORMAL && raw_key_prompt(state));
    if !yesno_mode && !prompt_mode {
        return None;
    }
    let lines = &state.messages.lines;
    for i in (lines.len().saturating_sub(3)..lines.len()).rev() {
        let l = &lines[i];
        // a `(D) Dungeon` list printed around a prompt is not the prompt
        if l.channel.is_some_and(|c| c != Msgch::PROMPT) {
            continue;
        }
        let text = formatted_string_to_text(&l.text);
        if lists_choices(&text) {
            continue;
        }
        // prompt.cc `yesno` prints the question bare, and uppercase is right for every call, so
        // the answers are Y and N. Pickup's prompt is not `yesno` and takes only lowercase.
        if yesno_mode && !YESNO_NAG_RE.is_match(&text) {
            let options = if PICKUP_YESNO_RE.is_match(&text) { vec![PromptOption::new("y", "Yes"), PromptOption::new("n", "No")] } else { default_yesno().options };
            let default = YESNO_DEFAULT_RE.captures(&text).map(|d| if d.get(1).is_some() { "Y".to_string() } else { "N".to_string() });
            return Some(ParsedPrompt { text, options, yesno: true, cancel: true, default, ..Default::default() });
        }
        if !prompt_mode {
            continue;
        }
        if LETTER_RE.is_match(&text) {
            return Some(letter_prompt(state, i, &text));
        }
        let chars: Vec<char> = text.chars().collect();
        let mut options = Vec::new();
        for c in HOTKEY_RE.captures_iter(&text) {
            // label: the word containing the hotkey, parens and all, so a chip reads exactly as
            // the prompt does in the message log ("(S)trength"); only the punctuation goes
            let at = char_at(&text, c.get(0).unwrap().start());
            let start = chars[..=at.min(chars.len().saturating_sub(1))].iter().rposition(|&ch| ch == ' ').map_or(0, |p| p + 1);
            let end = chars[at..].iter().position(|&ch| ch == ' ').map_or(chars.len(), |p| at + p);
            let word: String = chars[start..end].iter().collect();
            let label = word.trim_end_matches([',', '.', '?', '!', ':', ';']);
            options.push(PromptOption { hotkey: c[1].to_string(), label: if label.is_empty() { c[1].to_string() } else { label.to_string() }, held: None, colour: Some(log_colour_at(&l.text, start)) });
        }
        if !options.is_empty() {
            let stat_gain = STAT_GAIN_RE.is_match(&text);
            return Some(ParsedPrompt { text, options, cancel: !stat_gain, stat_gain, ..Default::default() });
        }
    }
    if prompt_mode {
        let ls: Vec<(String, Option<i64>)> = lines.iter().map(|l| (l.text.clone(), l.channel)).collect();
        return listed_prompt(&ls);
    }
    // a yes/no whose question the log does not show still has its two answers
    yesno_mode.then(default_yesno)
}

/// The new-game screen's own button back: Backspace's, or a Space that changes the other choice.
pub fn newgame_back(data: &serde_json::Map<String, serde_json::Value>) -> Option<i32> {
    let buttons = data.get("sub-items")?.get("buttons")?.as_array()?;
    let said = |b: &serde_json::Value| {
        let mut s = b.get("label").and_then(|v| v.as_str()).unwrap_or("").to_string();
        for l in b.get("labels").and_then(|v| v.as_array()).into_iter().flatten() {
            s.push(' ');
            s.push_str(l.as_str().unwrap_or(""));
        }
        formatted_string_to_text(&s)
    };
    let hk = |b: &serde_json::Value| b.get("hotkey").and_then(|v| v.as_i64());
    if buttons.iter().any(|b| hk(b) == Some(8)) {
        return Some(8);
    }
    if buttons.iter().any(|b| hk(b) == Some(32) && said(b).contains("Space - Change ")) {
        return Some(32);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use isorun_webtiles::GameMessage;

    fn state(mode: i64, lines: &[(&str, Option<i64>)]) -> GameState {
        let mut st = GameState { phase: Phase::Playing, input_mode: mode, ..Default::default() };
        st.messages.lines = lines.iter().map(|(t, c)| GameMessage { text: t.to_string(), channel: *c, ..Default::default() }).collect();
        st
    }
    fn labels(p: &ParsedPrompt) -> Vec<String> {
        p.options.iter().map(|o| format!("{}={}", o.hotkey.replace('\t', "Tab").replace('\r', "Enter"), o.label)).collect()
    }
    const P: Option<i64> = Some(2);

    #[test]
    fn modes() {
        let mut st = state(MouseMode::COMMAND, &[]);
        assert_eq!(derive_mode(&st), Mode::Command);
        st.input_mode = MouseMode::YESNO;
        assert_eq!(derive_mode(&st), Mode::Yesno);
        st.messages.more = true;
        assert_eq!(derive_mode(&st), Mode::More);
        st.messages.more = false;
        st.input_mode = MouseMode::NORMAL;
        st.messages.lines.push(GameMessage { text: "This altar belongs to (a) Trog".into(), channel: P, ..Default::default() });
        assert_eq!(derive_mode(&st), Mode::Prompt);
        st.ui.push(isorun_webtiles::Popup { kind: "newgame-choice".into(), ..Default::default() });
        assert_eq!(derive_mode(&st), Mode::Newgame);
        st.ui[0].hidden = true;
        assert_eq!(derive_mode(&st), Mode::Prompt);
        assert!(Mode::Popup.is_focus(false) && !Mode::Menu.is_focus(false) && Mode::Menu.is_focus(true));
    }

    #[test]
    fn yesno_prompts() {
        let p = parse_prompt(&state(MouseMode::YESNO, &[("Really quit? (y/N)", P)])).unwrap();
        assert!(p.yesno && p.cancel);
        assert_eq!(labels(&p), ["Y=Yes", "N=No"]);
        assert_eq!(p.default.as_deref(), Some("N"));
        assert_eq!(p.text, "Really quit? (y/N)");
        let pick = parse_prompt(&state(MouseMode::YESNO, &[("Pick up a dagger? ((y)es/(n)o/(a)ll/(m)enu/*?g,/q)", P)])).unwrap();
        assert_eq!(labels(&pick), ["y=Yes", "n=No"]);
        // the nag is not the question
        let nag = parse_prompt(&state(MouseMode::YESNO, &[("Leave the dungeon?", P), ("[Y]es or [N]o only, please.", P)])).unwrap();
        assert_eq!(nag.text, "Leave the dungeon?");
        // nothing named: the two answers anyway
        assert_eq!(parse_prompt(&state(MouseMode::YESNO, &[("You hit it.", Some(0))])), Some(default_yesno()));
        // command mode: never a prompt
        assert_eq!(parse_prompt(&state(MouseMode::COMMAND, &[("Really? (y/N)", P)])), None);
    }

    #[test]
    fn menu_yesno_prompts() {
        let m = crate::menu_nav::tests::fixture_menu(include_str!("../../../../apps/isorun/test/fixtures/menus/menu-prompt-1.json"));
        let mut st = state(MouseMode::YESNO, &[]);
        st.menus.push(m);
        let p = parse_prompt(&st).unwrap();
        assert!(p.yesno);
        assert!(!p.options.is_empty());
        let mut shop = MenuState { tag: "shop".into(), more: "Purchase items for 40 gold? (y/N)".into(), ..Default::default() };
        st.menus = vec![shop.clone()];
        let p = parse_prompt(&st).unwrap();
        assert_eq!((p.default.as_deref(), p.text.as_str()), (Some("N"), "Purchase items for 40 gold? (y/N)"));
        shop.more = "(Y/n)".into();
        st.menus = vec![shop];
        assert_eq!(parse_prompt(&st).unwrap().default.as_deref(), Some("Y"));
    }

    #[test]
    fn choice_prompts() {
        let p = parse_prompt(&state(MouseMode::PROMPT, &[("You have reached level 2!", Some(0)), ("<cyan>Increase (S)trength, (I)ntelligence, or (D)exterity? </cyan>", P)])).unwrap();
        assert_eq!(labels(&p), ["S=(S)trength", "I=(I)ntelligence", "D=(D)exterity"]);
        assert!(p.stat_gain && !p.cancel);
        assert_eq!(p.options[0].colour, Some(3));
        assert_eq!(prompt_lead(&p.text).as_deref(), Some("Increase"));
        assert_eq!(prompt_lead("Really (q)uit the game?"), None);
        assert_eq!(prompt_lead("(a)ll, (n)one"), Some(String::new()));
        let wield = parse_prompt(&state(MouseMode::PROMPT, &[("Wield which item? (- for none, * to show all)", P)]));
        // `-` and `*` are hints written as `x for y`: not `k - label`, nor `(k)ey` words
        assert_eq!(wield, None);
    }

    #[test]
    fn keyed_and_listed_prompts() {
        let p = parse_prompt(&state(MouseMode::PROMPT, &[("Waypoint (0-9, D - delete waypoint)", P)])).unwrap();
        assert_eq!(labels(&p)[..3], ["0=0", "1=1", "2=2"]);
        assert!(labels(&p).contains(&"D=Delete waypoint".to_string()));
        let shout = parse_prompt(&state(MouseMode::PROMPT, &[("What would you like to say?", P), (" t - Shout!", Some(0)), ("r - Retreat!             s - Stop attacking.", Some(0))])).unwrap();
        assert_eq!(labels(&shout), ["t=Shout!", "r=Retreat!", "s=Stop attacking"]);
        let swap = parse_prompt(&state(MouseMode::PROMPT, &[("Which ring to remove? < or c - a ring of protection from fire (worn)", P)])).unwrap();
        assert_eq!(labels(&swap), ["c=A ring of protection from fire"]);
        let travel = parse_prompt(&state(MouseMode::PROMPT, &[("(D) Dungeon        (T) Temple", P), ("Where to? (Tab/Enter - D:3, ? - help) ", P)])).unwrap();
        assert_eq!(labels(&travel), ["D=Dungeon", "T=Temple", "<=Up the stairs", ">=Down the stairs", "Tab=D:3", "?=Help"]);
        let altar = parse_prompt(&state(MouseMode::NORMAL, &[("This altar belongs to (a) Trog, (b) Okawaru or (c) Yredelemnul.", P)])).unwrap();
        assert_eq!(labels(&altar), ["a=Trog", "b=Okawaru", "c=Yredelemnul", "Enter=Convert"]);
    }

    #[test]
    fn letter_prompts() {
        let mut st = state(MouseMode::PROMPT, &[("c - a +0 dagger (weapon)", Some(0)), ("Adjust to which letter? (? for menu, Esc to quit)", P)]);
        st.player.inv.insert(0, isorun_webtiles::InvItem { slot: 0, name: Some("a +0 club".into()), ..Default::default() });
        let p = parse_prompt(&st).unwrap();
        assert_eq!(p.letters, Some(Letters::Item));
        assert_eq!(p.from.as_deref(), Some("c"));
        assert_eq!(p.options.len(), 52);
        assert_eq!(p.options[0].held.as_deref(), Some("a +0 club"));
        let sp = parse_prompt(&state(MouseMode::PROMPT, &[("Adjust which spell?", P), ("a - Magic Dart", Some(0)), ("Adjust to which letter?", P)])).unwrap();
        assert_eq!((sp.letters, sp.from.as_deref()), (Some(Letters::Spell), Some("a")));
    }

    #[test]
    fn keyed_scanner() {
        let m = keyed_matches("(. - D:3, ? - help, ! - show branch list)");
        assert_eq!(m.iter().map(|x| (x.2.as_str(), x.3.as_str())).collect::<Vec<_>>(), [(".", "D:3"), ("?", "help"), ("!", "show branch list")]);
        assert!(keyed_matches("Wield which item? (- for none)").is_empty());
    }
}
