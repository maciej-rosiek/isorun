//! The port of packages/gamedata/test/load-errors.test.ts (its abort tests have no
//! counterpart: the load here is synchronous and loads no images), and more ways a server's
//! files can be wrong.

mod common;

use isorun_gamedata::{gamedata_urls, load_gamedata, load_gamedata_at, ATLASES};

const SHA: &str = "acd3d60e20f899c1c8a546953d6ffa0f6c7fe0c8";

fn always(text: &'static str) -> impl FnMut(&str) -> Result<String, String> {
    move |_| Ok(text.to_string())
}

/// A shell whose gamedata proxy is not serving `/gamedata-proxy/*` answers with its own app
/// shell: HTTP 200, body HTML. The message names the file and the cause.
#[test]
fn names_the_file_and_the_cause_when_a_gamedata_url_answers_with_an_html_page() {
    let err = load_gamedata_at(
        always("<!doctype html>\n<html lang=\"en\"><head><title>Isorun</title></head></html>"),
        "/gamedata-proxy/crawl.dcss.io",
        SHA,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains(&format!(
            "/gamedata-proxy/crawl.dcss.io/gamedata/{SHA}/enums.js"
        )),
        "{msg}"
    );
    assert!(msg.contains("got an HTML page"), "{msg}");
    assert_eq!(err.file, "enums.js");
}

#[test]
fn names_the_file_when_a_gamedata_module_is_javascript_that_will_not_evaluate() {
    let err = load_gamedata_at(
        always("define(function(){ throw new Error(\"boom\") })"),
        "https://crawl.dcss.io",
        SHA,
    )
    .unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains(&format!("https://crawl.dcss.io/gamedata/{SHA}/enums.js")),
        "{msg}"
    );
    assert!(msg.contains("boom"), "{msg}");
}

#[test]
fn names_the_file_that_could_not_be_fetched() {
    let err = load_gamedata(
        |name: &str| {
            if name == "tileinfo-gui.js" {
                Err("HTTP 404".to_string())
            } else {
                std::fs::read_to_string(common::fixture_dir().join(name)).map_err(|e| e.to_string())
            }
        },
        SHA,
    )
    .unwrap_err();
    assert_eq!(err.file, "tileinfo-gui.js");
    assert_eq!(
        err.to_string(),
        format!("gamedata {SHA}/tileinfo-gui.js: HTTP 404")
    );
}

#[test]
fn names_the_module_that_is_broken_and_says_where() {
    let err = load_gamedata(
        |name: &str| {
            if name == "tileinfo-wall.js" {
                Ok("define([\"./tileinfo-floor\"], function(m) {\nvar exports = {};\nvar val = m.TILE_FLOOR_MAX;\nexports.A = val++\nexports.B = = 1;\n});".into())
            } else {
                std::fs::read_to_string(common::fixture_dir().join(name)).map_err(|e| e.to_string())
            }
        },
        SHA,
    )
    .unwrap_err();
    assert_eq!(err.file, "tileinfo-wall.js");
    assert!(err.message.contains("line 5"), "{}", err.message);
}

#[test]
fn does_not_panic_on_garbage() {
    for text in [
        "",
        "define(",
        "define(function(){ return 5; })",
        "define(function(){ var e = {}; e.x = e.y.z; return e; })",
        "define(function(){ for (;;) {} })",
        "define(function(){ function f() { return f(); } return f(); })",
        "\u{feff}define(function(){ return {}; })",
        "define(function(){ return { prepare_fg_flags: 1, prepare_bg_flags: function (t) { return t; } }; })",
        "/* unterminated",
        "define(function(){ return \"unterminated; })",
        "define(function(){ return [1, 2, 3][0x7fffffff]; })",
    ] {
        assert!(load_gamedata(always(text), "x").is_err(), "{text:?}");
    }
}

#[test]
fn a_status_icon_sizes_script_without_its_function_is_a_diagnostic() {
    let mut files = common::read_dir(&common::fixture_dir());
    files.insert(
        "status-icon-sizes.js".into(),
        "define([\"./tileinfo-icons\"], function (icons) { return {}; });".into(),
    );
    let gd = isorun_gamedata::Gamedata::from_files(&files, SHA).unwrap();
    assert!(!gd.has_status_icon_sizes());
    assert!(
        gd.diagnostics
            .iter()
            .any(|d| d.contains("has no status_icon_size")),
        "{:?}",
        gd.diagnostics
    );
    let berserk = gd.icons.id("BERSERK").unwrap();
    assert_eq!(gd.status_icon_size(berserk), 13);
}

#[test]
fn lists_what_load_gamedata_fetches_scripts_before_atlases_under_the_proxied_version_directory() {
    let urls = gamedata_urls("/gamedata-proxy/crawl.dcss.io/", SHA);
    let root = format!("/gamedata-proxy/crawl.dcss.io/gamedata/{SHA}/");
    assert!(urls.iter().all(|u| u.starts_with(&root)));
    assert_eq!(urls[0], format!("{root}enums.js"));
    let atlases: Vec<String> = ATLASES.iter().map(|a| format!("{root}{a}.png")).collect();
    assert_eq!(urls[urls.len() - ATLASES.len()..], atlases[..]);
    assert!(urls.contains(&format!("{root}status-icon-sizes.js")));
}
