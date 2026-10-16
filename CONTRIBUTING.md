# Contributing

Bug reports and questions go to [GitHub
issues](https://github.com/maciej-rosiek/isorun/issues). Every change is held to
the same rules: match WebTiles, work with a gamepad, and never break the
keyboard.

## Layout

npm workspaces. `apps/isorun` is the Vite app; `packages/*` are the
libraries, each with a README (see [README](README.md)); `tools/record`
captures fixtures from a live server; `tools/build` packs the front room's
tiles and draws the brand artwork. Those outputs are committed, so an ordinary
build never runs the tools.

## Commands

```
npm test            # vitest run
npm run typecheck   # tsc -b
npm run build       # build apps/isorun
npm run dev         # dev server (Vite, with the gamedata proxy)
```

Run `npm test` and `npm run typecheck` before calling a change done. The
renderer test bed is at `/testbed/iso.html` on the dev server.

## Fixtures

Prefer a recorded fixture over a hand-written one.

- `npm run record -- wss://crawl.dcss.io/socket 60 out.ndjson` spectates the
  least idle player for 60 seconds and writes every server message to NDJSON.
  Spectating needs no login. The reducer tests replay such files.
- `node tools/record/menus.mjs wss://crawl.dcss.io/socket 30 outdir` collects
  every menu the spectated players open, one JSON fixture per menu type.

## Live testing

Anything that logs in to a real server reads its credentials from `.env`,
which is gitignored. Copy `.env.example` to `.env`, fill it in, and run such
scripts with `node --env-file=.env ...`. Never commit `.env` and never paste a
password into a fixture.

## Changelog

`CHANGELOG.md` is public facing and shown in the app. Keep entries short: one
line per change, what changed and why it matters to a player.
