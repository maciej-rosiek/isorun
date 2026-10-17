//! The pad's on-screen keyboard over the front end's text fields (menu.ts `openOsk`, `typeText`):
//! A on a field (or X) brings it up; its keys type into the field, Y or Start submits the form
//! (or moves on), B puts it away. Into the rc file it types at the caret, with a key for a new
//! line, and Y or B comes back out. The input crate's `Osk` keeps the cursor over the keys.

use crate::front::{Field, FrontEnd, PadButton, PadEvent};
use crate::words::NavDir;
use isorun_input::bindings::OskOp;
use isorun_input::osk::{osk_prompts, Osk, OskEdit, OskExtra, OskTarget, OskText};

/// What the keyboard types into.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OskOn {
    Field(Field),
    Rc,
}

/// The keyboard and what it serves.
#[derive(Default)]
pub(crate) struct FrontOsk {
    pub osk: Osk,
    pub on: Option<OskOn>,
    pub text: OskText,
}

const RC_ID: &str = "rc-text";

impl FrontEnd {
    /// The keyboard is up.
    pub fn osk_visible(&self) -> bool {
        self.osk.on.is_some()
    }

    /// The field under the cursor, if the cursor is on one.
    fn cursor_field(&mut self) -> Option<Field> {
        let c = self.cursor.clone()?;
        self.screen().rows.iter().find(|r| r.id == c).and_then(|r| r.input)
    }

    /// Open the keyboard on the field under the cursor, or into the rc file: true when it opened.
    pub(crate) fn open_osk(&mut self) -> bool {
        if self.cursor.as_deref() == Some("rc:text") {
            let Some(rc) = self.rc.as_ref().filter(|r| r.state == crate::words::RcState::Ready) else { return false };
            // at the caret egui's field has, else at the end
            let caret = CTX.with(|c| c.borrow().as_ref().and_then(|ctx| egui::text_edit::TextEditState::load(ctx, egui::Id::NULL.with(RC_ID))).and_then(|s| s.cursor.char_range()).map(|r| r.primary.index));
            let mut t = OskText::new(&rc.text);
            if let Some(c) = caret {
                t.caret = usize::from(c).min(t.text.chars().count());
            }
            self.osk.text = t;
            self.osk.on = Some(OskOn::Rc);
            self.osk.osk.attach(OskTarget { extras: vec![OskExtra { ch: '\n', label: "New line".into() }], submit_button: None });
            return true;
        }
        let Some(f) = self.cursor_field() else { return false };
        let mut t = OskText::new(self.fields.get(&f).map(String::as_str).unwrap_or(""));
        t.password = matches!(f, Field::Password | Field::Password2);
        self.osk.text = t;
        self.osk.on = Some(OskOn::Field(f));
        self.osk.osk.attach(OskTarget::default());
        true
    }

    pub(crate) fn close_osk(&mut self) {
        self.osk.osk.detach();
        self.osk.on = None;
    }

    /// A pad event while the keyboard is up: true when it took it (it takes every one but the stick).
    pub(crate) fn osk_pad(&mut self, ev: PadEvent) -> bool {
        if !self.osk_visible() {
            return false;
        }
        let (op, dir) = match ev {
            PadEvent::Look { .. } => return false,
            PadEvent::Dir(d) => (OskOp::Move, Some(match d {
                NavDir::Up => 0,
                NavDir::Right => 2,
                NavDir::Down => 4,
                NavDir::Left => 6,
            })),
            PadEvent::Press(b) => (
                match b {
                    PadButton::A => OskOp::Type,
                    PadButton::B | PadButton::Select => OskOp::Cancel,
                    PadButton::X => OskOp::Backspace,
                    PadButton::Y | PadButton::Start => OskOp::Submit,
                    PadButton::LB => OskOp::Shift,
                    PadButton::RB => OskOp::Space,
                },
                None,
            ),
        };
        if let Some(e) = self.osk.osk.op(op, dir) {
            self.osk_edit(e);
        }
        true
    }

    /// An edit of the keyboard's, into its field.
    fn osk_edit(&mut self, e: OskEdit) {
        let Some(on) = self.osk.on else { return };
        match self.osk.text.apply(&e) {
            None => {
                let text = self.osk.text.text.clone();
                match on {
                    OskOn::Field(f) => {
                        self.fields.insert(f, text);
                    }
                    OskOn::Rc => {
                        if let Some(rc) = &mut self.rc {
                            rc.text = text;
                            rc.leaving = false;
                        }
                        let caret = self.osk.text.caret;
                        CTX.with(|c| {
                            if let Some(ctx) = c.borrow().as_ref() {
                                let id = egui::Id::NULL.with(RC_ID);
                                let mut s = egui::text_edit::TextEditState::load(ctx, id).unwrap_or_default();
                                s.cursor.set_char_range(Some(egui::text::CCursorRange::one(egui::text::CCursor::new(caret))));
                                s.store(ctx, id);
                            }
                        });
                    }
                }
            }
            Some(OskEdit::Submit) => {
                self.close_osk();
                // a form is sent; a field of no form moves on; the rc file is come out of
                if matches!(on, OskOn::Field(_)) {
                    let before = self.view.clone();
                    self.submit_form();
                    if self.view == before && !matches!(before, crate::front::View::Login | crate::front::View::Register | crate::front::View::AddServer) {
                        self.nav(NavDir::Down);
                    }
                }
            }
            Some(_) => self.close_osk(),
        }
    }

    /// Draw the keyboard over the screen; a click on a key types it.
    pub(crate) fn draw_osk(&mut self, ctx: &egui::Context) {
        if !self.osk_visible() {
            return;
        }
        // the field went (a screen changed under the keyboard): so does the keyboard
        let gone = match self.osk.on {
            Some(OskOn::Field(f)) => self.cursor_field() != Some(f),
            Some(OskOn::Rc) => self.rc.is_none() || self.view != crate::front::View::Rc,
            None => true,
        };
        if gone {
            return self.close_osk();
        }
        let preview = self.osk.text.preview();
        if let Some(e) = self.osk.osk.show(ctx, &preview, &osk_prompts("Submit")) {
            self.osk_edit(e);
        }
    }
}

use crate::front::CTX;
