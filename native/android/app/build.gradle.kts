// The APK holds no Java or Kotlin: NativeActivity loads libisorun_app.so (android_main), and
// crawl ships beside it as libcrawl.so. build.sh stages both under build/stage/jniLibs, the
// engine's data under build/stage/assets/engine and the icon under build/stage/res.
plugins {
    id("com.android.application")
}

val stage = rootProject.layout.projectDirectory.dir("build/stage")
val appVersion: String = providers.gradleProperty("isorunVersion").getOrElse("0.1.0")
val appVersionCode: Int = providers.gradleProperty("isorunVersionCode").getOrElse("1").toInt()

android {
    namespace = "io.github.maciejrosiek.isorun"
    compileSdk = 35
    defaultConfig {
        // its own id while it is a preview: it installs beside the Capacitor app (mobile/) and never
        // over it, so trying it cannot cost a player the Capacitor app's saves. It takes over the
        // Capacitor app's id (and the key it already shares) once it replaces that app.
        applicationId = "io.github.maciejrosiek.isorun.native"
        minSdk = 26
        targetSdk = 35
        versionCode = appVersionCode
        versionName = appVersion
    }
    sourceSets["main"].apply {
        jniLibs.srcDir(stage.dir("jniLibs"))
        assets.srcDir(stage.dir("assets"))
        res.srcDir(stage.dir("res"))
    }
    // the release key (mobile/README.md): ISORUN_KEYSTORE, ISORUN_KEYSTORE_PASSWORD, and
    // optionally ISORUN_KEY_ALIAS (isorun) and ISORUN_KEY_PASSWORD (the store's)
    val keystore = System.getenv("ISORUN_KEYSTORE")
    signingConfigs {
        if (!keystore.isNullOrEmpty()) {
            create("release") {
                storeFile = file(keystore)
                storePassword = System.getenv("ISORUN_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ISORUN_KEY_ALIAS") ?: "isorun"
                keyPassword = System.getenv("ISORUN_KEY_PASSWORD") ?: System.getenv("ISORUN_KEYSTORE_PASSWORD")
            }
        }
    }
    buildTypes {
        getByName("release") {
            isMinifyEnabled = false
            signingConfig = if (keystore.isNullOrEmpty()) signingConfigs.getByName("debug") else signingConfigs.getByName("release")
        }
    }
    packaging {
        jniLibs {
            // crawl is run as a program, so the libraries are unpacked on install
            // (android:extractNativeLibs); the Rust library is stripped by cargo already
            useLegacyPackaging = true
            keepDebugSymbols += "**/libcrawl.so"
        }
    }
    // the engine's data has dotfiles (dat/.../.gitignore are not needed, but crawl's des cache
    // names are plain): keep everything but version-control leftovers
    androidResources {
        ignoreAssetsPattern = "!.svn:!.git:!.ds_store:!*.scc:!CVS:!thumbs.db:!picasa.ini:!*~"
    }
}
