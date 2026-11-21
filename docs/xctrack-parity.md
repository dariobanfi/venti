# Cockpit checklist: parity with XCTrack

Last revised: 2026-11-21

XCTrack is the benchmark for the Fly mode. This document lists what XCTrack does, what our first cockpit release must match, and where we intend to go further.

## Evidence

XCTrack is closed source. This inventory was assembled from:

| Tag | Source | Checked |
|---|---|---|
| **official** | xctrack.org pages read directly: home, Manual, External Devices, Change Log, Competition, Open Data, JavaScript Interface, Maps, FAQ | 2026-11-18 |
| **config** | Real exported XCTrack configuration files published on GitHub (versions 0.9.12.x), which reveal widget classes and option names | 2026-11-18 |
| **tracker** | The developers' public issue tracker, gitlab.com/xcontest-public/xctrack-public | 2026-11-18 |
| **manual** | Search summaries of the AIR³ manual, which the XCTrack team recommends. Not read directly, except the airspace parts: [Preferences 2](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/preferences2/) and [Navigation widgets 2](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/xctrack-pro-widgets-navigation2/), read 2026-11-19. | To verify, apart from airspace |

Items without a tag are **config**. The stable line is 0.9.12.x; 1.0 has been in beta since June 2026 (1.0.7-beta on 2026-10-05).

## What is built

As of 2026-11-19, in the Rust core and tested by replaying a flight, but not yet flown: the vario pipeline with source fallback, averaged climb, ground speed and course, glide ratio, wind from circling, automatic take-off, landing and circling detection, thermal gain and average, distance and bearing to take-off, the trail behind the glider, crash-safe IGC recording, in-app replay, the vario tone description, and parsers for `LK8EX1`, `POV`, `XCTRC`, `LXWP0` and BlueFly `PRS` with the three serial-style Bluetooth services. Since the same day also: waypoint files in SeeYou `.cup` and GEO `.wpt` format (read, and written as `.cup`), and going to a waypoint or back to the take-off with distance, bearing, the glide needed, and the height of arrival predicted from the glider's trim speed and glide ratio and the wind last measured. Sunset and the end of civil twilight are worked out on the device (checked against the US Naval Observatory's figures) and shown when less than two hours of daylight are left. The flight is scored in the air every half minute by the same rules as in the logbook, and the cockpit shows the distance so far. Airspace from OpenAir files the pilot brings in, and the airspace the pilot's server collects, fetched around the pilot and kept on the phone, is drawn on the maps and warned about in the cockpit, with XCTrack's margins and a prediction of our own; the map zooms out to show the airspace a warning is about; see [airspace.md](airspace.md). The thermals the glider has left are marked on the map with their climb (where it circled, not corrected for drift), and the wind is kept by band of 200 m of height, as the mean of what was measured while circling there, with the climb the glider made while circling through the band. The cockpit names the landing among the pilot's waypoints that the glider would arrive highest above, with the height to spare, and says when none is within reach; the wind's age is shown once it is five minutes old, and a line appears when the wind leaves less than 10 km/h of way into it at trim speed. While circling the map pictures the thermal: the last fifty seconds as dots moved with the wind to where their air is now, coloured by climb and larger where the climb is stronger, with a dashed circle to fly around where the lift is and a line of words ("Stronger 20 m NW", "Centred"). The wind estimate works in a wind as strong as the glider is fast or stronger, which XCTrack's tracker lists as a weakness of its own (below); circling is recognised there too. How XCTrack's thermal assistant decides what to draw is not published and was not looked up in the app; ours is described in [architecture.md](architecture.md). The FAI triangle areas are drawn for the longest straight line flown so far, with a view of the whole flight to see them in. That choice of line is ours: the manual the XCTrack team recommends says only that the areas show "the area in which you should fly to complete a FAI triangle" ([fly-air3.com](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/xctrack-pro-widgets-xcontest2/), read 2026-11-19); which turnpoints XCTrack takes is to verify against the app itself. Since the same day the cockpit also flies a competition task or a route (see [Navigation and competition](#navigation-and-competition)). Since 2026-11-20 the core knows the height of the ground, read from the map's own elevation tiles (`crates/core/src/terrain.rs`), and the iPhone's and Android's cockpits use it for the height above ground, for airspace limits given above ground, for taking a landing behind a ridge out of reach, for a line on the map around what a straight glide still reaches, and in the side view; tested on the tiles that come with the app and not flown. Everything else in the tables below that does not say it is built is still to do.

Also built, as an engine without a screen: airspace. The crate `venti-airspace` reads OpenAir files, answers what the proximity widget and the side view ask, and produces the warnings; see [airspace.md](airspace.md) and the "Engine" notes in the airspace table below. It is tested on real airspace files and scripted flights, and since 2026-11-19 it is what the cockpit's warnings and the maps' airspace run on. It has not flown.

## The gap this project fills

- **There is no XCTrack for iOS.** The official FAQ says: "No and we don't plan it (the app would have to be almost rewritten again for iOS)." *(official)*
- **No barometer and accelerometer fusion.** The internal vario lags; the developers rely on external varios. *(tracker)*
- **No polar, MacCready or speed-to-fly.** The glider is modelled by two numbers, trim speed and glide ratio. Requests have been open since 2022. *(tracker)*
- **Wind estimate is weak for fast gliders in strong wind.** It assumes a perfect circle drifted by the wind, from GPS only. Open since 2020. *(tracker)*
- **Livetracking has no groups, no OGN, no other networks, no traffic radar and no collision alerts.** *(tracker)*
- **No wind forecast or wind-station display** in the cockpit. *(tracker)*
- **Mobile data use has grown,** mostly from thermal-map and rain-radar tiles. *(tracker)*

## Columns

- **v1:** in the first cockpit release (Phase 3, iOS).
- **Later:** planned after v1.
- **No:** not planned, with the reason.

## Pages and layout

| XCTrack | Ours |
|---|---|
| Four page types: XC assistant, competition, thermal assistant, blank canvas | v1 |
| Pages shown or hidden by navigation mode (back to takeoff, triangle closing, go-to waypoint, competition) | v1 |
| Free placement and resizing of widgets, separate portrait and landscape layouts | v1 ships tuned preset layouts; the free editor is Later |
| Per-widget border, background opacity, theme, title, unit override | Later, with the editor |
| Themes: white, white high-contrast, black, e-ink | v1: light, dark and a sunlight high-contrast theme. E-ink is Later (Android) |
| Automatic switch to the thermal page when circling starts and back when it ends | v1 |
| Layout export and import as a file | Later; importing XCTrack `.xcfg` layouts is a candidate for easing the switch |
| Pilot experience profiles (simple to expert) *(manual)* | v1 as two presets |

Thermalling detection in XCTrack *(official)*: start when heading has changed at least 90° in 30 seconds and the 30-second average vertical speed is −0.5 m/s or better; end when heading has changed less than 30° in 30 seconds and the average is below −0.5 m/s. We start from the same rule and make the thresholds configurable.

## Widgets

| Group | XCTrack widgets | Ours |
|---|---|---|
| Altitude | GPS altitude, barometric altitude (QNH automatic or manual), above ground, maximum, above takeoff, flight level, QNH, gain in current thermal | v1. Above ground: in the core since 2026-11-20, from the map's elevation tiles, and shown under the altitude in the iPhone's and Android's cockpits since the same day; tested on the tiles that come with the app and not in the air |
| Vario | Numeric with selectable averaging, colour bar, altitude-over-time graph, netto (external sensor only) | v1; netto Later |
| Speed and glide | Ground speed with blown-back indication, airspeed (estimated or external), current glide ratio, average XC speed | v1 |
| Wind | Speed, direction, combined compass and wind, wind and climb by altitude band | v1. Wind and climb by band of 200 m: built on the iPhone and on Android on 2026-11-19, the climb through each band since 2026-11-20, as a list opened from the cockpit's wind (`WindByHeightSheet`); tested by replaying a flight and not flown |
| Thermal | Thermal assistant: drift-corrected trail coloured by climb, previous thermals, optional airspace and navigation overlays | v1 |
| Navigation | Next turnpoint name, distance, arrival height, required glide, arrival time; distance and course to takeoff; bearing; coordinates; nearest airfield | v1 |
| Side view | Terrain and airspace cross-section along bearing or route, with glide lines | Built on the iPhone on 2026-11-20 for airspace, along the course or towards the target, ten kilometres, with one glide line. Since the same day the core draws the ground into it where the app holds elevation tiles, says where the glide meets it, and stands limits above ground on it; without tiles those limits are drawn as far as they may reach. Not along a task's route. On Android since the same day too (`fly/AheadSheet.kt`). Tried against a replayed flight, not flown |
| XC | Map with live XContest optimisation, FAI sectors, closing circle, leg distances; flown distance; unfinished-triangle estimate; FAI triangle potential | v1 |
| Competition | Task map, distance, altitude and glide to goal and to end of speed section, time to start, predicted time at start, speed to start, percent of speed section, task summary | Built on the iPhone as one line and a sheet, not as separate widgets: time to start, distance to the start's edge and speed to it, the time to the edge at the present speed, distance and glide to goal, distance to the end of the speed section, the task on the map and its summary. Percent of the speed section is worked out and not shown |
| Airspace | Proximity widget with nearest airspace, distance and direction | v1. In the core since 2026-11-21, with the height to floor or ceiling as well (`CockpitView::airspace_proximity`, [airspace.md](airspace.md#the-airspace-that-matters-next)); no phone shows it yet |
| Time | Clock, flight time, sunset, civil twilight | v1 |
| System | Status line (GPS, sensors, live, battery), brightness, volume, last notification | v1 |
| Livetracking | Incoming messages | v1 |
| Buttons | Zoom, next and previous waypoint, vario mute, brightness, phone, camera | v1 for zoom, waypoint, mute |
| Other | Free text; external data fields from the `$XCTOD` sentence; embedded web page with a JavaScript interface to flight data | External data Later. Web page widget: No (no web views) |
| Head-up displays | Widget sets for ActiveLook and Everysight glasses | Later |

## Sensors

XCTrack supports several sensors at once with priorities and automatic fallback; each sensor is assigned the data it supplies (GPS, barometer, wind, compass, FLARM, airspeed). *(official, config)* Ours: v1.

### Sentences XCTrack parses *(official)*

`$XCTOD`, `$XCTRC`, `$LK8EX1`, `$LXWP0`, `$GPRMC`, `$GNRMC`, `$GPGGA`, `$GNGGA`, `$PCPROBE`, `PRSD`, `_PRS`, `PRS`, `_BAT`, `BAT`, `$VARIO`, `$PFLAA`, `$PFLAU`, `$PFLAC`, `$PDGFTL1`, `$PTAS1`, `$PLKAS`, `$BFV`, `$MIPRC`, and from 1.0.3 an HDG sentence for external compasses. FANET names come from `FNNGB`. *(tracker)*

Ours, v1: `LK8EX1`, `LXWP0`, `XCTRC`, `POV` (OpenVario), `PRS`, GGA and RMC, `PFLAA`, `PFLAU`, `FNNGB`. The rest follow as devices need them. `POV` is not on XCTrack's list, but the owner's vario sends it. Each is one parser in the `sensors` crate with recorded test streams.

### Bluetooth LE services XCTrack subscribes to *(official)*

- Environmental Sensing Service (pressure, wind speed and direction, temperature, humidity)
- Battery Service
- Nordic UART Service
- XC Tracer service, SkyDrop service, leGPSBip service, Microchip RN4871 transparent UART
- FlyBeeper mini airspeed service, Flytec sensor ID service
- Vector Vario service (vario, netto, airspeed, G-force, heading, roll, pitch, GPS)

Over the UART-style services the payload is plain text sentences, one per line. *(tracker)*

Ours, v1: the UART-style services and the Environmental Sensing Service.

The first two drivers, in this order:

1. **MipFly MipBip**, the owner's vario. Its manual says it sends `$LK8EX1` and `$POV` sentences over Bluetooth, supports Bluetooth LE, and expects the app to open the connection itself (not the phone's Bluetooth settings). Which Bluetooth service it uses is found by scanning the device in the Phase 0 spike. *(manual, to verify on the device)*
2. **XC Tracer** (all current models), the most common vario among cross-country pilots. It has its own Bluetooth service and sends `XCTRC`, `LXWP0` or `LK8EX1` depending on its configuration. *(official)*

After those, the goal is every popular Bluetooth LE vario on XCTrack's device list.

### Devices XCTrack lists *(official)*

XC Tracer, XC Tracer II and Maxx; SkyDrop; BlueFly; Flymaster LIVE C, NAV C, LIVE DS, LIVE ONE; Skytraxx 2.1 and 4.0 (with FANET and FLARM); Syride Nav XL; Flytec SensBox; leGPSBip; FlyNet; Digifly AIR-BT; Compass C-Probe; Vector Vario; MipFly MipTrack; FlyBeeper mini BT; BRVARIO; AIR BOX; AltiSAN; BarON; Freevario; MillenAir V2; several open-hardware varios; and riser remote controls (XCRemote, XC Tracer Remote, AeroRemote).

### Other connections

Bluetooth classic, USB serial, and TCP or UDP data sources. *(official)* iOS allows neither Bluetooth classic serial for arbitrary devices nor USB serial in the same way, so v1 on iOS is Bluetooth LE only. Android gets the others Later.

## Vario sound

XCTrack *(official, config)*:

- Lift beeps rise in pitch and rate up to 6 m/s. A custom profile table maps climb rate to frequency, cycle length and duty.
- A weak-lift buzzer sounds just below the lift threshold when the short average exceeds the long average.
- A sink alarm, a double beep whose second tone falls with sink rate.
- A distinct alarm when barometric data stops, for a lost sensor connection.
- Separate volume for weak lift, mute when landed, climb and sink thresholds, averaging settings.

Ours: v1, all of it. The missing-data alarm matters most with an external vario.

## Navigation and competition

| XCTrack | Ours |
|---|---|
| Modes: back to takeoff, triangle closing, go-to waypoint, competition task | v1, competition included. Built on the iPhone and on Android on 2026-11-19: back to the take-off, going to a waypoint, a competition task or a route, and how far an open triangle is from closing; tested by replaying a flight and not flown |
| Waypoint files: CompeGPS, UTM, GpsDump GEO, OziExplorer, SeeYou `.cup`, XCSoar/WinPilot `.dat` *(official)* | v1: `.cup` and GEO `.wpt`; others Later |
| Live XContest scoring: free distance, flat triangle, FAI triangle, with configurable coefficients and closing distance | v1 |
| Glide range marks on map and side view, from actual and configured glide ratio with wind | v1. Built on the iPhone and on Android on 2026-11-20: a line on the cockpit's map around what a straight glide at the configured glide ratio, in the wind last measured, still reaches over the ground with a hundred metres to spare, and the glide line in the side view, from the glide of the last seconds or the configured one. Tested on the tiles that come with the app, not flown |
| Competition tasks: cylinders, start of speed section with race or elapsed-time start and time gates, end of speed section, goal cylinder or line, elevated goal, optimised route | Built on the iPhone, tested by replaying a flight and not flown: the cockpit counts down to the start gate, leads from turnpoint to turnpoint along the shortest way through the cylinders, shows the distance to goal and to the end of the speed section and the glide to goal, draws the task on its map and says what is reached. The engine (`venti-flight`, `task`) follows the FAI Sporting Code, Section 7F. Going back for a later start is done by stepping the task back by hand; stopped tasks are not followed; an elevated goal is shown as a height to arrive over and not checked. On Android since 2026-11-19 as well, tested the same way |
| Task exchange in the open `.xctsk` format: JSON file, compact QR form (`XCTSK:`), compressed form (`XCTSKZ:`) *(official)* | Built: all four forms are read and written, and checked against what XCTrack's own task tool writes for the same task. The iPhone takes a task from the clipboard, from a file (also one opened from another app) or from its QR code through the camera, and shows it as a QR code, compressed or not, or shares the file. The camera has not been tried: the simulator the tests run on has none. No instrument has scanned a code of ours yet: to verify with XCTrack itself. Android takes a task as text or as a file and hands it on as a file; it neither shows nor scans a QR code |
| Navigate toward a chosen live pilot (1.0) | Later |

## Maps and terrain

| XCTrack | Ours |
|---|---|
| Offline vector road maps in Mapsforge format with flying-optimised themes; one map widget per page *(official)* | v1, with our own alpine map style and no one-widget limit. Built on the iPhone on 2026-11-20 (`Venti/Maps`): a region's map is downloaded from the pilot's server, kept, and drawn in the cockpit wherever the glider is inside it. No real region has been built yet, and Android does not download one |
| Downloadable terrain used for height above ground, relief shading and the side view | v1, from 1 m elevation data in the first region. Built on 2026-11-20 from the map's own elevation tiles instead: the iPhone and Android fetch them around the glider and keep them, and the iPhone reads them from a downloaded region; the 1 m data is not used yet |
| Map rotation: north, heading, bearing, navigation target or wind at top | v1 |
| Pan mode with terrain height under the cross-hair and airspace inspection | v1 |

## Airspace and obstacles

| XCTrack | Ours | Engine, as of 2026-11-19 |
|---|---|---|
| Airspace by country from airspace.xcontest.org with temporary activations, checked at start, plus custom OpenAir files *(official)* | v1 for static airspace and OpenAir import. Activations depend on a data source; see [data-sources.md](data-sources.md#airspace) | Built: the OpenAir reader, with circles, arcs and both coordinate notations, reporting every airspace it leaves out. Static airspace by country comes from openAIP. Activations: not built; every airspace counts as active |
| Three warning levels with sounds, on-screen labels and suppression times | v1 | Built: near, very near and inside at XCTrack's margins, plus "entering" from a prediction XCTrack does not have; no flicker at a boundary; silencing for a time, until a time or for the rest of the day. Sounds and labels are the shell's |
| Flight levels measured against the barometer's standard altitude; an option to force satellite altitude *(manual)* | v1 | Built: exact with a pressure altitude. Without one the limit is taken toward the pilot and the warning marked uncertain. Limits above sea level use the cockpit's altitude; XCTrack uses the altitude at the pilot's QNH |
| Three kinds of airspace: restricted (warns), warning (class Q, drawn only), glider area (class W) *(official)* | v1 | Built as a table by class, type and country: avoid, caution, free. Danger areas warn, unlike in XCTrack. Glider areas never warn and never open the airspace around them |
| Per-airspace status: disabled or active, for today or permanently *(manual)* | v1 | Built in part: an airspace can be ignored for a time. Nothing is stored between flights |
| High airspace hidden until close | v1 | Built: a filter by box and by band of altitudes |
| Per-class colours and per-airspace floor and ceiling adjustment (1.0, paid) | Later | |
| Obstacle and cable warnings with safety margin and an emergency map zoom | v1 where open cable data exists (Tirol), Later elsewhere | Not built |

The widgets above that depend on it: the airspace proximity widget (nearest airspace, distance and direction, also the nearest way out from inside) is answered since 2026-11-21 by `WarningEngine::proximity`, which names the one airspace that matters next by the warnings' own checks ([airspace.md](airspace.md#the-airspace-that-matters-next)); `AirspaceIndex::nearby` lists all that are near. The airspace part of the side view is answered by `AirspaceIndex::cross_section`. Terrain for the side view and for limits above ground is in the core since 2026-11-20, from the map's elevation tiles (`crates/core/src/terrain.rs`).

## Livetracking

| XCTrack | Ours |
|---|---|
| Live position to XContest, nearby pilots and their tracks on the map, final glide of others | v1 on our own service. Built on the iPhone and on Android on 2026-11-19 and 2026-11-20: the position goes to the pilot's Venti server, the map tab shows the gliders of pilots who share with everyone and the flight so far of one, and the iPhone's cockpit map shows them too. Final glide of others is not built. Not flown |
| Text messages with presets, watchlist of pilots to follow | v1 |
| FANET and FLARM traffic from an external receiver, with IDs bound to names | v1 |
| SafeSky position sharing | Later |
| Walking detection | v1, for hike & fly |

## Logging

| XCTrack | Ours |
|---|---|
| IGC file with signature; a validator tool is published *(official)* | v1; signing under CIVL's self-certification scheme |
| Automatic takeoff and landing detection | v1 |
| One-tap and automatic upload to XContest | v1 to our logbook. Cross-posting to XContest needs XContest's agreement |
| Replay of an IGC file inside the app | v1; this is also our test harness |

## Automation

| XCTrack | Ours |
|---|---|
| Events: takeoff, landing, GPS fix, Bluetooth connected and lost, battery, airspace warnings, obstacle, turnpoint and start crossings, message received | v1 |
| A sound per event; custom sounds and spoken announcements (1.0, paid) | v1 sounds; speech Later, shared with the assistant's voice |
| Hardware keys and Bluetooth remotes for page, waypoint, zoom, pan and brightness | v1 for Bluetooth LE remotes |
| Phone tweaks: keep screen on, reject calls, mute notifications | v1 where iOS permits; iOS offers less control here than Android |
| Announcing events to other apps | No on iOS; Later on Android |

XCTrack's manual stresses battery-optimisation exemptions on Android, and 1.0.1 added detection of the system closing the app in the background. *(official)* Our recorder treats being killed in the background as the main failure to design against on both platforms.

## Overlays in the cockpit

| XCTrack | Ours |
|---|---|
| Thermal hotspot map from thermal.kk7.ch, including a time-adjusted variant | v1: the author has agreed, by the owner's word; to build ([data-sources.md](data-sources.md#thermalkk7ch-as-read-on-its-own-page-on-2026-11-20)) |
| Rain radar | v1 |
| SkySight forecast layers (finished, awaiting an agreement, paid) | Our own forecast and wind-station layers, v1 |

## A layout a pilot flies with: the owner's

A yardstick for the preset layouts. It is the owner's XCTrack layout, read on 2026-11-20 from a backup of July 2025 (XCTrack 0.9.12.3); the file itself is the owner's and stays outside the repository. What a setting *means* is read from XCTrack's own names for it and is *to verify* where XCTrack's manual does not say. Portrait, three pages, each shown in its navigation modes.

**Page 1, in every mode: the thermal page.** Top to bottom:

| Row | Left | Middle | Right |
|---|---|---|---|
| 1 | Ground speed | Altitude | Wind speed |
| 2 (small) | Bearing | Height over the ground | Wind direction |
| 3, most of the page | Thermal assistant: the last five minutes of track, close up, north up, with the circle drawn and the wind | | Cross-country map: airspace, obstacles, other pilots with their tracks, FAI areas, take-offs, towns, the thermal map of thermal.kk7.ch at 40 %, a final-glide line |
| over the corner of row 3 | Airspace proximity, out to 5 km | | |
| 4 | Climb, 2 s average | Air time | Glide, 8 s average |

**Page 2, with a competition task.** A row of altitude and, for the next turnpoint, the altitude there, the glide needed and the time of arrival; the task map; laid on the map's corner the time to the start and the arrival at the start against the gate; under the map the distance to goal, the glide to goal and the altitude over goal.

**Page 3, flying back to the take-off, closing a triangle or to a waypoint.** Altitude, the distance scored so far and the triangle if it were closed now, in a column at the left; beside them a large airspace proximity with its height line; the cross-country map under both, with the line to fly, the closing circle and the FAI areas with their smallest and largest.

**Settings that say how it is flown:** take-off noticed by itself, landing too; XCTrack's own vario sound off, since an external vario beeps; maps north up with the pilot in the middle.

What our cockpit already does in the layout's place, and where it differs:

| The layout has | Ours, 2026-11-20 |
|---|---|
| Speed, altitude, height over ground, wind speed and direction, climb, glide, air time, always in the same place | The same readings on the first page, the climb largest; air time in the line at the top. While circling, speed and glide give way to the thermal's gain and average |
| A thermal assistant **beside** the map, always there | Since 2026-11-20 on the iPhone, by the owner's word: a drawn close-up beside the first page's map, with the last circles where their air is now, the circle to fly next and the wind; on the glide the last minutes of track ([design.md](design.md#the-cockpit)); on Android too. Not flown |
| Airspace proximity as a standing reading (5 km), small on page 1 and large with a height line on page 3 | Airspace on the map, a band at the top once something is near, and a view from the side on the second page. Since 2026-11-21 the core also makes the reading: with every position the cockpit's view carries the one airspace that matters next, out to 5 km over the ground and also while no warning is up, with the distance and direction to its outline, the height to its floor or ceiling, and the words for a small place and a large one ("CTR Innsbruck · 3.2 km NE · floor 400 m above"). Which airspace that is among several, and what the reading does not know, is in [airspace.md](airspace.md#the-airspace-that-matters-next). **Gap, now the shells':** neither phone shows it; where it is to stand is proposed in [design.md](design.md#the-cockpit). Tested in the core on scripted positions and a replayed flight, not flown |
| Bearing as a reading | Since 2026-11-21 the core writes the course as a reading (`CockpitView::course_words`, "285° W": whole degrees and one of eight compass points), and leaves it out while the glider moves slower than 2 m/s over the ground. **Gap, now the shells':** neither phone shows it; the course still only turns the glider's arrow |
| The scored distance and the unclosed triangle beside the map | On the third page, away from the map. **Gap:** not visible while looking at the map |
| The thermal map of thermal.kk7.ch under the track | To build. The author has agreed, by the owner's word; his terms are read ([data-sources.md](data-sources.md#thermalkk7ch-as-read-on-its-own-page-on-2026-11-20)). It waits for a domain Venti can name to his server |
| Other pilots with their tracks | Other pilots who show their flight are on the cockpit's map; their tracks are not. *To check* |
| Obstacles on the map (cables), with a zoom-in when one is near | Cable cars from the base map only; no obstacle data and no warning yet (table above) |
| Take-offs and towns on the map | Towns from the base map. Flying sites are on the map tab, not in the cockpit. *To check* |
| FAI areas, closing circle | FAI areas, since 2026-11-19; the closing circle is *to check* |
| The competition page's nine readings | A task has its line, its start and its progress in the cockpit ([design.md](design.md#a-task-in-the-cockpit)); reading by reading against this page is *to do* |

What follows for the presets: the cross-country numbers belong on the map's page, and the nearest airspace should be readable while nothing is urgent. The first is not built. The second is made in the core since 2026-11-21, with the course in words, and is on no screen yet.

## Where we go beyond

1. An iPhone app at this level, which does not exist today.
2. Vario from barometer and accelerometer fusion for pilots without an external vario.
3. Groups, Open Glider Network traffic and a buddy view.
4. Live wind stations and forecast wind in the cockpit.
5. Reachable landing fields with public transport home.
6. A glider polar and speed-to-fly.
7. A voice assistant on the pilot's own AI account.
8. A Live Activity on the lock screen and, later, a watch companion.
9. The glide range over the terrain on the cockpit's map, and landings behind a ridge taken out of reach. Built on the iPhone and on Android on 2026-11-20 as a straight glide in every direction; flying around a ridge is not worked out.

## Still to verify

- The complete current widget list and names, from the AIR³ manual pages.
- The meaning of XCTrack's two wind algorithms ("classic" and "particle drift").
- The obstacle data's country coverage.
- Flyskyhy's and SeeYou Navigator's supported Bluetooth varios, from their own sites, since those are the apps iPhone pilots use today.
