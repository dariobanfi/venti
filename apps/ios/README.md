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

- **Logbook.** Import IGC flight files with the plus button, or open one from Files or Mail with "Open in Venti". Flights are listed by year with totals and each flight's scored distance. Tap the totals for statistics: the last twelve months, your gliders and your bests. Everything is stored on the iPhone only. With a Venti server set, "Everyone" beside "Mine" lists the flights the pilots of that server share with everyone, the newest or the best first; one opens like a flight of your own, from its file, which is fetched for that and not kept, and its menu leads to everything else its pilot shares. What should not be there can be reported from the same menu, from a pilot's flights and from a glider's page on the map: the report goes to whoever runs the server, who sees that it came from you, and to nobody else.
- **Flight.** The track on a terrain map, coloured by climb and sink, with the altitude profile below, drawn over the ground the flight went over once the app has fetched its elevation; the marker then also reads the height over the ground. Drag along the profile to move a marker along the track and read time, altitude and climb at that moment. The details list every thermal and glide, and how the flight scores as a cross-country flight; choose a thermal or glide to mark it on the map. Time on the ground before take-off and after landing is left out. More, "Replay in the cockpit" plays the flight through the cockpit as it would have looked in the air: the way to try the cockpit against your own flying. The word "Replay" at the top of the cockpit is a menu there: hold the replay at a moment, or run it in real time or ten or twenty times faster.
- **Map.** A terrain map with every logbook flight on it and, on request, your position. With a Venti server set, it also shows the live wind: every station as an arrow pointing where the wind blows, with mean wind and gusts in km/h. Tap an arrow for the station's last 24 hours, or the wind button for all stations in view as a list. A station whose reading is more than an hour old is a grey dot. With an account on the server, a calendar button opens the plans of your groups: who means to fly where on which day, with a way to say that you are in, to post a plan, and to make a group or join one with a code. It shows who is in the air: the gliders of pilots who show their flight to everyone, as a small wing with name and altitude; tap one for how old its position is and for its flight so far as a line on the map. Zoomed in to a valley, the map also shows the take-offs and landings the server knows from OpenStreetMap, which has only part of them; a site's page lists the wind stations near it with whether their wind comes from a direction the site works in, and for a take-off the newest flights that pilots of the server flew from there and share with everyone, and a site's page, a station's page and a long press anywhere on the map lead to the forecast for the place: the wind at each height through the day, cloud base, cloud, rain and temperature, as one weather model computes them.
- **Fly.** The cockpit. "Start flight" switches on the position, the pressure sensor and, if you have chosen one, your Bluetooth vario; it records from that moment, with the screen off too, and saves the flight to the logbook when you finish. It shows altitude, climb with a vario tone, speed, glide ratio and distance to the take-off; under the altitude stands your height over the ground, from the elevation tiles the map draws its relief from, which the app fetches around you as you fly and keeps, so that the ground is there again without a network (it is a model of the bare ground: no trees, no cables); when you circle in lift it changes to height gained, the thermal's average and the wind, and the map pictures the thermal: your last circles moved with the wind to where their air is now, coloured by climb and larger where it was better, with a dashed circle to fly around where the lift is and a line of words ("Stronger 20 m NW"). The wind is worked out from your circles, in any strength, and kept by band of height with how you climbed there; from five minutes on its age stands beside it, and a line says when it leaves you little way into it. At the map's upper left stands the landing among your waypoints you would arrive highest above, with that height, by a straight glide; where the iPhone has the ground, a landing behind a ridge is not offered, and the way to a place you chose says "Ground in the way" when a ridge is. In flight, with a network, the map also shows the other pilots around who show their flight to everyone. Where the iPhone has the ground, a green line on the map shows how far a straight glide still reaches every way with a hundred metres to spare, over the ground as it is. A lock beside the button that ends the flight keeps the screen from answering touches until you hold it. The screen goes to full brightness while the cockpit is open (Settings, "In the sun"). "Live tracking" on the Fly tab and a button in the cockpit share the flight while it is under way, with people you give a link or with everyone, through your Venti server; nothing is shared unless you choose it. A line under the map shows the way to the place you are flying to and the height at which you are expected to arrive: the take-off until you tap the line and choose one of your waypoints. Waypoints come from files (SeeYou `.cup`, GEO `.wpt`), brought in under Settings or from that list. With an airspace file brought in under Settings (OpenAir format), the cockpit warns when you come near airspace a paraglider must stay out of, are about to enter it or are inside: a band over the map, spoken once, which you can quiet. It knows only the airspace in your files and what it fetched from your server, and not what is active today. A second round button on the map opens the airspace ahead from the side: the next ten kilometres on your course, or towards the place you fly to while you circle, with each airspace as a block from its floor to its ceiling and your glide drawn through it. Where the iPhone has the ground, the picture has it too, and says where your glide meets it. With a task (Fly, Task: paste one from a message, scan its QR code or open an `.xctsk` file, the format XCTrack exchanges; such a file can also be opened from Files or Mail with "Open in Venti") the cockpit counts down to the start, leads from turnpoint to turnpoint along the shortest way through their cylinders, and shows how far it is to goal and the glide that needs; the task's page hands it on as a QR code or a file, and makes a plain route out of your own waypoints. Scanning with the camera has not been tried on a phone yet. "Replay the sample flight" runs a recorded flight through the same screen.

- **Assistant.** Ask about your own flying ("how was my season?", "where were my strongest thermals?"). It runs on your own Anthropic API key, which you paste in once and which stays in the iPhone's Keychain, or at another service you set by its address ("Another service": OpenAI, OpenRouter and others that speak the chat completions format, or a model you run yourself), with that service's name for a model and your key for it; your questions and what it looks up in your logbook go from the phone to Anthropic and are billed to your account. With a Venti server set it can also look up live wind ("how is the wind on the Zugspitze?"), from the same stations as the map; the Venti server is then asked for the stations around that place and sees neither your key nor your question. It can also look up there the airspace over and near a place, what the weather models forecast for a place on a day, the flying sites near a place, and the flights other pilots shared with everyone from near a place, each with what that data is worth. It does not say whether a day or a site can be flown, and it cannot see where other pilots are, webcams or rain radar. It has only ever talked to a scripted stand-in in tests: the first conversation with the real service is yours.
- **Account.** Optional. Signed in to a Venti server (Settings, Account), your flights are copied there, appear on your other devices, and can be shared: open a flight, More, "Share with a link", and choose who may see it. A flight is private until you do. Sync runs when you open the app and after the logbook changes; a flight you delete is deleted on the server and on your other devices too. The session is kept in the Keychain. Signing out or deleting the account leaves the flights on this iPhone.
- **Settings.** Your name and glider, which go into the flights you record; the vario sound; your waypoints; your airspace files; your glider's trim speed and glide ratio, which the arrival prediction uses; the address of the Venti server the wind comes from; and the credits for the data.

There is no public Venti server yet. To see the wind map today, start one that answers from recorded data on your Mac (`cargo run -p venti-server -- dev` in the repository) and type `http://127.0.0.1:8080` as the server address in the app on the simulator. It shows six stations as they were on the evening of 18 November 2026. The same server has a demo account to try signing in with: handle `demo`, password `demo-pilot-password`, with three made-up flights.

To connect a vario, open Fly, tap "Vario" and choose it from the list while it is switched on. Once connected, the same sheet shows the last lines it sent and has "Share a report": if the cockpit shows no climb from your vario, that report says why.

The cockpit has been tested with replayed flights on a simulator. It has not been flown, and nobody has yet connected a real vario to it. Fly it only beside the instrument you trust. That holds most of all for what is new and worked out from the flying itself: the wind in strong wind, the thermal's picture, the landing within reach and the line about a strong wind have only ever seen a made-up flight. Live tracking has talked to the server's own example answers and to no server; the first flight shared is yours.

A new logbook offers a sample flight. It is made up, generated by `venti-flight`, not a real pilot's.

Map tiles are fetched from OpenFreeMap and Mapterhorn until the project has its own server; `docs/architecture.md` has the terms. Label fonts are inside the app.

## Screenshots without a Mac

Every push that touches the app runs the UI tests on GitHub's Mac runners. They save a screenshot of each screen, in light and dark appearance, as the `ios-screenshots` artifact of the `iOS` workflow run.
