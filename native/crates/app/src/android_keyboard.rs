//! The soft keyboard on Android. egui asks for the keyboard while a text field wants typing (a
//! focused `TextEdit`, or crawl's text prompt in the overlays): the frame's `PlatformOutput::ime`.
//! eframe answers that through winit's `set_ime_allowed`, which android-activity turns into
//! `InputMethodManager.showSoftInput(decorView, SHOW_IMPLICIT)`. In a NativeActivity the decor view
//! is not the view the input method serves (the focused NativeContentView is), and Android ignores
//! a request for a view it does not serve ("Ignoring showSoftInput() ... is not served"), as it
//! may ignore an implicit one: a crawl prompt came with no keyboard at all. So the app asks too,
//! explicitly and for the focused view, as NativeActivity's own `showSoftInput` does: when a field
//! starts to want the keyboard, and again on a tap on the field (the player may have put the
//! keyboard away with Back); it puts the keyboard away when no field wants it.

use egui::Event;
use winit::platform::android::activity::AndroidApp;

/// Follow the frame's `ime` output: call once a frame, after the screens drew.
pub fn follow(ctx: &egui::Context) {
    let ime = ctx.output(|o| o.ime);
    let key = egui::Id::new("isorun-android-keyboard");
    let was: bool = ctx.data(|d| d.get_temp(key)).unwrap_or(false);
    let want = ime.is_some();
    let tapped = ime.is_some_and(|ime| ctx.input(|i| i.events.iter().any(|e| matches!(e, Event::PointerButton { pos, pressed: false, .. } if ime.rect.contains(*pos)))));
    ctx.data_mut(|d| d.insert_temp(key, want));
    let show = if want && (!was || tapped) {
        true
    } else if !want && was {
        false
    } else {
        return;
    };
    let Some(app) = crate::android_insets::app() else { return };
    if let Err(e) = call(app, show) {
        log::warn!("the soft keyboard: {e}");
    }
}

/// Show or hide the input method over the focused view (the decor view when nothing has focus).
fn call(app: &AndroidApp, show: bool) -> Result<(), String> {
    use jni::objects::{JClass, JObject};
    use jni::{jni_sig, jni_str, JValue};
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as _) };
    vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr() as _) };
        let window = env.call_method(&activity, jni_str!("getWindow"), jni_sig!("()Landroid/view/Window;"), &[])?.l()?;
        let mut view = env.call_method(&window, jni_str!("getCurrentFocus"), jni_sig!("()Landroid/view/View;"), &[])?.l()?;
        if view.is_null() {
            view = env.call_method(&window, jni_str!("getDecorView"), jni_sig!("()Landroid/view/View;"), &[])?.l()?;
        }
        let name = env.new_string("input_method")?;
        let imm = env.call_method(&activity, jni_str!("getSystemService"), jni_sig!("(Ljava/lang/String;)Ljava/lang/Object;"), &[JValue::Object(&name)])?.l()?;
        if imm.is_null() {
            return Ok(());
        }
        if show {
            // flags 0: an explicit request, the player's (SHOW_IMPLICIT may be ignored)
            let shown = env.call_method(&imm, jni_str!("showSoftInput"), jni_sig!("(Landroid/view/View;I)Z"), &[JValue::Object(&view), JValue::Int(0)])?.z()?;
            if !shown {
                // Android 11 and later: the window's insets controller shows the IME's insets
                let version = env.find_class(jni_str!("android/os/Build$VERSION"))?;
                let sdk = env.get_static_field(&JClass::from(version), jni_str!("SDK_INT"), jni_sig!("I"))?.i()?;
                if sdk >= 30 {
                    let controller = env.call_method(&window, jni_str!("getInsetsController"), jni_sig!("()Landroid/view/WindowInsetsController;"), &[])?.l()?;
                    if !controller.is_null() {
                        // WindowInsets.Type.ime()
                        env.call_method(&controller, jni_str!("show"), jni_sig!("(I)V"), &[JValue::Int(8)])?;
                    }
                }
            }
        } else {
            let token = env.call_method(&view, jni_str!("getWindowToken"), jni_sig!("()Landroid/os/IBinder;"), &[])?.l()?;
            if !token.is_null() {
                env.call_method(&imm, jni_str!("hideSoftInputFromWindow"), jni_sig!("(Landroid/os/IBinder;I)Z"), &[JValue::Object(&token), JValue::Int(0)])?;
            }
        }
        Ok(())
    })
    .map_err(|e| e.to_string())
}
