# Add Isorun to Steam

The Isorun app for Linux is one file, an AppImage, so it goes onto a Steam
Deck as a non-Steam game. It plays on the public servers and offline. It
takes five steps and a few minutes.

The first four happen in Desktop Mode: hold the power button and choose
**Switch to Desktop**.

## 1. Download the app

Open **Konsole** and run this line. It downloads the newest release to the
Applications folder in your home folder and makes it executable:

```sh
mkdir -p ~/Applications && curl -fL -o ~/Applications/Isorun-x86_64.AppImage https://github.com/maciej-rosiek/isorun/releases/latest/download/Isorun-x86_64.AppImage && chmod +x ~/Applications/Isorun-x86_64.AppImage
```

If you'd rather click than type, download `Isorun-x86_64.AppImage` from the
[latest release](https://github.com/maciej-rosiek/isorun/releases/latest),
move it to a folder where it can stay, right-click it, choose **Properties >
Permissions** and tick **Is executable**.

Keep the file where it is, so Steam's shortcut stays right. To update it, run
the same line again, or let Gear Lever or AppImageUpdate replace it in place.

## 2. Add it to Steam

Open Steam, and in its top menu choose **Games > Add a Non-Steam Game to My
Library**. Click **Browse**, set the file type to **All Files**, choose
`Isorun-x86_64.AppImage` and click **Add Selected Programs**.

## 3. Name it

Right-click `Isorun-x86_64.AppImage` in your library, choose **Properties**,
and rename it to `Isorun`. Leave **Launch Options** empty: the app opens full
screen on its own.

## 4. Add the artwork (optional)

Save each image, then set it in Steam where the line says:

- [icon.png](https://isorun.maciej-rosiek.workers.dev/steam/art/icon.png): in
  **Properties**, click the square next to the name.
- [logo.png](https://isorun.maciej-rosiek.workers.dev/steam/art/logo.png): on
  the library page, hover over the banner and choose **Set custom logo**.
- [hero.png](https://isorun.maciej-rosiek.workers.dev/steam/art/hero.png): on
  the library page, hover over the banner and choose **Set custom background**.
- [grid.png](https://isorun.maciej-rosiek.workers.dev/steam/art/grid.png):
  right-click the entry and choose **Manage > Set custom artwork**.
- [portrait.png](https://isorun.maciej-rosiek.workers.dev/steam/art/portrait.png):
  right-click it in the grid and choose **Manage > Set custom artwork**.

## 5. Set up the controller

Switch back to Gaming Mode. On Isorun's page, press the controller icon, go to
**Layout > Templates** and choose **Gamepad**.

The dungeon is waiting. To leave it, choose **Quit** on the home screen.
