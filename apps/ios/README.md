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
| `Venti/Assistant/` | The assistant: the chat screen, sending requests, the key in the Keychain. |
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

- **Logbook.** Import IGC flight files with the plus button, or open one from Files or Mail with "Open in Venti". Flights are listed by year with totals and each flight's scored distance. Tap the totals for statistics: the last twelve months, your gliders and your bests. Everything is stored on the iPhone only.
- **Flight.** The track on a terrain map, coloured by climb and sink, with the altitude profile below. Drag along the profile to move a marker along the track and read time, altitude and climb at that moment. The details list every thermal and glide, and how the flight scores as a cross-country flight; choose a thermal or glide to mark it on the map. Time on the ground before take-off and after landing is left out.
- **Map.** A terrain map with every logbook flight on it and, on request, your position. With a Venti server set, it also shows the live wind: every station as an arrow pointing where the wind blows, with mean wind and gusts in km/h. Tap an arrow for the station's last 24 hours, or the wind button for all stations in view as a list. A station whose reading is more than an hour old is a grey dot. Zoomed in to a valley, the map also shows the take-offs and landings the server knows from OpenStreetMap, which has only part of them; a site's page and a station's page lead to the forecast for the place: the wind at each height through the day, cloud base, cloud, rain and temperature, as one weather model computes them.
- **Fly.** The cockpit. "Start flight" switches on the position, the pressure sensor and, if you have chosen one, your Bluetooth vario; it records from that moment, with the screen off too, and saves the flight to the logbook when you finish. It shows altitude, climb with a vario tone, speed, glide ratio and distance to the take-off; when you circle in lift it changes to height gained, the thermal's average and the wind. A line under the map shows the way to the place you are flying to and the height at which you are expected to arrive: the take-off until you tap the line and choose one of your waypoints. Waypoints come from files (SeeYou `.cup`, GEO `.wpt`), brought in under Settings or from that list. With an airspace file brought in under Settings (OpenAir format), the cockpit warns when you come near airspace a paraglider must stay out of, are about to enter it or are inside: a band over the map, spoken once, which you can quiet. It knows only the airspace in your files and what it fetched from your server, and not what is active today. With a task (Fly, Task: paste one from a message or open an `.xctsk` file, the format XCTrack exchanges) the cockpit counts down to the start, leads from turnpoint to turnpoint along the shortest way through their cylinders, and shows how far it is to goal and the glide that needs; the task's page hands it on as a QR code or a file. Scanning a QR code is not built. "Replay the sample flight" runs a recorded flight through the same screen.

- **Assistant.** Ask about your own flying ("how was my season?", "where were my strongest thermals?"). It runs on your own Anthropic API key, which you paste in once and which stays in the iPhone's Keychain; your questions and what it looks up in your logbook go from the phone to Anthropic and are billed to your account. With a Venti server set it can also look up live wind ("how is the wind on the Zugspitze?"), from the same stations as the map; the Venti server is then asked for the stations around that place and sees neither your key nor your question. It can also look up the airspace over and near a place there. It cannot see forecasts or flying sites yet. It has only ever talked to a scripted stand-in in tests: the first conversation with the real service is yours.
- **Account.** Optional. Signed in to a Venti server (Settings, Account), your flights are copied there, appear on your other devices, and can be shared: open a flight, More, "Share with a link", and choose who may see it. A flight is private until you do. Sync runs when you open the app and after the logbook changes; a flight you delete is deleted on the server and on your other devices too. The session is kept in the Keychain. Signing out or deleting the account leaves the flights on this iPhone.
- **Settings.** Your name and glider, which go into the flights you record; the vario sound; your waypoints; your airspace files; your glider's trim speed and glide ratio, which the arrival prediction uses; the address of the Venti server the wind comes from; and the credits for the data.

There is no public Venti server yet. To see the wind map today, start one that answers from recorded data on your Mac (`cargo run -p venti-server -- dev` in the repository) and type `http://127.0.0.1:8080` as the server address in the app on the simulator. It shows six stations as they were on the evening of 18 November 2026. The same server has a demo account to try signing in with: handle `demo`, password `demo-pilot-password`, with three made-up flights.

To connect a vario, open Fly, tap "Vario" and choose it from the list while it is switched on. Once connected, the same sheet shows the last lines it sent and has "Share a report": if the cockpit shows no climb from your vario, that report says why.

The cockpit has been tested with replayed flights on a simulator. It has not been flown, and nobody has yet connected a real vario to it. Fly it only beside the instrument you trust.

A new logbook offers a sample flight. It is made up, generated by `venti-flight`, not a real pilot's.

Map tiles are fetched from OpenFreeMap and Mapterhorn until the project has its own server; `docs/architecture.md` has the terms. Label fonts are inside the app.

## Screenshots without a Mac

Every push that touches the app runs the UI tests on GitHub's Mac runners. They save a screenshot of each screen, in light and dark appearance, as the `ios-screenshots` artifact of the `iOS` workflow run.
