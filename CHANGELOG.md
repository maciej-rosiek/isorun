# What's new

Notable changes to Isorun, newest first.

## 0.1.6 — 2026-10-22

- A blow on an invisible monster ("You hit something.") shows and sounds: you swing, the hit lands on the cell you struck at with its sparks, its sound and its damage number; a blow from one ("Something hits you.") hurts you on the screen too.
- Aiming with the arrow keys and stepping with Space no longer opens a quick slot or a screen now and then: Tab (autofight) no longer puts the keyboard on the HUD's buttons.
- Android: no more "Isorun isn't responding" after the phone is locked and unlocked: the game waits for the lock screen to go before it draws again.
- Android: the sound comes back after the phone is locked or the app left: it pauses while the game is away and starts afresh when it returns.
- You, the monsters, the items and the fountains and plants look less flat: each part of a sprite (a head, an arm, a belly) is rounded and catches the light on its own, and everything that moves casts a soft shadow on the floor, away from the nearest wall torch. Tune the look's new Sprites section sets the relief, the form light and the shadows, or turns them off.
- Tune the look switches Sight light on and off at its top, so you can compare while you tune.
- A Sight light setting (Graphics, off by default): you carry a light whose shadows are exactly what you can see, and what you only remember, water and lava too, is drawn as a cool blue map.
- A Frame rate setting (Graphics): the app draws at most 60 frames a second by default, which saves a phone's battery and keeps it cool; 30, 90, 120, 144 or Unlimited are there too.
- A new look, Two-tone: a warm light from the left and a cool one from the right of the screen, so every wall shows at a glance which way it faces, whichever way you turn the view.
- A new look, Ink: the dungeon in grey ink on paper, and only what matters in colour: you, the monsters, the items, doors, stairs, altars and the like.
- A travel-excluded area shows on the dungeon floor again: crawl's red cross on each excluded cell, as the minimap has it.
- The inventory shows the right card when a weapon or armour and a scroll or potion share a letter (crawl now lets them); pointing at one no longer lights up both.
- Loot as in Diablo: an item a monster drops as it dies flies up out of it, turns over and lands on the floor with a clink.
- The dungeon has no sun any more: an even light and crawl's torch walls light it, so a narrow corridor is no darker than a room and the torches are the bright places; each frame does less work. The looks' Sun and Shadows settings are gone.
- The torches are crawl's own: the torch walls some vaults have light their rooms with a warm light, and a wall by a torch, or by several, no longer glares white. The torches Isorun added to the walls, and the sparks around them, are gone.
- The parts of a level above or left of where you came in are no longer drawn as if in sight with no torch light: what you only remember is darker there too, and the torch walls light them.
- Good items on the floor stand in a slim column of light with a soft glow around them: orange for a random artefact (legendary), gold for a named artefact (unique); a dropped one chimes when it lands.
- Find (Ctrl-F, the dock's Find) is a screen to browse: everything you have seen, on every level, in categories (weapons, armour, potions, scrolls, wands, jewellery, stairs, altars, shops, portals and more) with counts, your level first and new things on top; a tap travels there, a long press shows the details, and the search box still asks crawl.
- Offline games no longer freeze the app (and Android no longer says it is not responding) when you tap or press keys faster than crawl answers, as in a fight at low health; your keys wait for crawl instead of the screen.
- On a phone crawl's text prompts (Find with Ctrl-F, inscribe, naming, a menu's filter) bring up the phone's keyboard, in the app and in the browser; the field stands at the top where the keyboard cannot cover it, with Cancel and OK buttons, and a tap on it brings the keyboard back.
- On a touch screen an item's card (and the dock's, the action panel's and the cell's) shows only while your finger rests on it and goes when you lift it; a tap opens the item's actions with no card left over the backpack.
- In a phone's browser a tap on the dungeon walks there again, and the Settings over the game take taps (a finger that moved a little chose nothing); a tap beside them goes back.
- On a phone, menus where you choose several things (pick up, drop) have an Accept button that takes what you chose, gold and with the count ("Accept (2)"), and Select all where crawl has it; a tap on the bar's buttons no longer closes the menu. On a pad, Start accepts.
- Every screen that scrolls shows a thin gold scroll bar, on a phone too: the title screen, accounts, servers, Watch, What's new, About, the rc file, character creation and crawl's menus.
- A Minimap setting (Settings, Interface): Auto hides the minimap on a phone, where it covered the view, and shows it on a bigger screen; On and Off always do. The level map (X) is there either way.
- The minimap is smaller on a mid-size window (about 250 points on 1024x768, where it was 380).

## 0.1.5 — 2026-10-16

- Starwater shows a night sky: dark blue water with drifting violet and stars that twinkle and glow.
- Mud from a liquefying spell is thick wet brown mud, the floor of the Slime Pits shines under a film of slime, and corroding ground bubbles yellow-green; the cell card says mud and corroding ground.

## 0.1.4 — 2026-10-16

- Lava looks like lava: dark plates of crust on molten rock that flows and glows, with a red light on the floor around it. It was drawn as water.
- Swamp water is murky and green, the toxic bog bright green, and shallow water lighter than deep water, so you see where you can wade.
- The card about a cell names its liquid: deep water, shallow water, toxic bog or lava.

## 0.1.3 — 2026-10-16

- LT held to explore (Settings, Controls, LT explore: Hold) stops the walk when you let go, in a game on this device too: the offline game read the stop only once the walk was over.
- The card about the cell under the cursor says nothing about a wall, where it showed the wall's tile name ("Brick dark 1").

## 0.1.2 — 2026-10-16

- The desktop app starts from Steam on Linux: Steam's overlay crashed it at launch.

## 0.1.1 — 2026-10-16

- The desktop app's AppImage starts on a Linux with no libfuse2, such as CachyOS on a ROG Ally.

## 0.1.0 — 2026-10-15

- The dock: health and magic orbs, ten quick slots for spells, abilities and consumables, the experience bar, the actions the square offers and a button for every screen, so everything is a click or a pad press away; the inventory has a paper doll, and the character sheet every number. Settings, Interface, HUD switches back to the classic panes.
- Levelling up: a LEVEL banner with the hit points, magic and attributes it brought, and the attribute choice of every third level as three cards on the same screen, for the mouse, the keys and the pad.
- The game drawn isometrically: the DCSS tiles in 3D, upscaled, with lights, shadows, fog of war, water, doors and stairs; the camera turns, tilts and zooms; directions follow the screen; fights show blows, hits and deaths.
- Sound: steps, blows, hits and deaths, doors, stairs, pickups, gold, spells, potions, scrolls, menus, a heartbeat at low health, the drip of the dungeon, water and lava near you, and music by place; the Audio settings set the volumes and mute.
