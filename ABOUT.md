# About Isorun

Isorun is an unofficial [DCSS](https://crawl.develz.org) client that draws
the dungeon isometrically: the game's own tiles stood up in 3D, with lights,
shadows, fog of war, water, doors and stairs, and a camera you can turn, tilt
and zoom. It plays on the public
[WebTiles](https://crawl.develz.org/wordpress/howto) servers, or on your own
device with no server at all, with a gamepad, a keyboard or a mouse.

If you play on WebTiles, you keep your game, your account, your rc file and
your keys. Isorun adds the view and the controls on top, and asks you to
relearn nothing.

## Getting started

Pick a server. Log in with your WebTiles account, register a new one, or
spectate without one. Then start a game. There is nothing to install and no
Isorun account: your browser talks to the DCSS server directly.

## Features

- **The real game on the real servers.** Isorun is a WebTiles client, not a
  port. You log in to CDI, CDO or any other public server with the account
  you already have. Your saves, rc file, macros, morgues and scoreboard
  entries live on the server, just as they do in the official client. New
  releases and trunk work as soon as a server runs them.
- **Isometric.** The level drawn from above and to the side, with the camera
  turning, tilting and zooming, and directions that follow the screen. The
  level map, the monster list, the stats and the message log are all still
  there. Fights show their blows, hits and deaths.
- **Gamepad.** Autoexplore, autofight, rest, travel, your gear, casting,
  firing and examining are each one button away, and a bar shows what each
  button does right now. An on-screen keyboard covers prompts, inscriptions
  and chat.
- **The keyboard and the mouse.** Every key goes to the server as the
  official client sends it. Settings has the controls.
- **Offline.** Play with no server at all. The current release or trunk runs
  in your browser, and your saves stay on your device. Offline games don't
  reach a scoreboard.
- **Spectating**, from a link you can share.

## Version support

Isorun reads each server's own game data when it connects, so it keeps up
with stable and trunk without any work per release. A new tile or enum shows
up as soon as the server publishes it. Anything the server doesn't publish is
left undrawn rather than guessed at.

## Security and privacy

Isorun has no accounts of its own. Your browser connects straight to the DCSS
server you choose, over an encrypted WebSocket, just as the official WebTiles
client does. Your password is sent only in the login message and is never
stored. Saving a login keeps the server's login token, the same way the
official client keeps a cookie. Settings and tokens stay in your browser's
local storage and go nowhere else.

Your rc options are read from the server for each account, server and game
version. Isorun never caches or overrides them.

The hosted app relays the tile data your browser needs from the server you
picked. Your game traffic never passes through it.

The hosted app counts visits, so its developer knows if anyone plays. Each
page load gets a random number, which is forgotten when you close or reload
the tab. While the tab is in view, the page sends that number every 30
seconds, with whether you play or watch a game and on which server. The site
keeps this with your country and the times, for a year. It keeps no IP
address, no account name and no cookie, and nothing goes to anyone else. The
desktop app sends no counts at all.

## Support the people who run it

Isorun is only a client. The game is DCSS, made by a team of volunteers, and
every online game is played on a server that someone pays for and looks
after. Isorun takes no money; if you want to give some, give it to them.

- **CDI** (crawl.dcss.io) is funded through its admin's
  [Patreon](https://www.patreon.com/gammafunk).
- **CBR2** is funded through its admin's
  [Patreon](https://www.patreon.com/zureal).
- **DCSS** itself has no donation page. Bug reports, playtesting and code on
  [GitHub](https://github.com/crawl/crawl) are how its players give back.

## Credits

Isorun is a fork of [Orbrun](https://github.com/Caeous/orbrun) by Caeous: the
WebTiles client, the gamepad system, the menus and the packages under it are
Orbrun's work, and Isorun replaces its first-person view with the isometric
one. Thanks to Orbrun, and to [PocketZot](https://pocketzot.app/about), the
DCSS client for phones, for the ideas Orbrun took from it.

The game is [Dungeon Crawl Stone Soup](https://crawl.develz.org), by its
developers and contributors. Its tiles, which Isorun draws, are the work of
the DCSS tile artists, released under CC0.

Most of the code was written with Claude Code. [The source](https://github.com/maciej-rosiek/isorun) is licensed under
[AGPL-3.0-or-later](LICENSE), and [ATTRIBUTION.md](ATTRIBUTION.md) explains
how it relates to Orbrun and DCSS.
