# The hosted page's Worker

`isorun` on Cloudflare (`wrangler.jsonc` at the repository root), at
https://isorun.maciej-rosiek.workers.dev. Its static assets are the web build
(`native/web/build.sh` writes the whole site into `native/web/dist`); this Worker runs for the
paths `run_worker_first` names and for any path that is no file.

| path | what |
|---|---|
| `/` and every file of the build | the static assets, with the build's `_headers` |
| any other page load (`/native/`, the old client's `/watch/...`) | a 301 home, the query kept: the app does not route on the path |
| `/gamedata-proxy/<host>/gamedata/<hex version>/<file>` | a server's gamedata with CORS headers (`gamedata-proxy.ts`: only the files the client loads), cached at the edge |
| `/morgue-proxy/<host>/<path>.txt` or `.where` | a server's morgue or `.where` file, briefly cached |
| `POST /api/visit?id&e&m&s&t` | a visit beacon (`client::visits` in the app; `visit-beacon.ts` the format), into the `VisitLog` Durable Object (`visit-log.ts`) |
| `/stats?key=<STATS_KEY>` | who is on the page and in a game now, and visits, players and play time per day |
| `/engine/*` | the offline engine's Worker through the `ENGINE` service binding; unbound, a 404 |

`npm run check` at the root typechecks it and runs `test/` (vitest; `visits.test.ts` uses
`node:sqlite`, Node 22.5+). `npx wrangler dev` serves it with the built site locally, without a
login. `.github/workflows/isorun-cloudflare.yml` deploys it.
