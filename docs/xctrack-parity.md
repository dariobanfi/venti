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
| **manual** | Search summaries of the AIR³ manual, which the XCTrack team recommends. Not read directly, except the airspace parts: [Preferences 2](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/preferences2/) and [Navigation widgets 2](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/xctrack-pro-widgets-navigation2/), read 2026-11-19. | To verify, apart from airspace and from what [The smart features, one by one](#the-smart-features-one-by-one-2026-11-21) cites: its pages were read directly on 2026-11-21 |

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
| Airspace | Proximity widget with nearest airspace, distance and direction | v1. In the core since 2026-11-21, with the height to floor or ceiling as well (`CockpitView::airspace_proximity`, [airspace.md](airspace.md#the-airspace-that-matters-next)), and on the iPhone and Android since the same day |
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
| Airspace proximity as a standing reading (5 km), small on page 1 and large with a height line on page 3 | Airspace on the map, a band at the top once something is near, and a view from the side on the second page. Since 2026-11-21 the core also makes the reading: with every position the cockpit's view carries the one airspace that matters next, out to 5 km over the ground and also while no warning is up, with the distance and direction to its outline, the height to its floor or ceiling, and the words for a small place and a large one ("CTR Innsbruck · 3.2 km NE · floor 400 m above"). Which airspace that is among several, and what the reading does not know, is in [airspace.md](airspace.md#the-airspace-that-matters-next). On both phones since 2026-11-21: small at the lower right of the first page's map, large with a drawn height line in a strip under the map of the second page ([design.md](design.md#the-cockpit)). Tested in the core on scripted positions and a replayed flight, not flown |
| Bearing as a reading | Since 2026-11-21 the core writes the course as a reading (`CockpitView::course_words`, "285° W": whole degrees and one of eight compass points), and leaves it out while the glider moves slower than 2 m/s over the ground. On both phones since 2026-11-21, as the small line under the ground speed |
| The scored distance and the unclosed triangle beside the map | On the third page, and since 2026-11-21 as a badge on the map's page of both phones: the distance scored so far, what it scores as, and for an open triangle how far there is to close it. What the triangle scores once closed is in the core since 2026-11-21 (`LiveScore::closing`), on no screen yet |
| The thermal map of thermal.kk7.ch under the track | To build. The author has agreed, by the owner's word; his terms are read ([data-sources.md](data-sources.md#thermalkk7ch-as-read-on-its-own-page-on-2026-11-20)). It waits for a domain Venti can name to his server |
| Other pilots with their tracks | Other pilots who show their flight are on the cockpit's map; their tracks are not. Since 2026-11-21 the core keeps the Open Glider Network's aircraft around the glider with their tracks of the last five minutes, from the answers the shell hands it (`FlightSession::others_near`, [design.md](design.md#the-cockpit)); on no screen yet |
| Obstacles on the map (cables), with a zoom-in when one is near | Cable cars from the base map only; no obstacle data and no warning yet (table above) |
| Take-offs and towns on the map | Towns from the base map. Flying sites are on the map tab, not in the cockpit. Since 2026-11-21 the core gives the cockpit's map the take-offs the app holds around the glider, nearest first, fifty at most (`Core::cockpit_takeoffs`); on no screen yet |
| FAI areas, closing circle | FAI areas, since 2026-11-19. The closing circle since 2026-11-21 in the core, by XContest's rule (5% of the perimeter around the triangle's start), with the distance to its edge and what the triangle scores closed (`LiveScore::closing`), and in the route planner (`RoutePlan::closing_circle`); on no screen yet |
| The competition page's nine readings | A task has its line, its start and its progress in the cockpit ([design.md](design.md#a-task-in-the-cockpit)). Since 2026-11-21 the core makes the page's readings (`CockpitView::task_readings`): for the next turnpoint and goal the distance, the altitude needed on the glide in the wind along the shortest way, the altitude over or under it, the glide needed and the time of arrival; the time to the start gate and whether the glider arrives early; the speed section's average. A "Task" page for them is proposed ([design.md](design.md#the-cockpit)); on no screen yet |

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

## The smart features, one by one (2026-11-21)

What XCTrack works out or decides in flight, feature by feature, as its makers and the manual they recommend describe it in public, against what Venti's engine does today. Nothing here comes from the app itself: it was not downloaded, opened or taken apart, and nobody's copy of its code was read. Where the public pages do not say how something is done, the entry says so; Venti's own method is then ours to choose.

### The ten gaps most worth building for a cross-country pilot

1. **One QNH, and airspace limits above sea level measured at it.** Set from the take-off's known height or a METAR, by hand if wanted. *Medium.*
2. **Airspace that is switched on and off:** schedules, NOTAM activations, a warning 15, 10 and 5 minutes before the airspace the glider is in becomes active, and "all active" when the data is old. *Large: it needs a source of activations.*
3. **Events with a sound, a spoken line and an action each:** airspace levels, start gate about to open, turnpoint reached, instrument lost, battery low, thermal begun and ended. *Medium.*
4. **Other aircraft from the instrument, and each pilot once:** FLARM and FANET traffic through the vario's sentences, bound to names, merged with the same pilot on livetracking; a watchlist; messages. *Large.*
5. **Wind, airspeed and heading from the instrument when it has them,** and the airspeed and heading estimated from the wind as readings when it has not, with a notice when the glider is carried backwards. *Small to medium.*
6. **A cable warning that follows the glide:** will the glider arrive lower than the cable plus a margin, looking wider on the side it turns to, with the map zooming in. *Medium; the data is the hard part.*
7. **The cross-country readings XCTrack has and we lack:** the FAI triangle's smallest, largest and "real" potential as numbers, leg lengths and shares, average cross-country speed, a direction to the nearest point of the closing circle. *Small.*
8. **The vario's other sounds:** a buzzer just under the lift threshold, an alarm when the pressure data stops, a pattern that tells the ten-second average, and the pilot's own table of pitch, cycle and duty. *Medium.*
9. **Landing detection that knows the height above ground and the wind,** a mode that asks the pilot, and a mode that never ends the flight (hike and fly). *Small.*
10. **The task page's missing arithmetic:** altitude over the end of the speed section, an allowance of height for every turnpoint still to round, a choice of which speed an arrival time is worked out from, an elevated goal that is checked. *Small.*

Not in the ten because XCTrack does not have them either, by its own tracker: a polar, MacCready and speed to fly; netto and total energy of its own; a vario fused from barometer and accelerometer; wind without circling; wind taken from other pilots. They stay in [Where we go beyond](#where-we-go-beyond).

### Sources of this section

All read on 2026-11-21, late, into the first hour of the 22nd. Of the tracker's issues, the titles were searched; T-45, T-367, T-594, T-652, T-741, T-807, T-974, T-1119, T-1148 and T-1171 were read with their comments, and the others cited by title, state, date and votes only. The pages were read through a tool that hands back a page's content condensed, so a figure below is as that reading gave it: *to check against the page itself* before Venti takes a number over as its own.

| Key | Page |
|---|---|
| X-man | [xctrack.org/Manual.html](https://xctrack.org/Manual.html) |
| X-log | [xctrack.org/Change_Log.html](https://xctrack.org/Change_Log.html) |
| X-comp | [xctrack.org/Competition_Tutorial.html](https://xctrack.org/Competition_Tutorial.html) |
| X-dev | [xctrack.org/External_Devices.html](https://xctrack.org/External_Devices.html) |
| X-js | [xctrack.org/JavaScriptInterface.html](https://xctrack.org/JavaScriptInterface.html) |
| X-faq | [xctrack.org/Faq.html](https://xctrack.org/Faq.html) |
| A-p1 … A-p4 | AIR³ manual, Preferences [1](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/preferences/), [2](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/preferences2/), [3](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/preferences3/), [4](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/preferences4/) |
| A-nav, A-nav-ii | AIR³ manual, menu [Navigation](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/navigation/) and [Navigation II](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/navigation2/) |
| A-live, A-logs | AIR³ manual, menu [Livetracking](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/livetracking/) and [Tracklogs](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/tracklogs/) |
| W-air, W-fly | AIR³ manual, widgets [Air](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/xctrack-pro-widgets-air/) and [Flying](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/xctrack-pro-widgets-flying/) |
| W-nav1, W-nav2 | widgets [Navigation](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/xctrack-pro-widgets-navigation/) and [Navigation 2](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/xctrack-pro-widgets-navigation2/) |
| W-xc1 … W-xc3 | widgets [Xcontest](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/xctrack-pro-widgets-xcontest/), [2](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/xctrack-pro-widgets-xcontest2/), [3](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/xctrack-pro-widgets-xcontest3/) |
| W-comp1, W-comp2 | widgets [Competition](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/xctrack-pro-widgets-competition/) and [Competition 2](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/xctrack-pro-widgets-competition-2/) |
| W-sys, W-live, W-oth | widgets [System](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/system-xctrack/), [Livetracking](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/xctrack-pro-widgets-livetracking/), [Others](https://www.fly-air3.com/en/support/air3-xctrack-manual/xctrack-manual/xctrack-widgets-manual/xctrack-pro-widgets-other/) |
| A-fanet | AIR³ FAQ, [enabling FLARM, FANET and ADS-L](https://www.fly-air3.com/en/ufaqs/enable-flarm-fanet-on-air3-7-3/) |
| T-*n* | Issue *n* of the developers' tracker, `gitlab.com/xcontest-public/xctrack-public/-/issues/n`, with the developers' own answers in it |

The AIR³ pages also read and found to hold nothing on a computation: the manual's index and its Customize page.

"Size" is the work in Venti: **small** is a rule in an engine module that exists and a reading in the view; **medium** is a new module or a new kind of input; **large** needs a data source, a service or a protocol besides.

### Wind

**Wind from circling.**
- *For the pilot:* the wind's speed and direction where the glider last circled, for the wind readings, the compass, glide predictions and the thermal picture.
- *How, publicly:* from satellite positions only; a wind appears once a full circle is flown (W-air, W-fly). A developer in the tracker: the method assumes a perfect circle carried along by the wind, and a circle the pilot corrects spoils it (T-594, a developer's answer of 2020-07-03); the compass plays no part in it (T-974, a developer's answer of 2023-04-04). Earlier changes made it follow "more immediate" wind (X-log, 0.4.5) and reset it at take-off (X-log, 0.5.1.4). The method itself is not published. A pilot with a fast glider reports it wrong or silent in strong wind, open since 2020 and repeated in 2025 (T-594).
- *Venti:* `live/wind.rs`. A circle is fitted to the glider's velocities over the ground of the last 40 s; its centre is the wind and its radius the airspeed. It asks for samples in ten of twelve sectors of direction and a fit good to 2 m/s, and it takes a wind up to twice the airspeed. The wind's age is kept (`CockpitView::wind_age_s`).
- *Gap:* none; ours covers the case their tracker lists as a weakness. Not flown.

**Wind by height.**
- *For the pilot:* a graph of wind strength and direction in slices of 100 m, with a line at the present altitude; the same widget can show climb by slice (W-air, "Altitude statistics").
- *How, publicly:* slices of 100 m over the standard altitude when there is one. For the climb graph: the vario averaged over 20 s, the greatest value per slice kept for 40 minutes and then let fall evenly to nothing over the next 40 (W-air). How a slice's wind is aged is not said.
- *Venti:* `live/computer.rs` keeps the wind in bands of 200 m (`WIND_BAND_M`), as the mean of what was measured while circling there, with the climb through the band (`CockpitView::winds_by_height`).
- *Gap:* the climb by band does not age, so a morning's weak climb stands beside the afternoon's; no line at the present altitude is the shell's matter. *Small.*

**Wind from ground-speed differences in straight flight, with no sensor.**
- *How, publicly:* XCTrack does not do it. Wind comes from circles only (T-974); the manual says of every wind reading that it needs a full circle (W-air, W-xc2).
- *Venti:* nothing. The fit in `live/wind.rs` needs nearly a whole circle.
- *Gap against XCTrack:* none. For Venti it would be a step beyond: on a long glide the wind is a memory (`wind_age_s`). Without an airspeed it can only be had from turns of part of a circle, with a known airspeed. *Medium, and to prove on recorded flights before it is shown.*

**Wind and airspeed from an instrument.**
- *For the pilot:* a wind that is current on the glide, and a true airspeed, from an instrument with a probe.
- *How, publicly:* when wind or airspeed is switched on for an external device, XCTrack uses those in place of its own; a switch turns a device's wind direction by 180° (A-p4). Sentences that carry them: `LXWP0`, `PDGFTL1`, `PLKAS`, `PTAS1`, and the Bluetooth services of FlyBeeper and Vector Vario; the Environmental Sensing Service has true wind speed and direction (X-dev, X-log 0.9.5 to 0.9.11.3). True or indicated airspeed can be chosen for the reading where a device gives both (X-log, 0.9.12.4).
- *Venti:* the `sensors` crate reads an airspeed and a heading out of a sentence (`Reading::airspeed_ms`, `heading_deg`); the flight computer uses neither, and reads no wind from an instrument.
- *Gap:* take an instrument's wind and airspeed, each with its age and its source named, and prefer them over the circle's on a glide. With an airspeed and the course over the ground, a wind in straight flight follows. *Medium; needs an instrument that sends them.*

**Wind from FANET or from other pilots.**
- *How, publicly:* not found. Nothing read says XCTrack shows or uses the wind of FANET ground stations or of other gliders; weather-station wind in the cockpit is an open request since 2019 with twenty votes (T-426), and the manual's own way is a web page in a widget (W-air, W-oth). *To verify* that 1.0 has not added it.
- *Venti:* live wind stations are in the app (`crates/core/src/ground_wind.rs`), not in the flight computer.
- *Gap against XCTrack:* none.

**Airspeed and heading, estimated.**
- *For the pilot:* an airspeed reading without a probe, and a heading for the compass, the map and the thermal page.
- *How, publicly:* airspeed is the speed over the ground with the computed wind taken out, so it shows what the pilot does with brake and bar; heading is worked out from the track over the ground and the wind. A compass, the phone's or an instrument's, can be taken for the heading instead; the manual advises against it for its lag and its need of calibration (W-fly, W-nav1, A-p4). The web interface hands out `heading`, `airspeed` and `speedComputed` (X-js).
- *Venti:* `live/computer.rs` works out the direction through the air for its circling rule once a wind is known, and `live/wind.rs` gives the airspeed of the circle. Neither is a reading in `CockpitView`.
- *Gap:* two readings, each absent without a wind and marked with the wind's age. *Small.*

**Carried backwards.**
- *How, publicly:* the speed reading can mark flying backwards, as an option (X-log, 0.9.7 and 0.9.7.6); a developer says the compass is used for this warning and for nothing else in the wind (T-974).
- *Venti:* `CockpitView::strong_wind` says beforehand that the wind last measured leaves less than 10 km/h of way into it.
- *Gap:* saying it when it happens: the course over the ground lies opposite the heading. With a compass that is certain; from the wind estimate it is a deduction. *Small.*

### Thermals

**Noticing a thermal.**
- *How, publicly:* begins when the heading, not the track, has turned 90° or more in 30 s and the 30-second average of the vario is −0.5 m/s or better; ends when it has turned less than 30° in 30 s and the average is below −0.5 m/s (X-man, W-air). In 1.0 the pilot can set the window, the two turns and the two climbs (A-p3; X-log, 1.0.0 RC1). The manual warns that a smaller turn gives false starts and a larger one a late start (A-p3).
- *Venti:* the same rule and numbers in `live/computer.rs` (`Config::circling_start_deg` and its neighbours), on the direction over the ground or, once a wind is known, through the air.
- *Gap:* none in the engine; the settings are not on a screen.

**The thermal assistant.**
- *For the pilot:* a close-up of the last circles, coloured or sized by climb, with a circle drawn around the place of best lift.
- *How, publicly:* three ways of drawing past positions (W-air; X-log, 0.5.1.5). "Classic", the default: each is moved by the wind times its age. "Particle drift": each is moved by the wind times the height the glider has gained since, divided by the climb there plus one, on the thought that strong lift leans less. "None": the track as flown. The centre is "estimated from climb-rate data" and nothing more is said of it (W-air); the tracker records only that the "core detection algorithm" was changed in 2013 (X-log, 0.4). Settings: scale (120 m advised for a paraglider), length of track, north or another direction on top, airspace, a task's cylinders and line (W-air; X-log, 1.0.0 RC1).
- *Venti:* `live/thermal.rs`. The last fifty seconds moved with the wind to where their air is now, a circle fitted to them and the lean of the climb around it, and a line of words ("Stronger 20 m NW", "Centred") in `CockpitView::lift`.
- *Gap:* none in what is computed; ours also says which way to move. "Particle drift" is a second way of drawing the same thing and is not wanted for itself. Not flown.

**The last thermals, and going back to one.**
- *How, publicly:* the maps mark the latest thermals with their average climb, coloured by it; a slider says how many (W-xc2, W-comp1). A longer track in the assistant "helps relocate a lost thermal" (W-air). Whether a mark is moved with the wind after the glider has left is not said: *to verify.* An arrow back to the last thermal is not described anywhere read.
- *Venti:* `CockpitView::thermals`: the last twelve that gave 50 m or more, where the glider circled, not moved with the wind.
- *Gap against XCTrack:* none known. Beyond it: moving a left thermal's mark with the wind for as long as its air can still be reached, and a bearing to it. *Small.*

**Height gained in the thermal.** The present altitude less the altitude where circling began (W-air). Venti: `CockpitView::thermal`, with the average as well. No gap.

**The thermal map (thermal.kk7.ch).** A layer of where flights have found lift, plain or narrowed to the season and hour; the manual notes that each mark is where the lift leaves the ground and that the pilot must add the drift for the height (W-comp2). It is a picture, not a calculation in the app. Venti: to build, see [Overlays in the cockpit](#overlays-in-the-cockpit).

### The glide

**The glider's performance.**
- *How, publicly:* two numbers the pilot enters, trim speed and glide ratio; they feed the glide marks on the maps, the side view and the altitudes of arrival (A-p1). No polar, no MacCready setting, no speed to fly, no bugs or ballast: asked for since 2022 and open (T-807 with sixteen votes, T-862, T-1220).
- *Venti:* the same two numbers (`live/navigation.rs`).
- *Gap against XCTrack:* none. Beyond: a polar of three points gives the best glide in a head or tail wind and a speed to fly; bugs and ballast are a sailplane's matter and of no use to a paraglider. *Medium; needs each wing's polar, which makers rarely publish, so a pilot's own flights would have to supply it.*

**Glide ratio now.** Over an averaging time the pilot sets; shown as the vario instead while climbing (W-fly). Venti: `CockpitView::glide_ratio`, over the last quarter minute. Gap: the time is fixed. *Small.*

**Glide needed, and height on arrival.**
- *For the pilot:* for the next turnpoint, the end of the speed section, the goal or a waypoint: the glide ratio needed, with a reserve of height the pilot sets, and the height left on arrival (W-nav2, W-comp2).
- *How, publicly:* arrival height from the satellite altitude, the configured glide ratio and trim speed and the present wind, along the shortest way through the cylinders still to round, less a fixed 15 m for each of them (W-comp2). It can be shown above ground, above sea level, or as the height the leg costs (W-nav1). The glide needed is measured to the nearest point that counts, against the ground's height there when terrain is loaded, not the waypoint's (T-1148, a developer's answer of 2024-09-30); from 1.0.4 the waypoint's height is taken when terrain is missing (X-log). Each reading chooses its target: the centre, the optimised point or the nearest point of the cylinder (W-nav1, W-nav2).
- *Venti:* `live/navigation.rs` (required and expected glide, arrival height, time, whether a wind was known, ground in the way) and `task/glide.rs` (leg by leg along the shortest way, in the wind); `CockpitView::task_readings` for the next turnpoint and goal.
- *Gap:* the allowance per turnpoint; a reserve the pilot sets in the glide needed; the reading for the end of the speed section; a choice of height above ground or above sea level. *Small.*

**Time of arrival.** Worked out from a speed the pilot chooses: speed over the ground, airspeed with the wind taken into account, trim speed with the wind, or the flight's cross-country speed so far, each over an averaging time (W-nav1). Venti: trim speed in the wind only (`time_to_go_s`). Gap: the other three. *Small.*

**Glide marks and the side view.**
- *How, publicly:* on the line ahead of the glider on the maps, a filled mark where the glide of the moment comes down and an open one for the configured glide and trim speed in the computed wind (W-xc2). The side view shows 5 to 50 km of ground and airspace ahead, along the track or towards the target, with a solid line for the present glide (not valid in a climb) and a dotted one for the configured glide; a task's remaining turnpoints are lines in it (W-nav1). Whether the marks on the map stop at the ground is not said: *to verify.*
- *Venti:* `live/range.rs`: the edge of what a straight glide reaches over the ground in every direction, in the wind; the side view with one glide line and the ground.
- *Gap:* none; the outline over terrain is beyond what is described (a request for the area within reach was closed in 2017, T-25; only its title was read, not how it was answered). The side view does not follow a task's route (see the Widgets table).

**Height above ground and terrain.** Satellite altitude less the height of the ground from downloaded terrain files (W-fly); terrain also serves the side view, the glide needed (above) and the landing rule (below). Venti: `flight/terrain.rs`, `CockpitView::height_above_ground_m`. No gap.

**Landings within reach.** Not found in XCTrack: a nearest-landing reading is an open request (T-486, 2019; T-1301, 2026). The "nearest airfield" in the Widgets table above is *to verify*. Venti: the landing among the pilot's waypoints it would arrive highest above, with ridges in the way counted. Beyond.

### Airspace and obstacles

**How near is near.**
- *How, publicly:* one "scaled distance" in which a metre of height counts as four over the ground. Directly below an airspace: "close" from 500 m of height, "really close" from 100 m, and both limits shrink as the glider is further off to the side; "crossed" on entry (A-p2, W-nav2). No prediction from the glider's course is described. Each of the three is an event with an overlay, a sound, and a time for which its repeat is held back; from 2024 the "inside" warning can sound on entry only (T-1119, a developer's answer of 2024-06-14). The durations are not given: *to verify.*
- *Venti:* `venti-airspace`, with the same margins and the four to one, an "entering" warning from the path of the last 20 s looked 60 s ahead, no flicker at a boundary and silencing ([airspace.md](airspace.md#warnings)).
- *Gap:* none; the prediction is ours.

**Which altitude a limit is held against.**
- *How, publicly:* flight levels against standard pressure; limits above sea level against the barometric altitude at the QNH; limits above ground against the satellite altitude and terrain. A switch forces the satellite altitude for flight levels too, for competitions that rule so (W-nav2, A-p1, A-p2). The proximity reading can show a limit as written or turned into a satellite altitude with the QNH (W-nav2).
- *Venti:* flight levels exact with a pressure altitude; limits above sea level against the cockpit's best altitude ([airspace.md](airspace.md)). No QNH.
- *Gap:* a QNH (see [Altitude](#altitude-and-qnh)) and the switch. *Medium with the QNH.*

**Active or not.**
- *How, publicly:* airspace from XContest's service comes with schedules and, for some countries, NOTAM activations; it is refreshed at start and in flight (X-man; X-log, 0.9.4.5 and 1.0.0). When the data is too old, every airspace counts as active and the app says so. An airspace that will become active is drawn grey, and a glider inside it is warned 15, 10 and 5 minutes before. The pilot can set any airspace active or off, for the day or for good, and with the paid version move its floor or ceiling when a NOTAM covers part of it (A-p2).
- *Venti:* every airspace is active; one can be ignored for a time; nothing is kept between flights.
- *Gap:* all of it. *Large: schedules and NOTAMs need a source, per country, with a licence ([data-sources.md](data-sources.md#airspace)). The pilot's own on and off, kept from flight to flight, is small and worth having first.*

**High airspace kept back.** An airspace whose floor lies above a height the pilot sets is not shown until the glider is within a set distance (W-nav2, W-xc2). Venti: a filter by box and band of height. No gap.

**The proximity reading.** Name, height to floor or ceiling, distance to the outline, an arrow to the nearest edge or, from inside, the nearest way out, coloured by the scaled distance (W-nav2; X-log, 1.0.0 RC1). Venti: `CockpitView::airspace_proximity`. No gap.

**Obstacles.**
- *How, publicly:* cables and the like by country from XContest's service. The time to reach one is worked out with the worse of the configured and the present glide ratio and the faster of the configured and the present speed; the area looked at is widened and leans to the side the glider is turning; within a time window the pilot sets, an alert comes if the glider would arrive lower than the obstacle's height plus a margin. The map can zoom in by itself while one is near (A-p2, W-xc1; X-log, 0.9.9 and 0.9.10.1).
- *Venti:* `flight/cables.rs`: a cable within a margin of the straight course of the next 30 s, with the glider less than 100 m above the cable's height over the ground, or below it.
- *Gap:* ours compares heights now, not at arrival: a glider sinking toward a cable from well above is warned late, and one in a turn is looked at along a straight line. The map's zoom. *Medium for the rule; the data beyond Tirol is the larger part.*

### A competition task

**Reaching a turnpoint.** One fix inside a band of tolerance around the cylinder's edge, or two fixes in a row on either side of it; the direction does not matter, and the next turnpoint is then taken by itself (A-nav-ii, X-comp). Venti: `task/progress.rs` counts only a crossing of the edge itself, on purpose (Sporting Code 8.1). No gap; the difference is deliberate.

**The shortest way.** Distances, glides and times can each be measured to the turnpoint's centre, to the "optimised point" on its cylinder along the shortest way through the rest of the task, or to the cylinder's nearest point (W-nav1, W-comp2); the method of optimising is not published. The task's length is given both ways, shortest and through the centres (A-nav-ii). Venti: `task/route.rs`, by the Sporting Code's annex. No gap; *to verify* against a scored task, as its head says.

**The start.**
- *How, publicly:* time to the next gate; the predicted time of crossing the start, early (wait) or late, to the nearest point of the cylinder and to the optimised point; the speed needed to be there on time (W-comp2, X-comp). The tutorial on xctrack.org says the prediction is at trim speed and leaves the wind to the pilot (X-comp); the AIR³ manual gives the same four choices of speed as for an arrival time, two of them with the wind (W-comp2). The tutorial is the older text: *to verify* which holds today. An alert "speed section will open soon" can be a sound, a spoken line or both (A-nav-ii). The choice of entering or leaving the start cylinder was taken out (X-log, 0.9.7).
- *Venti:* `task/progress.rs` and `TaskReadings::start`: time to the gate, distance and speed to the edge, whether the glider arrives early; a start counts only when both fixes of the crossing are at or after the gate.
- *Gap:* the alert before the gate as an event (below). *Small.*

**End of speed section and goal.** Distance, glide needed and arrival height for each; the share of the speed section flown, for leading points; a goal line or cylinder; an elevated goal measured from the waypoint's height, with the end of the speed section then tied to the goal (W-comp2, A-nav-ii; X-log, 1.0.0). Venti: distance to both, glide and arrival height for goal, the speed section's average; the share is worked out and not shown; an elevated goal is shown and not checked. Gap: arrival height over the end of the speed section, and the check. *Small.*

**Speed needed.** Only "speed to start" is described. A speed needed to reach goal before the task's deadline is not on any page read: *to verify.* Venti: nothing of the kind. *Small if wanted.*

**Earth model.** WGS-84 or the FAI sphere per task; the two differ by about half a percent, 200 to 300 m on a cylinder of 50 km (X-comp, A-nav-ii). Venti: both (`task/mod.rs`). No gap.

**Going back for a restart, a stopped task.** Stepping the task back by hand, by a button or a key (A-nav-ii). Nothing automatic is described. Venti: the same. No gap.

### Cross-country without a task

**Scoring in flight.**
- *How, publicly:* the flight is scored as free distance over three turnpoints, flat triangle or FAI triangle, with the league's factors and closing distance as settings (A-p1, W-xc2); worked out again every 15 seconds (X-log, 0.4.8.6) and made "more precise" in 2020 (X-log, 0.9.4.4). The manual says the app's own optimiser is quicker and rougher than the server's, so the two can differ a little (A-logs). The method is not published.
- *Venti:* `flight/scoring.rs` on a position every ten seconds, every half minute (`CockpitView` score, `LiveScore`).
- *Gap:* none that a pilot would see.

**FAI areas and potential.**
- *How, publicly:* the areas in which a third corner makes an FAI triangle, with the smallest and largest triangle written in; three numbers as a reading: the largest FAI triangle still possible, the smallest still certain, and the "real" potential, the largest the flight so far supports, which never falls (W-xc2, W-xc3; X-log, 1.0.0). Lengths of the legs flown and each as a share of the triangle (W-xc2). Which side the areas stand on is not said: *to verify.*
- *Venti:* `flight/fai.rs`: areas on the longest straight line flown, `smallest_m`, `largest_m`, `is_inside`.
- *Gap:* the "real" potential, and leg lengths with shares. *Small.*

**Closing a triangle.**
- *How, publicly:* a closing circle appears once the flight counts as a triangle; the navigation mode leads to the nearest point of the circle and, once inside, to the take-off (A-nav; X-log, 0.6.0-263). A reading gives what the triangle is worth if the glider gets back (W-xc3).
- *Venti:* `LiveScore::closing`: the circle by XContest's 5%, the distance to its edge, the score closed.
- *Gap:* a direction to the circle's nearest point as the thing to navigate by, then the take-off. *Small.*

**Average cross-country speed.** The scored distance over three turnpoints divided by the time since take-off (W-xc3). Venti: nothing in the view. *Small.*

**Direction of travel.** A direction for the top of a map or compass that changes only when the glider has made more than 3 km in 20 minutes and has flown straight for about two minutes, so that circling does not turn the map (W-air, W-nav1). The map can also put the glider off centre, toward the back, when its speed over 20 minutes is like its cross-country speed (W-xc1). Venti: nothing. *Small; it belongs with map rotation.*

### Vario and altitude

**Climb rate.**
- *How, publicly:* from the pressure sensor through a low-pass filter whose strength is a slider; a calibration page shows the pressure, its rate, and the mean and spread of the altitude over 20 s (A-p4). A vario value sent by an instrument is used and averaged like the app's own (X-log, 0.5.0.2). No accelerometer: the developers tried it early and dropped it (T-652, a developer's answer of 2020-11-18). Each vario reading has its own averaging time (W-fly).
- *Venti:* `live/vario.rs`, a two-state Kalman filter over altitude that weighs each source by its noise; an instrument's own climb is preferred (`VarioSourceKind`); an average over 20 s.
- *Gap against XCTrack:* a second average the pilot sets (the owner flies with 2 s). *Small.* Beyond: fusing the phone's accelerometer, item 2 of [Where we go beyond](#where-we-go-beyond). *Large; per-frame work that stays in the shell by the architecture's rule.*

**Netto and total energy.** Netto only as a value an instrument sends (X-log, 0.9.12.4). Total energy is on no page read. Venti: neither. Without a polar and an airspeed neither can be worked out honestly. *Medium once both exist.*

**Sources and falling back.** Several sensors at once, each named for what it supplies and put in order; the internal satellite receiver lost for 30 s hands over to the external one, and the flight file is then marked as not complying with CIVL's rules (A-p4). Venti: source fallback for the vario; no mark in the file. *Small, and it matters for a signed file.*

### Altitude and QNH

- *How, publicly:* one QNH for the whole app since 0.9.6 (X-log). It is set from the satellite altitude on the ground before take-off, by hand, or in the paid version from the nearest airport's METAR or weather station at start and about hourly (A-p1, W-fly, W-air). Readings: satellite altitude; barometric altitude at the QNH or at 1013.25 hPa; flight level; height above ground; height above take-off (W-fly).
- *Venti:* `live/atmosphere.rs` gives pressure altitude; the cockpit has one "best altitude". No QNH, no flight level reading.
- *Gap:* a QNH worked out on the ground from the terrain's height under the glider (better than the satellite's), corrected by hand, and optionally from a METAR; the altitude at QNH for airspace limits above sea level; a flight level reading near airspace given in flight levels. *Medium. METARs are already read on the server (`sources-awc`).*

### Sound

**The vario's voice.**
- *How, publicly:* beeps whose pitch and rate rise evenly with the climb up to 6 m/s; below the lift threshold down to −0.5 m/s a double beep, only while the short average is above the long one; a sink alarm of two tones, the second a semitone lower for every 0.5 m/s; four falling tones every 30 s when pressure data stops; a change of tone colour, then a "tik-tak", to tell the ten-second average in steps of 1 m/s (X-man, A-p2). Settings: both thresholds, the averaging time of the sound, separate loudness for weak lift, silent before take-off; in the paid version a table from −7 to +7 m/s of pitch, cycle and duty, kept as a file (A-p2; X-log, 0.9.4).
- *Venti:* `live/tone.rs`: beeps from 0.2 m/s rising to 6 m/s, a steady falling tone below −2.5 m/s, silence between and on the ground. Fixed.
- *Gap:* the weak-lift buzzer, the alarm for lost data, thresholds and a table the pilot can set, the pattern for the average. *Medium, most of it in the shells' audio; the rules are small.*

### Take-off and landing

- *How, publicly:* a take-off speed the pilot can set, to be quicker or to keep the receiver's wandering from starting a flight (A-p3; X-log, 0.6.0-382); made "less prone" to false take-offs in 2023 (X-log, 0.9.10). Position sharing begins with a "significant flight": 150 m above ground for more than two minutes (A-live). Landing: automatic, "based on speed and lift" (A-p3), and by a developer's word on the flight's top speed, the wind and the height above ground (T-45, 2017-08-30), stricter high above ground and in strong wind (X-log, 0.5); a glider parked in wind at 400 m was taken for landed where terrain was not loaded (T-741). Or manual, where the app asks "landed or still flying"; or none, for hike and fly (A-p3; X-log, 0.6.0-263). After an automatic landing a new take-off starts a new file (A-p3).
- *Venti:* `live/computer.rs`: take-off at 4 m/s held for 5 s, or 25 m of steady height change in 30 s; landing below 2 m/s without climbing or sinking for 20 s, taken back if the glider flies again within two minutes.
- *Gap:* the landing rule does not look at the height above ground, which the core has; no asking mode and no never-land mode. *Small.*

### Events and what they set off

- *How, publicly:* events named on the pages read: take-off, landing, thermal begun and ended, satellite fix found, Bluetooth connected and lost, battery (an action at 5%), the three airspace levels, an airspace about to become active, an obstacle, phases of a task ("speed section will open soon"), a livetracking message (A-p2, A-p3, A-nav-ii; X-log, 0.5.1.1). Each can have a sound, a spoken text, the pilot's own sound or text, and a notice on the screen; and an action: change the page, change the display (backlight), change the app's state (A-p2, A-p3). Events can be announced to other apps (A-p4; X-log, 0.9.12.4). The full list is behind a "show all events" button and is not printed: *to verify.*
- *Venti:* the core hands out a task's events one at a time (`take_task_event`) and its airspace and cable warnings; the automatic switch to the thermal page hangs on `CockpitView::circling`. There is no one list of events.
- *Gap:* one stream of events from the core (what happened, when, how urgent, a line to say), and in each shell a table from event to sound, speech and page. *Medium.*

### Other pilots

- *How, publicly:* the glider's position goes to XContest every 60 s and other pilots within 20 km come back, for about 100 KB an hour; the list shows name, altitude, height above ground, take-off, distance flown and distance away, nearest first, within 15 km unless asked for more; a pilot ticked there has their track drawn (A-p1, A-live). A watchlist is followed without ticking when within 15 km (A-live; X-log, 1.0.0). Messages from a list the pilot prepares, shown for a minute (A-live, W-live). The status line counts who is watching (W-sys). The flight is claimed for the league by itself a minute after landing (A-logs).
- FANET and FLARM, through an instrument or the AIR³'s module: the IDs go out every second and the pilot's name every minute, only while flying (A-fanet); incoming `PFLAA` and `FNNGB` sentences are read (A-p4); a device's ID is bound to a pilot by XContest, by hand or by FANET, and where they disagree XContest's word goes before the pilot's own and that before FANET's, so that one pilot seen by radio and by internet is drawn once (A-live; X-log, 0.9.5). The newest beta sends ADS-L (X-log, 1.0.7). SafeSky: the position is sent out, at an interval the pilot sets (A-p1; X-log, 0.9.11).
- Navigating to a chosen live pilot is a navigation mode of its own in 1.0 (X-log, 1.0.0; A-p3 lists it among the modes a page can show in). How the target's position is aged between reports is not said: *to verify.*
- What it does not do, by its tracker: the Open Glider Network (T-367, twenty votes, open since 2018), a traffic radar or buddy-list reading (T-1171), any collision alert. The "final glide of others" in the Livetracking table above was not found on a page read today: *to verify.*
- *Venti:* the position to the pilot's Venti server, others who share on the map, OGN aircraft and their last five minutes in the core (`FlightSession::others_near`).
- *Gap:* traffic from the instrument's sentences; one pilot from two sources drawn once; a watchlist; messages; a pilot as a navigation target. *Large as a whole; the sentences and the merging are a medium first step.*

### Bearing, track and heading

XCTrack's words, as the manual uses them (W-fly, W-nav1, W-nav2): **bearing** is the direction the glider moves over the ground, which the manual also calls track; **heading** is where the glider points, worked out from the bearing and the wind or read from a compass; the direction to a target is the "navigation target" or "course to". A compass or map can have north, bearing, heading, the target, the wind or the direction of travel on top; the arrow to an airspace can be turned by north, bearing or heading; the wind can be read relative to the bearing, as with or against (W-nav1, W-nav2). Circling is judged on heading (X-man).

Venti's words differ: **course** is the direction over the ground (`course_deg`, `course_words`), **bearing** the direction to a place (`bearing_deg`). That is the common usage and should stay; a pilot coming from XCTrack will look for "bearing" and find the course. Gap: no heading reading (above), no wind relative to the course. *Small.*

### Also called out in the manual

- **Statistics of a flight** over 30 s, from barometric altitude where there is one (A-logs). Venti: `flight/analysis.rs`.
- **Time from the satellites** as the clock, and a correction of a receiver whose date is wrong by more than a minute, at the cost of CIVL compliance (W-fly, A-p4). Venti: nothing of the second.
- **Sunset and civil twilight**, as a time or as time left (W-fly). Venti has both.
- **Battery of an instrument** in the status line (W-sys). Venti: `InstrumentView::battery_percent`. A forecast of the phone's own battery lasting the flight is on no page read; Venti has none either.
- **Replay of a flight** through the whole app, faster if wanted (A-logs). Venti has it.
- **A web page as a reading**, handed the position (W-oth, X-js). Not for Venti: no web views.

### What this section could not confirm

- Every figure above against the page itself (see "Sources of this section").
- The full list of events, and the times for which a repeated warning is held back.
- Whether marks of left thermals move with the wind; which side the FAI areas stand on; whether glide marks on the map stop at the ground.
- Whether the start prediction counts the wind today (the two public texts differ).
- "Nearest airfield" and "final glide of others" from the tables above; a required speed to goal; wind from FANET stations in 1.0.
- How take-off is decided beyond a speed, and the landing rule's numbers: not published.
- The methods behind the wind, the thermal's centre, the route through cylinders and the in-flight scoring: not published, and not looked for anywhere else.

## Still to verify

- The complete current widget list and names, from the AIR³ manual pages.
- ~~The meaning of XCTrack's two wind algorithms ("classic" and "particle drift").~~ Read on 2026-11-21: they are ways of moving the thermal assistant's past positions with the wind, not ways of working out the wind; see [Thermals](#thermals).
- The obstacle data's country coverage.
- Flyskyhy's and SeeYou Navigator's supported Bluetooth varios, from their own sites, since those are the apps iPhone pilots use today.
