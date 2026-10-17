//! Frame timing (the port of `perf.ts`), for finding hitches where no profiler reaches (a Steam
//! Deck in Gaming Mode). On with `--perf` on the command line (`?perf` on the web).
//!
//! The frame loop marks its sections (`begin`, `mark`, `end`), and the profile keeps a ring of
//! recent frames: how far apart they arrived, and how long each section took. Everything in
//! the readout is of that window and nothing older, which is what makes it readable move by
//! move: frames age out by time (WINDOW_MS) as well as by count, the worst frame is the worst of
//! the window rather than of the session, and a section that has stopped firing leaves the
//! readout. Age is counted from the newest frame, not from the clock, so a report read while the
//! game rests still shows the turn that just happened.
//!
//! The wire is measured alongside it (`NetProfile`), because half of what a player calls
//! slowness is the server's answer and no frame counter shows it, and the two together make one
//! verdict (`verdict`): the one line that says whether what is on the screen is good.
//!
//! A tap on the pane writes the log (a sample a second) to a file, the only way off a device
//! with no console.

use egui::{Color32, FontId};
use web_time::Instant;

/// most frames kept for the percentiles
pub const WINDOW: usize = 240;
/// and how far back they are kept, measured from the newest frame
pub const WINDOW_MS: f64 = 4000.0;
/// how often a log sample is taken
pub const LOG_INTERVAL_MS: f64 = 1000.0;
/// the log's budget: the oldest samples go to hold it
pub const LOG_BYTES: usize = 24_000;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Section {
    pub name: String,
    pub n: usize,
    pub last: f64,
    pub p50: f64,
    pub p95: f64,
    pub max: f64,
}

#[derive(Clone, Debug)]
struct Frame {
    at: f64,
    gap: Option<f64>,
    total: f64,
    parts: Vec<(&'static str, f64)>,
    tags: Vec<String>,
}

#[derive(Clone, Debug, Default)]
pub struct Report {
    pub frames: usize,
    /// the display's period, taken as the shortest interval seen
    pub period: f64,
    /// frames of the window that arrived more than half a period late
    pub dropped: usize,
    pub span: f64,
    /// how long ago the newest frame ran, ms
    pub age: f64,
    pub interval: Section,
    pub body: Section,
    pub sections: Vec<Section>,
    /// the worst body of the window: its total, how long ago, its parts and tags
    pub worst: Option<(f64, f64, Vec<(&'static str, f64)>, Vec<String>)>,
}

/// The clock every timing reads, in ms.
fn now_ms() -> f64 {
    thread_local!(static START: Instant = Instant::now());
    START.with(|s| s.elapsed().as_secs_f64() * 1000.0)
}

#[derive(Default)]
pub struct FrameProfile {
    frames: std::collections::VecDeque<Frame>,
    prev_frame: Option<f64>,
    newest: f64,
    gap: Option<f64>,
    at: f64,
    frame_start: f64,
    mark_at: f64,
    cur: Vec<(&'static str, f64)>,
    tags: Vec<String>,
}

impl FrameProfile {
    /// The loop rested: the next interval has nothing to be measured against.
    pub fn resume(&mut self) {
        self.prev_frame = None;
    }

    pub fn begin(&mut self) {
        let now = now_ms();
        self.gap = self.prev_frame.map(|p| now - p);
        self.prev_frame = Some(now);
        self.at = now;
        self.frame_start = now;
        self.mark_at = now;
        self.cur.clear();
        self.tags.clear();
    }

    /// The section since the last mark ends here under `name`.
    pub fn mark(&mut self, name: &'static str) {
        let t = now_ms();
        self.cur.push((name, t - self.mark_at));
        self.mark_at = t;
    }

    /// Something notable this frame (a scene rebuild, a level change), shown with the worst frame if this is it.
    pub fn tag(&mut self, what: &str) {
        self.tags.push(what.to_string());
    }

    pub fn end(&mut self) {
        let total = now_ms() - self.frame_start;
        // every frame, for a hitch that one key or tap makes (RUST_LOG=isorun::frames=debug)
        if log::log_enabled!(target: "isorun::frames", log::Level::Debug) {
            let parts: Vec<String> = self.cur.iter().map(|(n, t)| format!("{n} {t:.1}")).collect();
            log::debug!(target: "isorun::frames", "gap {:.1} body {total:.1} | {} {}", self.gap.unwrap_or(0.0), parts.join(" "), self.tags.join(" "));
        }
        self.frames.push_back(Frame { at: self.at, gap: self.gap, total, parts: std::mem::take(&mut self.cur), tags: std::mem::take(&mut self.tags) });
        self.newest = self.newest.max(self.at);
        let from = self.newest - WINDOW_MS;
        while self.frames.len() > WINDOW || self.frames.front().is_some_and(|f| f.at < from) {
            self.frames.pop_front();
        }
    }

    pub fn reset(&mut self) {
        *self = FrameProfile::default();
    }

    pub fn report(&self) -> Report {
        let fs = &self.frames;
        let mut parts: Vec<(&'static str, Vec<f64>)> = Vec::new();
        for f in fs {
            for &(name, ms) in &f.parts {
                match parts.iter_mut().find(|p| p.0 == name) {
                    Some(p) => p.1.push(ms),
                    None => parts.push((name, vec![ms])),
                }
            }
        }
        let worst = fs.iter().max_by(|a, b| a.total.total_cmp(&b.total));
        let newest = fs.back().map_or(self.newest, |f| f.at);
        let gaps: Vec<f64> = fs.iter().filter_map(|f| f.gap).collect();
        // the shortest interval is the display's period: an average cannot tell 60 Hz held from
        // 120 Hz halved, and anything under 4 ms is two callbacks in one tick rather than a refresh
        let mut period = 16.7;
        for &g in &gaps {
            if (4.0..period).contains(&g) {
                period = g;
            }
        }
        Report {
            frames: fs.len(),
            period,
            dropped: gaps.iter().filter(|&&g| g > period * 1.5).count(),
            span: fs.front().map_or(0.0, |f| newest - f.at),
            age: (now_ms() - newest).max(0.0),
            interval: stats("interval", &gaps),
            body: stats("body", &fs.iter().map(|f| f.total).collect::<Vec<_>>()),
            sections: parts.iter().map(|(n, a)| stats(n, a)).collect(),
            worst: worst.map(|w| (w.total, newest - w.at, w.parts.clone(), w.tags.clone())),
        }
    }
}

pub fn stats(name: &str, xs: &[f64]) -> Section {
    if xs.is_empty() {
        return Section { name: name.into(), ..Default::default() };
    }
    let mut s = xs.to_vec();
    s.sort_by(f64::total_cmp);
    let at = |q: f64| s[((q * s.len() as f64).floor() as usize).min(s.len() - 1)];
    Section { name: name.into(), n: xs.len(), last: *xs.last().unwrap(), p50: at(0.5), p95: at(0.95), max: *s.last().unwrap() }
}

/// The client messages that are the player acting: what the server sends back after one is the latency they feel.
const INPUT_MSGS: [&str; 4] = ["key", "input", "text_input", "click_cell"];

#[derive(Clone, Debug, Default)]
pub struct NetReport {
    /// ms from the player's key going out to the first message the server sent back
    pub rtt: Section,
    /// ms from that key to the frame that drew the answer: the whole of what the hand feels
    pub ttd: Section,
    /// messages in and out a second, over the window
    pub ins: f64,
    pub outs: f64,
    /// ms since anything last arrived
    pub quiet: f64,
    /// keys sent with nothing back yet
    pub pending: usize,
    pub open: bool,
}

/// Traffic over the same window as the frames, aged by the clock: a socket that has gone quiet is quiet.
pub struct NetProfile {
    rtts: Vec<(f64, f64)>,
    ttds: Vec<(f64, f64)>,
    ins: Vec<f64>,
    outs: Vec<f64>,
    waiting: Vec<f64>,
    /// keys the server has answered, waiting on the frame that puts the answer on the screen
    undrawn: Vec<f64>,
    last_in: f64,
    pub open: bool,
}

impl Default for NetProfile {
    fn default() -> Self {
        NetProfile { rtts: vec![], ttds: vec![], ins: vec![], outs: vec![], waiting: vec![], undrawn: vec![], last_in: 0.0, open: true }
    }
}

impl NetProfile {
    /// A client message went out.
    pub fn sent(&mut self, name: &str) {
        let at = now_ms();
        self.outs.push(at);
        if INPUT_MSGS.contains(&name) {
            self.waiting.push(at);
        }
        self.trim(at);
    }

    /// Messages arrived and reached the state this frame: the first answers the oldest key waiting.
    pub fn arrived(&mut self, msgs: usize) {
        if msgs == 0 {
            return;
        }
        let at = now_ms();
        for _ in 0..msgs {
            self.ins.push(at);
        }
        self.last_in = at;
        if !self.waiting.is_empty() {
            let t = self.waiting.remove(0);
            self.rtts.push((at, at - t));
            self.undrawn.push(t);
        }
        self.trim(at);
    }

    /// A frame drew: the key whose answer it drew is the oldest answered but not yet shown.
    pub fn drew(&mut self) {
        if self.undrawn.is_empty() {
            return;
        }
        let t = self.undrawn.remove(0);
        let at = now_ms();
        self.ttds.push((at, at - t));
        self.trim(at);
    }

    fn trim(&mut self, now: f64) {
        let from = now - WINDOW_MS;
        self.rtts.retain(|r| r.0 >= from);
        self.ttds.retain(|r| r.0 >= from);
        self.ins.retain(|&t| t >= from);
        self.outs.retain(|&t| t >= from);
        // a key the server never answered is not waiting any more, it is lost
        while self.waiting.len() > 8 {
            self.waiting.remove(0);
        }
        while self.undrawn.len() > 8 {
            self.undrawn.remove(0);
        }
    }

    pub fn reset(&mut self) {
        *self = NetProfile { open: self.open, ..Default::default() };
    }

    pub fn report(&mut self) -> NetReport {
        let now = now_ms();
        self.trim(now);
        let per = WINDOW_MS / 1000.0;
        NetReport {
            rtt: stats("rtt", &self.rtts.iter().map(|r| r.1).collect::<Vec<_>>()),
            ttd: stats("ttd", &self.ttds.iter().map(|r| r.1).collect::<Vec<_>>()),
            ins: self.ins.len() as f64 / per,
            outs: self.outs.len() as f64 / per,
            quiet: if self.last_in > 0.0 { now - self.last_in } else { 0.0 },
            pending: self.waiting.len(),
            open: self.open,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rank {
    Good,
    Fair,
    Bad,
    Idle,
}

/// One line's worth of answer: is this good. The thresholds are read against the display's own
/// period rather than a fixed 60 Hz, and the worst complaint wins. Only what the player would
/// feel counts.
pub fn verdict(r: &Report, n: Option<&NetReport>) -> (Rank, String) {
    if n.is_some_and(|n| !n.open) {
        return (Rank::Bad, "disconnected".into());
    }
    if r.frames < 10 {
        return (Rank::Idle, "nothing to draw".into());
    }
    let period = r.period;
    let worst = r.worst.as_ref().and_then(|w| w.2.iter().max_by(|a, b| a.1.total_cmp(&b.1)).cloned());
    let blame = match worst {
        Some((name, ms)) if ms >= 1.0 => format!(" ({name} {ms:.0}ms)"),
        _ => String::new(),
    };
    let mut bad = Vec::new();
    let mut fair = Vec::new();
    // the count of late frames, not a percentile: one step in ten stuttering is what is felt
    let late = r.dropped as f64 / r.frames as f64;
    if late > 0.1 {
        bad.push(format!("{} of {} frames dropped{blame}", r.dropped, r.frames));
    } else if late > 0.02 {
        fair.push(format!("{} of {} frames dropped{blame}", r.dropped, r.frames));
    }
    if r.body.p50 > 8.0 {
        bad.push(format!("loop {:.1}ms every frame", r.body.p50));
    } else if r.body.p50 > 4.0 {
        fair.push(format!("loop {:.1}ms every frame", r.body.p50));
    }
    if r.body.max > 33.0 {
        bad.push(format!("a {:.0}ms frame{blame}", r.body.max));
    } else if r.body.max > period {
        fair.push(format!("a {:.0}ms frame{blame}", r.body.max));
    }
    if let Some(n) = n {
        if n.rtt.n > 0 {
            if n.rtt.p95 > 250.0 {
                bad.push(format!("server {:.0}ms behind", n.rtt.p95));
            } else if n.rtt.p95 > 120.0 {
                fair.push(format!("server {:.0}ms behind", n.rtt.p95));
            }
        }
        if n.ttd.n > 0 {
            if n.ttd.p95 > 400.0 {
                bad.push(format!("{:.0}ms key to pixels", n.ttd.p95));
            } else if n.ttd.p95 > 200.0 {
                fair.push(format!("{:.0}ms key to pixels", n.ttd.p95));
            }
        }
        if n.pending > 2 {
            fair.push(format!("{} keys unanswered", n.pending));
        }
    }
    if let Some(b) = bad.into_iter().next() {
        return (Rank::Bad, b);
    }
    if let Some(f) = fair.into_iter().next() {
        return (Rank::Fair, f);
    }
    let feel = match n {
        Some(n) if n.ttd.n > 0 => format!(", {:.0}ms key to pixels", n.ttd.p50),
        Some(n) if n.rtt.n > 0 => format!(", server {:.0}ms", n.rtt.p50),
        _ => String::new(),
    };
    (Rank::Good, format!("{}Hz{feel}", (1000.0 / period).round()))
}

pub fn format_verdict(v: &(Rank, String)) -> String {
    let (mark, name) = match v.0 {
        Rank::Good => ("▲", "good"),
        Rank::Fair => ("–", "fair"),
        Rank::Bad => ("▼", "bad"),
        Rank::Idle => ("·", "idle"),
    };
    format!("{mark} {name}  {}", v.1)
}

pub fn format_report(r: &Report, n: Option<&NetReport>) -> String {
    // a gamedata load or a lost packet puts four figures in a column: the decimal goes rather than the gap
    let f = |x: f64| if x >= 100.0 { format!("{x:>7.0}") } else { format!("{x:>7.1}") };
    let row = |s: &Section| format!("{:<9}{:>4}{}{}{}{}", s.name, s.n, f(s.last), f(s.p50), f(s.p95), f(s.max));
    let head = format!("{:<9}   n   last    p50    p95    max", format!("{}f {:.1}s", r.frames, r.span / 1000.0));
    let mut lines = vec![if r.age > 250.0 { format!("{head}  ({:.1}s ago)", r.age / 1000.0) } else { head }, row(&r.interval), row(&r.body)];
    lines.extend(r.sections.iter().map(row));
    if let Some((total, ago, parts, tags)) = &r.worst {
        let p: Vec<String> = parts.iter().map(|(n, ms)| format!("{n} {ms:.1}")).collect();
        let t = if tags.is_empty() { String::new() } else { format!("  [{}]", tags.join(", ")) };
        lines.push(format!("worst {total:.1}ms {:.1}s back: {}{t}", ago / 1000.0, p.join("  ")));
    }
    if let Some(n) = n {
        lines.push(row(&n.rtt));
        if n.ttd.n > 0 {
            lines.push(row(&n.ttd));
        }
        let quiet = if n.quiet >= 1000.0 { format!("  quiet {:.1}s", n.quiet / 1000.0) } else { String::new() };
        let pending = if n.pending > 0 { format!("  waiting {}", n.pending) } else { String::new() };
        lines.push(format!("net {}  in {:.0}/s  out {:.0}/s{pending}{quiet}", if n.open { "up" } else { "DOWN" }, n.ins, n.outs));
    }
    lines.join("\n")
}

/// What was on the screen at a sample.
#[derive(Clone, Debug, Default)]
pub struct LogContext {
    pub mode: String,
    pub place: String,
    pub cells: usize,
    pub crowd: usize,
}

/// The readout over the game and its log.
pub struct PerfOverlay {
    pub profile: FrameProfile,
    pub net: NetProfile,
    pub enabled: bool,
    log: Vec<String>,
    header: Vec<String>,
    last_sample: f64,
    last_context: String,
    /// what the last tap's save did, and when
    saved: Option<(Result<String, String>, f64)>,
    shown: (String, String, f64),
}

impl PerfOverlay {
    pub fn new(enabled: bool) -> Self {
        PerfOverlay { profile: FrameProfile::default(), net: NetProfile::default(), enabled, log: Vec::new(), header: Vec::new(), last_sample: 0.0, last_context: String::new(), saved: None, shown: Default::default() }
    }

    /// Start the log: what it was taken on.
    pub fn begin_log(&mut self, screen: egui::Vec2, ppp: f32, gpu: &str, ctx: &LogContext) {
        let profile = std::env::var("ISORUN_PROFILE").unwrap_or_default();
        self.header = vec![
            format!("isorun perf · native {} · {}", std::env::consts::OS, profile),
            format!("screen {:.0}x{:.0} ppp{ppp} · {} cores", screen.x, screen.y, std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0)),
            format!("gpu {gpu}"),
            format!("start {} · {} · {} cells", ctx.place, ctx.mode, ctx.cells),
        ];
        self.log.clear();
    }

    /// A sample a second for the log; a line of its own when what is on the screen changes.
    pub fn sample(&mut self, ctx: &LogContext) {
        let now = now_ms();
        if now - self.last_sample < LOG_INTERVAL_MS {
            return;
        }
        self.last_sample = now;
        let r = self.profile.report();
        let n = self.net.report();
        let v = verdict(&r, Some(&n));
        let what = format!("{} · {}", ctx.place, ctx.mode);
        if what != self.last_context {
            self.log.push(format!("{:>8.1}s -- {what} · {} cells · {} actors", now / 1000.0, ctx.cells, ctx.crowd));
            self.last_context = what;
        }
        let worst = r.worst.as_ref().map(|w| format!(" worst {:.1}", w.0)).unwrap_or_default();
        let parts: Vec<String> = r.sections.iter().map(|s| format!("{} {:.1}/{:.1}", s.name, s.p50, s.p95)).collect();
        let mut line = format!("{:>8.1}s {} f{} drop{} body {:.1}/{:.1}{worst} | {} | rtt {:.0}", now / 1000.0, format_verdict(&v), r.frames, r.dropped, r.body.p50, r.body.p95, parts.join(" "), n.rtt.p50);
        line.truncate(260);
        log::debug!("perf {line}");
        self.log.push(line);
        while self.log.iter().map(|l| l.len() + 1).sum::<usize>() > LOG_BYTES {
            self.log.remove(0);
        }
    }

    /// The whole readout as text, verdict first.
    pub fn text(&mut self) -> String {
        let r = self.profile.report();
        let n = self.net.report();
        format!("{}\n{}", format_verdict(&verdict(&r, Some(&n))), format_report(&r, Some(&n)))
    }

    /// Put the log on disk, in the data folder (or the working one).
    pub fn save(&mut self) -> Result<String, String> {
        let mut text = self.header.join("\n");
        text.push_str("\n\n");
        text.push_str(&self.log.join("\n"));
        text.push_str("\n\n");
        text.push_str(&self.text());
        let secs = web_time::SystemTime::now().duration_since(web_time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0);
        let name = format!("isorun-perf-{secs}.txt");
        let dir = std::env::var("ISORUN_DATA_DIR").map(std::path::PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
        let path = dir.join(&name);
        #[cfg(not(target_arch = "wasm32"))]
        {
            std::fs::write(&path, text).map_err(|e| e.to_string())?;
            Ok(path.display().to_string())
        }
        #[cfg(target_arch = "wasm32")]
        {
            let _ = (path, text);
            Err("no file system on the web".into())
        }
    }

    /// Draw the pane at the top centre (styles.css `.game pre.perf`), four times a second as the web does.
    pub fn ui(&mut self, ctx: &egui::Context) {
        if !self.enabled {
            return;
        }
        let now = now_ms();
        if now - self.shown.2 >= 250.0 {
            let r = self.profile.report();
            let n = self.net.report();
            let v = verdict(&r, Some(&n));
            let colour = match v.0 {
                Rank::Good => "good",
                Rank::Fair => "fair",
                Rank::Bad => "bad",
                Rank::Idle => "idle",
            };
            self.shown = (format!("{colour}|{}", format_verdict(&v)), format_report(&r, Some(&n)), now);
        }
        let (rank, verdict_text) = self.shown.0.split_once('|').unwrap_or(("idle", ""));
        let colour = match rank {
            "good" => Color32::from_rgb(0x66, 0xff, 0x66),
            "fair" => Color32::from_rgb(0xff, 0xdd, 0x66),
            "bad" => Color32::from_rgb(0xff, 0x66, 0x66),
            _ => Color32::from_gray(0x99),
        };
        let fresh = self.saved.as_ref().filter(|s| now - s.1 < 6000.0);
        let hint = match fresh {
            Some((Ok(p), _)) => format!("saved {p}"),
            Some((Err(e), _)) => format!("could not save: {e}"),
            None => "tap to save the log".into(),
        };
        let font = FontId::monospace(11.0);
        let screen = ctx.content_rect();
        let body = self.shown.1.clone();
        let resp = egui::Area::new(egui::Id::unique("isorun-perf")).order(egui::Order::Tooltip).anchor(egui::Align2::CENTER_TOP, [0.0, 4.0]).show(ctx, |ui| {
            egui::Frame::NONE.fill(Color32::from_black_alpha(153)).inner_margin(egui::Margin::symmetric(6, 4)).show(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.add(egui::Label::new(egui::RichText::new(verdict_text).font(font.clone()).strong().color(colour)).wrap_mode(egui::TextWrapMode::Extend));
                ui.add(egui::Label::new(egui::RichText::new(&body).font(font.clone()).color(Color32::from_rgb(0, 0xff, 0))).wrap_mode(egui::TextWrapMode::Extend));
                let saved = matches!(fresh, Some((Ok(_), _)));
                ui.add(egui::Label::new(egui::RichText::new(hint).font(font.clone()).color(if saved { Color32::from_rgb(0, 0xff, 0) } else { Color32::from_rgb(0x88, 0xaa, 0x88) })).wrap_mode(egui::TextWrapMode::Extend));
            });
        });
        let _ = screen;
        if resp.response.interact(egui::Sense::click()).clicked() {
            let r = self.save();
            if let Ok(p) = &r {
                log::info!("perf log saved to {p}");
            }
            self.saved = Some((r, now));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stats_take_percentiles() {
        let s = stats("x", &[5.0, 1.0, 3.0, 2.0, 4.0]);
        assert_eq!((s.n, s.last, s.p50, s.max), (5, 4.0, 3.0, 5.0));
    }

    #[test]
    fn verdicts() {
        let mut r = Report { frames: 5, period: 16.7, ..Default::default() };
        assert_eq!(verdict(&r, None).0, Rank::Idle);
        r.frames = 100;
        r.body = Section { p50: 2.0, max: 10.0, ..Default::default() };
        assert_eq!(verdict(&r, None), (Rank::Good, "60Hz".into()));
        r.dropped = 20;
        assert_eq!(verdict(&r, None).0, Rank::Bad);
        r.dropped = 0;
        r.body.p50 = 5.0;
        assert_eq!(verdict(&r, None).0, Rank::Fair);
    }
}
