# Product plan

Last revised: 2026-11-19

## Goal

Paragliding pilots use one app to check conditions, another to plan, another as flight computer, another to publish flights, and chat groups to organise flying days. None of them share data. This project puts those jobs into one free app with one community, so that a flight is planned, flown, analysed and shared in the same place.

## Owner decisions

These were set by the owner on 2026-11-18 and are not open for re-litigation in later sessions.

1. **Free and open source.** No monetisation planned. Sponsorship may come later.
2. **The project does not pay for AI.** Users connect their own assistant or credentials.
3. **Native UI on every platform.** SwiftUI on iOS, Jetpack Compose on Android, a real web app in the browser. No React Native, no Flutter, no web views inside the mobile apps.
4. **One Rust codebase** below the UI, shared by all clients and the server.
5. **Self-hosted on one Linux server** the owner pays for. No services that bill by usage.
6. **Every external data source sits behind an adapter.** Regions are configuration. See [data-sources.md](data-sources.md).
7. **First region: Germany and Austria**, then the whole Alps. The owner is based in Munich.
8. **iOS first.** The owner flies with an iPhone and an external Bluetooth vario.
9. **XCTrack is the benchmark for the cockpit.** See [xctrack-parity.md](xctrack-parity.md).
10. **Best available open maps, with very good terrain depiction.**
11. **AI sessions write and maintain all code.** The repository must stay understandable to a session that has never seen it.
12. **The app is called Venti** and is licensed under AGPL-3.0-or-later.
13. **It must be able to carry many users.** The server design starts on one machine and must be able to grow beyond it.

## Modes

The app has five modes on web and phone. The cockpit is phone-only.

| Mode | Job |
|---|---|
| **Today** | What is flyable right now: live wind, webcams, who is in the air |
| **Plan** | Forecast and route planning for cross-country and hike & fly |
| **Fly** | Live cockpit and flight recorder |
| **Review** | Logbook, 3D replay, insights, comparison with other pilots |
| **Community** | Feed, groups, events, spontaneous flying days |

All modes read and write the same objects through one API: sites, wind stations, airspace, flights, pilots, live positions. The assistant uses that same API as its tools.

## Features

This list is a draft. The owner has said more feature input will follow.

### Today and Plan
- Live wind stations from every usable network on one map, with history graphs.
- Forecast layers with a time slider: wind at several altitudes, thermal strength, cloud base, cloud cover, rain, live radar.
- Takeoffs and landings with allowed wind directions, a per-site flyability timeline, webcams, rules and recent flights.
- Airspace with altitude and time filters; cable and obstacle hazards.
- Thermal hotspots and common cross-country lines.
- Lifts, public transport stops and hiking trails.
- Later: cross-country route planner, hike & fly planner (trail up, glide range down, transit home), alerts such as "tell me when Wallberg is flyable".

### Fly
The full checklist is in [xctrack-parity.md](xctrack-parity.md). XCTrack is Android-only and its developers state that they do not plan an iOS version, so an iPhone cockpit at that level does not exist today. The principles:
- The recorder runs as a native background service. A UI crash or phone call never costs the track.
- Everything works offline: maps, terrain, airspace, sites and the last forecast are on the device.
- External Bluetooth varios are first-class, because phone barometers are too slow for a good vario.
- Safety-relevant warnings (airspace, terrain, landing options) are deterministic code. They never depend on a network connection or a language model.

### Review
- IGC upload and bulk import of existing flight history, with automatic scoring.
- 3D replay, with photos and videos pinned to the moment and place they were taken.
- Automatic breakdown of every thermal and glide, time spent climbing, gliding and searching, wind by altitude.
- Comparison with pilots in the same area that day: climb rate in the same thermals, where lines diverged and what that cost, lift flown past.
- Trends by season, wing and site.

### Community
- Profiles, follows, a feed of flights with media, comments, share pages with rich previews.
- Groups with chat and a "flying today?" board, visible to the group or to pilots nearby.
- Events with local discovery; site pages anyone can improve.
- Privacy controls from the first release: private, followers-only or public flights, a hidden home zone, delayed or friends-only live tracking.
- Later: leagues, classic-route leaderboards, club pages.

### Assistant
Users bring their own AI; details are in [architecture.md](architecture.md#bring-your-own-ai).
- **Connector:** the server exposes its tools over MCP, so a pilot adds the app to their own Claude or ChatGPT and asks there ("how is the wind toward Innsbruck this afternoon?").
- **In-app assistant:** runs on the pilot's own API key or account, speaks and listens in flight, and can raise proactive alerts.
- **Flagship question:** "which valley gets me home tonight?" Reachable landings from the current height and wind, then nearest stops, then train and bus itineraries, ranked by arrival time and glide margin.

## Roadmap

Phases are ordered so that each one is useful to a single pilot before the community exists. Code is written by AI sessions, so calendar time is driven by what cannot be automated: flight testing on flyable days, App Store review, and permission emails to data owners.

| Phase | Ships | Done when |
|---|---|---|
| **0 Foundations** | Repository, CI, Rust workspace, the adapter framework with two real adapters, and the spikes listed below | Every spike has a yes or no |
| **1 Map** | Today and Plan for Germany/Austria on iOS and web; MCP connector with read-only weather and site tools. Started on the server side: it collects the live wind stations of the first region and serves them over an HTTP API ([api.md](api.md)). Since 2026-11-19 the iPhone app draws them on its map, with a station's last 24 hours, against a server whose address the pilot sets, and the web app does the same against the server it is served from. Since 2026-11-19 the server also has the connector, with read-only tools for wind stations and airspace ([mcp.md](mcp.md)); it was tested with client programs and not yet in Claude or ChatGPT. Since 2026-11-19 the server also collects the take-offs and landings that OpenStreetMap has for the first region, once a day, and serves them ([api.md](api.md#get-v1sitesbboxwestsoutheastnorth)); no app draws them yet, and OpenStreetMap has only part of the sites, 252 take-offs and 142 landings in the first region ([data-sources.md](data-sources.md#flying-sites)). Since the same day the server also collects OpenStreetMap's flying sites and serves them, and the iPhone app draws them on its map, with a page for each and a way to keep one as a waypoint; the web app draws them too, with the list and the page, and no waypoints; the Android app does what the iPhone app does, tested on an emulator against the server's development mode. Since the same day the iPhone app has a forecast screen for a site or a wind station: the wind through the day at each height a model gives, cloud base, cloud, rain and temperature, model by model ([design.md](design.md#map)). The web app has the same screen, reached from a site's and a station's page. It is tested against canned answers and against what the server's development mode answers, which are recorded forecasts; it has not been seen with a live forecast. The Android app has the same screen, tested the same way. Since 2026-11-19 the server answers point forecasts of ICON-D2 and ICON-EU for a place, hour by hour, from an Open-Meteo instance the owner runs beside it ([api.md](api.md#get-v1forecastlatlon)); without such an instance it has none and says so. The source has no boundary-layer height and nothing that is thermal strength, and the day after tomorrow is covered only part of the time ([data-sources.md](data-sources.md#forecasts)). The server runs on no machine yet, so outside of tests there is still nothing to see. | The owner opens it before every flight |
| **2 Review** | Logbook, import, analysis, 3D replay, share pages. Started early, on the device only: the iOS app imports flight files into a logbook, shows a flight on the map with its altitude profile, and lists its thermals and glides, scores it by the World XContest's rules, and keeps statistics by month and glider with the pilot's bests. Since 2026-11-19 a flight is also checked against the airspace the app has, and its details list what it entered, with what that check cannot know. 3D replay and comparison are not built. The web app has the same logbook, statistics and flight screen in the browser, the airspace check included, with the flights kept in the browser; it is built and tested but published nowhere. On the server since 2026-11-19: a pilot's flights are kept with their files and reach the pilot's other devices, and a flight can be shared with a link that shows a page and a preview picture ([api.md](api.md#flights)). The iPhone app signs in, keeps its logbook in step with the server and shares a flight with a link, tested against the server's development mode; so do the web app and the Android app. | The owner's full flight history is in it |
| **3 Fly** | Recorder and cockpit on iOS with offline maps, Bluetooth vario and live tracking. Started: the cockpit, recorder, vario sound and Bluetooth vario link are built and tested by replaying a flight; so is going to a waypoint or back to the take-off, with the pilot's waypoints read from SeeYou and GEO files. Since 2026-11-19 the iPhone's cockpit also flies a task: a competition task or a route in the XCTSK format that XCTrack and other instruments exchange, brought in from the clipboard or a file, with the countdown to the start, the way from turnpoint to turnpoint and the distance to goal ([xctrack-parity.md](xctrack-parity.md#navigation-and-competition)). A route can also be made in the app, from the pilot's own waypoints tapped in order; a competition task with cylinders and a start can only be brought in. None of it has flown, and offline maps are not built. Of live tracking the server's side exists since 2026-11-19, and nothing else: a pilot's app can send the positions of a flight, for those who have a link or for everyone; the server hands them out over its API, lists the public ones for a part of the map, which is what "who is in the air" in the Today mode needs, shows a page to whoever opens a link, and removes a track 48 hours after its flight ([api.md](api.md#live-tracks)). Since the same day the iPhone app sends them: a "Live tracking" page on the Fly tab says who may watch (nobody, people with the link, everyone), the cockpit sends the positions from take-off on and shows whether they go out, and the link can be handed on from there. It has run in tests against the server's own example answers and not against a server, because none runs yet, and it has not flown. The iPhone's map tab also shows who is in the air: the gliders of pilots who show their flight to everyone, with a page for each, and while a page is open that pilot's flight so far as a line on the map. Android and the web neither send nor show gliders. Airspace exists as an engine with data: the OpenAir reader, the warnings and openAIP's airspace of Germany and Austria are built and tested ([airspace.md](airspace.md)); since 2026-11-19 the iPhone app reads OpenAir files the pilot brings in, draws them on its maps and warns in the cockpit, by sight and by voice. The server collects openAIP's airspace once a day and serves it ([api.md](api.md#get-v1airspacebboxwestsoutheastnorth)); since 2026-11-19 the iPhone app fetches the airspace around the pilot from there by itself, keeps it on the device and warns from it without a network, beside the pilot's own files. The web app does the same for its maps, and reads the pilot's files too; it has no cockpit and does not warn. The Android app does what the iPhone app does, tested on an emulator against the server's development mode. The ground elevation that limits above ground need, and activations, are not built, and none of it has flown. | Ten cross-country flights as primary instrument without a lost track |
| **4 Community** | Profiles, feed, groups, flying-today board, events; closed beta. Started on the server: accounts with a handle and a password, by invite, and the data of a pilot's public page. No follows, no feed, nothing to report or remove a shared flight with. The three apps sign in with a handle and a password. | The owner's group plans a flying day in it |
| **5 Assistant in flight** | In-app assistant, voice, proactive alerts, landing-plus-transit planner. Started: the in-app assistant answers questions about the pilot's logbook on the pilot's own Anthropic key, and, when a Venti server is set, about live wind, the airspace over a place, what the weather models forecast for a place and the flying sites near it. The wind and airspace tools also answer a pilot's own Claude or ChatGPT through the server's connector ([mcp.md](mcp.md)); the forecast is not offered there, because a connector's tool call must not make the server fetch, and the sites not yet. The core can also run the assistant at any other service that speaks the chat completions format, such as OpenAI, OpenRouter or a model the pilot runs at home; the iPhone app offers that choice in the assistant's options, the others not yet. All of it has been tested against scripted providers only, never against a real service. | Used for a real retrieve |
| **6 Breadth** | Android app, route and hike & fly planners, leagues, competition tasks, whole Alps. Started early with the Android app: it has the iPhone app's logbook, flight screen, statistics, map with live wind and flying sites, cockpit with waypoints, navigation and airspace warnings, airspace from the pilot's files and from the pilot's server, assistant and settings over the same core, built and tested on an emulator in CI. Since 2026-11-19 it also draws the server's flying sites and forecasts, signs in to a server, keeps the logbook in step with it and shares a flight with a link, as the iPhone app does, and its cockpit flies a task or a route, brought in as text or as a file or made from waypoints; it neither shows nor scans a task's QR code. It has run on no phone, and nothing about its sensors, its recording with the screen off or its Bluetooth link is verified. | |

Android was planned to follow iOS one phase behind once the shell patterns were settled; its first version was built on 2026-11-19, beside the iPhone app. Whether it now keeps step with the iPhone app or follows it is the owner's to decide. The cockpit must be testable on the ground by the time thermals return in March 2027.

### Phase 0 spikes

| Spike | Question it answers |
|---|---|
| Core bridge | Does a Rust core drive a SwiftUI shell cleanly through UniFFI under Swift 6 strict concurrency? **Yes, 2026-11-18.** Built with Xcode 26.6 and Swift 6.3 in Swift 6 language mode with no concurrency errors; the app ran on an iPhone 17 Pro simulator. See [architecture.md](architecture.md#the-bridge). Not yet tried: async calls, callbacks from Rust into Swift, a real device. |
| 2D map | Does MapLibre Native on iOS render our self-hosted vector, terrain and contour tiles from local files at 60 fps with a few thousand live symbols? **Partly answered, 2026-11-18.** Yes for rendering: vector base map, hillshading from terrain tiles and labels from bundled fonts all draw from files inside the app, on a simulator. Still open: contour tiles, and frame rate with thousands of live symbols, which needs a real phone. |
| Background recording | Does a six-hour recording survive backgrounding, a phone call and low-power mode on iOS? |
| Bluetooth vario | Can we connect to the owner's vario, parse its stream and drive vario audio in the background? |
| 3D terrain | Is a small Rust terrain renderer on wgpu good enough on iPhone and in the browser? |
| Adapters | Do DWD and GeoSphere Austria observations flow end to end through the framework, with fixtures and health reporting? **Yes, 2026-11-18**, up to validated canonical records; see [data-sources.md](data-sources.md#what-is-built). Since 2026-11-19 the server stores them and serves them, with a health report per source. |
| Transit | How much memory and import time does the transit router need for Germany, Austria, Switzerland and northern Italy on the owner's server? |

## Needed from the owner

| Item | State |
|---|---|
| Name | **Venti**, decided 2026-11-18. Domain `venti.fly` was unregistered that day; the owner intends to buy it. |
| Licence | **AGPL-3.0-or-later**, decided 2026-11-18. |
| Vario | MipFly MipBip, with XC Tracer as the other first-class target. All popular Bluetooth varios should follow. |
| GitHub repository | **Done**, 2026-11-18: `dariobanfi/venti`. One stale branch, `phase-0/foundations`, is left for the owner to delete; AI sessions cannot delete branches. |
| Server | **Open.** None yet. The recommended specification is in [architecture.md](architecture.md#sizing). The software is ready to be installed: the runbook is [deploy/README.md](../deploy/README.md). With the server come three small decisions: the domain name it answers on (links to shared flights are built from it), where the off-site copy of the backups goes, and whether CI may publish container images to GitHub's registry. |
| Contact line for data providers | **Open.** The server names itself to the providers as `venti-server/<version>` and nothing else. Some providers ask operators to name a contact in that line; whether to, and which, is the owner's decision (`VENTI_USER_AGENT`). |
| Apple Developer Program membership | **Not for now**, owner decision 2026-11-18. The owner tests on their own Mac and iPhone with their own signing settings, which stay out of the repository (`apps/ios/Config/Local.xcconfig`). Builds and tests run on GitHub's macOS runners, which are free for public repositories. Membership is needed later for TestFlight and the App Store. |
| Permission emails | **Open.** AI sessions draft them, the owner sends them. The list is in [data-sources.md](data-sources.md#outreach). |
| Accounts | **Open, three things.** (1) Sign-in is a handle and a password, and a forgotten password cannot be recovered; that stays so until the domain and the Apple membership allow passkeys, or the owner allows a mail relay. (2) While registration is by invite, the owner makes the codes (`venti-server invite`) and hands them out. (3) Before anyone the owner does not know can share a flight, the server needs a way to report and remove one; it has none. |
| Live tracking | **Open, four things**, all built one way for now ([architecture.md](architecture.md#live-tracks)). (1) A live track is kept for 48 hours after its last position and then removed, also from the backups; another number is one setting (`retention.live_hours`). (2) Every account may put its glider on everyone's map. While accounts are by invite that is the people the owner invited; with open registration one account can show 30 made-up gliders a day, and nothing exists to report a track or to take one down. (3) Positions are shown as they arrive. The plan promises "delayed or friends-only live tracking"; neither is built, and a delay would have to be weighed against what a live link is mostly for, which is someone at home knowing where the pilot is now. (4) A pilot's link shows the pilot's display name and handle to whoever has it, as a shared flight does. |
| Feature input | **Open.** The owner has said more will follow on the draft list above. |

## Risks

1. **Scope.** This is three mature products at once. The phase order is the mitigation.
2. **Empty community.** XContest has the network. The app must be the best logbook and cockpit with no friends on it, import history in bulk, and cross-post to XContest if XContest agrees.
3. **Lost tracks.** iOS background limits and Android battery managers kill trackers. One lost long flight loses a pilot for good.
4. **Data permissions.** Several sources that matter most at launch sites have no open licence: Holfuy wind stations, the DHV site database, airspace activation times, webcams.
5. **Liability.** Airspace data and assistant answers can be wrong. Everything is advisory, shows the age of its data and cites its source.
6. **Store review.** Background location and background audio need justifying; user content needs report, block and moderation.
