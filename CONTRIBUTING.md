# Contributing

Bug reports and questions go to [GitHub
issues](https://github.com/maciej-rosiek/isorun/issues). Every change is held to
the same rules: match WebTiles, work with a gamepad, and never break the
keyboard.

## Layout

`native/` is the app, a Cargo workspace of small crates, most with a README
([native/PORTING.md](native/PORTING.md) maps them); `native/assets` and
`native/fixtures` hold what it embeds and what its tests replay. `engine/` builds
crawl, `worker/` is the hosted page's Cloudflare Worker, `tools/record` captures
fixtures from a live server and `tools/build` packs tiles and draws the brand
artwork. Those outputs are committed, so an ordinary build never runs the tools.

## Commands

```
cd native && cargo test --workspace     # the app's tests
npm install && npm run check            # the Worker and the web build's engine bridge
native/web/build.sh && node native/web/test.mjs   # the web build, in headless Chromium
```

Run the tests of what you touched before calling a change done. The renderer's
test bed is `isorun --testbed` (`?testbed` on the web build).

## Fixtures

Prefer a recorded fixture over a hand-written one.

- `npm run record -- wss://crawl.dcss.io/socket 60 out.ndjson` spectates the
  least idle player for 60 seconds and writes every server message to NDJSON.
  Spectating needs no login. The reducer tests replay such files
  (`native/fixtures`).
- `node tools/record/menus.mjs wss://crawl.dcss.io/socket 30 outdir` collects
  every menu the spectated players open, one JSON fixture per menu type
  (`native/fixtures/menus`).

## Live testing

Spectating needs no account: CUE (`wss://underhound.eu:8080/socket`) lets
anyone watch. Anything that logs in to a real server reads its credentials from
a gitignored `.env`; never commit it and never paste a password into a fixture.

## Changelog

`CHANGELOG.md` is public facing and shown in the app. Keep entries short: one
line per change, what changed and why it matters to a player.
