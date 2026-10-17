//! What the bindings read of the focus layer (the port of `focus.ts`'s types): one cursor over
//! whatever the top overlay offers, driven by the d-pad or the arrow keys, with A / Enter to
//! activate and B / Escape to cancel. The layer itself is the overlays'; the bindings only see
//! what the cursor rests on ([`FocusInfo`]) and send it [`FocusOp`]s.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FocusOp {
    Next,
    Prev,
    Left,
    Right,
    Select,
    AltSelect,
    Cancel,
    PageNext,
    PagePrev,
    First,
    Last,
}

/// The cursor as the bindings see it: what A and B would do, for the bar.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct FocusInfo {
    /// label of the focused item, or None when nothing is focusable
    pub label: Option<String>,
    /// the focused item's colour (`Focusable.colour`), when it has one
    pub colour: Option<i64>,
    /// label of the cancel item when the screen has one ("No"), else None
    pub cancel_label: Option<String>,
    /// label of the focused item's second action (Y), when it has one
    pub alt_label: Option<String>,
    pub index: i64,
    pub count: i64,
}
