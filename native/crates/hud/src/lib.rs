//! Isorun's HUD on egui (the port of `apps/isorun/src` `hud.ts`, `dock.ts`, `screens.ts` and
//! their helpers): the dock along the foot of the screen, the classic HUD's panes on the console
//! grid, the tooltip cards, the inventory and character screens, damage numbers and the chat.
//! The README says how a game screen drives it.

pub mod chat;
pub mod classic;
pub mod damage_numbers;
pub mod dock;
pub mod dock_actions;
pub mod dock_memory;
pub mod gear;
pub mod glyphs;
pub mod grid;
pub mod hud;
pub mod icons;
pub mod item_compare;
pub mod pack_tabs;
pub mod paint;
pub mod quick_slots;
pub mod screens;
pub mod style;
pub mod tooltip;

pub use dock::{Dock, DockButtonId, DockEvent, DockView};
pub use gear::KeyStep;
pub use glyphs::{Glyph, PadKind};
pub use hud::{Hud, HudCx, HudLayout, HudOutput};

/// Something for the game screen's runner to do (bindings.ts `Action`, the kinds the HUD makes).
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    /// keys and text to crawl, each step waiting for crawl's prompt when it says so
    Keys { label: String, seq: Vec<KeyStep> },
    /// fire the quiver (runner.ts `fire`)
    Fire,
    /// a client message as it stands (`cm::inv_item_action`, `cm::inv_item_describe`)
    Send(serde_json::Value),
    /// a client message, then keys once crawl's description is up (runner.ts `sendThen`)
    SendThen(serde_json::Value, Vec<KeyStep>),
}

/// What the HUD asks of the game screen beyond sending to crawl.
#[derive(Clone, Debug, PartialEq)]
pub enum HudCommand {
    /// run this through the game screen's runner, which knows crawl's modes and paces travel
    Run(Action),
    /// the dock's Menu button and the panel's game-menu button: the system menu (Overlays.showSystem)
    SystemMenu,
    /// X's action tabs (action-tabs.ts) on this tab: `spells`, `abilities` or `quiver`
    ActionTab(&'static str),
    /// a row of the monster list was tapped: turn the camera to the monster at this cell
    SelectMonster { x: i32, y: i32 },
    /// the minimap was clicked: open the level map
    MinimapClick,
    /// the action panel's X was right-clicked: Isorun's own settings screen
    PanelSettings,
    /// close the client's own overlay the HUD is about to cover (a screen of the dock opens)
    CloseClientOverlay,
    /// the action panel's X or its placeholder: fold it away or bring it back (game.ts
    /// `setPanelShown`, which sets `action_panel_show`)
    PanelShown(bool),
    /// the pad's prompt at this index of the ones given was clicked: run its action
    Prompt(usize),
    /// a list to choose from (Overlays.showChoices): the pick comes back through `Hud::choose`,
    /// a close without one through `Hud::choices_closed`; `remember` names the cursor's memory
    Choices { title: String, choices: Vec<hud::Choice>, remember: &'static str },
}

/// What crawl is doing, as far as the HUD cares (context.ts `Mode`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Mode {
    /// crawl takes commands
    #[default]
    Command,
    Menu,
    Popup,
    LevelMap,
    Targeting,
    Prompt,
    /// anything else: a CRT screen, a dialog, the new-game chooser, the death screen
    Other,
}

/// What spoke last: the prompts and hints follow it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Device {
    #[default]
    Keyboard,
    Pointer,
    Pad,
}

/// Install the HUD's fonts beside isorun-ui's (call after `isorun_ui::install`).
pub fn install(ctx: &egui::Context) {
    style::install_fonts(ctx);
}
