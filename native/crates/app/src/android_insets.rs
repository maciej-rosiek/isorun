//! The window's insets on Android: the system bars (the status bar, the gesture bar) and the
//! display cutout, from `WindowInsets` over JNI, as the safe area (`isorun_ui::safe`). An app that
//! targets API 35 is drawn edge to edge, under the bars; the content rect NativeActivity reports
//! is then the whole window, so the insets are asked of the decor view. Read again every second
//! and on a resize (a rotation moves the cutout to a side).

use std::sync::OnceLock;
use winit::platform::android::activity::AndroidApp;

static APP: OnceLock<AndroidApp> = OnceLock::new();

pub fn set_app(app: &AndroidApp) {
    let _ = APP.set(app.clone());
}

/// The activity, for the other JNI calls (the soft keyboard).
pub fn app() -> Option<&'static AndroidApp> {
    APP.get()
}

/// Read the insets now and then; they are in pixels, the safe area in points.
pub fn poll(ctx: &egui::Context) {
    let now = ctx.input(|i| i.time);
    let size = ctx.content_rect().size();
    let key = egui::Id::new("isorun-android-insets");
    let last: Option<(f64, egui::Vec2)> = ctx.data(|d| d.get_temp(key));
    if last.is_some_and(|(t, s)| now - t < 1.0 && s == size) {
        return;
    }
    ctx.data_mut(|d| d.insert_temp(key, (now, size)));
    let Some(app) = APP.get() else { return };
    match read(app) {
        Ok(px) => {
            let ppp = ctx.pixels_per_point().max(0.1);
            isorun_ui::safe::set_insets(px.map(|v| v as f32 / ppp));
        }
        Err(e) => log::warn!("the window insets: {e}"),
    }
}

/// top, right, bottom, left, in pixels
fn read(app: &AndroidApp) -> Result<[i32; 4], String> {
    use jni::objects::{JClass, JObject};
    use jni::{jni_sig, jni_str, JValue};
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as _) };
    vm.attach_current_thread(|env| -> jni::errors::Result<[i32; 4]> {
        let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr() as _) };
        let window = env.call_method(&activity, jni_str!("getWindow"), jni_sig!("()Landroid/view/Window;"), &[])?.l()?;
        let decor = env.call_method(&window, jni_str!("getDecorView"), jni_sig!("()Landroid/view/View;"), &[])?.l()?;
        let insets = env.call_method(&decor, jni_str!("getRootWindowInsets"), jni_sig!("()Landroid/view/WindowInsets;"), &[])?.l()?;
        if insets.is_null() {
            return Ok([0; 4]);
        }
        let version = env.find_class(jni_str!("android/os/Build$VERSION"))?;
        let sdk = env.get_static_field(&JClass::from(version), jni_str!("SDK_INT"), jni_sig!("I"))?.i()?;
        if sdk >= 30 {
            // WindowInsets.Type: statusBars 1, navigationBars 2, captionBar 4, displayCutout 128
            let r = env.call_method(&insets, jni_str!("getInsets"), jni_sig!("(I)Landroid/graphics/Insets;"), &[JValue::Int(1 | 2 | 4 | 128)])?.l()?;
            let f = |env: &mut jni::Env, name: &'static jni::strings::JNIStr| env.get_field(&r, name, jni_sig!("I")).and_then(|v| v.i());
            Ok([f(env, jni_str!("top"))?, f(env, jni_str!("right"))?, f(env, jni_str!("bottom"))?, f(env, jni_str!("left"))?])
        } else {
            let m = |env: &mut jni::Env, name: &'static jni::strings::JNIStr| env.call_method(&insets, name, jni_sig!("()I"), &[]).and_then(|v| v.i());
            Ok([
                m(env, jni_str!("getSystemWindowInsetTop"))?,
                m(env, jni_str!("getSystemWindowInsetRight"))?,
                m(env, jni_str!("getSystemWindowInsetBottom"))?,
                m(env, jni_str!("getSystemWindowInsetLeft"))?,
            ])
        }
    })
    .map_err(|e| e.to_string())
}
