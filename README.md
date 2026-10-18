# ComSpotter

ComSpotter watches radio activity in [TrackAudio](https://github.com/pierr3/TrackAudio) and makes [ChasePlane](https://parallel42.com/products/chaseplane) automatically track the aircraft that is transmitting in Microsoft Flight Simulator 2024. When nobody is talking for a while, it re-enables ChasePlane's auto-spot. Optionally, it also switches to the saved tower view closest to the aircraft.

## Watch a demo

Check out this video to see what ComSpotter does. Not a single user input required.
On the left you'll see the ChasePlanes spotting interface and ComSpotters log so you can see what's happening behind the scenes.

[![Watch a demo](https://img.youtube.com/vi/6AYqGIecpmw/hqdefault.jpg)](https://www.youtube.com/watch?v=6AYqGIecpmw)

## Quick start

1. Start Euroscope and connect to Vatsim as a controller or observer. You need to use the TrackAudio client. 
2. Open MSFS and start a Free Flight on your chosen airport.
3. Open vPilot and connect using `.towerview`.
4. Run `comspotter.exe` (or `cargo run`/`cargo run --release` when building from source). The ComSpotter window opens and shows the connection status and what is being tracked.
5. On first start, `comspotter.options.toml` is created next to the executable with default values. Change options in the **Options** window (applied immediately, no restart needed) or edit the file and restart ComSpotter.

ComSpotter connects to ChasePlane and TrackAudio on their default local addresses unless changed in the options. If you are running TrackAudio and MSFS/ChasePlane on different devices you'll need to change these. If a connection fails or drops, ComSpotter keeps retrying in the background.

**Note:** For ComSpotter to track an aircraft it needs to be visible in the Chaseplane spotting menu. If any aircraft are not visible there, check the `AI Traffic Settings`. You can find them on the `Spotting` page in ChasePlane via the cogwheel icon next to `Traffic`. You can adjust filtering and range settings there.

## User interface

The GUI is built with [egui](https://github.com/emilk/egui) and runs on Windows, macOS and Linux.

**Main window**

- Connection status of ChasePlane and TrackAudio.
- The aircraft currently tracked in ChasePlane and whether that happens via auto-spot.
- Who is transmitting right now and who transmitted last (and whether that station is a known aircraft).
- The active airport and the view ComSpotter last switched to.
- **Pause tracking**: transmitting aircraft are no longer tracked while paused.
- **Pause auto-spot**: turns ChasePlane's auto-spot off and stops ComSpotter from enabling it (including its view switching) while paused.
- Buttons to open the **Details** and **Options** windows.

**Details window** shows the camera position and look-at position, all known traffic with every field ChasePlane sends, the available views (with the distance to the tracked aircraft), the options in effect and a log.

**Options window** edits all options. *Save & apply* writes them to the options file and applies them right away. Changing a URL or the view profile theme reconnects; everything else applies in place.

### Headless mode

Run `comspotter --headless` to run without a window. Activity is logged to the console and options are read from the options file at startup. Press Ctrl+C to stop the app.

### Linux

Building on Linux needs the usual windowing development libraries, e.g. on Debian/Ubuntu: `sudo apt install libxkbcommon-dev libwayland-dev libx11-dev libxcursor-dev libxi-dev libxrandr-dev libgl1-mesa-dev`.

## Options

Options live in `comspotter.options.toml` next to the executable, grouped into the sections `[chaseplane]`, `[trackaudio]` and `[tracking]`. They can be edited in the Options window or in the file itself (the file is rewritten, including its comments, when saving from the window). All durations are in milliseconds. Missing or invalid values fall back to their defaults.

### [chaseplane]

| Option | Default | Description |
|--------|---------|-------------|
| `url` | `"ws://127.0.0.1:8652/"` | WebSocket URL of the ChasePlane API. |
| `view_switching` | `false` | After tracking a station, also switch to the saved ChasePlane view (of the active airport) closest to the aircraft. |
| `force_view_switch` | `false` | By default no switch happens if the closest view is already the current one. Set to `true` to switch anyway which will cause a cut like transition rather than a smooth pan. Does not apply to auto-spotting. |
| `auto_spot_view_switching` | `true` | Also switch to the closest view when ChasePlane's auto-spot picks a new aircraft to follow. Only has an effect if `view_switching` is `true`. |
| `auto_spot_continuous_view_switching` | `true` | While auto-spot follows an aircraft, keep switching to the closest view as it moves. Set to `false` to switch only once when auto-spot starts following an aircraft. |
| `view_profile_theme` | `"WORLD_TOWER"` | Only ChasePlane views with this profile theme are used for view switching. `WORLD_TOWER` corresponds to the default "Tower" Group in ChasePlane |

### [trackaudio]

| Option | Default | Description |
|--------|---------|-------------|
| `url` | `"ws://127.0.0.1:49080/ws"` | WebSocket URL of the TrackAudio instance (full URL, `host` or `host:port`). |

### [tracking]

| Option | Default | Description |
|--------|---------|-------------|
| `track_threshold_ms` | `500` | Minimum continuous transmission time before a station is tracked in ChasePlane. |
| `auto_spot_threshold_ms` | `10000` | Idle time with no known traffic transmitting before ChasePlane's auto-spot is enabled. |
| `scene_change_threshold_ms` | `5000` | Minimum time between scene changes (tracking switches). |

## Setup tips

### ChasePlane

- Set up a fixed tower view, or several, for use with the `view_switching` option. For example place cameras directly above the control tower and apron control towers, if there are multiple and save them all in the Tower group. With `view_switching = true` ChasePlane will switch to the closest camera to the transmitting aircraft.
- By default the ChasePlane tower view group (`WORLD_TOWER`) is used. This can be changed with `view_profile_theme` in the options.
- A view with **"Skip when cycling views"** active is not used.

### AutoFPS and MSFS graphics settings

AutoFPS is optional but recommended, since it makes changing these values easy.

Fixed cameras like to zoom in a lot, especially with a single view on a large airport, so ground textures tend to get blurry quickly. If you use it as a tower view while controlling, it's a good idea to put MSFS in a small window to reduce the render resolution and improve performance.

In AutoFPS:

1. Choose a new user profile under flight type.
2. Set a reasonably low FPS target, such as 30.
3. Increase the TLOD and OLOD values to something large, such as 500 to keep textures sharp at longer distances (depending on your PC's performance).
4. If view changes lag, start reducing TLOD to find a sweet spot.

The same values can be set manually in the MSFS graphics settings if you don't use AutoFPS, but it's quite a hassle to change them often.

### Keeping the sim window on top

To keep the small sim window on top while controlling, the **Always on top** feature in [Windows PowerToys](https://learn.microsoft.com/windows/powertoys/always-on-top) works great.
