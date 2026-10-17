//! The safe area: the part of the window the system's bars and the camera cutout leave free
//! (Android's window insets, CSS `env(safe-area-inset-*)`). The 3D view may draw edge to edge;
//! everything that is read or touched (the HUD, the front end, the overlays) is laid out inside
//! [`rect`].
//!
//! On Android the app sets the insets from the window (`app` crate, `WindowInsets` over JNI);
//! `ISORUN_SAFE_AREA=top,right,bottom,left` (in points) sets them anywhere, to test a phone's
//! layout on the desktop.

use egui::Rect;
use std::sync::Mutex;

static INSETS: Mutex<Option<[f32; 4]>> = Mutex::new(None);

fn from_env() -> [f32; 4] {
    let mut v = [0.0; 4];
    if let Ok(s) = std::env::var("ISORUN_SAFE_AREA") {
        for (i, p) in s.split(',').take(4).enumerate() {
            v[i] = p.trim().parse::<f32>().unwrap_or(0.0).max(0.0);
        }
    }
    v
}

/// The insets in points: top, right, bottom, left.
pub fn insets() -> [f32; 4] {
    let mut g = INSETS.lock().unwrap_or_else(|e| e.into_inner());
    *g.get_or_insert_with(from_env)
}

/// Set the insets (points: top, right, bottom, left). `ISORUN_SAFE_AREA`, when set, wins.
pub fn set_insets(v: [f32; 4]) {
    if std::env::var_os("ISORUN_SAFE_AREA").is_some() {
        return;
    }
    let mut g = INSETS.lock().unwrap_or_else(|e| e.into_inner());
    *g = Some(v.map(|x| x.max(0.0)));
}

/// The window's rect less the insets.
pub fn rect(ctx: &egui::Context) -> Rect {
    shrink(ctx.content_rect())
}

/// `full` less the insets.
pub fn shrink(full: Rect) -> Rect {
    let [t, r, b, l] = insets();
    let out = Rect::from_min_max(egui::pos2(full.left() + l, full.top() + t), egui::pos2(full.right() - r, full.bottom() - b));
    if out.width() < 100.0 || out.height() < 100.0 {
        full
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_safe_rect_leaves_the_insets_out() {
        super::set_insets([32.0, 0.0, 48.0, 0.0]);
        let r = super::shrink(egui::Rect::from_min_size(egui::Pos2::ZERO, egui::vec2(412.0, 915.0)));
        assert_eq!((r.top(), r.bottom(), r.left(), r.right()), (32.0, 867.0, 0.0, 412.0));
    }
}
