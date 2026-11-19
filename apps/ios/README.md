# Venti for iOS

A SwiftUI shell over the Rust core. The shell draws and talks to iOS; everything else lives in `crates/core` and below.

## Building on a Mac

Needs Xcode 26 or later, Rust from [rustup.rs](https://rustup.rs) (the toolchain in `rust-toolchain.toml` installs itself) and Homebrew.

```bash
apps/ios/scripts/open-in-xcode.sh
```

That builds the Rust core, installs XcodeGen if it is missing, generates `Venti.xcodeproj` and opens it. Pick an iPhone simulator and press Run.

Run `apps/ios/scripts/build-core.sh` again whenever the Rust core changes, and `xcodegen generate` in `apps/ios` whenever files are added or `project.yml` changes.

## Running on your own iPhone

The simulator needs no signing. A real iPhone does, and the settings for it are personal, so they live in `Config/Local.xcconfig`, which git ignores. The setup script creates it from `Config/Local.example.xcconfig`; that file explains the two options: a free Apple ID (builds last seven days) or a certificate and provisioning profile you already have.

Never commit a certificate, a profile or a team ID.

### Without building on a Mac

Every push that touches the app also builds it for a real iPhone without a signature, as the `venti-unsigned-ipa` artifact of the `iOS` workflow run. Sign that file with your own certificate and provisioning profile, using whatever tool you sign apps with, and install the result. The profile must list your iPhone, and the signing tool will set the bundle identifier the profile allows.

## Layout

| Path | What it is |
|---|---|
| `project.yml` | The Xcode project description. `Venti.xcodeproj` is generated from it and not committed. |
| `Venti/App/` | The app's entry point, its tabs and where the pilot is in them. |
| `Venti/Design/` | Colours, number formatting and the metric component. The rules behind them are in `docs/design.md`. |
| `Venti/Logbook/` | The logbook screen, its model over the core, and import. |
| `Venti/Flight/` | One flight: map, panel, altitude chart, details sheet. |
| `Venti/Fly/` | The cockpit: the Fly tab, the cockpit screen and its map, and the sensors (position, pressure, Bluetooth vario, vario sound). |
| `Venti/Map/` | The map tab, the shared map setup and the style preparation. |
| `Venti/Settings/` | The settings tab and the credits. |
| `Venti/Assets.xcassets/` | The app icon, drawn by `tools/app-icon/make_icon.py`. |
| `Venti/Info.plist` | Declares the flight-file type the app opens. Xcode generates the rest. |
| `VentiTests/` | Unit tests that run inside the app on a simulator. |
| `VentiUITests/` | Tests that use the app like a pilot and keep a screenshot of every screen. |
| `Packages/VentiCore/` | The Rust core as a Swift package. Its binary and its generated Swift interface are build products. |
| `Packages/VentiCore/Tests/` | Tests of the Rust-to-Swift bridge. They run on the Mac with `swift test`, no simulator needed. |
| `scripts/build-core.sh` | Builds the core for device, simulator and Mac and packages it. |
| `scripts/open-in-xcode.sh` | From a fresh clone to the project open in Xcode. |
| `Config/` | Build settings. `Local.xcconfig` holds personal signing settings and is not committed. |
| `Venti/Resources/MapDemo/` | Label fonts, and a small map region for tests and offline development: base map and terrain. Rebuilt by `tools/demo-map/build.sh`. |
| `../../map/style.json` | The map style shared by all platforms. |

## What the app does today

- **Logbook.** Import IGC flight files with the plus button, or open one from Files or Mail with "Open in Venti". Flights are listed by year with totals. They are stored on the iPhone only.
- **Flight.** The track on a terrain map, coloured by climb and sink, with the altitude profile below. Drag along the profile to move a marker along the track and read time, altitude and climb at that moment. The details list every thermal and glide, and how the flight scores as a cross-country flight; choose a thermal or glide to mark it on the map. Time on the ground before take-off and after landing is left out.
- **Map.** A terrain map with every logbook flight on it and, on request, your position.
- **Fly.** The cockpit. "Start flight" switches on the position, the pressure sensor and, if you have chosen one, your Bluetooth vario; it records from that moment, with the screen off too, and saves the flight to the logbook when you finish. It shows altitude, climb with a vario tone, speed, glide ratio and distance to the take-off; when you circle in lift it changes to height gained, the thermal's average and the wind. "Replay the sample flight" runs a recorded flight through the same screen.

- **Settings.** Your name and glider, which go into the flights you record; the vario sound; and the credits for the map data.

To connect a vario, open Fly, tap "Vario" and choose it from the list while it is switched on. Once connected, the same sheet shows the last lines it sent and has "Share a report": if the cockpit shows no climb from your vario, that report says why.

The cockpit has been tested with replayed flights on a simulator. It has not been flown, and nobody has yet connected a real vario to it. Fly it only beside the instrument you trust.

A new logbook offers a sample flight. It is made up, generated by `venti-flight`, not a real pilot's.

Map tiles are fetched from OpenFreeMap and Mapterhorn until the project has its own server; `docs/architecture.md` has the terms. Label fonts are inside the app.

## Screenshots without a Mac

Every push that touches the app runs the UI tests on GitHub's Mac runners. They save a screenshot of each screen, in light and dark appearance, as the `ios-screenshots` artifact of the `iOS` workflow run.
