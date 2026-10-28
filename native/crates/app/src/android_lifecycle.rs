//! The activity's comings and goings on Android, around eframe's event loop handler.
//!
//! The window is drawn only while it has the focus. When the screen wakes, Android resumes the
//! activity while the lock screen still covers it, and the window's surface is there but not
//! shown: the compositor takes no frame from it, so the next frame's `get_current_texture` waits
//! (Android's Vulkan ignores wgpu's timeout) and the app's thread with it. NativeActivity's Java
//! callbacks (pause, resume, the window, the input queue) each wait for that thread, so the
//! system's own thread stood too, and after five seconds Android called the app not responding,
//! again after every Wait. The focus comes back once the lock screen is gone (Android's advice
//! for games: draw after `onWindowFocusChanged(true)`). A redraw while unfocused is dropped here;
//! eframe then runs the app's logic alone after its 250 ms wait for a redraw
//! (`MISSED_REDRAW_TIMEOUT`), and asks for a frame again when the focus comes back.
//!
//! The sound follows ([`Change`]): it pauses with the focus, and the device is let go of when
//! the window goes (the screen locked, another app in front), where Android may end a stream left
//! open and never give it back: the sound was gone after the phone was locked. It opens a new
//! stream when the focus comes back.

use std::cell::RefCell;

use eframe::UserEvent;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, DeviceId, StartCause, WindowEvent};
use winit::event_loop::ActiveEventLoop;
use winit::window::WindowId;

/// What became of the window, for the sound.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// The focus went (the notification shade, the lock screen coming).
    Hidden,
    /// The window went (the screen locked, the app in the background).
    Gone,
    /// The focus came back.
    Shown,
}

thread_local! {
    static LISTENER: RefCell<Option<Box<dyn Fn(Change)>>> = const { RefCell::new(None) };
}

/// Hear the window's changes (one listener: the app's sound).
pub fn listen(f: impl Fn(Change) + 'static) {
    LISTENER.with(|l| *l.borrow_mut() = Some(Box::new(f)));
}

fn tell(change: Change) {
    log::info!("the window: {change:?}");
    LISTENER.with(|l| {
        if let Some(f) = l.borrow().as_ref() {
            f(change);
        }
    });
}

/// Whether the window may be drawn, from what the activity said (pure, for the tests).
#[derive(Default)]
struct Focus {
    lost: bool,
}

impl Focus {
    fn focused(&mut self, focused: bool) -> Option<Change> {
        if self.lost == !focused {
            return None;
        }
        self.lost = !focused;
        Some(if focused { Change::Shown } else { Change::Hidden })
    }

    fn paints(&self) -> bool {
        !self.lost
    }
}

/// eframe's handler, drawn only while the window has the focus.
pub struct Lifecycle<A> {
    inner: A,
    focus: Focus,
}

impl<A> Lifecycle<A> {
    pub fn new(inner: A) -> Self {
        Lifecycle { inner, focus: Focus::default() }
    }
}

impl<A: ApplicationHandler<UserEvent>> ApplicationHandler<UserEvent> for Lifecycle<A> {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        log::info!("the window: resumed");
        self.inner.resumed(event_loop);
    }

    fn suspended(&mut self, event_loop: &ActiveEventLoop) {
        // the focus went first; should it not have, the window is not drawn until it comes
        self.focus.lost = true;
        tell(Change::Gone);
        self.inner.suspended(event_loop);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, window_id: WindowId, event: WindowEvent) {
        match &event {
            WindowEvent::Focused(focused) => {
                if let Some(change) = self.focus.focused(*focused) {
                    tell(change);
                }
            }
            WindowEvent::RedrawRequested if !self.focus.paints() => return,
            _ => {}
        }
        self.inner.window_event(event_loop, window_id, event);
    }

    fn new_events(&mut self, event_loop: &ActiveEventLoop, cause: StartCause) {
        self.inner.new_events(event_loop, cause);
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: UserEvent) {
        self.inner.user_event(event_loop, event);
    }

    fn device_event(&mut self, event_loop: &ActiveEventLoop, device_id: DeviceId, event: DeviceEvent) {
        self.inner.device_event(event_loop, device_id, event);
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.about_to_wait(event_loop);
    }

    fn exiting(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.exiting(event_loop);
    }

    fn memory_warning(&mut self, event_loop: &ActiveEventLoop) {
        self.inner.memory_warning(event_loop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawn_only_with_the_focus() {
        let mut f = Focus::default();
        assert!(f.paints());
        assert_eq!(f.focused(true), None);
        assert_eq!(f.focused(false), Some(Change::Hidden));
        assert!(!f.paints());
        assert_eq!(f.focused(false), None);
        assert_eq!(f.focused(true), Some(Change::Shown));
        assert!(f.paints());
    }

    #[test]
    fn a_window_that_went_is_drawn_again_with_the_focus() {
        let mut f = Focus { lost: true };
        assert_eq!(f.focused(true), Some(Change::Shown));
        assert!(f.paints());
    }
}
