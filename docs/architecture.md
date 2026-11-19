# Architecture

Last revised: 2026-11-19. Facts marked *(to verify)* come from research that could not reach the primary source and must be checked before they are relied on.

## Shape of the system

```mermaid
flowchart TB
  subgraph Clients
    iOS["iOS app (SwiftUI)"]
    Android["Android app (Compose)"]
    Web["Web app (SvelteKit, static build)"]
  end
  Core["Rust core: state, offline store, flight engine, sensors, assistant"]
  iOS --> Core
  Android --> Core
  Web --> Core
  Core -->|HTTPS, WebSocket| Server
  subgraph Box["One Linux server"]
    Caddy["Caddy: TLS, static files, map tiles"]
    Server["Rust server: API, live, MCP, auth, ingestion, jobs"]
    PG[("PostgreSQL + PostGIS")]
    Disk[("Disk: tiles, flights, media, raw payloads")]
    Motis["MOTIS: transit routing"]
    OM["Open-Meteo: forecast API"]
    Server --> PG
    Server --> Disk
    Server --> Motis
    Server --> OM
  end
  Server -->|adapters| Providers["External data providers"]
  Assistants["User's own Claude / ChatGPT"] -->|MCP| Server
  Core -->|user's own key| LLM["LLM provider"]
```

Three ideas carry the design:

1. **Rust owns everything except pixels and operating-system services.** The same flight engine runs in the phone's recorder, in the browser and on the server.
2. **Nothing outside an adapter knows a provider exists.** External data is normalised into one canonical model before anything else touches it.
3. **One server process, one database, one disk.** Extra services are added only when they are separate upstream projects (transit routing, forecast API).

## Rust workspace

Packages are named `venti-<directory>`, for example `venti-domain`. Built so far: `domain`, `sources`, `sources-dwd`, `sources-geosphere`, `regions`, `api-types`, `store`, `server`, `flight` (flight files, track summary, track views for drawing, and the live flight computer), `sensors` (sentence parsers for external varios), `core` (the logbook on the device and the cockpit session), `core-wasm` (the core for the browser) and the iOS shell in `apps/ios`. The rest are planned. `tools/uniffi-bindgen-swift` generates the Swift interface of `core`.

```
crates/
  domain         Canonical types, units, identifiers. No I/O.
  flight         IGC read/write, vario filter, wind estimate, thermal detection,
                 glide and airspace geometry, scoring, task logic. No I/O.
  sensors        Parsers for external instrument protocols. No I/O.
  sources        Adapter traits, manifest types, registry, pipeline.
  sources-*      One crate per provider (sources-dwd, sources-geosphere, ...).
  regions        Loads and validates region packs.
  store          Server persistence behind one interface: PostgreSQL with PostGIS,
                 and an in-memory twin for tests and the development mode.
  client-store   Client offline store (SQLite) and sync.
  api-types      The wire format of the HTTP API, shared by server and clients. No I/O.
  tools          Assistant tool definitions: one definition, served in-app and over MCP.
  llm            Provider clients for the in-app assistant.
  core           Client application core: state, events, view models. UniFFI exports.
                 Today it keeps the logbook: flight files in the app's storage and an
                 index beside them. This moves into `client-store` when sync arrives.
  core-wasm      The core as a WebAssembly module with TypeScript declarations
                 (wasm-bindgen). The web app's only way into Rust.
  terrain3d      Terrain and flight-replay renderer on wgpu.
  server         The server binary: ingestion scheduler, HTTP API, development mode,
                 and the one-shot live run that CI uses as its canary.
apps/
  ios/           SwiftUI shell
  android/       Compose shell
  web/           SvelteKit app
regions/         Region packs (configuration, no code)
deploy/          Compose files, Caddyfile, backup scripts
docs/
```

### Dependency rules

- Dependencies point inward. `domain` depends on nothing in the workspace. `flight`, `sensors` and `sources` depend on `domain`. Everything else may depend on those.
- A `sources-*` crate depends only on `domain` and `sources`. No other crate may name a provider-specific type; only `server`, which assembles the registry, depends on the provider crates.
- `api-types` and `store` depend only on `domain`. The wire format must stay usable in a browser and on a phone, and the store must not know how data was fetched.
- Nothing a client is built from reaches a server-side crate: `core`, and every crate that depends on it, never depends on `store`, `server`, `sources`, `regions` or a provider crate, not even through another crate.
- `flight` and `sensors` do no I/O and read no clock. Time, positions and bytes are passed in, which makes every algorithm replayable from recorded data.
- UI shells contain no business rules. If a shell needs an `if` about flying, the logic belongs in `core`.

The test `crates/sources/tests/dependency_rules.rs` enforces the first two rules on every CI run by reading the crate dependency graph. It also fails if anything other than `server` depends on a provider crate, if `api-types` or `store` grows a workspace dependency besides `domain`, or if a client crate can reach a server-side one, and it holds `core-wasm` to depending on `core` alone. Add the rules for `flight` and `sensors` to it when those crates are created.

## Clients

### The bridge

| Target | Mechanism |
|---|---|
| Swift, Kotlin | UniFFI (0.32 at the time of writing) |
| Web | wasm-bindgen (0.2.130 at the time of writing), in the crate `core-wasm` |

The core follows the one-way pattern used by Proton Mail, Bitwarden and the Crux framework: the shell sends events in, the core returns a view model, and the shell performs the side effects the core asks for (network, storage, sensors). We adopt the pattern, not the Crux framework, to keep the dependency surface small.

Bridge rules:

- The interface is coarse. A screen gets one view-model struct, not twenty getters.
- Per-frame and audio paths never cross the bridge. The vario tone is synthesised natively from a value the core updates a few times a second. Instruments are drawn natively from a snapshot struct.
- One error type crosses the bridge.

What the Phase 0 spike showed (2026-11-18, UniFFI 0.32, Xcode 26.6, Swift 6.3):

- The generated Swift interface compiles in Swift 6 language mode without changes. Records are `Sendable`, the core object can be shared across tasks, and Rust errors arrive as a Swift error with a readable message.
- The core is packaged as one XCFramework of static libraries (device, simulator, Mac) inside a local Swift package. The Mac slice lets the bridge tests run with `swift test` in seconds, without a simulator.
- The module map must be generated with the module name the generated Swift imports (`venti_coreFFI`); the generator's default name does not match.
- Not yet exercised: async functions, callbacks from Rust into Swift, and a physical device. UniFFI's own notes say async code does not yet conform to `Sendable`.

There is no first-party generator that covers WASM, so the web has its own binding crate, `core-wasm`. It is thin. The view types are the core's own: built with its `wasm` feature, the core derives their TypeScript declarations from the Rust definitions (with `tsify`), doc comments included. Only the list of calls exists twice, once for UniFFI and once for wasm-bindgen. A failed call throws a JavaScript `Error` whose message is the core's and whose name is the kind of error. BoltFFI would unify the two bindings but is young.

### Storage

The core keeps whole files under short names (`flights/<id>.igc`, `logbook-index.json`) and reaches them through one small interface, `Storage`: list a folder, read a file, write one, remove one. What makes two files the same flight, the index, summaries and totals are built on top of it and are the same code on every platform. There are two implementations:

- `DiskStorage` is a folder the app owns. The phones use it. The files are where they were before the interface existed.
- `StagedStorage` is for a host that can only store things asynchronously, which is every browser, while the core's interface is synchronous. It knows the names of the stored files and holds none of their contents. A read of a file that is not in memory fails and notes the file as wanted; the host fetches it, hands it over and makes the same call again. Writes and removals are staged, and the host applies them to its own store after the call, all together. What the host handed over is dropped once the call has succeeded.

This is the one-way pattern above applied to storage: the core decides what is stored, the shell does the reading and writing. It also keeps a long logbook out of memory. Opening needs the index only, and a flight's file is fetched when that flight is opened. A lost index is rebuilt a few files at a time.

One suite of tests runs against both implementations (`crates/core/src/storage.rs` and `crates/core/src/logbook.rs`). How the web app stores the files is under [Web](#web). The cockpit's recorder is not behind this interface: it appends to a file in the data folder and exists only on the phones.

### iOS (first target)

- SwiftUI, minimum iOS 26, iPhone in portrait for now. The Xcode project is generated from `apps/ios/project.yml` by XcodeGen and is not committed, so every project change is a readable diff. How the app looks and why is in [design.md](design.md).
- The shell holds no flying logic. One `Core` object is created at launch with the app's data folder; screens ask it for a view (`LogbookView`, `FlightView`) and draw it. Storing flights, recognising a flight that is already there, totals, grouping by year, climb rates and the reduced altitude profile are all the core's work.
- UI tests drive the real app on a simulator and keep a screenshot of every screen; the iOS workflow publishes them. This is how an AI session sees its own interface changes.
- The cockpit's sensors live in `apps/ios/Venti/Fly`: positions from Core Location (best accuracy, airborne activity type, background updates with a `CLBackgroundActivitySession`), pressure from `CMAltimeter`, an external vario over Core Bluetooth, and the vario tone synthesised sample by sample in an `AVAudioSourceNode`. The app declares the location, audio and Bluetooth-central background modes. To verify on a real iPhone, none of it can be exercised on a simulator: that recording survives hours in the background, that the vario connects and reconnects, and that the tone sounds right.
- The vario is chosen in the app, from a list of every Bluetooth LE device nearby. On a device with one of the known serial services the app listens to that service; on any other it listens to everything the device notifies outside the standard Bluetooth services, and the core keeps whatever is a sentence it knows. That is how a vario whose service nobody has documented (the owner's MipBip) can work without a driver. The vario list shows what the connected device offers and the last lines it sent, and can share that as a report, so an unknown device can be worked out from one test.
- The pilot's name, glider and sound setting are kept in the phone's own settings store, not in the core: they are preferences of this device until accounts exist. The names are passed to the core when a recording starts.
- Map: MapLibre Native (7.0), wrapping `MLNMapView` directly. The SwiftUI wrapper is pre-1.0 and incomplete.
- Recorder: Core Location with a `CLBackgroundActivitySession` and `CLServiceSession` started in the foreground. Apple's guidance and a 2026 developer-forum thread both point to missing sessions as the cause of suspension about a minute after backgrounding.
- Barometer: `CMAltimeter`. The update rate is undocumented and reported at about 1 Hz, which is too slow for a responsive vario on its own.
- External vario over Core Bluetooth with the `bluetooth-central` background mode and state restoration. This is the primary vario source for the owner's setup.
- Vario audio through the background audio mode. It is real audible content, which is what App Review requires.
- Live Activity for a running flight. The system ends a Live Activity after eight hours, so long flights need a restart strategy.

### Android

- Kotlin and Jetpack Compose, MapLibre Native Android (13.x, Vulkan by default).
- Recorder as a foreground service of type `location`, started while the app is visible. Android 17 adds the same visibility rule for background audio.

### Web

- SvelteKit built to static files and served by Caddy. No Node process on the server.
- MapLibre GL JS 6 for maps. The Rust core runs as WebAssembly for flight analysis and the offline store.
- Pages that must unfurl in chats and be indexed (flights, sites, profiles, events) get their HTML head and a text summary rendered by the Rust server; the web app then takes over. Revisit with a server-rendered SvelteKit build if search indexing proves too thin.

## The cockpit

How a sensor reading becomes a number on the screen:

```
phone GPS, phone barometer, Bluetooth vario      (shell: owns the sensors)
        │  each sample with the time it was taken
        ▼
core::FlightSession                              (core: one object per flying session)
   ├─ sensors::SentenceReader + parse            bytes from the vario → readings
   ├─ flight::live::FlightComputer               pure; no clock, no I/O
   └─ Recorder                                   appends each fix to a file at once
        │  CockpitView, pulled about five times a second
        ▼
screen and vario tone                            (shell: draws and synthesises)
```

- **No clock in the computer.** Every sample carries its own time and the shell passes "now" when it asks for a view. A recorded flight replayed through the same code therefore shows exactly what the pilot saw; `core::Replay` does that, and it is how the cockpit is tried on the ground and tested.
- **Climb rate** comes from the best source that is still talking: the external instrument's own vario, else a Kalman filter over pressure altitude (the instrument's pressure if it sends any, else the phone's), else the same filter over satellite altitude. A source silent for three seconds is dropped. When the pressure sensor changes, the filter starts again, because two sensors disagree by a constant.
- **Altitude** is pressure altitude plus a slowly followed correction toward satellite altitude: as smooth as the barometer, as true as the satellites.
- **Wind** is fitted while circling: ground velocities around a circle lie on a circle whose centre is the wind and whose radius is the airspeed. It needs no compass and no airspeed sensor, and it does not assume the pilot flew a perfect circle over the ground.
- **Circling** uses the rule XCTrack documents (90° of turn within 30 s while not sinking faster than 0.5 m/s). It ends when the turn within 30 s drops under 30°.
- **Recording** cannot lose a track to a crash: each fix is appended to `recordings/<start>.igc` in the data folder as it arrives, and that file is a complete flight file at every moment. Stopping imports it into the logbook and removes it; one found at launch is imported then.
- **The vario tone** is described by the core (pitch, rhythm) and synthesised by the shell.

To verify in the air: the take-off and landing thresholds, the tone table and the filter's settings are first values. None has flown yet.

## Maps and terrain

All map data is self-hosted open data, stored as PMTiles files and served as static files with HTTP range requests. The apps download the same files for offline use; MapLibre Native reads them from local storage.

Until the server exists, the iOS app fetches its tiles from two public services of the same open data, so that the map covers wherever a pilot has flown and not only the demo region:

| Layer | Service | Terms, read on the provider's page 2026-11-18 |
|---|---|---|
| Vector base map | OpenFreeMap, `tiles.openfreemap.org` (unmodified OpenMapTiles schema, the one our style is written for) | Free public instance, "no limits on the number of map views or requests", commercial use allowed, attribution required. [openfreemap.org](https://openfreemap.org/) |
| Terrain | Mapterhorn, `tiles.mapterhorn.com` | A public tile endpoint is documented on [mapterhorn.com/data-access](https://mapterhorn.com/data-access); no usage limit is stated there. To verify with the project before any public release. Attribution required. |

The app asks Mapterhorn for terrain only up to zoom 12, the level it serves for the whole world; finer tiles exist only for some countries and return "not found" elsewhere. Both services see the IP address and the map areas of whoever uses the app. This is a development arrangement, not the product: it ends when our own tile server is up, and it must end before a public release unless both projects agree.

| Layer | Source | Notes |
|---|---|---|
| Elevation | Mapterhorn | Includes Bavarian DGM1 (1 m), Austrian BEV (1 m), swissALTI3D and South Tyrol. Used for hillshade, colour relief, 3D and height-above-ground. |
| Contours | Pre-generated vector tiles from the same elevation data | MapLibre Native has no on-device contour generator, so all platforms use the same pre-built tiles. |
| Vector base map | OpenStreetMap through a custom Planetiler profile | Starts from the OpenMapTiles profile and adds what alpine flying needs: path difficulty (`sac_scale`), material ropeways, power lines, free-flying sites. |
| Style | Our own MapLibre style | One style shared by iOS, Android and web. A high-contrast variant for sunlight in the cockpit. |
| Official maps | basemap.at, Bavarian DTK25 | Switchable raster layers, both CC BY 4.0. |
| Imagery | basemap.at orthophoto, Bavarian DOP20, Sentinel-2 cloudless 2016 elsewhere | For the 3D viewer and an aerial layer. |

### What exists so far

- `map/style.json` is the shared style. It names its tiles and fonts with placeholders that each app replaces with local or remote URLs.
- A small demo region (the Tegernsee valley, about 7 MB) is bundled with the iOS app under `apps/ios/Venti/Resources/MapDemo`: a vector base map, terrain tiles and label fonts. `tools/demo-map/build.sh` rebuilds it. It uses the stock OpenMapTiles profile and terrain only to zoom 12; the custom profile, contours and high-resolution terrain are not built yet.
- The iOS app prepares the style for one of two tile sources: the public services above (the default), or that bundled region. The UI tests use the bundled region, so the offline path is drawn and checked on every push. Label fonts always come from inside the app.
- Confirmed on the simulator (2026-11-18): MapLibre Native reads vector and terrain archives through `pmtiles://file://` URLs, decodes the WebP elevation tiles, and loads label fonts from a `file://` glyph URL. No network is involved.

### 3D

MapLibre Native has no production 3D terrain. The work exists as a draft pull request with no timeline. MapLibre GL JS on the web does have it.

The 3D viewer is therefore our own renderer: the `terrain3d` crate on wgpu, reading the same elevation and imagery tiles. It draws a quadtree terrain mesh with draped imagery, track ribbons and pilot markers, with a free camera. It embeds in a Metal layer on iOS, a surface view on Android and a canvas on the web (WebGPU, with a WebGL2 fallback because Firefox on Linux and Android still lacks WebGPU by default).

This is the largest single piece of custom engineering in the project. It is confined to replay and route preview; the cockpit and planning maps are 2D with hillshade and contours and do not depend on it. Revisit when MapLibre Native ships terrain pre-releases.

### Live overlays

MapLibre Native slows down with around a thousand symbols that use sort keys and halos. Overlay rules: few symbol layers, no sort keys, feature-state for updates, and a custom layer for objects that move every second (live pilots).

## Server

Everything runs on one Linux server under Docker Compose.

| Process | Role |
|---|---|
| Caddy | TLS, static web app, PMTiles, media files |
| `server` (Rust, Axum) | REST API, live WebSocket, MCP endpoint, authentication, ingestion scheduler, background jobs. One binary; roles can be split later with flags. |
| PostgreSQL + PostGIS | The only database |
| MOTIS | Public-transport routing, self-hosted with open timetable feeds |
| Open-Meteo | Forecast API, self-hosted, syncing open model data |

Deliberate omissions: no Redis (live fan-out is in-process, the job queue is a Postgres table), no object store (blobs live on disk behind a small trait, so an S3-compatible store can replace it), no search engine (Postgres full-text and trigram), no Node runtime, no managed services.

### What the server does today

The first version of the server collects station observations and serves them. Accounts, flights, the live channel, the MCP endpoint, MOTIS and Open-Meteo are not built.

`venti-server` is one program with three subcommands:

| Subcommand | What it does |
|---|---|
| `serve` | Runs the server: collects the sources of the configured regions on their schedule, stores them in PostgreSQL, serves the API, removes old data |
| `dev` | Serves the same API over payloads recorded from the providers, from an in-memory store. No network, no database. Its clock stands still at the moment of the recording, so every start gives the same answers. For client work and UI tests |
| `ingest <region.toml>` | Runs a region's sources once against the live providers and prints a health line for each. Stores nothing. CI runs it daily as the canary |

**Collecting.** Each source has one task that runs it, waits and runs it again; that loop is what guarantees a source never runs twice at once and that a slow source delays no other. A run goes through the pipeline of the `sources` crate (fetch, archive, parse, merge, validate); the validated records and the outcome of the run are then written to the store in one transaction. The wait is measured from the start of one run to the start of the next and is never shorter than the source's cadence, which is what keeps the server inside a provider's hourly request limit. A random delay of up to a tenth of the cadence is added. After a second failed run in a row the wait doubles each time, up to an hour. A restarted server reads the run history and waits out what is left of the cadence. A run takes the last three hours from each payload, so values that a provider publishes late still arrive and a short database outage heals itself.

**Storing.** Stations carry a PostGIS geography position; a box query runs on a spatial index over its planar view. Observations are one row per station and time, in monthly partitions that are created when their first observation arrives. Writing an observation again merges it: a new measurement replaces the stored one, a missing measurement leaves it alone, and a row that would not change is not rewritten. Old data leaves by dropping whole months. The schema is a numbered list of SQL files compiled into the program and applied at start.

**Serving.** The API is described in [api.md](api.md). Every answer with data names the source of each record and carries that source's credit line; a station belongs to exactly one source, so data under different licences never shares a record. A station whose source is not in the server's configuration is not served, even if the store still holds it.

**Configuration** is a TOML file, environment variables, or both; a variable wins. `crates/server/config.example.toml` documents every setting. The database URL holds the only secret and is never printed.

How the server is run on a machine is in [deploy/README.md](../deploy/README.md).

### Storage

- **PostgreSQL:** users, flights, sites, airspace, stations, community data. Observations are partitioned by month and kept for thirteen whole months unless configured otherwise.
- **Disk:** IGC files and media under content-hash paths, PMTiles, and a rolling archive of raw provider payloads for replaying adapter bugs (one folder per source and day, fourteen days unless configured otherwise).
- **Backups:** nightly encrypted, incremental backups of the database and blobs to storage in a different location. This is the one recurring cost besides the server and it is not optional. Built so far: `deploy/backup.sh` writes a nightly dump of the database to the same machine and rotates old dumps. The copy at a different site is not set up; where it goes is the owner's decision.

### External dependencies that cannot be self-hosted

| Dependency | Why | Cost |
|---|---|---|
| Apple Push Notification service, Firebase Cloud Messaging | Push notifications | Free |
| An SMTP relay | Sign-in and notification email; self-hosted mail has poor deliverability | Small, or avoided at first with passkeys and Sign in with Apple |
| GitHub Actions | CI, including macOS runners for iOS builds | Free for public repositories |

### API and offline sync

- Static reference data (sites, airspace, obstacles, station metadata) ships as versioned, immutable **region bundles** that clients download and query locally. The map overlays render from the local bundle, so they work without a connection and need no tile server.
- Live data (observations, positions, forecasts) comes from the API with cache validators. Observations are built: see [api.md](api.md). Until region bundles exist, the station list of the API carries the station metadata too.
- Flights are recorded locally and uploaded later. Upload is idempotent by content hash.

### Authentication and accounts

Passkeys, email link and Sign in with Apple. The server is also an OAuth 2.1 authorization server, because MCP clients require it (PKCE, client ID metadata documents or dynamic registration).

### Sizing

The owner has no server yet and wants the system to carry many users. The recommendation is one machine that is large enough for development now and for a public launch, so nothing has to be migrated in between.

| Resource | Recommendation | Reason |
|---|---|---|
| CPU | 16 modern cores | Map builds, flight analysis and video transcoding. The API itself needs little. |
| Memory | 128 GB, error-correcting | Transit router (estimated 16 to 32 GB), forecast service (8 to 16 GB), database cache, map builds. 64 GB is the working minimum. |
| Fast storage | Two 2 TB NVMe drives, mirrored | Database, routing data, forecast data, map and terrain tiles |
| Bulk storage | None at first | Add mirrored hard disks or network storage when 3D imagery and user video arrive |
| Network | 1 Gbit/s without a traffic cap, in an EU data centre | Offline map downloads are the heaviest traffic |
| Backup | About 5 TB at a different site | See Storage above |

A Hetzner AX102 matches this (16 cores, 128 GB, two 1.92 TB NVMe drives). Its price could not be confirmed from this session: listings ranged from about 100 to 260 euros a month before VAT. Check the provider's configurator, and its auction of used servers, before ordering.

What this is expected to carry, as estimates that have not been measured:

- **API and live tracking:** tens of thousands of daily users and several thousand pilots in the air at once. Each pilot sends one small batch every 10 to 15 seconds.
- **First limit, bandwidth:** 1 Gbit/s moves at most about 10 TB a day. At 3 to 5 GB per offline region, that is roughly two thousand new installs a day. The remedy is a second machine or a content delivery network for the tile files only.
- **Second limit, disk:** user video. Clip length and resolution are capped from the start.
- **Single point of failure:** if the machine dies, restoring from backup takes hours. The cockpit is unaffected because it works offline. Before a public launch, add a second machine as a warm standby with database replication.

Growing beyond one machine does not need a redesign. The server binary's roles (API, live, ingestion, jobs) can run as separate processes on separate machines, PostgreSQL can replicate, and tiles are static files that any web server or CDN can serve.

The transit router's real memory use and the size of the terrain tiles for the Alps are measured in Phase 0 before these numbers are trusted.

### Deployment

What exists: the `deploy/` folder holds a Compose file (Caddy, the server, PostgreSQL with PostGIS), a Caddyfile, the server's Dockerfile, a backup script and a runbook for first install, update, backup and restore. The server builds its own image from a checkout of the repository. CI builds the same image on every push and starts it, so a broken Dockerfile is found early, but publishes it nowhere: publishing images is the owner's decision. The stack has run on a development machine only; the project has no server yet.

Recommended once the owner has a server: CI publishes container images to GitHub's registry, and a timer on the server pulls tagged releases and restarts the affected services. The owner runs one bootstrap script once. No credential that can reach the server is stored in GitHub, and a self-hosted CI runner on a public repository is ruled out because forks could run code on the server.

How an AI session inspects the running server when something breaks is still open. What exists for a person on the machine: `/v1/health`, the server's log with one line per collection run, the run history in the database, and the `ingest` subcommand to run a source by hand; see the runbook.

## Bring your own AI

The project pays for no inference. What the research found:

| Route | Allowed | Who pays | Notes |
|---|---|---|---|
| Sign in with a Claude subscription inside our app | **No** | | Anthropic's terms forbid third parties offering claude.ai login or routing requests through subscription credentials. |
| Our server as an MCP connector in the user's Claude | Yes | The user's Claude plan | Works on every plan including Free (one custom connector). A connector added on the web appears in the mobile apps. Use in voice mode with custom connectors is unproven and must be tested. |
| Our server as a custom MCP server in ChatGPT | Partly | The user's ChatGPT plan | Full custom MCP is documented for Business and Enterprise on the web; consumer plans realistically need a directory listing *(to verify)*. |
| User's own Anthropic API key in our app | Yes | The key owner | Anthropic states developers should use API-key authentication. The key stays in the device keychain and calls go from the device to Anthropic. |
| User's own OpenAI API key in our app | Grey | The key owner | Common practice, no explicit approval found. On-device storage only. |
| OpenRouter account connect (OAuth with PKCE) | Yes | The user's OpenRouter account | Best "connect" experience; needs no app registration. |
| Sign in with ChatGPT for open-source apps | Yes, in preview | The user's ChatGPT plan | Announced autumn 2026. Preview limits include no audio input and a loopback redirect, so the fit for a mobile app is unverified *(to verify)*. |

So the owner's wish is met in two ways:

1. **Connector.** The `tools` crate defines each tool once. The server exposes them over MCP at `/mcp` (Streamable HTTP, OAuth). A pilot adds the connector to their own assistant and asks there. This costs the project nothing and ships as soon as the API exists.
2. **In-app assistant.** The same tools run inside the app against the same API. The tool loop runs on the device in the Rust core, calling the provider directly with the user's credential. The server never sees a key. Providers sit behind one `LlmProvider` trait: Anthropic (Messages API over plain HTTPS, as there is no official Rust SDK), OpenAI, OpenRouter. Speech recognition and speech output use the phone's own engines.

Only the in-app assistant can speak first. An MCP connector can only answer. Proactive in-flight alerts therefore need the in-app route, and anything safety-relevant is deterministic code that works with no assistant at all.

Tool design rules: tools return numbers with their source and timestamp; calculations (glide range, airspace conflicts, itineraries) are done by the flight engine, never by the model; every tool declares whether it is read-only.

## Transit

Google's transit data is good, but its terms rule out the API for this project. The Routes API may not be used with a non-Google map; under the terms that apply to customers billed in the EEA, route steps may not be shown with any map at all; and passing raw results to a language model is not licensed. A plain "open in Google Maps" link is free, needs no key and is allowed.

- **Primary:** self-hosted MOTIS with the open timetable feeds that the community project Transitous uses (Germany-wide DELFI, the Austrian transport associations, Switzerland, South Tyrol and Trentino), plus OpenStreetMap for the walking legs. Results can be drawn on our map and given to the assistant.
- **Bootstrap:** the public Transitous API, which is the same software. It is free for open-source, non-commercial projects, asks for coordination above 720 routing requests a day and for automated use, and requires a link to its sources page.
- **Hand-off:** "open in Google Maps" and "open in Apple Maps" buttons as a cross-check for the pilot.

Realtime coverage is uneven: Germany has it, Tirol and Salzburg do not *(to verify)*. Small alpine bus lines should be tested against known routes before the feature is trusted.

## Quality rules

- **Replay tests.** The flight engine is tested by feeding it recorded flights and sensor logs and comparing against stored results. The cockpit can be run on the ground from a recorded flight.
- **Adapter contract tests.** See [data-sources.md](data-sources.md#testing).
- **One store suite, two stores.** The behaviour of the store is written once (`crates/store/tests/suite`) and run against the in-memory store everywhere and against a real PostgreSQL with PostGIS in CI. Locally the PostgreSQL run needs `VENTI_TEST_DATABASE_URL` and is skipped without it; CI refuses to skip it.
- **API tests through the real router.** They run against what the development mode serves and compare the answers with golden files, which are also the examples in [api.md](api.md).
- **Scheduler tests on virtual time.** The schedule is tested with a clock that the test moves; nothing sleeps.
- **CI gates:** formatting, lints with warnings as errors, tests, the dependency-graph test, a licence check on dependencies (`deny.toml`, which also bans OpenSSL and native-tls: TLS is rustls), and a build and start of the server image. All of them run on every push.
- **Decisions are written down.** A change to anything in the decision log below updates the log in the same commit.

## Decision log

| Date | Decision | Reason | Revisit when |
|---|---|---|---|
| 2026-11-18 | Native UI per platform over a shared Rust core; no cross-platform UI toolkit | Owner decision; performance and feel | Not open |
| 2026-11-18 | UniFFI for Swift/Kotlin, wasm-bindgen for web | Most widely deployed; used by Mozilla, Bitwarden, Element. Confirmed by the Phase 0 bridge spike. | Async or callback interfaces hit Swift 6 problems, or BoltFFI matures |
| 2026-11-18 | The iOS project is generated by XcodeGen; the core ships to the app as a local Swift package around an XCFramework | AI sessions cannot open Xcode, so the project must be editable as text and buildable from the command line | Not expected |
| 2026-11-18 | MapLibre Native and MapLibre GL JS for 2D maps | Open, one style on all platforms, local PMTiles, hillshade and colour relief built in | Not expected |
| 2026-11-18 | Own wgpu renderer for 3D | MapLibre Native has no 3D terrain; no web views allowed | MapLibre Native terrain pre-releases appear |
| 2026-11-18 | Pre-generated contour tiles on all platforms | No native contour generator | A native equivalent appears |
| 2026-11-18 | One server, PostgreSQL + PostGIS, disk storage, Docker Compose | Owner decision; cost | Load that one server cannot carry |
| 2026-11-18 | Static web build, no Node on the server | Fewer processes to run | Search indexing proves too thin |
| 2026-11-18 | Self-hosted MOTIS for transit; Google by link only | Google's terms forbid the API with our map and with an assistant | Not expected |
| 2026-11-18 | Self-hosted Open-Meteo for forecasts first, own model pipeline later | It already handles DWD's new native-grid feed; no usage limits when self-hosted | Soaring fields it lacks (boundary-layer height, model levels) are needed |
| 2026-11-18 | Users bring their own AI; MCP connector plus in-app assistant on user credentials | Owner decision; Claude subscription login in third-party apps is prohibited | Provider policies change |
| 2026-11-18 | Name: Venti | Owner decision. It is a common word (Italian for "winds" and "twenty", a coffee size, a game character), so plain searches for "Venti" will not find the app; the domain and "Venti paragliding" will. | Not open |
| 2026-11-18 | Licence: AGPL-3.0-or-later | Owner decision. Keeps forks open and allows reuse of GPL flight software such as XCSoar. The owner, as sole copyright holder, can publish in app stores. Before the first outside contribution is merged, a contributor agreement must let the owner keep doing so. | First outside contribution |
| 2026-11-18 | All work is committed straight to `main`; no branches, no pull requests | Owner decision; one person works on the project | Not open |
| Recommended | Pull-based deployment from GitHub's container registry. Until the owner decides, the server builds its image from a checkout and CI publishes nothing. | No server credential in GitHub | Owner orders the server |
| 2026-11-18 | iOS 26 is the minimum | The current system design (floating glass panels, the new tab bar) exists only from iOS 26. Supporting iOS 18 would mean a second, untested version of every such surface. iOS 26 runs on iPhone 11 and later. | Pilots with older iPhones ask for it |
| 2026-11-18 | Until the tile server exists, the iOS app fetches base map tiles from OpenFreeMap and terrain from Mapterhorn's public endpoint | A logbook has to show flights wherever they were flown, and the bundled demo region covers one valley. Both are the same open data we will host. | The server is up; in any case before a public release |
| 2026-11-18 | The logbook lives in the core: flight files kept as received plus a rebuildable index | All three apps get the same import, duplicate detection and totals. Keeping the original file keeps the recorder's signature for later upload to leagues. | Sync with the server is built (`client-store`) |
| 2026-11-18 | A flight's identity is a hash of its fixes, not of its file | The same recording downloaded from two places differs in headers but is one flight | Two recorders on one flight need to be recognised as one flight |
| 2026-11-19 | The core reaches its files through a storage interface with two implementations: a folder on disk, and a staged one for hosts that can only store asynchronously | A browser has no synchronous storage that fits a logbook, and the core's interface stays synchronous for UniFFI. With the seam at whole files, a flight's identity, the index, summaries and totals stay one piece of code. | `client-store` (SQLite) replaces the logbook's files, or the core gets an asynchronous interface |
| 2026-11-19 | In a browser the core holds the names of the stored files and the index, never every flight file | A logbook of a thousand flights is several hundred megabytes of text, more than a phone's browser should hold | Not expected |
| 2026-11-19 | The web binding is its own crate, `core-wasm`; TypeScript types are generated from the core's Rust types with tsify, behind the core's `wasm` feature | A second, hand-written copy of every view type would drift. A separate crate keeps wasm-bindgen out of the phones' builds. | UniFFI or BoltFFI can generate the web binding |
| 2026-11-19 | The UniFFI derives stay on in the WebAssembly build | The core compiles for `wasm32-unknown-unknown` with them. Their exports are dead weight in the browser (measured: 116 KB of 652 KB, 33 KB of 182 KB compressed), but switching them off would put a second attribute on every type in the core and break only the web build when one is forgotten. | The download size matters: remove the unused exports after linking, or gate the derives |
| 2026-11-19 | The flight computer takes time from its samples and reads no clock | A recorded flight replays to exactly what the pilot saw, which makes the cockpit testable on the ground and in CI | Not expected |
| 2026-11-19 | Climb rate from a two-state Kalman filter over altitude, with the external instrument's own vario preferred | Works at any sample rate and for any source; an instrument's vario already fuses sensors we cannot see | Accelerometer fusion is added for pilots without an external vario |
| 2026-11-19 | Wind by fitting a circle to ground velocities | XCTrack's own tracker lists its perfect-circle wind estimate as weak; this needs only GPS and tolerates uneven circles | A compass or airspeed sensor is available |
| 2026-11-19 | A recording is a flight file appended fix by fix in the data folder | A crash, a kill by the system or a dead battery costs at most the last seconds | Not expected |
| 2026-11-19 | The cockpit session is its own core object beside the logbook, and the shell imports a finished recording | Keeps the cockpit out of the logbook's storage while that is being reworked for the web | The storage interface has settled; then stopping can import in one call |
| 2026-11-19 | Everything shown about a flight describes the flown part of its recording: take-off to landing, found by the cockpit's own rules. The file is kept whole. | A recorder left on at the car park would otherwise inflate airtime and distance, and the app's own recorder runs from "Start flight", not from take-off | Recordings with several flights turn out to be common; then they are split |
| 2026-11-19 | Flight analysis judges circling with a window centred on each moment, unlike the cockpit's backward-looking rule | With the whole track available a thermal can be dated to when circling began, not to when it could be recognised | Not expected |
| 2026-11-19 | Flights are scored by the World XContest's published rules (free flight, free and FAI triangle, the 20% and 5% closing limits), searched on the track's corners: at most 400 for free flight with dynamic programming, at most 240 for triangles by trying every three | Exact search over every fix is far too slow, and the best turnpoints lie on corners. The points are labelled as ours: to verify against XContest's own result for the same flights, including which Earth model it measures on. | A flight scores differently on XContest |
| 2026-11-18 | The interface is checked by UI tests that publish screenshots | AI sessions cannot run Xcode or a simulator; without this nobody sees a change before the owner does | A session can drive a Mac directly |
| 2026-11-18 | Adapters are pure: they plan requests and parse responses, and the pipeline does all I/O | Adapters are testable from recorded payloads alone, and none can skip archiving, request budgets or validation | Not expected |
| 2026-11-18 | Validation removes an implausible value rather than the whole observation | One faulty sensor should not cost a good wind reading | Not expected |
| 2026-11-19 | `ingest` is a subcommand of the server, not a crate | One place holds the HTTP fetcher, the archive and the registry; the canary runs the code the server runs | Not expected |
| 2026-11-19 | The store is one interface with two implementations, PostgreSQL and in memory, held to one behavioural suite | The development mode and every API test run without a database, and cannot drift from what PostgreSQL does | A third store is wanted |
| 2026-11-19 | `tokio-postgres` with a `deadpool` pool and hand-written SQL; migrations are numbered SQL files compiled in and applied at start, all pending ones in one transaction | Few dependencies, no macros, no build-time database; a failed upgrade leaves the schema as it was. The connection is not encrypted because the database is on the same host. | The database moves to another machine (add TLS), or a migration needs a statement that cannot run in a transaction |
| 2026-11-19 | Observations are partitioned by month; a partition is created when its first observation arrives, and retention is counted in whole months (thirteen by default) | Dropping a partition removes a month at almost no cost, where deleting rows by time would scan the table. Creating on arrival cannot be forgotten the way a scheduled job can. | A source with far finer resolution than ten minutes makes a month too large |
| 2026-11-19 | A station's position is a `geography` column; box queries use a spatial index over its planar (`geometry`) view | A map viewport is a box in longitude and latitude, whose edges are not straight on the globe: the geography index would miss stations near them. Geography stays the column type for distance queries, which get their own index when the first one is written. | Queries by distance arrive |
| 2026-11-19 | Storing an observation again merges it field by field, and unchanged rows are not rewritten | Providers publish wind and temperature at different times, and one failed request must not erase what an earlier run stored. Each run delivers hours of data again; rewriting it would only make garbage. | Not expected |
| 2026-11-19 | One task per source; waits run from start to start and are never shorter than the cadence; backoff doubles from the second failure up to an hour; a restart honours the run history | The pipeline's per-run request budget assumes one run per cadence interval, so this is what keeps the hourly limits, also while a provider fails or the server restarts in a loop | A provider needs runs aligned to its publication times |
| 2026-11-19 | A source listed by several regions is collected once, over one box around all of them | A provider's limits apply to the source, not to a region | Regions far apart make the box wasteful; then a run takes a list of areas |
| 2026-11-19 | The API is versioned in the path (`/v1`); within a version answers only gain fields; an unknown value is left out, never `null` | Old apps keep working against a newer server, and stay installed for years | A breaking change is needed: add `/v2` beside `/v1` |
| 2026-11-19 | Every station belongs to one source and names it; every answer carries the credit of each source in it; stations of a source that is not configured are not served | The licences require credit, and data under different licences must not be merged. Without a manifest there is no licence to serve under. | The resolve stage links stations across sources: links, never merged records |
| 2026-11-19 | Data answers may be cached for 60 seconds and carry a weak `ETag`; the server compresses with gzip only | Data changes every ten minutes and is the same for everyone, so most repeated requests end as a bodiless 304. Weak, because compression changes the bytes and not the meaning. | Measurements show another codec is worth its dependency |
| 2026-11-19 | `/v1/health` answers 200 while the database is reachable and reports staleness in its body; a source is stale after three cadence intervals without a healthy run | A monitor needs "the server works" and "the data is fresh" as two separate facts | Not expected |
| 2026-11-19 | The station list is capped at 5000 stations with a `truncated` flag; an observation window is at most seven days | No request can ask for everything at once | The map needs clustering or tiles for station data |
| 2026-11-19 | The development mode runs the real adapters over the fixtures compiled into the provider crates, with the clock stopped at the recording | Client work needs neither network nor database and gets the same answers every time; the tests' recorded payloads have one definition | A client needs data that moves; then a replay mode |
| 2026-11-19 | Configuration is a TOML file plus `VENTI_*` variables, and the command line is parsed by hand | Compose sets variables, a developer keeps a file; three subcommands with a handful of options do not need a library | The command line grows |
| 2026-11-19 | Caddy in front for TLS and static files; database dumps with rotation on the same machine | Automatic certificates and range requests with a dozen lines of configuration. Off-site backup needs storage the owner has not chosen. | The owner chooses backup storage |
| Open | How AI sessions inspect the running server | Needed to maintain it | Owner orders the server |
