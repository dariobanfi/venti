# Notes for AI sessions working on the Android app

Working notes: what to know before changing the app or its tests. What the app does and what is not built is in [docs/plan.md](../../docs/plan.md) and [docs/design.md](../../docs/design.md), how it is built and what is still to verify on a phone in [docs/architecture.md](../../docs/architecture.md#android). The `README.md` beside this file is for people.

## Checks before a push

- From `apps/android`, with `ANDROID_HOME` set: `./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebugAndroidTest -Pventi.abis=x86_64`. The unit tests call the real core; the last task only compiles the UI tests.
- Run them again after every rebase that brings a change to `crates/core`. A field added to a record of the core breaks the tests that build that record by hand: `ReachTest` (`ReachView`), `AheadTest` (`AirspaceAheadSpan`), `WindByHeightTest` (`WindAtHeightView`), `GroupsModelTest` (`GroupMember`). Where a test needs a changed record, take a real one from the core and use `copy`.
- Lint fails on a drawable that nothing uses: add an icon in the commit that uses it.
- The UI tests cannot run in a cloud session. The `Android` workflow runs them, in about half an hour. A run that has started finishes; of the runs waiting behind it only the newest is kept, so a push is often tested together with later commits.
- When a run is red: the artifact `android-test-reports` holds `TEST-*.xml` with the failed test and the line, and `android-screenshots` holds `failed-<test>.png`. That picture is the last frame that was drawn, which can be older than what the failed assertion looked at.

## Pitfalls of the UI tests

- Compose draws a test's frames only when the test asks. Never wait with `Thread.sleep`; use `TestDevice.letTheAppRun`.
- A replay goes on while a test looks, by two seconds of the clock at most for each refresh. What belongs together during a thermal or a glide is looked for at one look (`waitForAll`); what comes late in a flight is waited for with `TestDevice.WHOLE_FLIGHT_SECONDS`.
- A screenshot waits until the map has drawn all it was given (`TestDevice.keepScreenshot`, `letTheMapFinish`). The glider and its track are one large piece of data and reach the map later than the line to the target does.
- Rows that are one element for a screen reader end their modifiers with `clearAndSetSemantics`: a test tag before it stays, tags on what is inside the row are gone. Find such a row by its tag, its description and what it says (`saying`).
- An instrument is read as "1,453 m, 238 m over ground": the value, then the line under it. A test that takes a number from it takes the first.
- Words on screen are mostly the core's. Read them in `crates/core` before asserting them: in a plan's list of who is in, the pilot of the phone is "You"; a place out of reach says "Out of reach." or "Out of reach: the ground is in the way."
- `venti-server dev` keeps what the tests send until it stops, and the light and the dark test run against the same one. A second report about the same thing is not a new one, so the two appearances report two pilots' flights; both post a plan for today. Its demo pilot signs in as `demo` with `demo-pilot-password`; neither is a secret.
- A bottom sheet stops at half height only if its content is taller than half the screen when it opens: wrap the content in `Box(Modifier.fillMaxHeight())`.
- With a sheet open over a sheet there are two "Done" buttons: give the upper one a test tag (`report-close`).
- Now and then the emulator's renderer cuts off part of the status bar. That is not the app.
- The emulator in the workflow is plain Android 16 without Google's services, which is also the check that the app needs none. It draws with the `swangle_indirect` software renderer: the usual `swiftshader_indirect` draws none of the map's symbols (no place names, no wind arrows, no dashed lines), so its screenshots say nothing about a map. One test asks the map itself which place names it has placed, so missing fonts are found whatever the renderer draws.
- For the wind map the workflow starts `venti-server dev` and names it to the tests. To run that test yourself, start the server and add `-Pandroid.testInstrumentationRunnerArguments.ventiServer=http://10.0.2.2:8080` to `connectedDebugAndroidTest`; without the argument the test is skipped.
- Download the screenshots with `gh run download --name android-screenshots --dir /tmp/venti-screens`.

## Conventions in the code

- A number reaches the screen through `Format` (`design/Format.kt`), which names the locale numbers are written in. Never format a number with the phone's own locale: beside the core's words a screen then shows "2,5 km" next to "10.0 km NE" on a phone set to German.
- Words that are not the core's stand in an object beside the screen (`PlanWords`, `GroupWords`, `ReportWords`, `AheadWords`, `ReachWords`), so that a unit test can hold them.
- Talking to the pilot's server is one way round: the core puts the request together, `AccountModel.answer` sends it with the session's token, the core reads the answer. Nothing of groups, plans or reports is kept on the phone.
- The look is the design pack's (`design/`, read `design/AGENTS.md`). Colours come from `Venti.colours` (the theme in use: light, dark, or flight in the cockpit) or `MaterialTheme`, sizes from `VentiSpace`, `VentiSize`, `VentiType` and `MaterialTheme.shapes`; none is written in a composable. What is drawn on the map takes the light theme's values (`Palette`), since the map is light in both appearances.
- Icons: the pack's flying icons are `R.drawable.venti_<name>`, made at build time from `design/assets/icons`. Every other icon is Lucide (lucide-static 0.460.0), stroke 2, 24 px, converted once into `res/drawable/ic_<role>.xml` with a comment naming the Lucide icon; the role names are those the app used before, which were Material Symbols' names.
- The `designPack` task (in `app/build.gradle.kts`) takes `VentiTheme.kt` as it is and writes `VentiTokens.kt` from `tokens.json`; both are in the package `app.venti.design`. A token the pack lacks is noted for the owner in `docs/design.md`, never added to the pack.

## How the build makes the core

Four tasks in `app/build.gradle.kts` build what the app takes from the rest of the repository; each runs again only when what it reads has changed:

| Task | What it does |
|---|---|
| `coreForAndroid` | `cargo ndk` builds `libventi_core.so` for each ABI in `venti.abis` (`gradle.properties`): `arm64-v8a` and `x86_64`. |
| `coreBindings` | Builds the core for the build machine and runs UniFFI's generator (the `uniffi-bindgen` program in `tools/uniffi-bindgen-swift`) on it to generate the Kotlin interface, package `fly.venti.core`. Settings in `uniffi.toml`. |
| `sharedAssets` | Gathers the sample flight, the map style, the bundled map region and the fonts' licence from where the repository keeps them. |
| `designPack` | Takes the design pack into the build: its Kotlin tokens, the rest of its tokens as Kotlin, its fonts, its flying icons as vector drawables, the launcher and notification icons. |

Nothing they produce is committed: no generated Kotlin, no compiled core, no second copy of a shared file.

## Layout

| Path | What it is |
|---|---|
| `app/build.gradle.kts` | The app's build: Android settings, libraries, the three tasks above. Library versions and their licences are in `gradle/libs.versions.toml`. |
| `uniffi.toml` | How the Kotlin interface of the core is generated. |
| `app/src/main/AndroidManifest.xml` | Permissions, the one activity and the files it opens, the recording service. |
| `app/src/main/kotlin/fly/venti/app/` | The entry point (`MainActivity`), what the app holds while it runs (`Session`), its tabs and where the pilot is in them (`RootScreen`, `AppNavigation`), the one place requests to servers leave from (`Http`), and how a secret is kept (`SealedStore`). |
| `.../design/` | Colours, the Material theme, number formatting, the metric component, the pieces of list-like screens, chart axes. |
| `.../logbook/` | The logbook screen, its model over the core, import, the statistics. |
| `.../flight/` | One flight: map, panel, altitude chart, details sheet; another pilot's flight (`PublicFlightScreen.kt`). |
| `.../fly/` | The cockpit: the Fly tab, the cockpit screen and its map, the sensors (position, pressure, Bluetooth vario, vario sound, the recording service), live tracking (`LiveScreen.kt`; the sending is `FlySession.kt`'s), the task's line (`TaskStrip.kt`). |
| `.../map/` | The map tab, the shared map setup and the style preparation. |
| `.../airspace/` | The pilot's airspace from files and from the server, how airspace is drawn, and the cockpit's warnings: banner, sheet, voice. |
| `.../waypoints/` | The pilot's waypoints: the list in the settings and its model. |
| `.../wind/` | Live wind: asking the server, the map's symbols, a station's page with its charts, the list. |
| `.../sites/` | Flying sites: asking the server, the map's signs, a site's page, the list. |
| `.../forecast/` | The forecast of a place: one day of one model as a table, and the words around it; the forecast map of the map tab, the kind of day square by square. |
| `.../community/` | Everyone's public flights page by page, the list on the Logbook tab, opening one from its file, and the report form. |
| `.../groups/` | Groups and their plans: the board, a plan, its form, the groups and what keeper and members can do. Asked for and not kept. |
| `.../terrain/` | The ground: elevation tiles around the glider, fetched, kept and handed to the core. |
| `.../live/` | Who is in the air: asking the server, the gliders' pictures, the list, a glider's page, following its flight. |
| `.../account/` | Signing in, the sync, sharing a flight with a link, the session token in Android's keystore. |
| `.../task/` | Bringing a task in, its screen with the sketch and the turnpoints, planning a route from waypoints. |
| `.../assistant/` | The chat screen, sending requests, where the key is kept. |
| `.../settings/` | The settings tab and the credits. |
| `app/src/main/res/` | The app icon and the interface's icons as vector drawables, the window theme, backup rules, the note on requests without encryption. |
| `app/src/test/` | Unit tests. They run on the build machine and call the real core through the generated interface. |
| `app/src/androidTest/` | UI tests that use the app like a pilot and keep a screenshot of every screen. |

The map style is `map/style.json` at the repository's root; the sample flight, the label fonts and the demo map region are read from `apps/ios/Venti/Resources`.

## What has never been tried

The app has run on an emulator in CI, driven by the UI tests, and on no phone. One test records a flight from made-up positions handed to Android as if its satellite receiver had found them, so the permission, the location manager, the recording service with its notification, the altitude conversion of Android 14 and later, and saving to the logbook all run there. What an emulator cannot show (recording with the screen off and battery savers, the altitude conversion against a known height, the pressure sensor as a vario, Bluetooth, the tone, "Open with" from real apps) is listed in [docs/architecture.md](../../docs/architecture.md#android). Besides that:

- **Flying.** The cockpit has only seen replayed flights.
- **The flight that records itself.** Tested against the core with made-up positions in the unit tests (stand, fly three minutes, stand), and on the emulator only with "Record now". A real take-off noticed by the core, a landing two minutes before the save with the screen off, and two hours of waiting are untried on a phone.
- **The cockpit's pages.** The UI tests turn them with `turnTo("Map")` and end a session with `holdToEnd()`, a long press of 1.6 s on `cockpit-end`; the readings are found by their names ("Ground speed", "Wind", "Gained"), the wind's reading by `wind-reading`. The map's round buttons and the task's and the target's lines are on the second page, the lock, live tracking and "Record now" on the third.
- **The lock.** Tried in a replay on the emulator, where a test presses the button that ends the replay through the cover and holds the lock open. A glove, rain, a phone in a pocket of the harness and a long press that the system takes for itself are untried.
- **The landing within reach.** Seen in the replay of the sample flight, with one made-up landing among the waypoints. Nobody has compared it in the air with what a glider did.
- **How far it glides.** The green line is drawn from what the core works out, which the unit tests hold against the region that comes with the app; in the screenshots it shows only where the map is wide enough. Whether its reserve of a hundred metres is right, and whether it helps or clutters, is for the air to say.
- **Ahead, from the side.** Seen in replays over the bundled region, with the glide ending on the ground. An airspace block has been drawn in a test only when the made-up zone lay on the line when the test looked; several airspaces one over another, one whose height could not be placed, and a long name in a narrow block are untried.
- **The height over the ground.** Seen in replays over the bundled region, whose heights the core reads as 725 m for the Tegernsee and 1,620 m for the sample flight's take-off. Tiles fetched on a phone, a flight that leaves the tiles it started with, a day without a network and what the store of tiles weighs after a season are untried; nothing removes a kept tile. The number is a model's of the bare ground in pixels 13 m wide, held against no landing and no known height.
- **Others in the air, in the cockpit.** Seen against the development mode's demo gliders, which fly the sample flight itself, in the view of the whole flight. Many gliders around a thermal and a pilot's own glider among them (left out by its link) are untried.
- **The wind on the ground.** Seen in a replay against the development mode's stations, which stand fifty kilometres and more from the sample flight, so the badge has only shown a far station's wind with its distance. The warning of a strong wind at a landing has been tested against the core with Venti's recorded stations and a made-up field beside one, and not seen on a screen; no warning has been heard.
- **The line about a strong wind.** Seen in a replay with a made-up glider trimmed at 18 km/h, slower than the settings allow, in the sample flight's wind of 11 km/h.
- **The thermal's picture.** Seen in the replay of the sample flight, whose one thermal lifts evenly all round, so the badge has said little but "Centred". A real thermal with a stronger side and a wind that changes with height is untried.
- **Navigating to a waypoint.** Seen in replays only; waypoint files have been read from made-up text, not from real instruments' files.
- **Airspace warnings.** Seen in replays against one made-up zone. Airspace from a server has only come from the development mode's recorded sample of openAIP's data; a country's whole airspace, a slow connection and a day without a network are untried. A phone without a speech engine shows the banner and stays silent; no warning has been heard.
- **The assistant.** Only ever against a scripted stand-in. Its airspace, forecast and site look-ups came with the shared core and have not been tried on Android, as the scripted provider of its tests only asks for wind.
- **Servers on the internet.** Wind, sites, forecasts, everyone's flights, who is in the air, the account and reports have only talked to the development mode on the build machine, without encryption. HTTPS, a slow connection and a server that stops answering are untried.
- **Everyone's flights.** Seen with the development mode's five flights of three made-up pilots; a second page only against canned answers. A flight of many hours and a pilot who takes a flight back while the list is open are untried.
- **Flying plans and groups.** The board, a plan posted for today, a plan's page and a group's page have been seen against the development mode's one made-up group. Changing and deleting a plan, "I am in" and "I am out", making and joining a group, renaming, a new code, removing a member, handing the keeping on, leaving and deleting were tried only against answers the server's own tests keep. The day and time pickers, a board of many days and two phones in one group are untried.
- **Reporting.** Reports have gone to the development mode only, which forgets them when it stops. The form with the keyboard open has not been looked at on a phone.
- **Replaying one's own flights.** Only the sample flight has been replayed from a logbook; a flight from a real instrument, with gaps, and one of many hours are untried.
- **Who is in the air.** Only the development mode's two made-up pilots, with a clock that stands still. A track that grows while watched, many gliders in one valley and a glider that goes quiet are untried.
- **A live forecast.** Only recorded forecasts of two November days. A server that takes half a minute to fetch a place's first forecast, a summer day of sixteen hours of daylight and a model the app does not know are untried.
- **The account.** A logbook of hundreds of flights, a connection that drops in a sync, a sync while recording, an invite code and a password manager filling the form are untried. The token's key has only lived in the emulator's keystore.
- **Live tracking.** No position has gone from a phone to a server. On the emulator a flight is shared and its track deleted before take-off, when nothing is sent yet; the sending is tested against the core with a made-up server. A server that asks to wait is untried.
- **Flying a task.** One made-up task, in a replay of the sample flight. A real competition task, a start with several gates, an end of speed section before goal, a goal line and the voice over the vario's tone are untried. The tests cannot press Android's file picker or fill its clipboard.
- **Importing a folder.** The tests cannot press the folder picker's buttons; what follows (finding the flight files, counting, the summary) is tested with files shared to the app instead. A folder on an SD card, in a cloud drive or with a thousand files is untried.
- **Offline maps.** Downloaded from the development mode, which hands out the region that comes with the app, over the emulator's own network; a download that breaks off and goes on is tried against a server played by the unit tests. A real region of hundreds of megabytes, a phone that goes to sleep in a download, too little room and a map tab without a network (Android's connectivity callback) are untried.
- **Drawing a route on the map.** Tried with taps a test puts at fractions of the map's size, over the empty land around the region that comes with the app. A finger, a long route and keeping one while a task is being flown are untried.
- **Two flights side by side.** Tried with the sample flight and the same flight ten minutes later, brought in together; another pilot's flight from "Flown here that day" only in the chooser's code, since the development mode has no two flights of one day and place. Two long flights of very different lengths are untried.
- **Sending a flight to a league.** Nothing has been sent to the DHV-XC: the unit tests play it with the example answers of its specification, which `crates/leagues/tests/fixtures` keeps, and the UI test fills the form with made-up words and never presses "Send". Never write a test that sends or presses it. Saving the file with Android's dialog and opening XContest's page are untried, since the tests cannot press the system's dialog or look into a browser.
- **Moving the logbook to a new phone**, which the backup rules allow from Android 12.
- **The aerial view.** `map/style.json` marks the layers of one view only with `"venti:view": "aerial"` or `"drawn"` in their metadata. `MapStyle.views` reads the two lists from the style once; `MapSetup.showAerial` shows one list and hides the other on a loaded style, and `OverviewMap` and `FlightMap` take `showAerial(on)` from their screens, which read `Preferences.mapShowsAerial` (one choice for every map, off by default). The aerial layers are hidden in the file, so nothing is fetched from the photographs' hosts until the view is on; `MapStyleTest.theAerialViewIsHiddenUntilItIsAskedFor` keeps it so. A new aerial or drawn layer needs only its metadata in the style, no Kotlin.
- **What the map style asks of Venti.** A source in `map/style.json` whose `tiles` hold `__VENTI_SERVER__` (`aerial-austria`, Austria's photographs through Venti, since 3c46437) is asked of the Venti address the pilot has set: `MapSetup.server`, which `Session` points at the wind's, is handed to `MapStyle.json`. Without an address that source is left out with every layer that draws from it, and `showAerial` passes over a layer that is not there. `MapSetup` keeps the prepared style per source and address; a map made before the address changed keeps the style it was made with. The same holds for every placeholder in a tile address: `__APP_DOMAIN__` takes `MapStyle.APP_DOMAIN`, `null` until Venti has a domain, and a source that names a placeholder without a value is left out with its layers.
- **The thermal view beside the cockpit's map.** `fly/ThermalView.kt` draws the close-up on a Compose canvas from the cockpit view alone (`lift`, `trail`, `thermals`, `wind`, `courseDeg`, `position`); `fly/CloseUp.kt` is its geometry (metres from the glider, the span, the scale bar), pure and unit-tested in `CloseUpTest`. Which points, the circle and the words are the core's; add no rule there. `CockpitMap.closeUp` tells the first page's map to keep the glide's zoom and leave out the lift dots; the second page's map sets it false.
- **The watched folder** (`logbook/WatchedFolder.kt`): the folder new flight files come in from whenever the app comes to the front (`RootScreen`, on STARTED). The read permission is persisted with `takePersistableUriPermission`; the files seen are kept as keys (path, size, last changed) in `Preferences.watchedSeen`. It is tried only through the unit test of its bookkeeping; no UI test can answer the system's folder picker.
- **The thermal map.** Layers marked `"venti:layer": "thermals"` are read into `MapStyle.views().thermals`; `MapSetup.offersThermals` says whether the prepared style has them (only with `MapStyle.APP_DOMAIN` set, and never for a kept region), and `MapSetup.showThermals` shows or hides them. The map tab's "Thermals" chip and the cockpit's flight-page button share `Preferences.mapShowsThermals`.
- **The forecast map** (the "Forecast" chip, the kind of day square by square): `forecast/ThermalMapModel.kt` asks and keeps the map, `forecast/ThermalMapLayer.kt` makes the GeoJSON and the fill layer, `forecast/ThermalMapViews.kt` the card and a square's sheet. The layer is inserted under the base style's first layer of names, borders or cable cars (`ThermalMapLayer.OVER`); a new layer of that kind in `map/style.json` that must stay over the squares needs its id there. `OverviewMap.showForecast` compares the list by identity, so hand it the core's list of a day, never a copy. The UI tests `theMapShowsTheKindOfDaySquareBySquare` and `theKindOfDayOnTheMapInDarkAppearance` need `madeUpDayServer`; they switch wind, sites and pilots off so that a tap in the middle of the map finds a square (`TestDevice.drawnInTheMiddle`). The map's days are named by the server's clock. The stand-in dates its map from that clock (three days from the development server's 18 November), so they read "Today", "Tomorrow" and a third; the tests pick the days by their place in the switch all the same, since which day is today also hangs on the emulator's time zone. The stand-in's own grid (47.5 N, 11.5 E, 5 by 9 tenths of a degree) has weak thermals at the middle of the tests' view, 47.7 N 11.9 E, on every day. The forecast map's days are a `SegmentedPills` (`design/Segmented.kt`, Today's day switch too) under the tag `forecast-days`; `tab()` in the tests leaves both switches out.
- **A made-up day with thermals.** The recorded forecasts are of late November and have no thermals. The workflow starts `tools/made-up-day/serve.py` in front of `venti-server dev` on port 8081, and names it to the tests as `madeUpDayServer` (`http://10.0.2.2:8081`); `todaySaysWhatKindOfDayTheThermalsMake` is skipped without it. Its forecast is made up, and so is everything its screenshots (`today-5-big-day`, `today-6-kinds-of-day`, `today-7-site-on-a-big-day`) say: say so wherever they are shown.
