//! The pad's on-screen keyboard over the server's text field (overlays.ts `oskOp`, `setDevice`):
//! the input crate's `Osk` keeps the cursor over the keys and turns the pad's ops into edits,
//! which go to the field as its own keys do, so the prompt's hotkeys (`special`) hold for both.

use crate::text_field::FieldKey;
use crate::{Device, OverlayCx, Overlays};
use isorun_input::bindings::{submit_on_start_only, OskOp};
use isorun_input::osk::{osk_prompts, OskEdit, OskExtra, OskTarget};
use isorun_scene::Dir8;
use isorun_webtiles::TextInputState;

/// travel.cc `_travel_depth_keyfilter`: the keys that move the default target, as a row of the keyboard.
const TRAVEL_DEPTH_KEYS: [(char, &str); 4] = [('<', "Up"), ('>', "Down"), ('^', "Entrance"), ('$', "Deepest")];

/// What the keyboard types into: a prompt's own keys over the letters, and the button that submits.
pub(crate) fn target(ti: &TextInputState) -> OskTarget {
    let tag = ti.tag.as_deref();
    let extras = if tag == Some("travel_depth") { TRAVEL_DEPTH_KEYS.iter().map(|(ch, label)| OskExtra { ch: *ch, label: (*label).into() }).collect() } else { Vec::new() };
    // the prompt opened off Y (Set target on a skill row), so Y does not also confirm it: Start does
    let submit_button = submit_on_start_only(tag).then_some(isorun_input::Button::Start);
    OskTarget { extras, submit_button }
}

impl Overlays {
    /// The on-screen keyboard is up.
    pub fn osk_visible(&self) -> bool {
        self.osk.visible()
    }

    /// A pad operation on the on-screen keyboard (the router's `osk` op in a text prompt). The
    /// first press brings the keyboard up over the field with its cursor on the first key, and
    /// types nothing: it is a reveal, not a step, a letter or a shift.
    pub fn osk_op(&mut self, op: OskOp, dir: Option<Dir8>, cx: &mut OverlayCx) {
        self.sync(cx, None);
        if cx.watching {
            return;
        }
        let Some(f) = self.field.as_ref() else { return };
        if !self.osk.visible() {
            self.osk.attach(target(&f.source));
            if matches!(op, OskOp::Move | OskOp::Type | OskOp::Shift) {
                return;
            }
        }
        if let Some(e) = self.osk.op(op, dir) {
            self.osk_edit(e, cx);
        }
        if matches!(op, OskOp::Submit | OskOp::Cancel) {
            self.osk.detach();
        }
    }

    /// An edit of the keyboard's, as the field's own key.
    fn osk_edit(&mut self, e: OskEdit, cx: &mut OverlayCx) {
        let k = match e {
            OskEdit::Type(c) => FieldKey::Char(c),
            OskEdit::Backspace => FieldKey::Backspace,
            OskEdit::Submit => FieldKey::Enter,
            OskEdit::Cancel => FieldKey::Escape,
        };
        self.field_key(k, cx);
    }

    /// The pad taking over brings the keyboard with a prompt that is up, and a physical key or
    /// the mouse puts it away, so the field it covered shows what is typed (overlays.ts `setDevice`).
    pub(crate) fn device_changed(&mut self) {
        let Some(f) = self.field.as_ref() else { return };
        if self.device == Device::Pad && !self.osk.visible() {
            self.osk.attach(target(&f.source));
        } else if self.device != Device::Pad && self.osk.visible() {
            self.osk.detach();
        }
    }

    /// Draw the keyboard over the field; a click on a key types it.
    pub(crate) fn draw_osk(&mut self, ctx: &egui::Context, cx: &mut OverlayCx) {
        if !self.osk.visible() || cx.watching {
            return;
        }
        let Some(f) = self.field.as_ref() else { return };
        let preview = f.text.clone();
        if let Some(e) = self.osk.show(ctx, &preview, &osk_prompts("Done")) {
            self.osk_edit(e, cx);
        }
    }
}
