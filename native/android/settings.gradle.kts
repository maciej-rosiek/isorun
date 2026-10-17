// The Android app of the native build: a NativeActivity APK around the Rust app
// (crates/app, android_main) and crawl, built by build.sh. See README.md.
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}
dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
    }
}
rootProject.name = "isorun-native"
include(":app")
