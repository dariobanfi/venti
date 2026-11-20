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
| `app/src/main/kotlin/fly/venti/app/` | The entry point (`MainActivity`), what the app holds while it runs (`Session`), its tabs and where the pilot is in them (`RootScreen`, `AppNavigation`), the one place requests to servers are sent from (`Http`), and how a secret is kept (`SealedStore`). |
| `.../design/` | Colours, the Material theme, number formatting, the metric component, the pieces of list-like screens, chart axes. The rules behind them are in `docs/design.md`. |
| `.../logbook/` | The logbook screen, its model over the core, import, the statistics. |
| `.../flight/` | One flight: map, panel, altitude chart, details sheet. |
| `.../fly/` | The cockpit: the Fly tab, the cockpit screen and its map, the sensors (position, pressure, Bluetooth vario, vario sound, the recording service), and live tracking: its page (`LiveScreen.kt`) and the sending, which is the session's (`FlySession.kt`). |
| `.../map/` | The map tab, the shared map setup and the style preparation. |
| `.../airspace/` | The pilot's airspace, from files and from the pilot's server; how airspace is drawn on a map; and the cockpit's warnings: the banner, the sheet and the voice. |
| `.../waypoints/` | The pilot's waypoints: the list in the settings and its model over the core. |
| `.../wind/` | Live wind: asking a Venti server, the symbols for the map, a station's page with its charts, the list of stations. |
| `.../sites/` | Flying sites: asking a Venti server, the signs for the map, a site's page, the list of sites. |
| `.../forecast/` | The forecast of a place: one day of one weather model as a table, and the words around it. |
| `.../community/` | Everyone's public flights on a Venti server: asking for them page by page, the list on the Logbook tab, and opening one from its file. The screen it opens in is `flight/PublicFlightScreen.kt`. |
| `.../live/` | Who is in the air: asking a Venti server, the gliders' pictures for the map, the list, a glider's page, and following its flight while the page is open. |
| `.../account/` | The pilot's account on a Venti server: signing in, the sync with the server, sharing a flight with a link, and the session token in Android's keystore. |
| `.../task/` | The pilot's task: bringing one in, its screen with the sketch and the turnpoints, and planning a route from waypoints. Its line in the cockpit is `fly/TaskStrip.kt`. |
| `.../assistant/` | The assistant: the chat screen, sending requests, where its key is kept. |
| `.../settings/` | The settings tab and the credits. |
| `app/src/main/res/` | The app icon and the interface's icons as vector drawables, the window theme, backup rules, and the note on requests without encryption. |
| `app/src/test/` | Unit tests. They run on the build machine and call the real core through the generated interface. |
| `app/src/androidTest/` | Tests that use the app like a pilot and keep a screenshot of every screen. |
| `../../map/style.json` | The map style shared by all platforms. |
| `../ios/Venti/Resources/` | The sample flight, the label fonts and the demo map region. The Android build reads them from there. |

## What the app does today

- **Logbook.** Import IGC flight files with the plus button: several files at once, or a whole folder with the folders inside it, which is how a history of years comes in. Or open one from a file manager or a mail with "Open with Venti", or share one or many to Venti. While many are read, a bar under the list counts them ("Importing 37 of 312") and the list fills; at the end one sentence says how many flights were added and how many were there already. In a folder, files that are not flight files are passed over. Flights are listed by year with totals and each flight's scored distance. Tap the totals for statistics: the last twelve months, your gliders and your bests. Press and hold a flight to delete it. Without an account everything is stored on the phone only; Android's cloud backup is switched off for the app. With a Venti server set, the tab has "Mine" and "Everyone": Everyone lists the flights the pilots of that server share with everyone, the newest first or the best by points, each with the shape of its track, the pilot, the day and the glider, the scored distance and the airtime. Tap one to see it like one of your own. Its file is fetched for that and not kept, and the button at the top right hands on the flight's page on the server. The distances and points are the server's own reckoning by the World XContest's rules, not a league's.
- **Flight.** The track on a terrain map, coloured by climb and sink, with the altitude profile below and the flight's airtime, scored distance, highest point and best climb. Drag along the profile to move a marker along the track and read time, altitude and climb at that moment; the play button runs the marker along the flight. The details list every thermal and glide, and how the flight scores as a cross-country flight; choose a thermal or glide to mark it on the map. The map also draws the airspace the app has around the flight, and the details list where the flight was inside any of it, when and for how long: checked against the airspace of today, not of the day of the flight, and without knowing what was active, so a help for your own review and nothing more. The menu shares the flight's file, and with an account it shares the flight with a link: "Only me", "Anyone with the link" or "Everyone", private until you say otherwise. "Replay in the cockpit" in the same menu plays the flight through the cockpit, in real time or 10 or 20 times faster: the wind it works out, each thermal's picture, the airspace warnings and your task, as the cockpit would have shown them on that day. Nothing is recorded and nothing is shared.
- **Map.** A terrain map with every logbook flight on it, your airspace and, on request, your position. With a Venti server set, it shows live wind: every station in view as an arrow pointing where the wind blows, with mean wind and gusts in km/h. Tap an arrow for the station's reading, its last 24 hours and whose data it is; the button above the position button lists the stations by name. Zoomed in on a valley, the map also shows the flying sites the server knows: a dark sign for a take-off, a light one for a landing. Tap a sign for what its source says about the site, and "Add to my waypoints" to keep it for the cockpit; a third button lists the sites by name. A site's page lists the wind stations within 15 km with their wind, and says for each whether that wind comes from a direction the site works in, which is not a word on whether the site can be flown. A site's page and a station's page lead to the forecast for that place, and so does a long press anywhere on the map: a table of one day, with the wind at each height a weather model gives through the hours of daylight, then cloud base, cloud, rain, temperature and the height of the 0 °C level, model by model and with what the numbers are worth. The sites are OpenStreetMap's, which anyone can edit and which has only part of them: a sign is not permission to fly there, and a place without one may still be a site. The map also shows who is in the air: the pilots on your server who show their flight to everyone, each as a small wing turned the way it flies, green while it flies, grey when nothing was heard of it for two minutes, and a grey disc once its pilot's app said that it landed. Tap one, or choose it from the list behind the button with two figures, for the pilot's name, how old the position is, altitude, climb and speed; while that page is open the map draws the flight so far in the colours of climb. A glider that has gone quiet is never called landed: no network is far more likely.
- **Fly.** The cockpit. "Start flight" switches on the position, the pressure sensor and, if you have chosen one, your Bluetooth vario; it records from that moment and saves the flight to the logbook when you finish. It shows altitude, climb with a vario tone, speed, the glide you are making and the glide you need; when you circle in lift it changes to height gained, the thermal's average and the wind. The wind over the map says how old it is once that is five minutes ("11 km/h 12 min"), and a line under it warns while the wind leaves your glider less than 10 km/h of way into it at the trim speed you set ("Into the wind 4 km/h"). While you circle, the map pictures the thermal: the last circles as dots where their air is now, in the colour of the climb and larger where it is stronger, around a dashed circle to fly, and a badge under the wind says which way the lift is stronger and how far ("Stronger 20 m NW"), or "Centred". A thermal you have left stays on the map as a dot with its climb, to be found again, and a tap on the wind over the map lists the wind in every band of 200 m you have circled in, and under each wind how fast you climbed through that band: where the thermal was strong, and where it weakened. At the map's upper left stands the landing to think of first among your waypoints, with the height you are expected to arrive above it ("Sample landing field +510 m"): plain with a hundred metres or more to spare, amber with less, and red with "No landing in reach" when the glide reaches none. Tap it for the list of places to fly to, where the same height stands beside every waypoint. It is worked out from your glider's trim speed and glide ratio and the wind last measured, in a straight line: the ground in between is not looked at. A line above the instruments shows the way to the place you are flying to: the take-off, until you tap it and choose one of your waypoints. It says which way to turn, how far it is, and how high you are expected to arrive, from your glider's trim speed and glide ratio and the wind last measured. The line at the top shows the cross-country distance so far, and the map the route it is measured along. Once you have flown three kilometres in a straight line the map also draws, dashed, the two areas a turnpoint must lie in to make that line one side of an FAI triangle (Settings can leave them out), and the round button at the map's left edge shows the whole flight at once, and again goes back to following you. Before take-off the Fly tab says in a line what the cockpit will warn from ("openAIP, 344 airspaces · 1 file", or "None"), and a tap lists it. With airspace on the phone, the map shows the airspace around you, and a banner over it warns when you come near an airspace, are about to enter one or are inside; the phone says a new warning aloud, and the map steps back as far as it takes to show the airspace the warning is about. Tap the banner to see every warning, where its airspace is from, and to quieten one: "Seen" for ten minutes, or "Does not apply today". "Replay the sample flight" runs a recorded flight through the same screen. "Live tracking", on the Fly tab and behind a round button in the cockpit, shows your flight while it is under way to nobody, to the people you give its link, or to everyone who looks at your server's map; it needs an account on a Venti server, sends nothing before take-off, and the choice is kept for the next flights. The button is green while your positions go out and amber while they wait for a network. The page behind it says how the sharing stands and how many positions the server has, hands the link to another app, and deletes the track, which takes the flight away from everyone at once. With a task, the cockpit flies it: the Fly tab's "Task" line shows the task of the day or a route, brought in by pasting its text, by opening a file ending in .xctsk (the format XCTrack and other instruments exchange), or made from your own waypoints tapped in order. A line in the cockpit then counts down to the start, runs the clock, says which turnpoint is next and how far it is to goal; the map draws the cylinders, the next one heavier; the phone says aloud when the start is taken, a turnpoint reached and the goal made; and a tap on the line lists the turnpoints, with a button to skip one and one to go back.
- **Assistant.** Ask about your own flying ("how was my season?", "where were my strongest thermals?"). It runs on your own Anthropic API key, which you paste in once and which is kept on the phone, encrypted under a key in Android's keystore; your questions and what it looks up go from the phone to Anthropic and are billed to your account. With a Venti server set, it can also look up live wind and the airspace over a place: the app then asks your server, as the map does, and no Venti server sees your key or your questions. The airspace look-up came to Android with the shared core and has not been tried here. Through the shared core it can also look up forecasts and flying sites on that server; on Android that has not been tried, as the scripted provider of its tests only asks for wind.
- **Settings.** Your name and glider, which go into the flights you record; the vario sound; your waypoints, brought in from SeeYou files (.cup) and GEO files (.wpt); your airspace, kept from your server and brought in as OpenAir files; your glider's trim speed and glide ratio; the address of a Venti server for live wind, flying sites, forecasts and airspace, and your account on it; and the credits for the map data, the wind data and the libraries.

To connect a vario, open Fly, tap "Vario" and choose it from the list while it is switched on. Once connected, the same sheet shows the last lines it sent and has "Share a report": if the cockpit shows no climb from your vario, that report says why.

A new logbook offers a sample flight. It is made up, generated by `venti-flight`, not a real pilot's.

Not in the Android app, though the iPhone app has it: showing a task as a QR code for another pilot, and scanning one. Both need a library on Android, and none has been chosen.

Map tiles are fetched from OpenFreeMap and Mapterhorn until the project has its own server; `docs/architecture.md` has the terms. Label fonts are inside the app.

### Airspace

Airspace reaches the phone in two ways, and the app uses both as one set, on the maps and in the cockpit.

- **From your Venti server.** With a server's address in the settings, the app asks it for the airspace around what the map shows and around the first position of a flight, for about 220 km to each side, and keeps the answer on the phone: the cockpit then warns without a network. It asks again once a day, and when you are near the edge of what it keeps or beyond. The server has it from openAIP, whose data is not certified; the list of your airspace shows the provider's notice, credit and licence in full, the map names the provider, and the warnings repeat the notice. "Airspace from your server" in that list switches it off and deletes what was kept.
- **From files.** OpenAir files, the kind other flight instruments use, brought in from the same list. What a file left out is said when it is brought in and stays on its row in red.

The list opens from the Fly tab's "Airspace" line and from the settings. The app knows the airspace listed there and no other: where none of it reaches, the cockpit says that nothing is checked. [docs/airspace.md](../../docs/airspace.md) has the rules.

### Live wind

The app has no server built in. Until the project's own is online, the wind comes from a Venti server you run yourself: write its address in Settings under "Venti server", with or without `https://`. Without an address the map works, without wind.

The app asks a server on the internet over HTTPS only. An address that starts with `http://` is accepted for a server on your own network: this phone, a name without a dot or ending in `.local`, or an address from the private ranges (`10.x`, `172.16` to `172.31`, `192.168.x`, link-local, and `fc00::/7`). Android itself cannot draw that line, so its configuration allows requests without encryption and `Http.mayBeAsked` refuses the rest. Redirects are not followed.

To try it against recorded data: `cargo run -p venti-server -- dev` in the repository, and `http://10.0.2.2:8080` as the address on an emulator, or the computer's own address on your network on a phone.

### Your account

An account on your Venti server is optional. Without one, your flights stay on the phone. With one, they are copied to the server, appear on your other devices, and can be shared with a link; a flight is private there until you share it.

Open Settings, "Venti server", "Account". Sign in with your handle and password, or choose "New account": a handle of 3 to 20 letters, digits or underscores, a name others see, a password of at least 15 characters, and the invite code if the server asks for one. A forgotten password cannot be recovered: the server knows no email address.

Signed in, the phone brings the logbook and the server in step by itself: when you open the app, after an import, a deletion or a flight, and when you press "Sync now". The account screen says how many flights the server has, how many still wait to be sent, since when the two are in step and what the last sync did. A flight you delete is then deleted on the server and on your other devices too, and the question before deleting says so. "Sign out" leaves your flights on the phone and on the server. "Delete account" asks for your password and removes the account and every flight on the server at once; the flights on the phone stay.

The phone keeps no password. What it keeps is the token the server gave it at sign-in, encrypted under a key in Android's keystore, in a file that is in no backup and is not copied to a new phone: a new phone signs in for itself. The server's list of your devices shows this one by its maker and model.

### Not verified

The app has run on an emulator in CI, driven by the UI tests. One of them records a flight from made-up positions handed to Android as if its satellite receiver had found them, so the permission, the location manager, the recording service with its notification, the altitude conversion of Android 14 and later, and saving to the logbook all run there. Nobody has yet run the app on a real phone. In particular, nothing below has been tried, because an emulator cannot:

- **Flying with it.** The cockpit has only seen replayed flights. Fly it only beside the instrument you trust.
- **The landing within reach.** Seen in the replay of the sample flight, with one made-up landing among the waypoints. The prediction looks at no terrain, so a ridge between you and the field is not seen, and nobody has compared it in the air with what a glider did.
- **The line about a strong wind.** It has been seen in a replay with a made-up glider that trims at 18 km/h, slower than the settings let you choose, in the sample flight's wind of 11 km/h. Whether it comes early or late enough in a real wind is the core's rule to try in the air, and nobody has.
- **The thermal's picture.** It has been seen in the replay of the sample flight, whose one thermal lifts evenly all around: the badge has said "Centred" there and nothing else for long. A real thermal, with a stronger side, a wind that changes with height and a phone's own positions, is untried.
- **Recording with the screen off.** The foreground service, its notification and the wake lock are written to Android's documentation. Whether a recording survives hours in a pocket, and what the battery savers of Samsung, Xiaomi and others do to it, is unknown.
- **The satellite receiver.** Positions come from Android's own location manager, not from Google Play Services. Altitude is converted from the ellipsoid height Android reports to height above sea level by the system on Android 14 and later, and from the receiver's own text output before that; neither has been checked against a known altitude.
- **The pressure sensor**, and whether twenty readings a second make a good vario.
- **Bluetooth.** No vario has been connected: scanning, connecting, reconnecting after a loss, and listening to a device with no known serial service are untried.
- **The vario tone** through a real loudspeaker, and whether it keeps sounding with the screen off on Android 15 and later.
- **Navigating to a waypoint in the air.** The way to a place, the glide needed and the arrival height have been seen in replays only, and waypoint files have been read from made-up text, not from the files of real instruments and sites.
- **Airspace warnings in the air.** They have been seen in replays against one made-up zone. The app knows the airspace in your files and what it keeps from your server and no other, not which areas are active, and not the height of the ground, so a limit above ground is only "possibly" reached. Airspace from a server has only been fetched from one on the build machine, which answers from a recorded sample of openAIP's data; a country's whole airspace, a slow connection and a day without a network are untried. Whether the phone speaks a warning depends on its speech engine: a phone without one shows the banner and stays silent, and none has been heard.
- **The assistant with the real service.** It has only ever talked to a scripted stand-in in tests: the first conversation with Anthropic is yours.
- **Live wind from a server on the internet.** The wind map has only asked a server on the build machine, which answers from recorded data, without encryption. HTTPS to a real server, a slow connection and a server that stops answering mid-flight are untried on a phone.
- **Everyone's flights.** Seen with the five flights of the three made-up pilots of a server on the build machine. A list long enough for a second page has only been tried against canned answers. A server on the internet, a flight of many hours, and a pilot who takes a flight back while you look at the list are untried.
- **Replaying your own flights.** Only the sample flight has been replayed from a logbook. A flight from a real instrument, with gaps in its positions, and one of many hours are untried.
- **Who is in the air.** The gliders have only been asked of a server on the build machine, whose development mode has two made-up pilots and a clock that stands still. A real pilot's track that grows while it is watched, a day with many gliders in one valley, a glider that goes quiet, and tapping one with a finger among wind arrows are untried.
- **Flying sites from a server on the internet.** Like the wind, they have only been asked of a server on the build machine, which answers from a recorded copy of OpenStreetMap's sites for the first region. Tapping a sign with a finger, among wind arrows and on a real screen, is untried.
- **A live forecast.** The forecast screen has only shown recorded forecasts of two November days, from a server on the build machine. A server that has to fetch a place's first forecast, which can take half a minute, a summer day of sixteen hours of daylight, and a model the app does not know are untried.
- **An account on a real server.** Signing in, the sync and sharing have only talked to a server on the build machine, in its development mode, without encryption and with a demo pilot's three flights. A logbook of hundreds of flights, a connection that drops in the middle of a sync, a sync while a flight is being recorded, an invite code, and a password manager filling the form are untried. The token's key lives in the emulator's keystore in those tests; a real phone's, and what a system update does to it, are untried.
- **Live tracking.** No position has gone from a phone to a server. The sending is tested against the core with a made-up server, and on the emulator a flight is shared and its track deleted before its take-off, where nothing is sent yet. Hours in a pocket with the network coming and going, what a phone's battery saver does to requests sent with the screen off, a server that asks to wait, and whether the server is still told of the landing when you leave the app right after "Finish flight" are untried.
- **Flying a task.** The cockpit has flown one made-up task, in a replay of the sample flight. A real competition task from XCTrack or a QR code's text, a start with several gates, an end of speed section before goal, a goal line, and what the voice says over the vario's tone are untried. Opening a `.xctsk` file from a messenger or a mail app, and pasting one, have not been done on a phone: the tests cannot press Android's file picker or fill its clipboard.
- **"Open with Venti"** from real file managers and mail apps. Flight files have no type Android knows, so each app describes them differently.
- **Importing a folder.** Nobody has picked a folder in Android's own picker yet: the tests cannot press its buttons. What happens after it (finding the flight files, counting, the summary) is tested, with files shared to the app in its place. A folder on an SD card, in a cloud drive, or with a thousand files is untried.
- **Moving the logbook to a new phone**, which the backup rules allow from Android 12.

## Screenshots without a phone

Every push that touches the app runs the UI tests on an emulator on GitHub. They save a screenshot of each screen, in light and dark appearance, as the `android-screenshots` artifact of the `Android` workflow run:

```bash
gh run download --name android-screenshots --dir /tmp/venti-screens
```

The emulator is plain Android 16 without Google's services, which is also the check that the app needs none of them.

It draws with the emulator's `swangle_indirect` software renderer. The usual one, `swiftshader_indirect`, draws none of the map's symbols: no place names, no wind arrows, no dashed lines, so a screenshot from it says nothing about a map. Now and then the renderer in use cuts off part of the clock and the icons in the status bar; that is the emulator, not the app. One test asks the map itself which place names it has placed, so missing fonts are found whatever the renderer draws.

For the wind map the workflow starts `venti-server dev` and tells the tests where it is. To run that test yourself, start the server and add `-Pandroid.testInstrumentationRunnerArguments.ventiServer=http://10.0.2.2:8080` to the `connectedDebugAndroidTest` command; without the argument the test is skipped.
