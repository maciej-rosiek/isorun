//! Share the log (`isorun_client::platform::diag`), for a report of a problem: on Android the
//! system's share sheet with the log as text (to a mail, a chat, a note, Drive); on the desktop
//! and the web the clipboard, and natively the file it is kept in.

/// Hand the log on; what to tell the player.
pub fn share(ctx: &egui::Context) -> String {
    let text = isorun_client::platform::diag::text();
    #[cfg(target_os = "android")]
    {
        let _ = ctx;
        match android(&text) {
            Ok(()) => "The log went to the share sheet.".into(),
            Err(e) => {
                log::warn!("the log was not shared: {e}");
                format!("The log could not be shared ({e}).")
            }
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        ctx.copy_text(text);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(path) = isorun_client::platform::diag::file_path() {
            return format!("The log is copied, and kept in {}.", path.display());
        }
        "The log is copied: paste it into the report.".into()
    }
}

/// `Intent.ACTION_SEND` of the text, through the chooser.
#[cfg(target_os = "android")]
fn android(text: &str) -> Result<(), String> {
    use jni::objects::{JClass, JObject};
    use jni::{jni_sig, jni_str, JValue};
    let app = crate::android_insets::app().ok_or("no activity")?;
    let vm = unsafe { jni::JavaVM::from_raw(app.vm_as_ptr() as _) };
    vm.attach_current_thread(|env| -> jni::errors::Result<()> {
        let activity = unsafe { JObject::from_raw(env, app.activity_as_ptr() as _) };
        let class = env.find_class(jni_str!("android/content/Intent"))?;
        let class = JClass::from(class);
        let action = env.new_string("android.intent.action.SEND")?;
        let intent = env.new_object(&class, jni_sig!("(Ljava/lang/String;)V"), &[JValue::Object(&action)])?;
        let kind = env.new_string("text/plain")?;
        env.call_method(&intent, jni_str!("setType"), jni_sig!("(Ljava/lang/String;)Landroid/content/Intent;"), &[JValue::Object(&kind)])?;
        let put = |env: &mut jni::Env, key: &str, value: &str| -> jni::errors::Result<()> {
            let k = env.new_string(key)?;
            let v = env.new_string(value)?;
            env.call_method(
                &intent,
                jni_str!("putExtra"),
                jni_sig!("(Ljava/lang/String;Ljava/lang/String;)Landroid/content/Intent;"),
                &[JValue::Object(&k), JValue::Object(&v)],
            )?;
            Ok(())
        };
        put(env, "android.intent.extra.SUBJECT", &format!("Isorun {} log", env!("CARGO_PKG_VERSION")))?;
        put(env, "android.intent.extra.TEXT", text)?;
        let title = env.new_string("Share the log")?;
        let chooser = env
            .call_static_method(
                &class,
                jni_str!("createChooser"),
                jni_sig!("(Landroid/content/Intent;Ljava/lang/CharSequence;)Landroid/content/Intent;"),
                &[JValue::Object(&intent), JValue::Object(&title)],
            )?
            .l()?;
        env.call_method(&activity, jni_str!("startActivity"), jni_sig!("(Landroid/content/Intent;)V"), &[JValue::Object(&chooser)])?;
        Ok(())
    })
    .map_err(|e| e.to_string())
}
