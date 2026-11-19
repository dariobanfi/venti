# Cockpit checklist: parity with XCTrack

Last revised: 2026-11-18

XCTrack is the benchmark for the Fly mode. This document lists what XCTrack does, what our first cockpit release must match, and where we intend to go further.

## Evidence

XCTrack is closed source. This inventory was assembled from:

| Tag | Source | Checked |
|---|---|---|
| **official** | xctrack.org pages read directly: home, Manual, External Devices, Change Log, Competition, Open Data, JavaScript Interface, Maps, FAQ | 2026-11-18 |
| **config** | Real exported XCTrack configuration files published on GitHub (versions 0.9.12.x), which reveal widget classes and option names | 2026-11-18 |
| **tracker** | The developers' public issue tracker, gitlab.com/xcontest-public/xctrack-public | 2026-11-18 |
| **manual** | Search summaries of the AIR³ manual, which the XCTrack team recommends. Not read directly. | To verify |

Items without a tag are **config**. The stable line is 0.9.12.x; 1.0 has been in beta since June 2026 (1.0.7-beta on 2026-10-05).

## What is built

As of 2026-11-19, in the Rust core and tested by replaying a flight, but not yet flown: the vario pipeline with source fallback, averaged climb, ground speed and course, glide ratio, wind from circling, automatic take-off, landing and circling detection, thermal gain and average, distance and bearing to take-off, the trail behind the glider, crash-safe IGC recording, in-app replay, the vario tone description, and parsers for `LK8EX1`, `POV`, `XCTRC`, `LXWP0` and BlueFly `PRS` with the three serial-style Bluetooth services. Everything else in the tables below is still to do.

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
| Altitude | GPS altitude, barometric altitude (QNH automatic or manual), above ground, maximum, above takeoff, flight level, QNH, gain in current thermal | v1 |
| Vario | Numeric with selectable averaging, colour bar, altitude-over-time graph, netto (external sensor only) | v1; netto Later |
| Speed and glide | Ground speed with blown-back indication, airspeed (estimated or external), current glide ratio, average XC speed | v1 |
| Wind | Speed, direction, combined compass and wind, wind and climb by altitude band | v1 |
| Thermal | Thermal assistant: drift-corrected trail coloured by climb, previous thermals, optional airspace and navigation overlays | v1 |
| Navigation | Next turnpoint name, distance, arrival height, required glide, arrival time; distance and course to takeoff; bearing; coordinates; nearest airfield | v1 |
| Side view | Terrain and airspace cross-section along bearing or route, with glide lines | v1 |
| XC | Map with live XContest optimisation, FAI sectors, closing circle, leg distances; flown distance; unfinished-triangle estimate; FAI triangle potential | v1 |
| Competition | Task map, distance, altitude and glide to goal and to end of speed section, time to start, predicted time at start, speed to start, percent of speed section, task summary | Later |
| Airspace | Proximity widget with nearest airspace, distance and direction | v1 |
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
| Modes: back to takeoff, triangle closing, go-to waypoint, competition task | v1 except competition |
| Waypoint files: CompeGPS, UTM, GpsDump GEO, OziExplorer, SeeYou `.cup`, XCSoar/WinPilot `.dat` *(official)* | v1: `.cup` and GEO `.wpt`; others Later |
| Live XContest scoring: free distance, flat triangle, FAI triangle, with configurable coefficients and closing distance | v1 |
| Glide range marks on map and side view, from actual and configured glide ratio with wind | v1 |
| Competition tasks: cylinders, start of speed section with race or elapsed-time start and time gates, end of speed section, goal cylinder or line, elevated goal, optimised route | Later |
| Task exchange in the open `.xctsk` format: JSON file, compact QR form (`XCTSK:`), compressed form (`XCTSKZ:`) *(official)* | Later; we implement the published format so tasks are interchangeable with XCTrack, Flyskyhy and Naviter devices |
| Navigate toward a chosen live pilot (1.0) | Later |

## Maps and terrain

| XCTrack | Ours |
|---|---|
| Offline vector road maps in Mapsforge format with flying-optimised themes; one map widget per page *(official)* | v1, with our own alpine map style and no one-widget limit |
| Downloadable terrain used for height above ground, relief shading and the side view | v1, from 1 m elevation data in the first region |
| Map rotation: north, heading, bearing, navigation target or wind at top | v1 |
| Pan mode with terrain height under the cross-hair and airspace inspection | v1 |

## Airspace and obstacles

| XCTrack | Ours |
|---|---|
| Airspace by country from airspace.xcontest.org with temporary activations, checked at start, plus custom OpenAir files *(official)* | v1 for static airspace and OpenAir import. Activations depend on a data source; see [data-sources.md](data-sources.md#airspace) |
| Three warning levels with sounds, on-screen labels and suppression times | v1 |
| High airspace hidden until close | v1 |
| Per-class colours and per-airspace floor and ceiling adjustment (1.0, paid) | Later |
| Obstacle and cable warnings with safety margin and an emergency map zoom | v1 where open cable data exists (Tirol), Later elsewhere |

## Livetracking

| XCTrack | Ours |
|---|---|
| Live position to XContest, nearby pilots and their tracks on the map, final glide of others | v1 on our own service |
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
| Thermal hotspot map from thermal.kk7.ch, including a time-adjusted variant | v1 if permission is granted |
| Rain radar | v1 |
| SkySight forecast layers (finished, awaiting an agreement, paid) | Our own forecast and wind-station layers, v1 |

## Where we go beyond

1. An iPhone app at this level, which does not exist today.
2. Vario from barometer and accelerometer fusion for pilots without an external vario.
3. Groups, Open Glider Network traffic and a buddy view.
4. Live wind stations and forecast wind in the cockpit.
5. Reachable landing fields with public transport home.
6. A glider polar and speed-to-fly.
7. A voice assistant on the pilot's own AI account.
8. A Live Activity on the lock screen and, later, a watch companion.

## Still to verify

- The complete current widget list and names, from the AIR³ manual pages.
- The meaning of XCTrack's two wind algorithms ("classic" and "particle drift").
- The obstacle data's country coverage.
- Flyskyhy's and SeeYou Navigator's supported Bluetooth varios, from their own sites, since those are the apps iPhone pilots use today.
