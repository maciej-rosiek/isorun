# @isorun/render-iso

`IsoRender3d` draws a `@isorun/scene` `Scene` isometrically with three.js and
an orthographic camera. It implements `MapRenderer`, so it is interchangeable
with `@isorun/render-2d` and `@isorun/render-3d`.

The look is "DCSS HD-2D": the DCSS tiles, upscaled 4x (Scale2x twice) with
generated normal maps, under lights with shadows. Walls are blocks of the
wall tiles with plain stone caps, floors and decals lie flat, doors and stairs
are built in 3D from the level's own tiles, and monsters, items and upright
features stand as sprites that turn with the view. Walls between the camera
and the player drop to stubs. Some walls carry a torch (a KayKit model) that
lights the room.

It runs on three's `WebGPURenderer` (with its WebGL2 fallback). `setLights`
sets the look (`IsoLights`): the fill light, a sun from the left of the
screen, the shadow-casting torch lights, the shading (`shading.ts`: matte
light that wraps around surfaces and can step into flat tones), painted
tiles (a Kuwahara filter, a lift of the dark tones and fewer tones when a
tile is upscaled), bevelled wall tops with moss, and the screen effects:
ambient occlusion (GTAO), outlines, a painted (Kuwahara) screen filter,
bloom, a tilt-shift blur and a colour grade with a vignette. The `diorama`
preset is the bright, clay-like look; the others are dark dungeons.

The renderer reads the view from the `Camera`: `yaw` turns the ground (0 =
north is up the screen), `-pitch` is the camera's elevation over the ground,
and `eyeX`, `eyeY` is the cell the view centres on. `setView` sets the zoom
and the wall height (`IsoView`), `setAim` shows the steering arrow from the
player to a neighbouring cell, `setAssets(IsoAssets)` gives it the torch model,
and `act(id, action, target)` plays an attack, a hit or a death on an actor.

`stickToDir(x, y, yaw)` turns a stick or key vector in screen space into the
compass direction it points at on the ground, for the current yaw.

Depends on `@isorun/scene`; `three` is a peer dependency. AGPL-3.0-or-later.
