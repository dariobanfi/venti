# Venti for Android

A Kotlin and Jetpack Compose shell over the Rust core. The shell draws and talks to Android; everything else lives in `crates/core` and below. It has the same sections as the iPhone app and follows the same design, with Material 3 components.

## Installing it without building

Every push that touches the app builds it on GitHub. The run of the `Android` workflow has an artifact called `venti-debug-apk`: download it, unpack the zip, and open `app-debug.apk` on the phone. Android asks once whether the browser or file manager may install apps.

From a computer with `gh` and `adb`:

```bash
gh run download --name venti-debug-apk --dir /tmp/venti   # the newest run
adb install -r /tmp/venti/app-debug.apk
```

It is a debug build, signed with a throwaway key that each build machine makes for itself. Two consequences: a build from another machine will not install over it (remove the app first; the logbook goes with it), and it is not what a store release would be. It runs on 64-bit phones with Android 8 or later, and on the x86_64 emulator.

## Building

Needs:

- Java 17 or later.
- Rust from [rustup.rs](https://rustup.rs) (the toolchain in `rust-toolchain.toml` installs itself), with the Android targets: `rustup target add aarch64-linux-android x86_64-linux-android`.
- `cargo install cargo-ndk --version 4.1.2 --locked`.
- The Android SDK with platform 37, build tools 36.0.0 and NDK 29.0.14206865. Android Studio installs them when it opens the project. Without it: `sdkmanager "platforms;android-37.0" "build-tools;36.0.0" "ndk;29.0.14206865"`, and `ANDROID_HOME` pointing at the SDK, or `sdk.dir=...` in `apps/android/local.properties`.

Then, in `apps/android`:

```bash
./gradlew :app:assembleDebug          # the app: app/build/outputs/apk/debug/app-debug.apk
./gradlew :app:installDebug           # onto a connected phone or a running emulator
./gradlew :app:testDebugUnitTest      # unit tests, on this machine
./gradlew :app:lintDebug              # Android's lint; warnings fail it
./gradlew :app:connectedDebugAndroidTest   # UI tests, on a connected phone or emulator
```

Gradle builds the Rust core itself. Three tasks in `app/build.gradle.kts` do it, and run again only when something under `crates/` has changed:

| Task | What it does |
|---|---|
| `coreForAndroid` | `cargo ndk` builds `libventi_core.so` for each ABI in `venti.abis` (`gradle.properties`): `arm64-v8a` and `x86_64`. |
| `coreBindings` | Builds the core for the build machine and runs UniFFI's generator (the `uniffi-bindgen` program in `tools/uniffi-bindgen-swift`) on that library to generate the Kotlin interface, package `fly.venti.core`. Settings in `uniffi.toml`. |
| `sharedAssets` | Gathers the sample flight, the map style and the bundled map region from where the repository keeps them. |

Nothing these produce is committed: no generated Kotlin, no compiled core, no second copy of a shared file.

The Gradle wrapper pins Gradle 9.9.1 and checks the download against its published checksum. Library versions are in `gradle/libs.versions.toml`, with their licences.

## Layout

| Path | What it is |
|---|---|
| `app/build.gradle.kts` | The app's build: Android settings, libraries, and the three tasks above. |
| `uniffi.toml` | How the Kotlin interface of the core is generated. |
| `app/src/main/AndroidManifest.xml` | Permissions, the one activity and the files it opens, the recording service. |
| `app/src/main/kotlin/fly/venti/app/` | The entry point (`MainActivity`), what the app holds while it runs (`Session`), its tabs and where the pilot is in them (`RootScreen`, `AppNavigation`), and the one place requests to servers are sent from (`Http`). |
| `.../design/` | Colours, the Material theme, number formatting, the metric component, the pieces of list-like screens, chart axes. The rules behind them are in `docs/design.md`. |
| `.../logbook/` | The logbook screen, its model over the core, import, the statistics. |
| `.../flight/` | One flight: map, panel, altitude chart, details sheet. |
| `.../fly/` | The cockpit: the Fly tab, the cockpit screen and its map, and the sensors (position, pressure, Bluetooth vario, vario sound, the recording service). |
| `.../map/` | The map tab, the shared map setup and the style preparation. |
| `.../waypoints/` | The pilot's waypoints: the list in the settings and its model over the core. |
| `.../wind/` | Live wind: asking a Venti server, the symbols for the map, a station's page with its charts, the list of stations. |
| `.../assistant/` | The assistant: the chat screen, sending requests, the key in Android's keystore. |
| `.../settings/` | The settings tab and the credits. |
| `app/src/main/res/` | The app icon and the interface's icons as vector drawables, the window theme, backup rules, and the note on requests without encryption. |
| `app/src/test/` | Unit tests. They run on the build machine and call the real core through the generated interface. |
| `app/src/androidTest/` | Tests that use the app like a pilot and keep a screenshot of every screen. |
| `../../map/style.json` | The map style shared by all platforms. |
| `../ios/Venti/Resources/` | The sample flight, the label fonts and the demo map region. The Android build reads them from there. |

## What the app does today

- **Logbook.** Import IGC flight files with the plus button (several at once), or open one from a file manager or a mail with "Open with Venti", or share one to Venti. Flights are listed by year with totals and each flight's scored distance. Tap the totals for statistics: the last twelve months, your gliders and your bests. Press and hold a flight to delete it. Everything is stored on the phone only; Android's cloud backup is switched off for the app.
- **Flight.** The track on a terrain map, coloured by climb and sink, with the altitude profile below and the flight's airtime, scored distance, highest point and best climb. Drag along the profile to move a marker along the track and read time, altitude and climb at that moment; the play button runs the marker along the flight. The details list every thermal and glide, and how the flight scores as a cross-country flight; choose a thermal or glide to mark it on the map. The menu shares the flight's file.
- **Map.** A terrain map with every logbook flight on it and, on request, your position. With a Venti server set, it shows live wind: every station in view as an arrow pointing where the wind blows, with mean wind and gusts in km/h. Tap an arrow for the station's reading, its last 24 hours and whose data it is; the button above the position button lists the stations by name.
- **Fly.** The cockpit. "Start flight" switches on the position, the pressure sensor and, if you have chosen one, your Bluetooth vario; it records from that moment and saves the flight to the logbook when you finish. It shows altitude, climb with a vario tone, speed, the glide you are making and the glide you need; when you circle in lift it changes to height gained, the thermal's average and the wind. A line above the instruments shows the way to the place you are flying to: the take-off, until you tap it and choose one of your waypoints. It says which way to turn, how far it is, and how high you are expected to arrive, from your glider's trim speed and glide ratio and the wind last measured. The line at the top shows the cross-country distance so far. "Replay the sample flight" runs a recorded flight through the same screen.
- **Assistant.** Ask about your own flying ("how was my season?", "where were my strongest thermals?"). It runs on your own Anthropic API key, which you paste in once and which is kept on the phone, encrypted under a key in Android's keystore; your questions and what it looks up go from the phone to Anthropic and are billed to your account. With a Venti server set, it can also look up live wind: the app then asks your server for the stations around a place, as the map does, and no Venti server sees your key or your questions. It cannot see forecasts, sites or airspace yet.
- **Settings.** Your name and glider, which go into the flights you record; the vario sound; your waypoints, brought in from SeeYou files (.cup) and GEO files (.wpt); your glider's trim speed and glide ratio; the address of a Venti server for live wind; and the credits for the map data, the wind data and the libraries.

To connect a vario, open Fly, tap "Vario" and choose it from the list while it is switched on. Once connected, the same sheet shows the last lines it sent and has "Share a report": if the cockpit shows no climb from your vario, that report says why.

A new logbook offers a sample flight. It is made up, generated by `venti-flight`, not a real pilot's.

Map tiles are fetched from OpenFreeMap and Mapterhorn until the project has its own server; `docs/architecture.md` has the terms. Label fonts are inside the app.

### Live wind

The app has no server built in. Until the project's own is online, the wind comes from a Venti server you run yourself: write its address in Settings under "Live wind", with or without `https://`. Without an address the map works, without wind.

The app asks a server on the internet over HTTPS only. An address that starts with `http://` is accepted for a server on your own network: this phone, a name without a dot or ending in `.local`, or an address from the private ranges (`10.x`, `172.16` to `172.31`, `192.168.x`, link-local, and `fc00::/7`). Android itself cannot draw that line, so its configuration allows requests without encryption and `Http.mayBeAsked` refuses the rest. Redirects are not followed.

To try it against recorded data: `cargo run -p venti-server -- dev` in the repository, and `http://10.0.2.2:8080` as the address on an emulator, or the computer's own address on your network on a phone.

### Not verified

The app has run on an emulator in CI, driven by the UI tests. One of them records a flight from made-up positions handed to Android as if its satellite receiver had found them, so the permission, the location manager, the recording service with its notification, the altitude conversion of Android 14 and later, and saving to the logbook all run there. Nobody has yet run the app on a real phone. In particular, nothing below has been tried, because an emulator cannot:

- **Flying with it.** The cockpit has only seen replayed flights. Fly it only beside the instrument you trust.
- **Recording with the screen off.** The foreground service, its notification and the wake lock are written to Android's documentation. Whether a recording survives hours in a pocket, and what the battery savers of Samsung, Xiaomi and others do to it, is unknown.
- **The satellite receiver.** Positions come from Android's own location manager, not from Google Play Services. Altitude is converted from the ellipsoid height Android reports to height above sea level by the system on Android 14 and later, and from the receiver's own text output before that; neither has been checked against a known altitude.
- **The pressure sensor**, and whether twenty readings a second make a good vario.
- **Bluetooth.** No vario has been connected: scanning, connecting, reconnecting after a loss, and listening to a device with no known serial service are untried.
- **The vario tone** through a real loudspeaker, and whether it keeps sounding with the screen off on Android 15 and later.
- **Navigating to a waypoint in the air.** The way to a place, the glide needed and the arrival height have been seen in replays only, and waypoint files have been read from made-up text, not from the files of real instruments and sites.
- **The assistant with the real service.** It has only ever talked to a scripted stand-in in tests: the first conversation with Anthropic is yours.
- **Live wind from a server on the internet.** The wind map has only asked a server on the build machine, which answers from recorded data, without encryption. HTTPS to a real server, a slow connection and a server that stops answering mid-flight are untried on a phone.
- **"Open with Venti"** from real file managers and mail apps. Flight files have no type Android knows, so each app describes them differently.
- **Moving the logbook to a new phone**, which the backup rules allow from Android 12.

## Screenshots without a phone

Every push that touches the app runs the UI tests on an emulator on GitHub. They save a screenshot of each screen, in light and dark appearance, as the `android-screenshots` artifact of the `Android` workflow run:

```bash
gh run download --name android-screenshots --dir /tmp/venti-screens
```

The emulator is plain Android 16 without Google's services, which is also the check that the app needs none of them.

For the wind map the workflow starts `venti-server dev` and tells the tests where it is. To run that test yourself, start the server and add `-Pandroid.testInstrumentationRunnerArguments.ventiServer=http://10.0.2.2:8080` to the `connectedDebugAndroidTest` command; without the argument the test is skipped.
