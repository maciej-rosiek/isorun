//! Where an overlay stands and what holds it: the popup's place on the screen (styles.css
//! `.popup`: hanging from the top, 20px further down for each one under it, or in the middle when
//! crawl says `ui-centred`), its sizes, the panel painted under it, its scrolling body (and the
//! browser's scrollbar there), and the round close button a touch screen gets.

use egui::{Color32, Pos2, Rect, Sense, Vec2};
use isorun_ui::widgets::{self, hex, PanelLook};

/// The screen the overlays stand on, and the CSS sizes that follow from it.
#[derive(Clone, Copy, Debug)]
pub struct Screen {
    pub rect: Rect,
    pub scale: f32,
}

impl Screen {
    pub fn new(ctx: &egui::Context, scale: f32) -> Self {
        Screen { rect: isorun_ui::safe::rect(ctx), scale }
    }
    /// The screen less a bar of `bottom` points at its foot (the touch bar), which popups keep clear of.
    pub fn above(ctx: &egui::Context, scale: f32, bottom: f32) -> Self {
        let mut rect = isorun_ui::safe::rect(ctx);
        rect.max.y = (rect.max.y - bottom.max(0.0)).max(rect.min.y + 0.5 * rect.height());
        Screen { rect, scale }
    }
    pub fn w(&self) -> f32 {
        self.rect.width()
    }
    pub fn h(&self) -> f32 {
        self.rect.height()
    }
    /// `1rem`: the root's 15px times the UI scale.
    pub fn rem(&self) -> f32 {
        15.0 * self.scale
    }
    /// `.popup { font-size: 0.95em }`
    pub fn popup_font(&self) -> f32 {
        self.rem() * 0.95
    }
    /// touch.css's compact screen: a window under 900 x 560 css px
    pub fn compact(&self) -> bool {
        self.w() < 900.0 || self.h() < 560.0
    }
    /// `--popup-top`: 50px, 20px when the window is under 780px tall
    pub fn popup_top(&self) -> f32 {
        if self.h() <= 780.0 {
            20.0
        } else {
            50.0
        }
    }
    /// `max-height: calc(95vh - 2 * var(--popup-top))`
    pub fn popup_max_h(&self) -> f32 {
        0.95 * self.h() - 2.0 * self.popup_top()
    }
    /// `max-width: min(94vw, 78rem)`, or the whole width less 16px on a compact screen
    pub fn popup_max_w(&self) -> f32 {
        if self.compact() {
            self.w() - 16.0
        } else {
            (0.94 * self.w()).min(78.0 * self.rem())
        }
    }
    /// `min-width: 24rem` (none on a compact screen)
    pub fn popup_min_w(&self) -> f32 {
        if self.compact() {
            0.0
        } else {
            24.0 * self.rem()
        }
    }
    /// dock.css `--hu`: 1px at 1080 lines, more on a taller window, times the UI scale
    pub fn hu(&self) -> f32 {
        self.scale * (self.h() / 1080.0).min(self.w() / 1700.0).clamp(0.75, 1.6)
    }
}

/// The width a popup takes (CSS shrink-to-fit): its natural width, but no wider than the room
/// right of its `left: 50%` (half the screen) unless a word does not fit there, and within its
/// min and max width. All widths are of the border box. A compact screen (a phone) gives every
/// popup its whole width less the margins (touch.css): half a phone is too narrow to read in.
pub fn shrink_to_fit(screen: &Screen, natural: f32, min_content: f32, min_w: f32, max_w: f32) -> f32 {
    if screen.compact() {
        return max_w.ceil();
    }
    let avail = screen.w() / 2.0;
    natural.min(min_content.max(avail)).max(min_w).min(max_w).ceil()
}

/// The longest run without a space in plain text, in characters (a mono font's `min-content`).
pub fn longest_word(text: &str) -> usize {
    text.split(|c: char| c == ' ' || c == '\n').map(|w| w.chars().count()).max().unwrap_or(0)
}

/// A popup's border box: `width` x `height`, `depth` popups under it (up to three deep), or centred.
pub fn place(screen: &Screen, size: Vec2, depth: usize, centred: bool, touch: bool) -> Rect {
    let x = screen.rect.center().x - size.x / 2.0;
    let y = if centred {
        screen.rect.center().y - size.y / 2.0
    } else {
        // `.popup:has(> .touch-close) { margin-top: 6px }`
        screen.rect.top() + screen.popup_top() + depth.min(3) as f32 * 20.0 + if touch { 6.0 } else { 0.0 }
    };
    Rect::from_min_size(Pos2::new(x.round(), y.round()), size)
}

/// The panel of a server popup: the dock's look, or the plain one without it.
pub fn look(dock: bool) -> PanelLook {
    if dock {
        PanelLook::dock_popup()
    } else {
        PanelLook::popup()
    }
}

/// The rules inside a popup (a title's foot, a more line's top).
pub fn rule_colour(dock: bool) -> Color32 {
    if dock {
        widgets::palette::RULE
    } else {
        hex(0x333333)
    }
}

/// Run `f` in a layer of its own over `rect`, which takes the pointer there (a popup is solid).
pub fn layer<R>(ctx: &egui::Context, id: egui::Id, rect: Rect, f: impl FnOnce(&mut egui::Ui, egui::Response) -> R) -> R {
    let area = egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(rect.min).constrain(false).show(ctx, |ui| {
        ui.set_clip_rect(ctx.content_rect());
        let (_, resp) = ui.allocate_exact_size(rect.size(), Sense::click_and_drag());
        f(ui, resp)
    });
    ctx.move_to_top(area.response.layer_id);
    area.inner
}

/// A full-screen layer that takes no input (the dim behind some popups).
pub fn backdrop(ctx: &egui::Context, id: egui::Id, colour: Color32) {
    let rect = ctx.content_rect();
    let area = egui::Area::new(id).order(egui::Order::Foreground).fixed_pos(rect.min).interactable(false).show(ctx, |ui| {
        ui.painter().rect_filled(rect, 0.0, colour);
    });
    ctx.move_to_top(area.response.layer_id);
}

/// A scrolling box's bar: the shared one (`isorun_ui::scroll::bar`), a thin thumb over the box's
/// right edge that takes no room from the text (the web build is read in a browser whose bars
/// overlay the page). Returns the scroll after a drag on it.
pub const SCROLLBAR_W: f32 = 0.0;

pub fn scrollbar(ui: &mut egui::Ui, id: egui::Id, viewport: Rect, content_h: f32, scroll: f32) -> f32 {
    isorun_ui::scroll::bar(ui, id, viewport, content_h, scroll)
}

/// The wheel over `viewport`, or a finger dragging it, moves `scroll`; returns the new scroll,
/// kept in range.
pub fn wheel(ui: &egui::Ui, viewport: Rect, content_h: f32, scroll: f32) -> f32 {
    isorun_ui::scroll::scroll(ui, viewport, content_h, scroll)
}

/// `.touch-close`: a popup's round close button on a touch screen, over its top-right corner.
pub fn touch_close(ui: &mut egui::Ui, id: egui::Id, popup: Rect) -> bool {
    let rect = Rect::from_min_size(Pos2::new(popup.right() + 6.0 - 40.0, popup.top() - 12.0), Vec2::splat(40.0));
    widgets::button(ui, rect, id, "✕", widgets::ButtonLook::RoundClose).clicked()
}
