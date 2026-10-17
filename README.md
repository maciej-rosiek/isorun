<p align="center"><img src="assets/logo.svg" alt="Isorun" width="560"></p>

# Isorun

**Dungeon Crawl Stone Soup drawn isometrically, played with a gamepad, keyboard, mouse or touch.**

Isorun is an unofficial client for [Dungeon Crawl Stone
Soup](https://crawl.develz.org/). It talks to the public DCSS servers over
[WebTiles](https://crawl.develz.org/wordpress/howto), the same protocol the
official client uses, and replaces everything you see and touch: the DCSS
tiles are stood up in 3D and drawn from an isometric camera that turns, tilts
and zooms, with lights, shadows, fog of war, water, doors and stairs. It plays
on your own WebTiles account, or offline with crawl built in.

Isorun is a fork of [Orbrun](https://github.com/Caeous/orbrun) by Caeous.

**Play it in the browser: <https://isorun.maciej-rosiek.workers.dev>.** No
install, and no Isorun account: log in with your WebTiles account, register
one, or watch without one. What the hosted page counts is in
[ABOUT.md](ABOUT.md#security-and-privacy).

**Or get the app** from the [latest release](https://github.com/maciej-rosiek/isorun/releases/latest),
with offline games: `Isorun-x86_64.AppImage` (Linux, a Steam Deck or a ROG Ally; [STEAM.md](STEAM.md)
puts it in Steam's library), `Isorun-linux-x86_64.tar.gz`, `Isorun-win-x64.zip` (Windows) and
`Isorun.apk` (Android).

## Build it

Isorun is one Rust app (egui and wgpu) for the desktop, Android and the web, in
[`native/`](native/PORTING.md):

```sh
engine/native.sh stable                            # crawl, for offline games (once)
cd native
cargo run --release -p isorun-app --bin isorun     # the desktop app
cargo test --workspace
```

- the desktop packages: [native/desktop](native/desktop/README.md);
- the APK: [native/android](native/android/README.md);
- the web build, the hosted page's site: [native/web](native/web/README.md), served by the
  Cloudflare Worker in [worker/](worker/index.ts);
- crawl itself, native and WebAssembly: [engine/](engine/README.md).

Most crates have a README; [native/PORTING.md](native/PORTING.md) maps them.
[CONTRIBUTING.md](CONTRIBUTING.md) is the short guide, [ABOUT.md](ABOUT.md)
the tour and the credits, [CHANGELOG.md](CHANGELOG.md) what's new.

## License

[AGPL-3.0-or-later](LICENSE). Isorun is an independent project, not affiliated
with or endorsed by the DCSS development team. [ATTRIBUTION.md](ATTRIBUTION.md)
explains how it relates to Orbrun and DCSS and lists the third-party credits.
