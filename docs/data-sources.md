# Data sources

Last revised: 2026-11-21

The app depends on dozens of external data providers, and the set differs by country. This document defines how they plug in, and lists what is available for the first region.

Status words used in the catalogue:

- **Open:** licence allows our use and re-serving. Build the adapter.
- **Built:** the adapter exists and its licence was verified on the provider's own pages.
- **Ask:** usable only with written permission. See [Outreach](#outreach).
- **Link only:** we may link or embed, not store or re-serve.
- **Skip:** closed, or not worth it.

Entries marked *(to verify)* come from research that could not reach the primary source. Verify the licence on the provider's own page before the adapter ships.

## Design

### Vocabulary

| Term | Meaning |
|---|---|
| **Kind** | A category of data with one canonical shape: observations, forecasts, airspace, sites, and so on. |
| **Adapter** | Code that fetches one kind from one provider and converts it to the canonical shape. One crate per provider. |
| **Manifest** | Declarative facts about an adapter: coverage, licence, attribution, limits, cadence. |
| **Region pack** | Configuration that says which adapters are active in an area and how conflicts between them are resolved. No code. |
| **Pipeline** | The fixed sequence every fetched payload passes through. |

Adding a provider means adding one adapter crate. Adding a country means adding adapters for its providers and one region pack. Neither touches the core, the server's API or the apps.

### Kinds and traits

Each kind has one trait in the `sources` crate. An adapter implements the traits for the kinds its provider offers.

| Kind | Trait | Canonical type |
|---|---|---|
| Station observations | `ObservationSource` | `Station`, `Observation` |
| Point forecasts | `ForecastSource` | `ForecastSeries` |
| Forecast fields for map layers | `FieldSource` | `ForecastField` |
| Radar and nowcast | `RadarSource` | `RadarFrame` |
| Airspace | `AirspaceSource` | `Airspace` |
| Airspace activations | `ActivationSource` | `Activation` |
| Obstacles and cables | `ObstacleSource` | `Obstacle` |
| Flying sites | `SiteSource` | `Site` |
| Protected zones | `ZoneSource` | `Zone` |
| Webcams | `WebcamSource` | `Webcam` |
| Live positions | `LiveSource` | `LivePosition` |
| Transit routing | `TransitRouter` | `Itinerary` |
| Map imagery | `ImagerySource` | tiles: `TileAddress`, `Coverage`, `CutTile` |
| Map layers | declared in the region pack only | |

Adapters are pure. They do no I/O and read no clock. An adapter says which URLs it needs and turns the bytes that come back into canonical records; where a provider needs several steps, a response can ask for further requests. The pipeline does the fetching, archiving, merging and validation, the same way for every source.

```rust
pub trait ObservationSource: Send + Sync {
    fn manifest(&self) -> &Manifest;
    /// The requests that start a run.
    fn plan(&self, ctx: &PlanCtx) -> Vec<Request>;
    /// Converts one response into records and, if needed, further requests.
    fn parse(&self, request: &Request, body: &[u8], ctx: &PlanCtx) -> Result<Parsed, SourceError>;
}
```

`PlanCtx` carries the time of the run and the area it covers. Because an adapter never touches the network, its tests are recorded payloads in and canonical records out, and no adapter can skip the archive, the request budget or validation.

`AirspaceSource` has the same three methods and returns airspaces. What differs is the run around it, because airspace is a different kind of data:

| | Observations | Airspace |
|---|---|---|
| Changes | Every ten minutes | With the 28-day publication cycle of aviation data; openAIP rebuilds its files daily |
| A run delivers | What is new, in pieces that are merged | The whole set, which replaces the one before |
| A request that fails | Tolerated up to one in ten: a station is offline | Makes the run unfit to replace the last set. A country that could not be fetched would be airspace nobody is warned about |
| A record that fails validation | Loses the faulty value | Is removed whole and reported by name |
| A record the adapter cannot read | Is skipped and reported | The same, with one difference: a limit that cannot be read is replaced by the ground or by "unlimited" and the airspace is kept, larger than published |

The canonical record, what is checked and how the cockpit uses it are in [airspace.md](airspace.md).

`SiteSource` is run like airspace: a run delivers every flying site the source has in the area, and that set replaces the one before, but only if every request worked. A set with a hole in it would show on a map as "no sites here". What differs is validation: a site is a point with a few facts, and a fact that cannot be right (a height of 30 km, a wind direction of 400°) is removed while the site stays. The set is dated by what the source says about its data: an Overpass instance names the moment its copy of OpenStreetMap was taken.

`ForecastSource` is the one kind that is not collected for an area. A point forecast is asked for when a pilot wants to know about a place, so the trait's `plan` takes the place:

```rust
pub trait ForecastSource: Send + Sync {
    fn manifest(&self) -> &Manifest;
    /// The requests that start the forecast for `point`.
    fn plan(&self, point: GeoPoint, ctx: &PlanCtx) -> Vec<Request>;
    fn parse(&self, request: &Request, body: &[u8], point: GeoPoint, ctx: &PlanCtx)
        -> Result<ParsedForecast, SourceError>;
}
```

Everything else is the framework as it is: the adapter is pure, and `run_forecast` fetches through the same fetcher, archives every payload, keeps to the request budget and validates. A run delivers one `ForecastSeries` per weather model. Each series names its model, the run of the model, the grid cell its values are for and the height of the model's ground there, and holds one record per hour with the values at the ground and the wind at levels above it. A series whose run the source does not tell is not delivered: nobody could say how old it is. Validation removes single values that cannot be weather, levels that do not say where they are, and pressure surfaces that the model puts under its own ground. How often a forecast may be asked for, and for how long an answer is reused, is the server's business: see [architecture.md](architecture.md#forecasts).

A source that can answer for many places in one request says so: `places_per_request` names how many (zero, the default, for one place at a time), `plan_places` takes one model and a list of places, and `parse_places` turns the answer into one series for each place. `run_forecast_places` runs that through the same pipeline and validation, for each place, and refuses a series of another model than was asked for. The server's thermal map is made this way, of ICON-D2 and of ICON-EU on the same points, one request a model at a time ([architecture.md](architecture.md#the-thermal-map)); Open-Meteo's adapter takes 80 places in a request.

`ImagerySource` is not collected either, and holds no records. A client's map asks the server for one tile, and the adapter says whether the provider has it, where it lies against what may be shown (inside, outside, or on the edge), the address to fetch it at, and, for a tile on the edge, what of it may go out: it cuts the tile. The server fetches, keeps the answer on its disk and answers ([architecture.md](architecture.md#the-aerial-view-through-the-server)). The adapter does no I/O, like every other. Imagery sources are not in the registry: nothing schedules them and no region pack names them.

`RadarSource` is collected for an area on a schedule, like observations, and what a run delivers is pictures: each frame the provider has that the server does not hold yet, laid by the adapter on the grid the run names and turned into Venti's steps of rain. So its `parse` is told what is wanted:

```rust
pub trait RadarSource: Send + Sync {
    fn manifest(&self) -> &Manifest;
    /// The requests that start a run: as a rule the provider's list of frames.
    fn plan(&self, ctx: &PlanCtx) -> Vec<Request>;
    fn parse(&self, request: &Request, body: &[u8], ctx: &PlanCtx, wanted: &RadarWanted)
        -> Result<ParsedRadar, SourceError>;
}
```

`RadarWanted` names the moment after which frames are wanted and the grid: squares of the web map over the run's area (`RadarGrid`, in `domain`). A `RadarFrame` is a source, the end of the interval it is of, how long that interval is, the grid, and one value a cell: no rain, a step from 1 to 6 by `RAIN_STEPS_MM_H` (0.1, 0.2, 1, 4, 10 and 30 mm/h; ours, and the same for every source), or "not seen". The provider's projection, units and file format end in the adapter. `run_radar` goes through the same fetcher and budget as every run; validation removes a frame whole that is of another source or another grid than was asked for, whose cells are not as many as its grid's or hold a value that is no step, or that is dated after the run. A run is healthy when every request worked and the provider's newest frame is no older than three of its intervals; one that found nothing new is healthy, and one of a provider that has stopped publishing is not. What the server keeps of the frames, and how it hands them out, is in [architecture.md](architecture.md#the-rain-radar).

On-device instrument drivers follow the same idea in the `sensors` crate: one parser per protocol, fed bytes, tested from recorded streams.

### Manifest

```toml
id = "geosphere-tawes"
provider = "GeoSphere Austria"
kinds = ["observations"]
countries = ["AT"]
cadence_seconds = 600
auth = "none"
# The larger of the two answers of a run was 177 kB on 2026-11-19.
max_response_kb = 4096

[rate_limit]
per_second = 5
per_hour = 240

[licence]
name = "CC BY 4.0"
commercial = true
redistribution = true
share_alike = false
attribution = "Data source: GeoSphere Austria - https://data.hub.geosphere.at"
link = "https://data.hub.geosphere.at"
constraints = ["Users must accept GeoSphere Austria's general terms and the Data Hub terms of use."]
verified_on = "2026-11-18"
verified_from = [
    "https://data.hub.geosphere.at/dataset/tawes-v1-10min",
    "https://data.hub.geosphere.at/en/legal",
]
```

An adapter that is built with someone else's data that it does not fetch, such as the outline `sources-basemapat` cuts its tiles along, names it in an `[[asset]]` section: what it is, the file, the provider, what was done to it, and its own `[asset.licence]` with the same fields as the source's. A manifest whose asset is of another licence family than the source is refused, since the asset is merged with the source's data.

Each adapter crate has its manifest in `manifest.toml`, compiled into the crate. Unknown fields are rejected, so a typo cannot silently drop a licence term.

The manifest is data, not comments. The server reads it to:

- send the credit of every source with the data it serves: the credit line, the licence's name and the link (`attribution`, `name`, `link`), and the `notice` if the provider asks for one to be shown with its data, as openAIP does;
- enforce rate limits and schedules;
- bound what one request may bring: `max_response_kb` is the largest answer taken from the provider, and the fetcher stops reading there. A manifest that says nothing gets 32 MB;
- refuse to start with a source whose licence does not allow serving its data onward;
- apply licence constraints automatically (see below);
- exclude non-commercial sources from a build with one switch, should the project ever take money.

### Region pack

```toml
id = "alps-de-at"
name = "Germany and Austria"
languages = ["de", "en"]

[bounds]
south = 46.3
west = 9.4
north = 48.5
east = 17.2

[observations]
sources = ["dwd-10min", "geosphere-tawes", "suedtirol-meteo", "awc-metar"]
priority = ["dwd-10min", "geosphere-tawes", "suedtirol-meteo", "awc-metar"]   # wins when two sources report the same station

[airspace]
sources = ["openaip"]

[sites]
sources = ["osm-free-flying"]

[radar]
sources = ["dwd-radar-ry"]
```

Region packs live in `regions/<id>/region.toml`. The area is a bounding box for now; a boundary polygon replaces it when a region needs a shape a box cannot express. Sections for the other kinds (map layers and so on) are added as those kinds are built.

A region may leave `[airspace]` out. Airspace from two sources is never merged, so `priority` there says which layer a client prefers where two cover the same air, not which fields win. A region's airspace sources are checked against the build's adapters like its observation sources: a server that cannot collect a region's airspace does not start.

`[sites]` follows the rules of `[airspace]`: a region may leave it out, sources are never merged, and a server that cannot collect a region's sites does not start. `[radar]` follows them too: each radar source is a layer of its own, with its own pictures and its own credit. `[forecasts]` is the one section whose sources a server may lack: a forecast comes from a service that whoever runs the server runs beside it, and a server that was not told where that service is starts all the same, says in its log which forecasts it goes without, and reports that it has none.

A manifest needed nothing new for airspace beyond the kind. `cadence_seconds` is how often the provider publishes, a day for openAIP, and the request budget and the staleness rule follow from it as for any source.

### Pipeline

Every payload goes through the same stages, in the `sources` crate:

1. **Fetch.** The server's scheduler runs each source at its cadence and within its rate limit; see [architecture.md](architecture.md#what-the-server-does-today). The fetcher keeps to bounds of its own, whatever an adapter or a provider's answer asks of it; see [What the fetcher will not do](#what-the-fetcher-will-not-do).
2. **Archive.** The raw payload is written to disk and kept for a rolling window. A bug report can be replayed exactly.
3. **Normalise.** The adapter converts to canonical types: SI units, UTC, WGS84, heights above mean sea level.
4. **Merge and validate.** Partial observations for the same station and time are merged (some providers publish wind, gusts and temperature in separate files). Implausible values are removed and counted, so one faulty humidity sensor does not cost a good wind reading. An observation with nothing left is dropped. Nothing unvalidated reaches the store.
5. **Resolve.** The same real-world object from two providers is matched (by position, name and type) and linked in a mapping table. The region pack's priority decides which fields win. Every stored field records its source. Not built as designed. For stations a smaller form is built since 2026-11-21, which acts when the stations of an area are answered and not in the pipeline: see [The same station from two sources](#the-same-station-from-two-sources).
6. **Store and publish.** Canonical records go to the database; changes are published to the live channel and to the region-bundle builder. Storing is built; publishing changes is not.

For airspace, stage 4 has nothing to merge, and the store replaces instead of adding: see the table under [Kinds and traits](#kinds-and-traits). The pipeline also dates every airspace by its file: by what the adapter found in the file, else by the `Last-Modified` the provider's server sent with it, else by the time of the run.

### What the fetcher will not do

An adapter names addresses, and some of them it reads out of what a provider sent. A provider's answer can come from someone else. So the one fetcher every source goes through (`crates/server/src/ingest/http.rs`) holds every request to this, since 2026-11-19:

- **HTTPS only.** An address that begins with anything else is not asked. The one exception is an address whoever runs the server wrote into its configuration with `http://`: a forecast service or an Overpass instance of their own, on a private network beside the server. The server hands the fetcher those addresses at start (`Providers::addresses`), and plain HTTP is accepted for exactly their scheme, host and port.
- **No redirect to another host.** A redirect is followed, three times at most, if it keeps the scheme, the host and the port. Any other is a failed request whose error names where it led (without the query). A provider that moves its files to another host therefore fails loudly, in the canary and in `/v1/health`, and the adapter's address is corrected by hand.
- **No answer larger than the manifest allows.** The body is read in pieces up to `max_response_kb` and no further; an answer that states a larger size is refused before any of it is read. Largest answers on 2026-11-19: DWD 467 kB (a station list), GeoSphere 177 kB, openAIP 3.6 MB (Italy), an Overpass instance 163 kB (the first region's flying sites; 276 kB since 2026-11-21, when the outlines of areas came with them), an Open-Meteo instance 19 kB (one model's forecast for one place; since 2026-11-21 the thermal map asks it for 80 places at once, and the largest such answer measured is 1.98 MB, [below](#several-places-in-one-request-tried-2026-11-21)).
- **Ten seconds to connect, two minutes for a request**, the body included.
- **No unpacking without bound.** That is the adapter's part, because only it knows what is inside: DWD's adapter takes at most 1 MB out of an archive, a hundred days of a table that holds one, and does not believe what the archive says about its own size.

The live run of 2026-11-19 through this fetcher: DWD 104 requests, GeoSphere 2, openAIP 8, none failed and none redirected. OpenStreetMap's Overpass instance refused the connection from that machine, as it had before the change (see [Flying sites](#flying-sites)).

### Licence rules built into the pipeline

- **Different licence families never merge into one record or one bundle.** Non-commercial and share-alike sources (openAIP, thermal.kk7.ch, ParaglidingEarth, recent Sentinel-2 mosaics) stay separate layers with their own licence tag. Resolution links their records to others but does not copy fields across.
- **Constraints are declared and enforced.** For example the Open Glider Network requires honouring pilots' opt-outs and forbids redistributing positions older than 24 hours. Those are manifest constraints that the live service applies; they are not left to each call site.
- **Credentials live in server configuration,** never in the repository.

### Health

Each adapter reports last success, record counts, quarantine counts and data age. The server keeps the outcome of every run and exposes the summary per source at `/v1/health` ([api.md](api.md)). Every data point shown to a pilot carries its age, and a stale source is shown as stale. One failing adapter does not affect the others: each source runs in a task of its own, a failure or even a crash inside an adapter ends that run only, and a source that keeps failing is tried less and less often, down to once an hour.

### Testing

Every adapter crate contains:

- **Fixtures:** real recorded payloads, including at least one malformed one. The healthy ones are also compiled into the crate (`fixtures::recorded()`), where the server's development mode finds them. A source of observations that is added after the development mode's clock was set (19:05 UTC on 2026-11-18) cannot be recorded for that moment, and an observation dated after the clock is refused as one from the future. Such a crate keeps two things: the whole answers as recorded, which its tests read the adapter against, and a sample for the clock's moment (`fixtures::evening()`), made from what the provider's service gives for that moment when asked later. The sample's notice says how it was made, and a test holds it against the recorded answers it was made from. `suedtirol-meteo` and `awc-metar` are the first two.
- **Golden files:** the expected canonical output for each fixture. CI fails if normalisation changes.
- **A canary:** the `Source canary` workflow runs every region's sources against the live providers once a day (`venti-server ingest`, run from the image a server runs) and fails when a source delivers nothing or reports problems. Providers change formats without notice; two such changes are already dated in [Known changes](#known-changes).

Golden files are rewritten by running the tests with `VENTI_UPDATE_GOLDEN=1`, and the change is reviewed like any other diff.

### Adding a provider

1. Confirm the licence on the provider's own page and record it in the manifest with the date and URL.
2. Create `crates/sources-<provider>` implementing the relevant traits.
3. Record fixtures and golden files.
4. Add the adapter and its recorded payloads to the registry in `crates/server/src/adapters.rs`, and the source to the region packs that should use it.
5. Add the canary.

What the source has to declare for the fetcher:

- **`max_response_kb` in the manifest**: the provider's largest answer with room to grow, measured and dated in a comment beside it. Left out, the bound is 32 MB, which is far more than most sources should be allowed.
- **Its address, if whoever runs the server chooses it.** Add it to `Providers` in `crates/server/src/adapters.rs` and to `Providers::addresses`. Only then may it be `http://`. An address the adapter knows by itself must be `https://`.
- **Addresses that answer themselves.** The fetcher follows no redirect to another host, so the adapter names the host that serves the data, not one that forwards to it.
- **A bound on whatever the adapter unpacks**, with a fixture that tries to get past it.

### Adding a region

1. List the providers for each kind and their licences.
2. Add the missing adapters.
3. Write the region pack and its boundary.
4. Build the map and terrain extracts for the area.

## What is built

As of 2026-11-19:

| Part | State |
|---|---|
| Canonical types for stations and observations (`venti-domain`) | Built |
| Manifest, adapter trait, registry, pipeline stages 1 to 4 (`venti-sources`) | Built for observations |
| Adapters: `dwd-10min`, `geosphere-tawes` | Built, with fixtures recorded 2026-11-18 and golden files |
| Adapter: `suedtirol-meteo` (`venti-sources-suedtirol`): the weather stations of South Tyrol | Built 2026-11-21, with the two whole answers of a run recorded that day, a sample of the evening of 2026-11-18 for the development mode, and golden files; in the first region's pack |
| Adapter: `awc-metar` (`venti-sources-awc`): the airfields' weather reports | Built 2026-11-21, with the whole answer for the first region recorded that day, a sample of the evening of 2026-11-18 for the development mode, and golden files; in the first region's pack |
| Region pack loader and the `alps-de-at` pack (`venti-regions`) | Built for observations |
| Store for stations, observations and run history (`venti-store`): PostgreSQL with PostGIS, and in memory | Built |
| Server (`venti-server`): scheduler, raw-payload archive with a rolling window, HTTP API with attributions, health per source, development mode over the recorded fixtures | Built for observations; installed on no machine yet |
| `venti-server ingest`, which runs a region's sources live once and stores nothing (formerly the separate tool `venti-ingest`) | Built |
| Daily canary | Built as a CI workflow |
| Request limits | The per-run budget from the hourly limit and a concurrency cap at the per-second limit are enforced by the pipeline. The scheduler spaces runs by the cadence, also after failures and restarts. A true per-second rate limit is not built; no source that makes many requests publishes one. |
| Resolve stage | Not built as designed. Since 2026-11-21, for stations, in a smaller form: where two sources have a station at the same place the server hands out one of the two, by the region's priority, and merges nothing; see [The same station from two sources](#the-same-station-from-two-sources). In the store such a station is still two stations |
| Change events | Designed, not built |
| Canonical type for airspace (`venti-domain`), and reading and questioning it (`venti-airspace`) | Built; see [airspace.md](airspace.md) |
| `AirspaceSource`, its registry and its run with stages 1 to 4 (`venti-sources`); the `[airspace]` section of a region pack (`venti-regions`) | Built |
| Adapter: `openaip` | Built, with fixtures recorded 2026-11-19 and golden files |
| Airspace in the server: collecting once a day, storing as one set per source, serving at `GET /v1/airspace`, health, the development mode, the canary | Built; installed on no machine yet |
| Region bundles | Not built. Until they are, the airspace endpoint is how an app gets a region's airspace |
| Canonical types for flying sites and point forecasts (`venti-domain`): `Site`, `ForecastSeries` | Built |
| `SiteSource` and `ForecastSource`, their registries and their runs (`venti-sources`) | Built |
| Adapter: `osm-free-flying` | Built, with an answer recorded for the whole first region (2026-11-19, and anew with the outlines of areas on 2026-11-21) and golden files |
| Flying sites in the server: collecting once a day, storing as one set per source, serving at `GET /v1/sites`, health, the development mode over the whole first region, `ingest` and with it the canary | Built; installed on no machine yet |
| The `[sites]` and `[forecasts]` sections of a region pack (`venti-regions`); what the store keeps about forecast models | Built |
| Adapter: `open-meteo` | Built, with answers recorded for five places, and for four in one request, from an Open-Meteo instance (first on 2026-11-19 from one run for the purpose, last on 2026-11-21 from the owner's installation), and golden files |
| Forecasts in the server: fetched from the operator's Open-Meteo instance when a place is asked for and kept for half an hour, served at `GET /v1/forecast`, a look every ten minutes for the health report, the development mode over the recorded answers, `ingest` | Built; run against no instance on a schedule yet, and installed on no machine |
| `ImagerySource` and the `Imagery` kind; `[[asset]]` in a manifest (`venti-sources`) | Built 2026-11-20 |
| Adapter: `basemap-at` (`venti-sources-basemapat`): Austria's orthophoto, its tile addresses, and the cut along Austria's outline | Built 2026-11-20, with six tiles recorded that day and golden files of the cut |
| Map imagery in the server: `GET /v1/imagery/{source}/{z}/{x}/{y}`, kept on the disk under a cap, the development mode over the recorded tiles | Built 2026-11-20; installed on no machine yet, and no app asks for it yet |
| `RadarSource`, `RadarFrame` and the `Radar` kind, their registry and their run (`venti-sources`, `venti-domain`); the `[radar]` section of a region pack | Built 2026-11-21 |
| Adapter: `dwd-radar-ry` (`venti-sources-dwd-radar`): DWD's five-minute rain radar composite, with a reader for its HDF5 files and its projection | Built 2026-11-21, with the list of frames and two frames recorded that day, whole, and golden files; in the first region's pack |
| Rain radar in the server: collected every five minutes, the last two hours kept in memory as pictures, served at `GET /v1/radar` and `GET /v1/radar/{source}/{time}.png`, health, the development mode over the recorded frames, `ingest` and with it the canary | Built 2026-11-21; installed on no machine yet, and no app draws it |
| Every kind other than observations, airspace, sites, point forecasts, map imagery and rain radar | Designed, not built |

A live run of the first region on 2026-11-18 delivered 50 DWD and 267 GeoSphere stations in under six seconds, using 104 and 2 requests.

On 2026-11-21 at 10:52 UTC the server's own fetcher ran the four station sources of the first region, one after the other (`venti-server ingest regions/alps-de-at/region.toml --source <id>`): 435 stations, none of the 109 requests failed.

| Source | Stations | Observations | Newest | Requests | Left out |
|---|---|---|---|---|---|
| `dwd-10min` | 50, of which 30 gave a wind | 451 | 22 min old | 104 | |
| `geosphere-tawes` | 267, of which 253 gave a wind | 264 | 2 min | 2 | |
| `suedtirol-meteo` | 96, of which 81 gave a wind | 95 | 12 min | 2 | The values of Salurn, south of the box; two humidities over 100 % |
| `awc-metar` | 22, all with a wind | 22 | 22 min | 1 | |

The two new sources have not run on a schedule against a database: *to verify* on the first installation, with everything else.

On 2026-11-21 at 12:21 UTC the server's own fetcher ran the radar source once (`venti-server ingest regions/alps-de-at/region.toml --source dwd-radar-ry`): four requests, the list and the frames of 12:10, 12:15 and 12:20 UTC, none failed, nothing left out or rejected; DWD's newest frame was one minute old. On a schedule it has not run: *to verify* on the first installation.

On 2026-11-19 openAIP's export files for Germany and Austria were fetched by hand from the address the adapter uses, without a key: 756 and 262 airspaces, 2.0 MB and 0.8 MB, both written that night. The adapter and the pipeline read both whole files from disk: 1,018 airspaces, none skipped, none rejected, no type the adapter does not know. Of them the cockpit's warning table marks 637 to avoid, 134 as a caution and 247 as free. The OpenAir exports of the same two countries, read by the OpenAir reader of `venti-airspace`, give the same name, class, type and limits for every one of the 1,018. `VENTI_OPENAIP_DIR=<folder> cargo test -p venti-sources-openaip --test pipeline whole -- --nocapture` repeats the check on freshly fetched files.

Later that day the server's own fetcher ran the adapter against openAIP (`venti-server ingest regions/alps-de-at/region.toml --source openaip`): two requests, no failure, 344 airspaces in the first region's box, 257 of Austria dated 01:12 UTC and 87 of Germany dated 01:57 UTC by openAIP's file store. The same files, read whole by the server and served at `/v1/airspace`, are 1.7 MB as JSON and 369 kB with gzip for both countries; see [api.md](api.md#get-v1airspacebboxwestsoutheastnorth). The server has not yet run the source on its schedule against a real database for a day: *to verify* on the first installation.

The same day the adapter was given every other country the first region's box takes in. Their files were fetched by hand first: Switzerland 196 airspaces (0.45 MB), Italy 1,557 (3.6 MB), Slovenia 64 (0.16 MB), Croatia 419 (0.83 MB), Hungary 192 (0.43 MB), Slovakia 130 (0.48 MB), all written that night between 01:41 and 03:21 UTC. `li_asp.geojson` does not exist (the file store answers "NoSuchKey"). Czechia's file was fetched too and is not used: none of its 356 airspaces reaches into the box, whose northern edge at 48.5° N lies 6 km south of the southernmost corner of any of them. The adapter and the pipeline read the eight files whole from disk: 3,575 airspaces, no feature the adapter could not read, no type it does not know, and one record rejected by validation, the Hungarian LHB09 BIHARI-SÍK II, which openAIP publishes from the ground to the ground. It lies at 21° E, far outside the box.

Then the server's own fetcher ran the adapter against openAIP again: eight requests, no failure, nothing left out, nothing rejected, 506 airspaces in the first region's box. By country: Austria 257, Germany 87, Italy 45, Croatia 33, Slovenia 26, Hungary 21, Switzerland 20, Slovakia 17, each dated by openAIP's file store. A daily run downloads 8.8 MB. What the warning table makes of each country's airspace is in [airspace.md](airspace.md#outside-germany-and-austria).

On 2026-11-19 the server ran against the live providers and a real database for 22 minutes: three scheduled runs per source, all healthy, the same 50 and 267 stations, 1638 observations stored. The newest German values were 25 to 44 minutes old when fetched, the Austrian ones under ten. Of the 1939 observations the second and third runs delivered, 1272 were already stored; one of them had changed and was rewritten. The raw payloads of those six runs were 318 files and 5 MB, which makes about 240 MB a day.

## Catalogue: Germany and Austria

### Live wind and weather stations

| Source | Provides | Licence | Status |
|---|---|---|---|
| DWD open data, 10-minute observations | Wind, gusts, temperature across Germany. One file per station and dataset. On 2026-11-18 the wind files were 20 to 30 minutes behind real time and the temperature files about 50 minutes (one evening's sample). | CC BY 4.0, verified 2026-11-18 | **Built** |
| GeoSphere Austria, TAWES 10-minute | Austrian station network. All stations in one request; values were under 10 minutes old on 2026-11-18. Whether a time stamp marks the start or end of its interval is *to verify*. | CC BY 4.0, verified 2026-11-18; 240 requests an hour and 5 a second | **Built** |
| Avalanche service Tirol (avalanche.report) | About 200 mountain stations with wind and gusts | Files are public; licence not stated in them, Land Tirol open data is CC BY 4.0 *(to verify)* | Open after confirmation |
| Autonomous Province of Bolzano - South Tyrol, open data (`suedtirol-meteo`) | The weather stations of the province's Agency for Civil Protection: 97, of which 83 have an anemometer, from 212 m to 3399 m; 96 and 82 of them in the first region's box, 34 of those above 2000 m. Mean wind, gust, direction, temperature and humidity every ten minutes, all stations in two requests; the newest values were 12 to 21 minutes old on 2026-11-21. Raw values that the province has not validated. Its pressure is reduced to sea level and is not taken. See [South Tyrol's stations](#south-tyrols-stations) | CC0 1.0, verified 2026-11-21 on the [dataset's page](https://data.civis.bz.it/de/dataset/misure-meteo-e-idrografiche); no limit on requests published | **Built** |
| MeteoSwiss open data | SwissMetNet | CC BY 4.0 | Open (for later) |
| METAR through the Aviation Weather Center of the United States (`awc-metar`) | The weather reports of the airfields, 22 in the first region's box. **The wind at an airfield, not at a take-off:** on a valley floor or the plain, a ten-minute mean in whole knots, every half hour, 7 to 47 minutes old when asked on 2026-11-21. A gust only when the report has one, no humidity, and a pressure reduced to sea level, which is not taken. One request for the box. See [Airfield reports](#airfield-reports-metar) | Public domain by the [disclaimer](https://www.weather.gov/disclaimer) of the National Weather Service, whose centre it is, verified 2026-11-21, on three conditions; 100 requests a minute by the [service's page](https://aviationweather.gov/data/api/). Whether the weather services that write the reports hold rights in them that the disclaimer does not speak for is *to verify* | **Built** |
| FANET weather stations through the Open Glider Network | Club stations that broadcast on FANET | ODbL and two rules of the network, read 2026-11-21 on its [page](https://www.glidernet.org/ogn-data-usage/): follow the pilots' privacy choices, and pass on no data older than 24 hours | Open and **not built**: the data comes over a connection that stays open, which the pipeline has no kind for, and the 24 hours stand against a station's history. See [Looked at and not built](#looked-at-on-2026-11-21-and-not-built) |
| Pioupiou / OpenWindMap | Community wind sensors, many of them at take-offs and landings. 45 in the first region's box on 2026-11-21, 41 of them with a reading of the last ten minutes; the "little coverage" this row said before was not measured | Its own "Community License", read 2026-11-21 on its [page](https://developers.pioupiou.fr/data-licensing/): free, also commercially, with credit and a link, **on condition that every other weather sensor in the application is open data** by its definition | **For the owner to decide**; not built. See [Looked at and not built](#looked-at-on-2026-11-21-and-not-built) |
| **Holfuy** | The stations at most launch sites | No published terms; keys are issued only to station owners | **Ask** |
| Avalanche services Bavaria, Salzburg, Vorarlberg, Carinthia | Mountain stations | No open licence found | Ask |
| winds.mobi | Aggregator; in this region it is essentially Holfuy plus METAR | Code AGPL-3.0; no API terms published | Skip (we ingest the originals) |
| Windy stations, cable-car operators | Various | Custom or none | Skip for now |

#### South Tyrol's stations

Read and recorded on 2026-11-21.

- **Whose and under what.** The dataset "Webservice der meteorologischen und hydrographischen Messungen" of the province's Agency for Civil Protection (Agentur für Bevölkerungsschutz), on the province's open data portal: [data.civis.bz.it](https://data.civis.bz.it/de/dataset/misure-meteo-e-idrografiche). The page names the licence "Creative Commons CCZero", and each of its resources carries Italy's code for CC0 1.0 (`A11_CCO10`). The portal's [questions and answers](https://data.civis.bz.it/de/faq) say that the province "normally" publishes under CC0 and what that allows: use that is free and without charge, private and commercial. CC0 asks for no credit. Venti names the source all the same, as it names every source.
- **The service.** `https://geoservices.buergernetz.bz.it/services/meteo/v1/` with three parts, `stations`, `sensors` and `timeseries`, described in a PDF on the dataset's page (its revision of 2026-09-01 was read). No key. No limit on requests is published. The addresses older pages give (`daten.buergernetz.bz.it`, `dati.retecivica.bz.it`) answer 404 with a line that says the service has moved.
- **A run** asks `stations` and `sensors` once each: 95 kB and 184 kB. The documentation calls a station's code obligatory for `sensors`; without one the service answers for every station, and the dataset's page links that address itself. It takes no list, of stations or of sensors (tried: an empty answer), so the values of all stations come or those of one.
- **The stations.** 230 in the list, of which 97 measure weather and 133 water: rivers, lakes and groundwater. Which is which shows in the last two letters of a station's code, `MS`, `SF` and `WS` for weather. That is not in the documentation; it was read off the data and is *to verify*. A station of another ending that measured wind would have its values reported by every run as values without a station. 83 stations have an anemometer: 48 `MS`, in the valleys and villages, 21 `SF`, on snow fields, and 14 `WS`, on summits and ridges. Of the 82 in the first region's box, 45 stand above 1500 m and 34 above 2000 m; the highest is the Signalgipfel of the Wilder Freiger at 3399 m. Salurn at 46.24° N is the one weather station south of the box, which ends at 46.3° N: the pack's box was left as it is.
- **Taken:** mean wind speed (`WG`, m/s), gust (`WG.BOE`, m/s), mean wind direction (`WR`, degrees), air temperature (`LT`) and relative humidity (`LF`). A row whose unit is not the documented one is left out and reported. A value every ten minutes: 144 in the 24 hours of a time series. The catalogue said five minutes before, which was not measured.
- **Not taken: the pressure.** `LD.RED` is "reduzierter Luftdruck", reduced to sea level: between 1015.2 and 1023.3 hPa at every one of the 59 stations that send it, from 212 m to 2055 m. The canonical record's pressure is the pressure at the station's own height, so this value has no place in it. **A foehn reading wants exactly this value**, the reduced pressure on both sides of the main ridge. It needs a field of its own in the record first; GeoSphere and DWD send the pressure at the station. Rain, snow depth, sunshine and radiation are not taken either, and the service has no direction of the gust.
- **Raw values.** The documentation: "hochaufgelöste und NICHT validierte Rohdaten". In the recording two hygrometers read 100.1 % and 100.3 %. Validation removes such a value, keeps the rest of the observation and counts the value as rejected, in every run for as long as the air there is saturated. One station's sensors had no value, which the service writes as rows without a date and a value. One thermometer was in the answer twice, and seven sunshine sensors, which are not read, had a second row dated 2026-07-30; the adapter takes the newest row of a sensor.
- **Times** are local time with `CET` or `CEST` written behind them. Whether a time marks the start or the end of its interval, and over which interval the wind is a mean, is not in the documentation: *to verify*. It is treated as the end, as for the other sources.
- **Names.** A station has four: German, Italian, Ladin and English. The German one is taken, since a station has one name and German is a language of the first region. A pilot who reads Italian sees "Sterzing" and not "Vipiteno".
- **What every run reports as left out:** the values of Salurn. The service sends the values of all stations, the adapter cannot tell from them where a station stands, and the pipeline leaves out the values of a station outside the area and says so.

#### Airfield reports (METAR)

Read and recorded on 2026-11-21.

- **What a METAR is worth to a paraglider.** It is the weather at an airfield, written for the aircraft that land there. An airfield lies on a valley floor or on the plain, so its wind is the valley wind or the wind of the foreland, never the wind at a take-off. It is worth having for three things: the valley wind where a landing field lies near (Innsbruck, Bolzano, Samedan in the Engadin at 1708 m), the wind of the plain before the Alps (Memmingen, Lechfeld, Altenstadt, Oberpfaffenhofen), and the places where none of the weather services Venti collects has a station within 30 km (Samedan, Maribor, Sármellék). It is coarse: whole knots, which is steps of half a metre per second; the direction in steps of ten degrees or "variable", which is no direction; a gust only when it exceeds the mean by ten knots; whole degrees of temperature; and a report every half hour, a few airfields every hour. What the rules of a METAR are was written here from knowledge of the format (ICAO Annex 3) and not read at a source: *to verify*.
- **Whose and under what.** The reports are written by the weather offices of the airfields and exchanged between the world's aviation weather services. The Aviation Weather Center, a centre of the United States' National Weather Service, decodes them and serves them for the whole world. Its site's disclaimer, [weather.gov/disclaimer](https://www.weather.gov/disclaimer), which the service's page links: the information on the service's pages is "in the public domain, unless specifically noted otherwise", and may be used for any lawful purpose by whoever does not claim it as their own, does not imply an endorsement by or an affiliation with NOAA or the service, and does not present changed content as official government material. The name "National Weather Service" is a trademark, to be used only for the service's unaltered content or a link, so Venti's credit names the Aviation Weather Center and its site. Whether a German or Austrian weather office holds rights in its own reports that an American disclaimer cannot waive was not looked into: *to verify*, and the owner's decision whether it matters.
- **The service.** [aviationweather.gov/data/api](https://aviationweather.gov/data/api/): `GET /api/data/metar?bbox=south,west,north,east&format=json`, the newest report of every airfield in a box, at most three decimals in a bound and at most 400 reports in an answer. No key. "All requests are rate limited to 100 requests per minute"; the page asks for requests "limited in scope and frequency" and for a user agent of the client's own, which the server sends by default (its name and version; `VENTI_USER_AGENT` changes it). Parameters outside the documentation are refused. The answers are renewed once a minute.
- **A run** is one request: 9.6 kB for the first region's box, 22 airfields: nine in Austria, eight in Germany, two in Switzerland, and one each in Italy, Slovenia and Hungary. Each report carries the airfield's name, position and height, so a report is a station and an observation at once. The source is collected every ten minutes. An airfield reports half-hourly, but the airfields do not report at the same minutes and a special report can come in between; asked every ten minutes, a half-hourly report is replaced before it is an hour old, which is when the apps grey a reading. An airfield that reports once an hour is greyed for a part of every hour, which is the truth about it.
- **Taken:** the mean wind and the gust, from knots to metres per second to a hundredth; the direction, unless it is variable or the wind is calm, which a METAR writes as direction 0 with speed 0; the temperature; the time of the observation (`obsTime`: the answer's `reportTime` is rounded to the half hour). The service documents its wind speeds as knots whatever unit the report itself is written in; no airfield in the box reports in metres per second, so that was not seen: *to verify* before a region with such airfields.
- **Not taken:** the pressure, which is the altimeter setting and reduced to sea level, like South Tyrol's; the humidity, which a METAR does not have (it has the dew point, which is not converted); visibility, clouds and weather.
- **How old.** When asked at 10:37 UTC, the reports were of 10:30 (one), 10:20 (seventeen), 10:15, 10:00 (two) and 09:50 (Bolzano): 7 to 47 minutes old. On the evening of 2026-11-18 the reports of 18:50 UTC had reached the service between 18:52 and 18:56, and Bolzano's at 19:09.
- **The same airfield twice.** Ten of the 22 airfields have a station of DWD or GeoSphere beside them. The reports come last in the region's priority, so the weather service's station is the one handed out there, and the report only while that station has no wind to show: see [The same station from two sources](#the-same-station-from-two-sources).

#### Looked at on 2026-11-21 and not built

**FANET weather stations through the Open Glider Network.** The network's [rules](https://www.glidernet.org/ogn-data-usage/), whole: its data is under the ODbL; whoever passes it on follows the privacy choices pilots made in its device database; and nobody passes on data older than 24 hours. Three things stand between that and an adapter:

1. *It is a connection, not a fetch.* The network's data comes as a stream from servers a client stays connected to. An adapter of the pipeline names addresses, and the fetcher asks each once and reads an answer of bounded size. A kind for a connection that stays open (`LiveSource` in the table of kinds) is designed and not built, and bending `ObservationSource` to hold one would give up what the pipeline rests on: that an adapter does no I/O. How a FANET station's reading looks in the stream was not read on the network's pages: *to verify* when the kind is built.
2. *The 24 hours.* The server keeps a station's readings for thirteen months and serves any seven days of them. For a source under this rule it would have to forget, or stop serving, after a day: a constraint in the manifest that the store and the API apply, which does not exist.
3. *Share-alike.* The ODbL makes it a licence family of its own. As a source of its own, whose stations are never merged with another's, that is within the rules as they are.

**Pioupiou / OpenWindMap.** One request, `https://api.pioupiou.fr/v1/live/all`, answers with every sensor: 764 on 2026-11-21, 463 kB, no key. 45 stand in the first region's box, and their names say where: at take-offs (the Kreuzjoch and the Osterfelder above Garmisch, the Kampenwand, the Hochobir, the Schöckl, the Vigiljoch, the Hirzer) and at landing fields (Oetz, Garmisch). 41 had a reading of the last ten minutes. These are stations where pilots fly, which the two sources built on this day are not.

It was not built because of one condition of its [Community License](https://developers.pioupiou.fr/data-licensing/), which is otherwise plain: free, without registration, for any use including commercial; a credit and a link in a visible place, for which it gives the wording "Wind data (c) contributors of the OpenWindMap wind network <https://openwindmap.org>"; no claim to own the data. The condition: "If your application integrates data from other weather sensors, the data from those other weather sensors must be open", by a definition of its own: on a public site or API, at no cost, for anybody to reuse for any application including commercial ones and redistribution, readable by a computer, in full resolution, without delay, and with the full archive retrievable where an archive is used. An application whose other sensors are not all open may not use the Community License and is pointed to an "Extended License" by agreement. Pioupiou reserves the right to add terms to the definition.

So the condition binds the whole of Venti and not this one source. Today every station source Venti has would pass by a plain reading: CC BY 4.0, CC0 and public domain. A source by permission would not: Holfuy's stations and the avalanche services' are "Ask", and the day one of them says yes on its own terms, the free licence for Pioupiou's data ends. It is the owner's choice which of the two the app is built on, or a letter to Pioupiou about an Extended License before either. Parts of the definition read as duties of the application ("You must provide the same dataset as the one you're using"); what they ask of Venti is *to verify* with Pioupiou. The licence families of the manifest have no name for such a condition.

#### The same station from two sources

To the store, a station that two sources report is two stations: two identifiers, two sets of values, two histories, two credits. Until 2026-11-21 no such station existed. DWD has Germany's stations, GeoSphere Austria's and the province South Tyrol's, and the nearest two stations of different sources among them are 4.8 km apart: Kiefersfelden and Kufstein, two places. The airfield reports brought the first. Measured between the positions the four sources gave in the live run of that day, from each of the 22 airfields to the nearest station of another source:

| Distance | Airfields | Apart in height |
|---|---|---|
| Under 1 km: the same instruments or ones beside them | 8. Munich (DWD's München-Flughafen, 40 m), Klagenfurt (40 m), Salzburg (60 m), Vienna (70 m), Linz (250 m), Zeltweg (430 m), Augsburg (DWD, 720 m), Innsbruck (960 m); the six Austrian ones are GeoSphere's | 1, 0, 3, 4, 8, 0, 0 and 8 m |
| 1 to 2.5 km | 2. Graz (GeoSphere's Graz-Thalerhof-Flughafen, 1.9 km), Wiener Neustadt (2.45 km) | 3 and 9 m |
| 3.7 to 7.5 km: another place | 5. Friedrichshafen, Vöslau, Bolzano (the province's Bozen, 4.3 km), Altenrhein, Memmingen | 17, 34, 6, 1 and 11 m |
| More than 11 km | 7. Altenstadt, Oberpfaffenhofen, Laupheim, Lechfeld, Maribor, Samedan, Sármellék | |

So ten airfields were on the map twice. Each time the weather service's station is the better one: a value every ten minutes with gust, humidity and pressure, against a report every half hour in whole knots.

**Built on 2026-11-21: one of the two is handed out.** This is not stage 5 of the [pipeline](#pipeline) as designed, which makes one record of two sources and links them in the store. It is a smaller form that leaves the store and the record as they are (`crates/server/src/api/same_place.rs`):

- **Where it acts.** At the moment the stations of an area are answered: `GET /v1/stations`, and with it the assistant's wind tool, which is answered from the same list. Both stations go on being collected and stored, each with its own history, and each answers for its observations by its identifier as before.
- **The same place** is two stations of different sources that stand no farther apart than **2.75 km** and differ by no more than **50 m** in height. The ten pairs are 40 m to 2.45 km apart. The nearest two stations of two sources that are known to be two places are 3.03 km apart: Innsbruck's airfield by its report and GeoSphere's station at the university, known because GeoSphere's station on the airfield is a third. After them come an airfield and a second station in the town at Klagenfurt (3.18 km) and Salzburg (3.38 km). The bound lies halfway between 2.45 and 3.03 km. A report gives its airfield's position rounded to a thousandth of a degree, which can move it by some 70 m, so 300 m to either side is room. In height the ten pairs differ by 0 to 9 m, and 50 m is five times that and far less than lies between a valley station and one on the ridge above it. No pair of the first region is kept apart by the height today; the bound is there for stations at take-offs, which stand above airfields. A station whose height is not given is at the same place as no other, and two stations of one source never are.
- **Which one.** The one whose source comes first in the region pack's `priority`: in the first region DWD, GeoSphere Austria, South Tyrol, and the airfields' reports last. With one exception: a station that has no wind speed of the last hour does not stand for one that has. When a weather service's station falls silent, or measures no wind, the airfield's report is handed out in its place, alone, until the station is back. An hour is when the apps grey a reading. Sources the priority does not name come after those it names and have no order among themselves, so both their stations are handed out.
- **Nothing is copied.** The station that is handed out has its own values and credit and nothing of the other's, and the other's source is credited only if another of its stations is in the answer. Every station still belongs to one source, as decided on 2026-11-19, and the rule on licence families has nothing to forbid.
- **Whatever the box.** The store is asked for the box with 2.8 km of room around it, so that a station at the edge is judged against its neighbour outside and does not come and go as a map is moved. The exception is a box that holds more stations than an answer lists (5000): it is cut as before, and a double at its edge may show.
- **With several regions** the priority lists are taken one after the other, and a source keeps the place the first region gave it. An order that differs from region to region is not built.

Applied to the stations of the live run of 2026-11-21, the rule leaves out ten of 435: the reports of Munich, Augsburg, Klagenfurt, Salzburg, Vienna, Linz, Zeltweg, Innsbruck, Graz and Wiener Neustadt. 425 are handed out, twelve of them airfields. The development mode hands out all twelve of its stations: the nearest two of different sources in it are the Innsbruck pair at 3.03 km, and GeoSphere's station on the airfield is not among its recordings.

**The owner's to change:** the distance, the height and the hour; and whether one record should be made of two after all, with fields of each, as stage 5 was designed. That needs a mapping table in the store and a source for every field of the record, and changes the record on the wire. What the smaller form cannot do: have a pairing corrected by hand, pair a station that gives no height, or tell a pilot that a second source measures at the place.

### Forecasts

**The model is ICON-D2**, by the owner's word of 2026-11-20: the whole forecast is to be built on it, as the best model there is for the Alps, and it is the one Burnair uses. That is what is built: every forecast Venti serves is read from ICON-D2 on its own grid (about 2 km, a run every three hours, 48 hours ahead), and from ICON-EU only for the hours after ICON-D2 ends; flyability reads a day from the finer of the two, unless the coarser has more of the day's hours ([design.md](design.md#flyable-marginal-not-flyable)). What Burnair uses, read on its own pages on 2026-11-21: ICON-D2 for the first 48 hours and ICON-EU for days two to four ("Neuerdings wird 48h im Voraus mit dem ICON-D2 Modell gerechnet (2,2 km Segment)", "Ab Tag 2–4 (ab übermorgen) wird mit dem ICON-EU Modell (7 km Segment)" gerechnet), with its thermal forecast made from ICON by the Regtherm model, which it licenses from its author, and an algorithm of its own of "more than 40 parameters", among them "Thermikstärke, Basishöhe, Wind Gradienten, Regen oder Inversionen" ([burnair.ch, 2024](https://www.burnair.ch/2024/01/01/neue-regionen-thermik-prognose-2024/), [burnair.ch, Thermik Prognose](https://www.burnair.ch/portfolio-item/thermik-prognosen/), [lu-glidz, 2024](https://lu-glidz.blogspot.com/2024/01/mehr-thermikdaten-bei-xc-therm-und.html)). It rates a region for a day, and half-hour by half-hour, "in 5 unterschiedlichen Farben"; which colour means what is not on those pages and is from a review: red for flying not recommended, grey for weak thermals and a glide down, yellow for regional thermal flights, green for a good cross-country region, blue for a possible "Hammertag" ([lu-glidz, 2022](https://lu-glidz.blogspot.com/2022/02/burnair-eine-app-fur-fast-alles.html)). Neither Regtherm nor Burnair's algorithm is open, and Venti uses neither: its thermal estimate is its own, from ICON-D2's public fields ([design.md](design.md#thermals)). A finer open model exists for the Alps and is not used: MeteoSwiss's ICON-CH1-EPS, 1 km over the whole Alpine arc, 33 hours ahead, published as open data since 2025 ([opendata.swiss](https://ckan.opendata.swiss/dataset/numerisches-wettervorhersagemodell-icon-ch1-eps), read 2026-11-20; licence and terms to verify before use). It would be a second model beside ICON-D2, for the owner to decide once the first region flies.

| Source | Provides | Licence | Status |
|---|---|---|---|
| Self-hosted Open-Meteo, `open-meteo` | Point forecasts of DWD's ICON-D2 (about 2 km) and ICON-EU, hour by hour: wind and gusts, wind at heights, temperature, dew point, cloud by layer, precipitation, CAPE, freezing level, convective cloud base and top, the heat the ground gives to the air, and from ICON-D2 the strongest updraft | Data CC BY 4.0 from both Open-Meteo and DWD, verified 2026-11-19; code AGPL-3.0-or-later, run unmodified as a separate service. Needs 8 to 16 GB memory and fast disk | **Built** (ICON-D2 and ICON-EU). ICON-CH and ECMWF through the same instance: open |
| DWD ICON-D2 raw GRIB | Model levels, mixing height, maximum updraft | CC BY 4.0 | Open (later, for fields Open-Meteo lacks) |
| GeoSphere Austria C-LAEF 1 km | Alpine high-resolution model, hourly | CC BY 4.0 | Open (later) |
| ECMWF open data | Global 0.25° | CC BY 4.0 | Open |
| Soaringmeteo | WRF thermal forecasts for the Alps | Code GPL-3.0; data licence not stated | Ask |

#### Open-Meteo: licence and terms

Read on 2026-11-19.

- **The data.** [open-meteo.com/en/licence](https://open-meteo.com/en/licence): "API data are offered under Attribution 4.0 International (CC BY 4.0)", and "You must include a link next to any location Open-Meteo data are displayed, for example: Weather data by Open-Meteo.com". The model output is Deutscher Wetterdienst's, which its [legal notice](https://www.dwd.de/EN/service/legal_notice/legal_notice_node.html) puts under CC BY 4.0 "providing the source is acknowledged". DWD's [templates](https://www.dwd.de/DE/service/rechtliche_hinweise/vorlagen_quellenangabe.html) give "Quelle: Deutscher Wetterdienst" for unchanged data and "Datenbasis: Deutscher Wetterdienst" with a note of the change for changed data. The credit in the manifest carries both: "Weather data by Open-Meteo.com. Datenbasis: Deutscher Wetterdienst, aufbereitet von Open-Meteo", linked to open-meteo.com.
- **The public API.** [open-meteo.com/en/terms](https://open-meteo.com/en/terms): free for non-commercial use, under 10,000 requests a day, 5,000 an hour and 600 a minute, counted per address. Venti does not use it: not in production, not as a fallback, and not for the recorded fixtures (below).
- **The software.** AGPL-3.0-or-later, [on GitHub](https://github.com/open-meteo/open-meteo). It runs as a service of its own beside the server; none of its code is in this repository.
- **Running an instance.** Open-Meteo documents two ways ([getting started](https://github.com/open-meteo/open-meteo/blob/main/docs/getting-started.md), [remote data](https://github.com/open-meteo/open-meteo/blob/main/docs/remote-data.md), [sync](https://github.com/open-meteo/open-meteo/blob/main/docs/sync-command.md)), both from its container image `ghcr.io/open-meteo/open-meteo` and both fed from Open-Meteo's open data on AWS, `https://openmeteo.s3.amazonaws.com/data/`, which needs no key. *Remote data*: the instance reads the parts of the files it needs when it is asked, and keeps them in a cache on disk. *Sync*: a second process copies the files of chosen models and variables to disk every few minutes, for a box if one is given, and the instance answers from disk. What the owner has to set up is in [deploy/README.md](../deploy/README.md#forecasts).

#### What was tried, 2026-11-19

An instance was started from the image as built on 2026-11-03, in remote-data mode, and asked the adapter's own requests for five places. That is where the first recorded fixtures were from; they were recorded again in the same way, from the same image, on 2026-11-21, when the adapter began to ask for the heat the ground gives to the air (an answer was then 19.4 kB, for 72 hours of 45 values; the first request took 21 and 30 seconds for the two models, later ones for other places 8 seconds or a few milliseconds). They were recorded a third time that day at 09:25 UTC, when the adapter began to ask ICON-EU for four days, from the forecast service of the owner's installation, which is the same image in the same mode: an answer of ICON-D2 is 19.5 kB there, for 72 hours of 45 values, and one of ICON-EU 25.1 kB, for 96 hours of 44. `crates/sources-openmeteo/tests/fixtures/NOTICE.txt` says how. What the first try showed:

- **It works as documented**, including the two models under their names on the native grid, `dwd_icon_d2_native` and `dwd_icon_eu_native`, which Open-Meteo's bucket holds.
- **The run of a model is at `/data/<model>/static/meta.json`**, and only if the instance has `API_SYNC_APIKEYS` set. Without it the address answers 404, and then the server delivers no forecast: a series must say its run. With it set (to any value), the file is served without a key.
- **A first request for a place is slow in remote-data mode**: 25 to 60 seconds through the slow connection of the machine that tried it, and about 270 reads of 64 kB each from the bucket; a neighbouring place afterwards took under half a second, and a repeated one a few milliseconds. On a server with a direct connection the first request will be faster; *to verify* by how much. Sync mode has no such first request.
- **An answer is 19 kB**, for 72 hours of 44 values. The two answers about the runs are 494 bytes each.
- Open-Meteo's public API answered the same machine "Daily API request limit exceeded. Please try again tomorrow." for every forecast request that day: the address is shared with others. That is the limit working as its terms say, and one more reason not to build on it.

#### Several places in one request, tried 2026-11-21

Open-Meteo's `/v1/forecast` takes comma-separated lists in `latitude=` and `longitude=`: its [documentation](https://open-meteo.com/en/docs), read on 2026-11-21, says "Multiple coordinates can be comma separated" and "For multiple locations, elevation can also be comma separated". The rest below is what an instance did. An instance from the same image (digest `sha256:bf94e8b3…`, built 2026-11-03), in remote-data mode, was asked on 2026-11-21 between 03:18 and 03:45 UTC, while it held ICON-D2's run of 00 UTC. What it showed:

- **The answer is a JSON list**, one object per place in the order asked, each the same as the answer for that place alone: for four places of ICON-D2, the cell, the height of the ground and every hourly value were equal to four single requests made just before. Every object but the first carries `location_id`, its position in the list. A request for one place answers the object alone, as before.
- **`elevation=nan` takes one value per place.** One `nan` for four places is refused with `400` and `{"reason":"Parameter 'elevation' must have the same number of elements as coordinates","error":true}`; `elevation=nan,nan,nan,nan` is accepted, and a list that mixes `nan` and a height applies each to its place. The adapter sends one `nan` per place.
- **`cell_selection` is one value for all places.** A list (`nearest,nearest,…`) is refused (`400`, "Cannot initialize GridSelectionMode from invalid String value"). With `elevation=nan` the cells and values were the same for `nearest`, `land` and none given, for these four places, which are all cells of land; over a lake they are not (below, "A cell of water has no height"). The adapter keeps `nearest`.
- **Size, with three days asked of each model.** 77.4 kB for four places of ICON-D2 and 78.1 kB of ICON-EU; 485 kB for 25 places, 1.94 MB for 100. The whole grid of the first region, 1,817 points a tenth of a degree apart, was asked in 23 requests of 80 places: each answer 1.10 to 1.55 MB, 19.2 kB a place. The manifest's limit for one answer went from 512 kB, room for 25 places, to 2 MB; the adapter asks for 80. ICON-EU's whole grid was not asked for that night; on the owner's installation, later that morning, all 23 of its answers were read under that limit ([deploy/AGENTS.md](../deploy/AGENTS.md#what-has-run-where)).
- **Size, with four days asked of ICON-EU** (since 2026-11-21, [below](#what-a-series-holds)). ICON-D2's answers are as they were: 77.6 kB for the four recorded places. ICON-EU's have a third more hours: 100.7 kB for the four recorded places, and 1,981,505 bytes for the first 80 places of the first region's grid (the row at 46.3 N from 9.4 E on, and one place of the next row), 96 hours and 24.8 kB a place, measured on 2026-11-21 against the owner's installation. That is 5 % under the 2 MB of the limit (2,097,152 bytes), which is too little room for longer numbers elsewhere on the grid: the limit is 4 MB since that day (`max_response_kb = 4096`), twice the largest answer measured. The other 22 requests of ICON-EU's grid have not been measured with four days: *to verify* in the installation's log that each is read.
- **Time.** In remote-data mode, through this machine's slow connection, a first request for 80 places took 72 seconds, later ones for other places of the region 1.4 to 21 seconds, and a repeated request a quarter of a second. The fetcher gives a request two minutes. ICON-EU's four days for 80 places, asked of the owner's installation for the first time on 2026-11-21, took 121 seconds, because the instance read the model's files from the bucket; asked again, under a second. That first request is longer than the fetcher waits: on a forecast service that has just started, the map's first request of a model fails and the map waits a look, as that installation showed the same morning ([deploy/AGENTS.md](../deploy/AGENTS.md#what-has-run-where)). An instance in sync mode answers from its disk; *to verify* how long it takes there for 80 places.
- **How many places one request may name** was not tried beyond 100. *To verify* if the adapter is to ask for more.

The recorded answers for four places, of both models, are in `crates/sources-openmeteo/tests/fixtures/places/`.

#### What a series holds

Two models, each asked for by itself and delivered as a series of its own. Open-Meteo's "seamless" blends are not used.

| | ICON-D2 | ICON-EU |
|---|---|---|
| Cells | Triangles of 2.1 km mesh | Triangles of 6.5 km mesh, which the apps call "7 km cells" |
| Runs | Every 3 hours | Every 3 hours |
| Reach of a run | 48 hours | 120 hours from 00, 06, 12 and 18 UTC, of which the first 78 hour by hour; 30 hours from 03, 09, 15 and 21 UTC |
| Asked for | Today and the two days after, by the date in UTC (`forecast_days=3`) | Today and the three days after (`forecast_days=4`); until 2026-11-21 the two days after |
| At the instance | About 70 minutes after the run began (18 UTC run: 19:10; 06 UTC run: 07:09) | About 2 hours 50 minutes after a run of 03, 09, 15 or 21 UTC (15 UTC run: 17:50; 21 UTC run: 23:53), and 3 hours 14 minutes after the one run of the four others that was recorded (06 UTC run: 09:14) |

The reach is from Open-Meteo's downloader, [`IconDownloadDomains.swift`](https://github.com/open-meteo/open-meteo/blob/main/Sources/App/Icon/IconDownloadDomains.swift), read on 2026-11-19; the times at the instance are those of the runs that were recorded, on 2026-11-19, 20 and 21.

**The size of the cells** is DWD's own figure, read on 2026-11-21 on its page about the models ([dwd.de, Modellvorhersagedaten](https://www.dwd.de/DE/leistungen/modellvorhersagedaten/modellvorhersagedaten.html)): for ICON-EU "Die horizontale Maschenweite beträgt im nativen Modellgitter 6.5 km, im regulären Ausgabegitter 0.0625° (~ 7 km)", for ICON-D2 "2,1 km". The mesh is the side of a square as large as a cell. The centres of two triangles that share an edge lie closer than that, at about 0.88 of it: 5.7 km for ICON-EU, and 5.3 km between two neighbouring centres in the recordings. This page once took that distance for the size of the cells and said "about 5 km"; that was wrong. What DWD changed on 2026-10-06 was the area of ICON-EU's nest, which grew ("Erweitertes EU-Nest-Gebiet", 1.15 times as many grid points, [newsletter of 2026-07-28](https://www.dwd.de/DE/leistungen/opendata/neuigkeiten/opendata_juli2026_2.html)); the resolution raised that day was ICON-EU-EPS's, the ensemble's, from 13 to 10 km. The page with the 6.5 km carries no date: *to verify* against the definition of the new grid (number 62) that the figure still holds after that day.

**A series holds only the hours its run computed.** An instance keeps one value per hour, the newest it has. Asked for some days, it answers every hour of them that it has a value for: the hours before the newest run began are older runs', and so are the hours beyond a short run's reach. On 2026-11-19 at 19:25 UTC, asked for three days, the instance answered all 72 hours for ICON-EU, whose newest run was the one of 15 UTC that reaches 30 hours; the other hours came from the run of 12 UTC or an older one, and nothing in the answer says so. On 2026-11-21 at 09:25 UTC, asked for four days while it held ICON-EU's run of 06 UTC, it answered all 96 hours: the seven up to the hour the run began at, the 78 of the run, and eleven after them, which are the ones Open-Meteo fills in from values three hours apart (in the recorded answer for Tegernsee the sky of the last evening closes from 0 to 100 % between 18 and 21 UTC in two steps of a third). Of ICON-D2, whose run of 06 UTC it held, it answered the seven hours up to the one the run began at and the run's 48; the 17 hours after the run's end came empty. The adapter delivers the hours after the run began as far as that kind of run reaches, and no others. The hour the run begins at is left out as well: Open-Meteo's downloader skips gusts, precipitation and updraft for it, so those three would be the run before's.

**What is asked for.** Each model is asked for as many days as its longest run reaches into hour by hour, today by the date in UTC being the first: three for ICON-D2, whose 48 hours end on the day after tomorrow at the latest, and four for ICON-EU, whose runs of 00, 06, 12 and 18 UTC reach 78 hours, to 06, 12, 18 and 24 UTC of the third day after today. Until 2026-11-21 both were asked for three, and ICON-EU's hours of that third day after today were not fetched. One hour of a run is still beyond what is asked: the last of the run of 18 UTC, which is the midnight that ends the four days. ICON-EU's hours after the 78th, to the 120th, are three hours apart in the model and are not read.

**The gap this leaves.** The days asked for are not always covered:

- The hours of today before the newest run are never in a series.
- ICON-D2 ends 48 hours after its run, which is tomorrow or somewhere on the day after.
- ICON-EU reaches beyond that only from a run of 00, 06, 12 or 18 UTC: to the third day after today, and to the end of that day only from the run of 18 UTC. A run in between reaches 30 hours, to tomorrow or the night after it. By the times at the instance above, a run of those four is the newest the instance holds from 3 hours 14 minutes after it began until the next run is there, some 2 hours 40 minutes later (one such run was timed: *to verify* over more). So a forecast for a place has ICON-EU's hours of the day after tomorrow and of the day after that for less than half of every six hours.
- The thermal map does not lose those days in between: it keeps the judged hours of every run it has seen ([architecture.md](architecture.md#the-thermal-map)). A forecast for a place is one run's.

Closing it without guessing needs the run of every hour. Two ways are open: Open-Meteo's single-runs API, which answers for one named run (`&run=`), if a self-hosted instance can serve it (*to verify*; it needs each run kept whole); or the server remembering which runs it saw arrive, which it already notes for the health report.

**Between two runs.** Open-Meteo writes a new run into its files variable by variable and updates `meta.json` at the end. For some minutes an answer can hold values of the next run under the name of the one before. The server keeps an answer for half an hour at most. *To verify* how long that window is.

#### The values

All are the model's values for its own cell: the request asks for the nearest cell and switches off Open-Meteo's shift of temperatures to the height of the place (`cell_selection=nearest`, `elevation=nan`). The series carries the centre of the cell and the height of the model's ground there. East of the Tegernsee, whose shore lies at 725 m, ICON-D2's ground is at 1069 m and ICON-EU's at 1100 m.

**A cell of water has no height.** Found on 2026-11-21 in the first thermal map a server made of the real grid: ten of its 1,817 cells had their ground at 0 m, all of them over lakes (Lake Constance, the Chiemsee, the Ammersee, the Starnberger See, the Mondsee, the Traunsee, the Neusiedler See). The nearest cell of ICON-D2 to such a place is one the model counts as water, and Open-Meteo keeps no height for a cell of water: its file of heights marks it (-999), and the answer's `elevation` is then 0, for the sea and for a lake at 395 m alike ([`Gridable.swift`](https://github.com/open-meteo/open-meteo/blob/main/Sources/App/Domains/Gridable.swift), read on 2026-11-21: "-999 marks sea points", and `case .sea: return 0`). With a ground at 0 m every height above the ground was some hundred metres too low, and the thermals of the cell were estimated from the level of the sea. Since that day the adapter reads an `elevation` of exactly 0 as no height: the series has no `cell_elevation_m`, its values are handed on as the model's for the air over the water, and nothing is said of thermals there ([design.md](design.md#thermals)). On the map such a square has a verdict by the wind at the ground, the gusts and the rain, and is otherwise empty. What the model computes over the water is not what it computes for the shore: at 47.61 N 9.50 E over Lake Constance the instance answered 20.6 km/h of wind at noon, and 13.7 km/h for the cell of land beside it. So a take-off whose nearest cell is water gets the lake's air, and no thermals. *To do*, if a take-off turns out to lie so: ask for the nearest cell of land instead. Open-Meteo's `cell_selection=land` is not the way while its shift of temperatures is switched off: asked with `elevation=nan` for the same place, the instance (the image of 2026-11-03) answered the values of the cell of land beside the lake under the coordinates of the cell of water, with a height of 0 and temperatures 2.3 K higher, which is the land's temperature shifted down to the level of the sea.

| Value | From | What it is, and what it is not |
|---|---|---|
| Wind at 10 m, speed and direction | `U_10M`, `V_10M` | Open-Meteo computes speed and direction from the two components |
| Gusts at 10 m | `VMAX_10M` | The strongest in the hour before |
| Temperature at 2 m | `T_2M` | |
| Dew point at 2 m | `T_2M`, `RELHUM_2M` | Computed by Open-Meteo from temperature and relative humidity. ICON's own dew point is not used by it |
| Cloud cover, total and low, mid, high | `CLCT`, `CLCL`, `CLCM`, `CLCH` | Where "low" ends is ICON's definition |
| Precipitation | `TOT_PREC` | Rain, showers and snow of the hour before, as water |
| CAPE | `CAPE_ML` | Of a parcel mixed over the lowest layer. It says whether showers and storms can grow, not how good thermals are |
| Freezing level | `HZEROCL` | Metres above sea level. Open-Meteo keeps it in steps of 10 m. Where the air at the ground is at about 0 °C or colder the value lies under the model's ground and is no height in the air over the place: in the series recorded on 2026-11-21 at 09:25 UTC, ICON-D2 has between 1060 and 910 m in the 16 such hours of a cell whose ground is at 1069 m, at 0.2 to -2.3 °C, and ICON-EU as little as 470 m in a cell at 1100 m, at -9 °C. An earlier recording had 1050 to 1090 m in three such hours of that cell of ICON-D2, and this page then said that ICON writes the height of its ground where the whole column is below 0 °C; the later recording does not bear that out. What ICON writes there is *to verify* in DWD's description of the field |
| Updraft, ICON-D2 only | `W_CTMAX` | The strongest rising air anywhere in the column up to 10 km, in the hour before. It is what a model with 2 km cells resolves: showers, storms, flow over mountains. **It is not thermal strength.** In the series recorded on 2026-11-21 for five places, two days with precipitation and no storm (CAPE at most 10 J/kg), the values are 0.02 to 0.74 m/s |
| Convective cloud base and top | ICON-D2: `HBAS_SC`, `HTOP_SC`. ICON-EU: `HBAS_CON`, `HTOP_CON` | Not the same thing in the two models. ICON-D2 computes deep convection itself and estimates only shallow convection, and these are the base and top of that. ICON-EU estimates all convection. ICON writes -500 m for "none"; in every recording the instance handed that on for the hours of ICON-EU's run and wrote 0 for ICON-D2 (and 0 in some of the hours it fills in after the 78th of a run of ICON-EU, which are not read); the adapter reads both as no value. Whether the heights are above sea level is *to verify* on DWD's description of the fields; the recorded values fit that |
| Heat the ground gives to the air | `ASHFL_S` | ICON's sensible heat flux at the ground, which the model keeps as an average since the run began and Open-Meteo turns into the mean of each hour ([`IconVariable.swift`](https://github.com/open-meteo/open-meteo/blob/main/Sources/App/Icon/IconVariable.swift), read on 2026-11-21: "Sensible heat net flux at surface (average since model start)"). **The model counts it towards the ground** and Open-Meteo hands it on so: an instance asked on 2026-11-21 answered negative values under the midday sun and positive ones in the night, when the air warms the ground. The adapter turns the sign, so that the record's `heat_to_air_w_m2` is positive when the ground warms the air. Open-Meteo keeps it in steps of about 7 W/m². In the series recorded on 2026-11-21 for five places, four days of late November, it stays between -56 and +97 W/m²; a summer noon gives a few hundred. *To verify* in DWD's own description of the field that the sign is as observed |
| Wind and temperature at 80, 120 and 180 m | `U`, `V`, `T` on model levels | The third, fourth and fifth level from the ground; the heights are nominal. Open-Meteo's comment gives 78 m and 126 m for the two lowest of ICON-D2 |
| Wind, temperature and height on 950, 850, 700, 600, 500 hPa | `U`, `V`, `T`, `FI` | Both models compute these five surfaces. The height is the geopotential divided by 9.80665. A surface under the model's ground is not delivered: at Tegernsee the 950 hPa surface lay at 480 to 593 m in the recorded hours of ICON-D2, under a ground at 1069 m, and the instance answered it with the wind at 10 m, to within 0.1 m/s |

Not asked for: the surfaces of 925, 900 and 800 hPa, which ICON-D2 does not compute and Open-Meteo fills from their neighbours; and anything "seamless". Answered and not read: ICON-EU's hours after the 78th of a run, which Open-Meteo fills from values three hours apart, where the four days asked for hold some.

**What a soaring forecast needs, and what is made of it:**

- **Boundary-layer height.** DWD publishes the mixing layer height (`MH`) for ICON-D2 and ICON-EU, since September 2026 on the native grid. Open-Meteo does not take it. Venti works out how high thermals reach itself, from the temperature at the ground and at the levels above it ([design.md](design.md#thermals)).
- **Thermal strength.** No field of either model is a climb rate a glider sees. Since 2026-11-21 Venti derives one in the flight engine, from that height and the heat the ground gives to the air: the convective velocity scale, less a glider's sink. It is an estimate with known weaknesses, written down where it is described ([design.md](design.md#thermals)). The forecast for a place hands on the model's figures only, and the apps make the estimate from them; the server makes the same estimate for the cells of its thermal map ([architecture.md](architecture.md#the-thermal-map)).
- **Wind on model levels above 180 m.** Between 180 m above the ground and the pressure surfaces, which are 1.5 km apart, there is nothing. In the mountains that is where pilots fly.
- **A cloud base for ordinary cumulus.** ICON-D2's value is for the shallow convection it estimates and is often absent on a day with cumulus the model resolves or misses.

These need DWD's own files, read on the native grid: the "DWD ICON-D2 raw GRIB" row above.

#### What DWD's change of 2026-11-30 means

DWD's [announcement](https://www.dwd.de/DE/leistungen/opendata/neuigkeiten/opendata_september2026_1.html), read on 2026-11-19: after 30 November 2026 the old addresses of ICON, ICON-EU and ICON-D2 are switched off. The new ones (`opendata.dwd.de/weather/nwp/v1/m/<model>/p/<field>/…`) carry the same fields, reach and hours, on the original triangular grid only: ICON-D2 loses its second delivery on a regular grid and ICON-EU changes grid. Visibility is dropped, and the mixing layer height is there on the triangular grid for the first time.

- **For Venti's forecasts: nothing to do.** Venti does not read DWD's files. Open-Meteo does, and its downloader reads the new addresses already; the two `_native` models the adapter asks for are the ones made from them. Open-Meteo's page says its models without `_native` in the name "will use native grids" after that day. Whether the `_native` names stay beside them is *to verify* on 1 December.
- **For the instance:** it has to be an Open-Meteo version that knows the native grids. The image of 2026-11-03 does.
- **For a pilot:** values are for a triangle about 2 km across, not for a square of a regular grid, and the series says where its centre is.
- **For a raw ICON adapter, later:** it has to read the triangular grid, and it gets the mixing layer height.

### Radar and lightning

| Source | Provides | Licence | Status |
|---|---|---|---|
| DWD radar composite RY | A rain rate on a 1 km grid over Germany every five minutes, from DWD's 17 radars, as HDF5; 150 km around each radar, so in the first region Bavaria and the north of Tirol and Vorarlberg, and weak behind the first chains of the Alps | CC BY 4.0, read on DWD's own page on 2026-11-21; see below | **Built** as `dwd-radar-ry`, a layer of its own |
| GeoSphere Austria nowcast (INCA), `nowcast-v1-15min-1km` | A nowcast on a 1 km grid over Austria and around it, a run every quarter of an hour, 13 steps of a quarter of an hour each (three hours); Venti takes the first step's precipitation sum `rr` alone, as an analysis of the rain that fell (*to verify*, see below). No radar composite of GeoSphere Austria is open | CC BY 4.0, read on the dataset's own page on 2026-11-21; see below | **Built** as `geosphere-inca-rr`, the rain radar's second layer |
| RainViewer | Radar tiles | Free for small community use with credit; limits tightened in 2026 | Open with limits |
| Blitzortung | Lightning | Use in apps needs written permission | Ask |

No open real-time lightning source was found.

**DWD's composite, as read on DWD's own pages and in its files on 2026-11-21:**

- **Licence.** DWD's [legal notice](https://www.dwd.de/DE/service/rechtliche_hinweise/rechtliche_hinweise_node.html): "Alle frei zugänglichen Geodaten und Geodatendienste sowie die als hochwertige Datensätze / high value datasets (HVD) festgelegten Leistungen des DWD dürfen unter den Bedingungen der Lizenz Creative Commons BY 4.0 (CC BY 4.0) unter Beigabe eines Quellenvermerks weiterverwendet werden. Zu den Geodaten zählen alle bereitgestellten meteorologischen Wetter- und Klimainformationen mit Ortsbezug." Its [open data page](https://www.dwd.de/DE/leistungen/opendata/opendata.html) names radar data among what the open-data server holds, points to that notice as the terms of use, and says that no level of service or availability is promised for `opendata.dwd.de`.
- **Credit.** DWD's [page on source credits](https://www.dwd.de/DE/service/rechtliche_hinweise/vorlagen_quellenangabe.html): the credit stands "unmittelbar an der verwendeten DWD-Information"; unchanged data, extracts and a changed format are credited "Quelle: Deutscher Wetterdienst"; for data changed further DWD expects at least to be named, and gives as wording of a notice of change, among others, "Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben". Venti lays the composite on another grid and turns its values into six steps, which is more than a change of format, so the manifest's credit is that sentence. *For the owner:* whether to ask DWD if "Quelle: Deutscher Wetterdienst" would do as well; nothing depends on it.
- **Which product.** The open-data server has two folders of composites. `weather/radar/composite/` holds DMAX, HG, HX, HymecNG, PG, RS, RV, VII and WN; `weather/radar/radolan/` holds RW, RY, SF and YW. RY is the one built: "qualitätsgeprüfte Radardaten nach Anwendung der verfeinerten Z-R-Beziehungen in Niederschlagshöhen umgerechnet", every five minutes ([format description](https://www.dwd.de/DE/leistungen/radolan/radolan_info/radolan_radvor_op_komposit_format_pdf.pdf), version 2.6 of 2025-01-14). A file is named `raa01-ry_10000-YYMMDDhhmm-dwd---bin.hdf5`, 50 to 435 kB in the two days the folder held (the more rain over Germany, the larger), and the folder keeps two days of them, 576 files, with the newest once more as `…-latest-…`. RV is the same analysis with a nowcast of two hours in 25 files, a tar of 3 to 12.6 MB every five minutes; HX and WN are reflectivity; YW is RY adjusted to gauges and was not looked at. PG and HG are empty folders.
- **The format.** Since a migration that began in 2023 the RADOLAN products are HDF5 by EUMETNET's ODIM_H5 2.3, on the WGS84 ellipsoid, 1200 km by 1100 km (the same format description, 1.3). A recorded frame: a superblock of version 0, object headers of version 2, one dataset `/dataset1/data1/data` of 1200 rows by 1100 columns of unsigned 16-bit integers in one chunk packed with deflate, and its meaning in attributes. `quantity` is `RATE`; `gain` 0.01, `offset` 0; `nodata` 65535 for what no radar saw (627,073 of the 1,320,000 pixels, 47.5 %) and `undetect` 0 for what was seen without an echo.
- **The unit** is taken as millimetres an hour. ODIM defines `RATE` as a rain rate, and its own example of one is "0.1–0.5 mm/h"; the recorded frames agree: their smallest value that is not zero is 12, which is 0.12 mm/h, the 0.01 mm in five minutes that was the old format's smallest step; and the largest value of that morning of light rain is 24.9, which is a rate one can believe by the hour and not an amount in five minutes (the frame of 2026-11-19 12:35 UTC, rain over much of Germany, has 72 at its most). DWD's description still calls the values of RY "Niederschlagshöhen", amounts, as it did when they were hundredths of a millimetre in five minutes. If that were still so, every rate here would be twelve times too low. *To verify*: against DWD's own rain gauges of the same minutes, or by a question to DWD (opendata@dwd.de); and the unit in ODIM's table of quantities, which could not be read out of the PDF here.
- **The time.** The file named `…2611211315…` says of itself that it is of 13:10:00 to 13:15:00 UTC with the nominal time 13:15:00, and the server dated it 13:15:17: a frame is published some twenty seconds after the five minutes it is of. Venti dates a frame by the end of its interval.
- **The grid.** A frame's `/where` gives a polar stereographic projection as a PROJ definition (`+proj=stere +lat_0=90 +lat_ts=60 +lon_0=10 +a=6378137 +b=6356752.3142451802 +no_defs +x_0=543196.83521776402 +y_0=3622588.8619310018 +units=m`), a pixel of 1000 m, and the four corners in degrees, from 45.68° to 55.86° N. The adapter reads all of it from each frame and refuses a frame whose corners do not lie where its projection and size put them. Two checks that it reads them rightly: the four corners land on the grid's corners within a millimetre; and the edge of what is seen lies 150 km from the radars at Isen, Feldberg, Boostedt, Prötzel, Borkum and Rostock, whose places the format description gives, with the rows read from the north as ODIM says and only so.
- **What it sees in the first region.** 150 km around Feldberg, Memmingen, Isen, Türkheim and the others: to 46.8° N on Isen's meridian. Of the first region's box 57 % is not seen in the recorded frames: everything east of about 14° E and south of the main ridge. The composite's own grid reaches to Vienna; no German radar does. Inside the range the beams are blocked behind the first chains, which the product does not mark: such a valley reads as dry. That is the limit the catalogue's "weak behind the main Alpine ridge" means, and the apps say it in words.

**GeoSphere Austria's nowcast, as read on GeoSphere Austria's own pages and in its files on 2026-11-21** (the addresses are in the adapter's manifest and in `crates/sources-geosphere-radar/tests/fixtures/NOTICE.txt`):

- **Licence and credit.** The [dataset's page](https://data.hub.geosphere.at/dataset/nowcast-v1-15min-1km) names "Creative Commons Namensnennung 4.0 International". The Data Hub's [legal page](https://data.hub.geosphere.at/legal) asks for the credit "Datenquelle: GeoSphere Austria - https://data.hub.geosphere.at"; the manifest carries it in English, as the station adapter's does. The data is changed before it is shown (laid on another grid, a sum turned into steps of rate), which CC BY 4.0 asks to say with the credit.
- **Which product, and what is to verify.** No radar composite is open. The dataset is INCA's nowcast; the adapter takes the first step of each run, the step at the run's reference time with a lead time of 0 hours, which is the sum of the quarter of an hour ending then. *To verify* with the provider: that this first step is INCA's analysis, rain as measured by radars and gauges and put together, and not already a forecast. The dataset's page calls the dataset a forecast and does not say what its first step is. The later steps are not used.
- **The service.** `https://dataset.api.hub.geosphere.at/v1/grid/forecast/nowcast-v1-15min-1km`: `/metadata` lists the six runs held, the newest first; a frame is asked for with `forecast_offset`, the run's place in that list, `start` and `end` at the run's time, a box and `output_format=netcdf`. The run of 12:15 UTC was listed between 12:36:51 and 12:37:21: some 22 minutes after its quarter of an hour; the run of 13:00 was in the list at 13:17:11, 17 minutes after. The place shifts when a run arrives between the list and the frame, so the adapter reads the file's own time and lead time and refuses a file of another moment. The guide calls the forecast endpoints "prerelease".
- **The limit.** 5 requests a second and 240 an hour for an address, shared with `geosphere-tawes`: the manifests give the stations 4 and 120, this source 1 and 100.
- **The format.** netCDF-4, which is HDF5: a superblock of version 2; `rr` as 1 by 263 by 608 signed 16-bit integers (the first region's box widened by 0.05°) in one chunk packed with shuffle and deflate, `scale_factor` 0.01, `_FillValue` and `missing_value` -999, in `kg m-2`; `lat` and `lon` as floats for every point; `x`, `y`, `time` and `leadtime` stored plainly. The reader is `crates/hdf5`, the one DWD's files are read with. A frame of the first region's widened box is 426 kB.
- **The values.** Hundredths of a millimetre in a quarter of an hour; the rate in mm/h is four times the sum. The recorded frames have a value at every point (none is -999), the largest 2.38 mm in the quarter of an hour.
- **The grid.** EPSG:31287 (MGI / Austria Lambert), 1 km, the whole of it 701 by 431 points; the file's `crs_wkt` gives the projection and the datum's shift to WGS84. With that shift the adapter's projection is within 0.95 m of the degrees the file gives for each point; without it 53 to 116 m off. The rows run from the south.
- **The box.** The service gives the rows and columns of the grid that reach into the box asked for. With the first region's own box, 107 cells in the middle of the map's last row lay south of that rectangle and were "not seen" (a recording of 2026-11-21, 12:39 UTC, since replaced). The adapter asks for the box widened by 0.05° on every side, and the recorded frames are answers to that request (13:17 UTC on the same day): 608 by 263 points, 159,904 values, and every one of the 162,864 cells of the first region's map has a value.
- **What it sees in the first region.** All of the box in the recorded frames (162,864 of 162,864 cells), against 42.9 % for DWD's radars. That a point has a value does not say a radar saw it: far from the Austrian radars and gauges the analysis has less to go on, and the product does not say where.

**What the catalogue had wrong before 2026-11-21:** it called GeoSphere Austria's product a "15-minute analysis and three-hour nowcast" as if an analysis were published by itself. What is published is the nowcast, whose first step Venti takes for the analysis, *to verify*. And of DWD's: it named no product; the composite it described as "5-minute precipitation" is RY, which no longer comes in the RADOLAN binary format that descriptions of it give but as HDF5; and its values are a rate, not an amount.

### Airspace

| Source | Provides | Licence | Status |
|---|---|---|---|
| DFS AIP data sets | Official German airspace as AIXM, per AIRAC cycle, no key | CC BY 4.0 for the airspace data sets, stated on DFS's own pages and read there on 2026-11-19; see below. Not confirmed by letter | Open after confirmation. Not used; no adapter |
| DAeC OpenAir file | Germany including glider sectors | No licence stated | Ask |
| Austro Control | Austrian airspace as OpenAir and KML | Own terms; re-serving not addressed | Ask |
| openAIP | Worldwide airspace, airports, obstacles, hotspots. Airspace per country as GeoJSON and OpenAir files, rebuilt daily, no key | CC BY-NC 4.0, verified 2026-11-19, with a share-alike condition in openAIP's own terms; see below | **Built** for the airspace of the eight countries the first region's box takes in: Germany, Austria, Switzerland, Italy, Slovenia, Croatia, Hungary and Slovakia. Non-commercial, kept as a separate layer |
| XContest airspace service | Airspace curated for paragliding with activation status | No published terms | Ask |

**Activation times are the gap.** There is no open machine-readable feed of which restricted areas are active today, in either country. XContest maintains one by hand. Austria publishes a daily plan as a PDF. Until this is solved, the app shows restricted areas as potentially active and says so.

**openAIP, as read on its own pages on 2026-11-19:**

- *Licence.* The home page and the footer of every page: "OpenAIP data is licensed under the Attribution-NonCommercial 4.0 International (CC BY-NC 4.0) license." The legal page ([openaip.net/legal](https://www.openaip.net/legal)) adds terms of use: the data must be attributed so that "it must be clearly visible to any user that the used data is openAIP data or is based on openAIP data"; it may not be sold on its own but "you may ship openAIP data with paid/commercial applications as long as you don't exclusively sell openAIP data"; and "If you alter, transform, or build upon openAIP Data, you may distribute the resulting work only under the same or similar license to this one." That last sentence is a share-alike condition, which CC BY-NC 4.0 itself does not contain. The manifest declares the stricter reading. **For the owner to decide:** whether to ask openAIP which of the two holds.
- *Credit.* The wording openAIP offers, used as the manifest's credit line: "Data used comes from openAIP and is licensed under the Creative Commons Attribution-NonCommercial 4.0 International License. Visit https://www.openaip.net and contribute to better aviation data, free for everyone to use and share."
- *Warranty.* "OpenAIP data is not certified and must not be used for primary navigation or flight planning. Never rely on openAIP data! OpenAIP data contains errors. Using openAIP data may result in serious injury or death." (read again on 2026-11-19, in the script that draws [openaip.net/legal](https://www.openaip.net/legal); the page has no text without it). The first three sentences are the manifest's `notice`. The server sends it with every answer that carries openAIP's airspace, and the app must show it wherever it shows this data.
- *Access without a key.* The documentation ([openaip.net/docs](https://www.openaip.net/docs)): "The export files are stored in the public S3 object storage bucket openaip-system-exports and can be downloaded anonymously via HTTPS from: https://storage.openaip.net/openaip-system-exports/ ... The export files are created on a daily basis ... without authentication or credentials. Each file is available at `https://storage.openaip.net/openaip-system-exports/<filename>`, where the filename follows the pattern `<country>_<type>.<extension>`." Only the API needs an account and a key; the adapter does not use it. The owner has nothing to sign up for.
- *Format.* The adapter reads `<country>_asp.geojson`. It carries more than the OpenAir exports (hours, NOTAM and on-request flags, the period of temporary areas, a second floor, remarks, change times, a stable identifier) and gives corners in decimal degrees where the OpenAir exports round to whole seconds. The numbers in it are explained by openAIP's [schema](https://api.core.openaip.net/api/schemas/response/airspace/airspace-schema.json).
- *To verify:* whether the hours of operation are UTC or local; what tells "on demand" from "on request".
- *Countries.* The adapter fetches one file for each country in its manifest: Germany, Austria, Switzerland, Italy, Slovenia, Croatia, Hungary and Slovakia, which are all the countries the first region's box reaches into that openAIP has a file for. The list was made from the box and checked against the data: every one of the eight has airspace inside it, and Czechia, which an earlier version of this document listed, has none. Another country is one line in the manifest and a recorded fixture. The same licence covers all of openAIP's data, so nothing new had to be verified. The API's answer names the countries (`countries`).
- *Gap: Liechtenstein.* It lies in the box and openAIP has no file for it. The Swiss file's records lie over it: the Swiss flight information region, the control area above the Alps, a glider area. That fits Liechtenstein's airspace being published with Switzerland's; whether anything over it is missing from openAIP is *to verify* against the Swiss AIP. It is not named in `countries`.
- *Gap: one run, eight files.* A run counts only if every file arrived, so one country's file missing for a night keeps all eight countries' airspace a day older. That is the rule that no half set replaces a whole one; keeping a set per country would confine it to the country concerned and is not built.
- *Gap: the warning table.* It has rules of its own for Germany and Austria only. What a pilot gets in the other six countries, and what of it was checked, is in [airspace.md](airspace.md#outside-germany-and-austria).
- *Not whole countries.* The server keeps the airspace that reaches into its regions' box: German airspace north of 48.5° N is not in it. Since 2026-11-19 the API's answer says so: every layer names the box inside which the server's set is whole (`covers`), and a layer is in the answer also when it has nothing in the box asked for. An app reads "no airspace here" only for a place inside that box, and says everywhere else that its airspace does not reach there. See [api.md](api.md#where-a-layer-is-whole).

**DFS, as read on its own pages on 2026-11-19, not used:** [aip.dfs.de/datasets](https://aip.dfs.de/datasets/) states under "Copyrights and terms of use": "Aeronautical data originated by DFS is published under the Creative Commons Attribution 4.0 International licence (CC BY 4.0). This means that the data may be used for a wide range of purposes, including commercial purposes, provided DFS is credited as the rights holder." Data originated by airport operators is CC BY-NC 4.0, and for data from other originators the originator must be asked. The [terms document](https://aip.dfs.de/datasets/documentation/Nutzungsbedingungen_EN_Website.pdf) linked there lists the licence per data set, among them "Airspace ReferencedBorders: CC-BY-4.0" and "Airspace StrokedBorders: CC-BY-4.0", and adds: "The specific licence terms will be documented shortly in the metadata for the data sets, but the data sets may already be used as described above." So the statement can be verified in writing on the provider's own page. **For the owner to decide:** whether that page is confirmation enough or the letter in [Outreach](#outreach) is still wanted. Not checked: whether the airspace data set contains glider sectors, and which of its parts come from other originators.

### Obstacles and cables

| Source | Provides | Licence | Status |
|---|---|---|---|
| Land Tirol "Flughindernisse" | About 2,000 cable spans hazardous to air-sport pilots: material ropeways, power lines, tourist installations | Land Tirol open data, CC BY 4.0 *(to verify for this dataset)* | Open after confirmation |
| DFS and Austro Control obstacle data sets | Tall obstacles, AIXM | CC BY 4.0 reported for DFS; Austro Control not stated | Open / ask |
| OpenStreetMap | Aerialways and power lines | ODbL | Open; incomplete for exactly the dangerous hay and material ropeways |

### Flying sites

| Source | Provides | Licence | Status |
|---|---|---|---|
| OpenStreetMap free-flying tags | Takeoffs and landings, with the outline of those mapped as areas and what their mappers wrote about them, asked for through an Overpass instance: one request a day for a region's box | ODbL 1.0, verified 2026-11-19; share-alike, kept as a separate layer | **Built**. Coverage measured: thin, see below |
| ParaglidingEarth | Worldwide sites, variable quality | CC BY-SA 3.0, newer contributions also ODbL | Open, share-alike, separate layer. Not used |
| **DHV site database** | The authoritative list of approved sites for Germany, also covering Austria; bulk files exist | No terms published | **Ask**. Not used |
| Our own community edits | Take-offs and landings that pilots make on Venti, each with its history | CC0 1.0, the owner's decision of 2026-11-21; given by the pilots themselves, so there is no provider's page to verify and no manifest | **On the server since 2026-11-21** as the layer `venti`, apart from every source in this table; the core reads it for the apps, which do not write to it yet. It is no adapter: [data-platform.md](data-platform.md#take-offs-and-landings-made-by-pilots) |

**OpenStreetMap, as read on its own pages on 2026-11-19:**

- *Licence.* [openstreetmap.org/copyright](https://www.openstreetmap.org/copyright): "OpenStreetMap is open data, licensed under the Open Data Commons Open Database License (ODbL) by the OpenStreetMap Foundation (OSMF). In summary: You are free to copy, distribute, transmit and adapt our data, as long as you credit OpenStreetMap and its contributors. If you alter or build upon our data, you may distribute the result only under the same license." The last sentence makes it share-alike: sites from OpenStreetMap are a layer of their own and are merged with no other source's records.
- *Credit.* The Foundation's [attribution guidelines](https://osmfoundation.org/wiki/Licence/Attribution_Guidelines): "Attribution must be to 'OpenStreetMap'. Attribution must also make it clear that the data is available under the Open Database License. This may be done by making the text 'OpenStreetMap' a link to openstreetmap.org/copyright"; "The historical forms of attribution '© OpenStreetMap contributors' or '© OpenStreetMap' are acceptable"; the credit must be visible without interaction. For data passed on as data, which is what the API does: "You must include attribution to OpenStreetMap and either the text of the ODbL or a link to it as part of the database". The manifest's credit line is "© OpenStreetMap contributors", linked to the copyright page, and its `notice` names the licence with its address, so both travel with every answer.
- *The tags.* [Tag:sport=free_flying](https://wiki.openstreetmap.org/wiki/Tag:sport%3Dfree_flying), an approved scheme: `free_flying:takeoff`, `:landing`, `:towing`, `:training` with `yes`, `no`, `discouraged` or `permissive`; `direction` for "in which wind directions this site is usable"; `ele` for the height; `free_flying:official`, `:unsafe`, `:paragliding`, `:hanggliding`. The page lists `free_flying:site=takeoff|landing|towing|training` and `free_flying:site_orientation` as deprecated. In the first region nearly every element still carries the deprecated keys: 382 against 25.
- *Directions.* [Key:direction](https://wiki.openstreetmap.org/wiki/Key:direction): degrees from 0, clockwise from north, or one of the sixteen compass points "using up to three uppercase characters [NWSE]"; a range's "sector is calculated clockwise from the given range border values". [Key:ele](https://wiki.openstreetmap.org/wiki/Key:ele): "Elevation of a point above sea level, in metres."

**How the data is fetched.** The adapter sends one query in Overpass QL to an Overpass API instance, as a GET, for the box of the run. Since 2026-11-21 it asks for the tags and the geometry of every element (`out tags geom qt`, before that `out tags center qt`), and the answer for the first region is 276 kB against the 4 MB the manifest allows. Which instance is a setting of the server (`overpass_url`), as the operator of the main one recommends: "Die URLs unserer Services sollten nicht in der App fest einprogrammiert werden". The default is that main instance, `overpass-api.de`, run by FOSSGIS e.V. What its operator asks, read on the [OpenStreetMap wiki's list of instances](https://wiki.openstreetmap.org/wiki/Overpass_API#Public_Overpass_API_instances), in the [Overpass manual](https://dev.overpass-api.de/overpass-doc/en/preface/commons.html) and in [FOSSGIS's terms](https://www.fossgis.de/arbeitsgruppen/osm-server/nutzungsbedingungen/):

- "If you set something up that uses the Overpass API regularly, then [...] less than 100 queries fetching less 10 MB of data per day [is] fine." The server sends one query a day per source, and the manifest's budget allows a run two requests.
- "No parallel running of multiple scripts", and scripts "sind auf eine Verbindung zu beschränken". The manifest allows one request at a time.
- A User-Agent that identifies the application. The server sends `venti-server/<version>`. FOSSGIS also asks that an operator's e-mail address be easy to find on the application's website: **for the owner**, with the contact line that is already open in [plan.md](plan.md#needed-from-the-owner).
- "Commercial use should use self-hosted or paid Overpass servers." The project is not commercial.
- A link to `https://www.openstreetmap.org/fixthemap`, "um Ihren Nutzern zu erlauben, Fehler in unseren Daten zu melden". It is in the manifest's `notice`, and every site record carries the address of its own element.
- "Nowadays this server is overloaded [...] do not expect high reliability." A run that fails changes nothing and is tried again a day later.

Regional extracts (Geofabrik) were the alternative. Bavaria and Austria are some 1.5 GB of download a day and need a reader for the PBF format, to keep a few hundred points: not chosen.

**Recorded from another instance.** Neither `overpass-api.de` nor `overpass.private.coffee` accepted a connection from the machine the fixtures were recorded on (2026-11-19: the first reset it, the second timed out; 2026-11-21: the status page of the first timed out, and no query was sent to it). The answer in the repository is from "Overpass Europe", `ovp.dresel.systems`, which the wiki lists as "Free to use, no API key" with Geofabrik's extract of Europe, updated nightly. It was recorded anew on 2026-11-21 at 11:45 UTC with one request, sent by the server's own `ingest` command with `--archive`, so with the adapter's query and the server's User-Agent. It took 53 seconds for the first region on 2026-11-19 and 22 on 2026-11-21, and the server's fetcher gives a request 120. *To verify* on the first installation: that the server's own run against `overpass-api.de` works from where the server stands, and how long it takes.

**Coverage, measured for the first region** (46.3 to 48.5° N, 9.4 to 17.2° E; OpenStreetMap as of 2026-11-20 21:21 UTC, and the same in every number of the first twelve rows as of 2026-11-18; the test `the_first_region_holds_what_was_measured` keeps these numbers):

| | |
|---|---|
| Elements that carry the tags | 517: 307 points, 209 areas, 1 relation. 48 of them lack `sport=free_flying` and are found by the scheme's own keys |
| Schools, clubs and shops that name no use, left out | 15 |
| Sites | 502 |
| Take-offs | 252, of them 207 with a name, 51 with a height |
| Landings | 142, of them 115 with a name, 21 with a height |
| Both at once | 2 |
| Training slopes | 9 |
| Winch sites | 0 |
| Places that carry the tags and say no use | 101: kept without a use. Many are called "Landeplatz ..."; nothing guesses a use from a name |
| Take-offs with wind directions the adapter reads | 93 (37 %) |
| Wind directions tagged and left out | 34 elements: 19 written with an `O`, which is east in German and west in Italian and French; 7 ranges under the deprecated key, which defines none; 6 ranges that span more than half the compass when read clockwise, as the key defines them, and are far more likely written the wrong way round (`S-SE`); 2 that are no compass point |
| Marked official / not official | 96 / 15 |
| Marked as not for paragliders / not for hang gliders | 4 / 8 |
| Marked unsafe or discouraged | 3 |
| With a web page | 107 (21 %): 55 take-offs, 38 landings |
| With a description | 43 (9 %) |
| That say whether a fee is asked | 18: 17 `yes`, 1 `no` |
| That say whether guests have rules to read | 34: 13 yes, 21 no |
| That name a time without flying | 8, of 23 that carry the key: 15 say `no` |
| With an outline | 205 (41 %): 82 of the 142 landings, 91 of the 252 take-offs. Every site that is a closed way has one |

That is thin. The first region holds the busiest flying sites of two countries, and well-known ones are missing or bare. Looked for by name on 2026-11-19 and not found as a take-off: Wallberg (two unnamed ramps "vermutlich für Drachenflieger", no paraglider take-off, no landing), Brauneck (three take-offs without a name, found by their position), Tegelberg, Hochries by name (its take-offs are "Startplatz West" and "Startplatz Nord"), Rauschberg, Osterfelder, Hochgrat, Buchenberg, Hörnle, Laber, Herzogstand, Jochberg, Hohe Salve, Zettersfeld, Emberger Alm, Schöckl, Gaisberg, Stoderzinken, Krippenstein, Hochkar, the Nordkette above Innsbruck, Rofan, Penken, Wildkogel. Found: Kampenwand, Unternberg, Nebelhorn, Wank, Breitenberg, Unterberghorn, Bischling, Elfer and Kreuzjoch in the Stubai valley, Choralpe, Gerlitzen, Niedere, Planai. A search by name misses sites that are in the data under another name, so the list of the missing is an upper bound. How many sites there are in truth was not counted; the DHV's database is the reference and may not be used.

What follows: the layer is useful where it has data and must never be shown as complete. A pilot who knows a missing site can add it to OpenStreetMap, and it is in the app a day later. Whether to seed OpenStreetMap ourselves, and what from, is **for the owner**; the DHV's data may not be copied into it either.

**More about a site, and areas with their outlines** (recorded and counted on 2026-11-21; the notice beside the recording, `crates/sources-osm/tests/fixtures/NOTICE.txt`, has the query, the instance and the sizes).

*What was checked first, on the wiki's own page.* [Overpass QL](https://wiki.openstreetmap.org/wiki/Overpass_API/Overpass_QL), read on 2026-11-21 in the page's own text, says of the `out` statement: "One of the following modificators for geolocated information; default is to print none", and lists `geom` ("Add the full geometry to each object. This adds coordinates to each node, to each node member of a way or relation, and it adds a sequence of "nd" members with coordinates to all relations."), `bb` ("Adds only the bounding box of each element to the element. [...] For ways it is the enclosing bounding box of all nodes.") and `center` ("This adds only the centre of the above mentioned bounding box to ways and relations. Note: The center point is not guaranteed to lie inside the polygon"). So it is one of the three: an answer asked for with `geom` has the outlines and no centres, and the centre it had before was the middle of the box around an area. The recorded answer bears both out: each of its 209 ways has `bounds` and `geometry` and no `center`. Of `out tags` the page says "Print only ids and tags for each element and not coordinates or members", and with `geom` beside it the answer has the coordinates all the same; the one relation came with its `bounds` and without its members.

*What the adapter does with it.* A point is where it is. An area is at the middle of the box around it, reckoned as Overpass reckons its centre, so that no site moved: all 517 elements of the recording of 2026-11-19 are in the new one, and each of the 502 sites has the position it had, to the last digit (compared when it was recorded, with the adapter itself). The arithmetic matters in the seventh decimal: where the two edges of a box are an odd number of ten-millionths of a degree apart, the middle falls between two steps, and which one Overpass writes follows from how it holds a latitude (as a whole number counted from 91° south). Halving the edges as they are written puts 37 of the 210 areas one step, a centimetre, to the north or south; reckoned Overpass's way, none. That this is how Overpass reckons is concluded from the two recordings and *to verify* in its source.

A way that is closed gives its outline: the ring of its points as OpenStreetMap has them, 4 to 64 of them. 205 sites have one. Two landing fields are drawn with more points, 67 and 203, and are passed on with 35 and 58: points are left out that lie within 1 m and within 4 m of the line that stands for them (the method of Ramer, Douglas and Peucker, the distance doubled from half a metre until the ring fits), and the run's report says so for each. The bound of 64 is ours: half the outlines have 6 points or fewer. One site is a way of two points, a line, and has a place and no outline; the one relation is a school's building and no site. A relation that is a site would have a place and no outline, because the answer has only the box around it: asking for its members is a later step if one turns up.

*Which tags are read now, and which are not.* Counted over the 502 sites of the raw answer, which held 109 different keys:

| Key | Sites | |
|---|---|---|
| `website` | 105 | **read**: a web page about the site, "the operators website or a website with further information" by the scheme's page. Every one of them is an `http` or `https` address; another kind would be refused by the pipeline |
| `contact:website` | 3, of them 2 without `website` | **read** where there is no `website`: 107 sites have a web page |
| `description` | 43 | **read**, as written, line breaks and all; OpenStreetMap keeps no value longer than 255 characters |
| `free_flying:guest-guidelines` | 34: 13 `yes`, 21 `no` | **read**: "Whether guests (pilots who don't fly this site often and are not member of the local club) have to take notice of a guest guide" |
| `free_flying:no-flight-time` | 23: 15 `no`, 5 `01.04-30.09`, 3 `01.05-30.09` | **read**, as written: "A time in which the take off, the landing or the towing-place is not allowed to use. Use `no` if the place might be used the whole year". The page marks the key as deprecated in favour of `opening_hours`, and the region's sites carry it eleven times as often. Its `no` is no such time and is not passed on, so 8 sites have one |
| `fee` | 18: 17 `yes`, 1 `no` | **read**, as written: "Whether a payment is required in order to use this site" |
| `operator` | 97, with 39 different values | **not read, and removed from the recording.** The page calls it the "maintainer's name (In France, Germany, Austria that would be the local Club)", and on most sites it is a club or a flying school. But one value is the name of a person and nothing else (an element there also carries `operator:type=private_individual`), and one school is called by its owner's name. A key that holds people's names is not copied into this repository or onto a site's page. *For the owner:* whether it is read after all, with a rule that tells a club from a person, which no rule can do for certain |
| `phone`, `email`, `contact:phone`, `contact:email` | 1 and 1 on the sites; 5, 3, 1 and 1 on all elements | **never read**, removed |
| `opening_hours` | 2 | not read: too few for a line of a page. It is the key that is to replace `free_flying:no-flight-time`; when mappers move to it, it is read in that key's place |
| `charge` | 5 | not read: how much the fee is, on five sites |
| `direction:ideal` | 1 | not read |
| `wind_speed` | 0 | not read |
| `surface` | 104 (83 `grass`) | not read: what the ground is. No line of a page yet; *for the owner* if it should be one |
| `frequency` | 12 (11 of them one frequency) | not read: a radio frequency, perhaps of the club's. What it is for is not said by the scheme's page |
| `website_1`, `website_2`, `website_3`, `url`, `image`, `contact:webcam`, `note`, `description:de` | 10 or fewer each | not read |
| `free_flying:tandem`, `:school`, `:club`, `:repair` | 15, 13, 12, 1 | not carried: they mark a business or a club, by the scheme's page "A free flying school", "A free flying repair shop", "A free flying club", "A place where you can pay to have a tandem flight with professional pilots" (read on the page on 2026-11-21); an element that names no use and carries one is left out as before |
| `free_flying:orientation` | 2 | not read: an undocumented spelling |

*What a recorded file holds.* Of an element's tags, those the adapter reads and no others: 83 keys were removed from the answer, by a rule that keeps a list and drops the rest, and the notice names the list. What was kept was read through for people's names, telephone numbers and e-mail addresses before it went into the repository. One description names the telephone number of an airfield that a pilot is to tell before flying there; it is an airfield's, not a person's, and stands as written. Two of the web pages look like one person's own page about a site. A description is free text, and what mappers write tomorrow is not known today: the next recording is read through again. *For the owner:* whether descriptions are shown at all, since the server passes on whatever OpenStreetMap has when it asks, and nobody reads that first.

*What is not interpreted.* A text is its mapper's and is shown as written. The page says of `opening_hours` on a site: "This tag specifies at which times this site is usable and accessible, this is in regard to legal usability or accessibility. This tag is not meant to specify at which times this site is fly-able." The time without flying is passed on in the same spirit: no code works out from it whether a site may be used today, and a site that names none may still have one. The pipeline checks only form: a web address that is `http` or `https` with a host, a description of 1000 characters at most, a fee and a time without flying of 100 at most, and nothing that does not print. What fails is removed whole and counted in the run's report.

### Thermal maps

| Source | Provides | Licence | Status |
|---|---|---|---|
| thermal.kk7.ch | Thermal maps and skyways as map tiles | CC BY-NC-SA 4.0, non-commercial as its author defines it (below) | **Agreed, by the owner's word; to build** as a separate overlay |
| Our own flights | Hotspots and lines computed from uploaded tracks | Ours | Later |

#### thermal.kk7.ch, as read on its own page on 2026-11-20

The owner said on 2026-11-20 that the author agreed in person to Venti showing the maps. That is the owner's word and stands here as such; it is not in writing, and the rule for "Ask" sources asks for writing. *To do, by the owner:* a line from the author that says so, and in it the two things the page leaves open (the `src` value for an app, and what share-alike means for an app that only shows the tiles). Everything else below is the author's own text at `https://thermal.kk7.ch/` (sections "Developers", "License" and "Usage Policy"), and what an app must keep to.

- **Licence.** "The Paragliding Thermal Maps Project by M. von Känel is licensed under a Creative Commons Attribution-NonCommercial-ShareAlike 4.0 International License. NonCommercial means: the maps must not be used in commercial applications (paid apps, apps with paid features or ads, commercial services) without a separate agreement." Venti is free, has no advertising and no paid feature, and the tiles are shown as they come, on a layer of their own: share-alike and non-commercial data is merged with nothing (the rule above).
- **Credit.** As the page's own examples write it: "thermal.kk7.ch CC-BY-NC-SA", the licence's name linked to `https://creativecommons.org/licenses/by-nc-sa/4.0/`. Shown with the map's credits while the layer is on, and on each app's credits screen.
- **Who is asking.** "For tracking purposes every remote request has to append the URL parameter ?src=[hostname], or the domain of your app." "Web pages must send a valid Referer header and not suppress it via Referrer-Policy." "Fat clients should identify themselves in the User-Agent header." Measured on 2026-11-20: a `src` without a dot in it is refused (status 400). **Venti has no domain yet, so there is no honest value to send: the layer is not switched on until the owner names one.**
- **Load.** "Fetch tiles on demand only, for the area and zoom a user is actually looking at." "Pre-fetching is not allowed, in particular not in bulk (whole regions, zoom ranges or layers in advance)." "A caching proxy or a fat client should send no more than 10 requests per second." "Caching tiles on your side (server, app or device) is allowed. Tiles change only when the maps are updated and are served with a 30 day cache lifetime." So: fetched by the apps straight from the author's server, only while the layer is on, never for a map kept for flying without a network, and not through Venti's server ("If you plan on including thermal tiles into your own map server contact me directly with your expected load").
- **The tiles.** `https://thermal.kk7.ch/tiles/{layer}_{season}_{time}/{z}/{x}/{y}.png?src=…`, with y counted from the south (the TMS scheme; an ordinary y gets an empty tile, measured), PNG with transparency, 256 px. Layers `thermals` (to zoom 12) and `skyways` (to zoom 13); beyond that the last level is enlarged, since the server answers an empty tile and not an error. Seasons `all`, `jan`, `apr`, `jul`, `oct`; times of day `all`, `04` (morning), `07` (midday), `10` (evening). The whole year at any hour is `thermals_all_all`. `Access-Control-Allow-Origin: *` is sent, so a web page can read them.
- *To verify:* whether the licence is meant for the tiles, the data or both (the page says "the maps"); where the credit has to stand; whether the hotspot downloads and the 3D tiles on the same site may be used (they are not asked for here).

### Webcams

No webcam network in the region has an open licence. Webcams are **link only**: the app lists them and opens the operator's image or page, with permission requested from the main operators (foto-webcam.eu, Panomax). The Windy webcams API has a free tier with short-lived image links and a required link back.

### Live tracking

| Source | Provides | Licence | Status |
|---|---|---|---|
| Open Glider Network | One feed carrying FLARM, FANET, SafeSky, PureTrack, satellite trackers and others | ODbL; honour opt-outs; nothing older than 24 hours | Open |
| Our own apps | Positions from pilots flying with the app | Ours | The server takes and serves them since 2026-11-19 ([api.md](api.md#live-tracks)); no app sends them yet. They are a pilot's own data and no source: no adapter, no manifest, and they are never stored or served mixed with the positions of the sources in this table |
| SPOT, Garmin inReach | Per-pilot share feeds that the pilot links to their account | Per pilot | Open |
| FlyXC | Open-source tracker aggregator (MIT) | | Reference implementation |
| XContest Live, SafeSky direct, PureTrack direct | | Closed or per-user | Skip |

### Transit

Self-hosted MOTIS with open timetable feeds; see [architecture.md](architecture.md#transit).

| Feed | Licence | Realtime |
|---|---|---|
| Germany: DELFI nationwide timetable | CC BY 4.0 | Yes |
| Austria: Mobilitätsverbünde datasets (Tirol, Salzburg, Vorarlberg and others) | Own terms with credit; free account needed *(to verify)* | Styria only |
| Switzerland: opentransportdata.swiss | Open with token | Yes |
| South Tyrol and Trentino | Free for non-commercial use | Yes |

### Protected zones

Geometry is available from Bavarian and Austrian state geoportals and OpenStreetMap. The overflight rules are legal texts, not data. The first version curates the few zones that matter to pilots by hand (for example Nationalpark Berchtesgaden and its seasonal golden-eagle zones) and cites the rule.

### Maps, terrain and imagery

| Source | Provides | Licence | Status |
|---|---|---|---|
| Mapterhorn | Elevation tiles combining Bavarian 1 m, Austrian 1 m, Swiss and South Tyrolean data | Source licences, all CC BY-like or freer | Open |
| OpenStreetMap | Vector base map | ODbL | Open |
| basemap.at | Austrian official map, terrain and orthophoto | CC BY 4.0 | Built for the orthophoto: the server cuts it at the border (below); no app uses it yet |
| Bavarian survey office open data | DTK25 topographic map, DOP20 orthophoto, DGM1 | CC BY 4.0 | Open; the orthophoto is in use for the aerial view |
| basemap.de | German official vector map | CC BY 4.0 | Open |
| swisstopo | Swiss maps and imagery | Free with credit, swisstopo's own terms | Open; the orthophoto SWISSIMAGE is in use for the aerial view |
| Autonome Provinz Bozen - Südtirol | Orthophoto of 2023 at 20 cm | CC BY 4.0 | Open; in use for the aerial view |
| Sentinel-2 cloudless 2016 | 10 m imagery fallback | CC BY 4.0 (later years are non-commercial share-alike) | Open; in use for the aerial view |
| OpenTopoMap | Raster server shut down in January 2026 | | Skip |

#### The aerial view's photographs

The apps fetch EOX's, swisstopo's, South Tyrol's and Bavaria's tiles themselves, from the provider, as they fetch the base map: nothing of them passes through Venti, and nothing is fetched until a pilot switches the view on. basemap.at's tiles go through the Venti server, which cuts them at Austria's border. Their addresses stand in `map/style.json`, the same for every app. Each provider's terms were read on its own pages on 2026-11-20, swisstopo's and South Tyrol's on 2026-11-21; what "measured" says was seen in single requests on the same day as the reading.

| Provider | What, and where from | Licence and where it was read | Credit, as the provider words it | Limits and notes |
|---|---|---|---|---|
| EOX IT Services | EOxCloudless (Sentinel-2 cloudless) **2016**, the whole world at about 10 m. `https://tiles.maps.eox.at/wmts/1.0.0/s2cloudless_3857/default/GoogleMapsCompatible/{z}/{y}/{x}.jpg`, JPEG, 256 px, asked to zoom 13 and enlarged beyond | CC BY 4.0, for 2016 only: "For the year 2016, EOxCloudless is licensed under the Creative Commons Attribution 4.0 International License." The years 2018 to 2025 are CC BY-NC-SA 4.0 and are not used. [cloudless.eox.at/license-non-commercial](https://cloudless.eox.at/license-non-commercial) | "EOxCloudless https://cloudless.eox.at by EOX IT Services GmbH (Contains modified Copernicus Sentinel data 2016 & 2017)". "In compact mobile UI designs, displaying the attribution in an on-click pop-up or in the app information is allowed." ([FAQ](https://cloudless.eox.at/documentation/faq)) | "Commercial apps may access the free 2016 endpoint under CC BY 4.0, provided traffic is reasonable", no uptime promise, a fair use policy applies, and the servers slow down or block spikes: "cache tiles on your own CDN or device" (FAQ). *To verify:* the text of the fair use policy, which is not published; the 2017 layer, whose service description says CC BY 4.0 while the licence pages name only 2016 |
| Bayerische Vermessungsverwaltung (LDBV) | The layer "Luftbild Bayern" (`by_dop`) of the "WMTS Geobasisdaten Bayern": Bavaria's orthophoto at 20 cm, and satellite images at small scales. `https://wmtsod1.bayernwolke.de/wmts/by_dop/smerc/{z}/{x}/{y}`, 256 px, zoom 0 to 19; the app shows it from zoom 11 | CC BY 4.0. [Nutzungsbedingungen](https://www.geodaten.bayern.de/odd/m/3/html/nutzungsbedingungen.html). The service's notes allow it in apps by name: "Er kann z. B. in Apps und Internetanwendungen eingebunden werden." ([WMTS-Nutzungshinweise](https://www.geodaten.bayern.de/odd/m/3/pdf/WMTS_Nutzungshinweise.pdf), 27.10.2023) | "© Datenquellen: Bayerische Vermessungsverwaltung, Europäische Union, enthält Copernicus Sentinel-2 Daten 2018, verarbeitet durch das Bundesamt für Kartographie und Geodäsie (BKG)" (the same notes) | No key and no stated limit; no promise that the service or a layer stays. "Bei jeder Bildschirmdarstellung ist eine Quellenangabe anzugeben." *To verify with the LDBV before a public release:* whether credits one tap away, as a phone's map has them, meet that sentence. Measured: outside Bavaria the tiles are empty and transparent, so the layer ends cleanly at the border |
| basemap.at (Stadt Wien for the Austrian provinces) | Orthofoto, Austria at 29 cm and finer. `https://mapsneu.wien.gv.at/basemap/bmaporthofoto30cm/normal/google3857/{z}/{y}/{x}.jpeg`, JPEG, 256 px, to zoom 19 | CC BY 4.0: "gemäß der Open Government Data Österreich Lizenz CC-BY 4.0 sowohl für private als auch kommerzielle Zwecke frei sowie entgeltfrei nutzbar" ([basemap.at](https://basemap.at/)) | "Datenquelle: basemap.at", with basemap.at as a link to it | **In use, through the server.** The apps ask Venti for it, not basemap.at: the layer `aerial-austria` in `map/style.json`, at the server's `/v1/imagery/basemap-at/{z}/{x}/{y}`, shown from zoom 11 between the world's mosaic and Bavaria's photographs. An app with no Venti server set has no such layer. Measured: its tiles are JPEG, and white wherever they reach beyond Austria (all of a tile over Munich or Bolzano, half of one on the border). Laid on the map as they are they would paint white over Bavaria, South Tyrol and the Engadin, and a map style cannot cut them. So the server does, along an outline of Austria from the BEV's administrative boundaries at 1:50 000 (CC BY 4.0, read in the data.gv.at catalogue; the BEV's own credit wording *to verify*), with a band of 50 m on each side in which the provider's white decides, and keeps the result; it never asks for a tile beyond the border ([architecture.md](architecture.md#the-aerial-view-through-the-server)). The adapter is `crates/sources-basemapat`. Measured on 2026-11-20 as well: at the Achenpass and the Brenner the photograph itself reaches beyond the border before the white begins, and the service answers some tiles inside its box with a 404 (Munich at zoom 12 and 14) where others are white (zoom 10). No registration and no limit by a fact sheet of 2017; the addresses stand "voraussichtlich bis Ende 2027" ([changelog](https://basemap.at/changelog/)); basemap.at asks for a privacy notice, since requests reach Stadt Wien |
| swisstopo (Bundesamt für Landestopografie) | SWISSIMAGE: Switzerland at 10 cm, 20 to 25 cm in the Alps, renewed in a cycle of three years; Liechtenstein is in it (seen at Vaduz). The layer "SWISSIMAGE Zeitreise" (`ch.swisstopo.swissimage-product`) at its newest state, `current`. `https://wmts.geo.admin.ch/1.0.0/ch.swisstopo.swissimage-product/default/current/3857/{z}/{x}/{y}.png`, PNG with transparency, 256 px, zoom 0 to 20; the app shows it from zoom 11 and asks to zoom 19, inside the box the layer's own record names (5.9395° to 10.5190° E, 45.8061° to 47.8095° N) | swisstopo's own terms for its free geodata and geoservices, version 1.3.2021: "The free geodata and geoservices of swisstopo may be used, distributed and made accessible. Furthermore, they may be enriched and processed and also used commercially." ([Terms of use](https://www.swisstopo.admin.ch/en/terms-of-use-free-geodata-and-geoservices); the [product's page](https://www.swisstopo.admin.ch/en/orthoimage-swissimage-10) names them as its terms). For the service, the [general terms of the federal geodata infrastructure](https://www.geo.admin.ch/en/general-terms-of-use-fsdi) of 01.01.2025: "Use of the geoservices of the FSDI does not require registration and is free-of-charge." | "A reference to the source is mandatory", as one of six wordings, among them "Bundesamt für Landestopografie swisstopo", "Federal Office of Topography swisstopo" and "©swisstopo", which is the one the style has. The product's page adds, for the middle of the 20 largest lakes, which is satellite imagery: "The correct copyrights statement is “Includes material © (2016-2018) Planet. All rights reserved”"; the style says that as well | **In use**: the layer `aerial-switzerland` in `map/style.json`. No key and no registration. "The integration of geoservices in web applications with an average of 20,000 users per day or desktop applications corresponds to fair use"; "In the event of excessive use of geoservices, access may be restricted or refused." The terms name an availability of 98 % in office hours and best effort around the clock; a change that breaks clients is announced a month ahead. swisstopo's technical pages ([docs.geo.admin.ch](https://docs.geo.admin.ch/get-started/overview.html), "Fair Use") ask more: "Send a User-Agent header with the name of your application and a contact (e-mail or URL)", "If you get HTTP status 429, wait and retry with exponential backoff. If you get 403, stop", "For a high number of requests, contact webgis@swisstopo.ch before you start"; and tiles may be renewed under the same address, so whoever keeps them has to ask the service's `cacheUpdate` when (it said 2026-11-18). Measured: a tile outside what SWISSIMAGE covers (Bregenz, Munich) is a PNG of 334 bytes, wholly transparent; on the border the photograph runs on, to straight edges that fit the product's sheets of one kilometre, and is transparent beyond (a fifth of the tile at Taufers is photograph; all of the one at Martina, more than half of which is Austria by the Austrian adapter's outline). **A tile is heavy**: 162 to 197 kB as PNG in twelve tiles between zoom 11 and 20, where the same tile as JPEG is 18 to 29 kB, a seventh to a ninth. JPEG is not used because it is white where the PNG is transparent, and a style cannot cut it. The service's other layer of the same photographs, `ch.swisstopo.swissimage` ("SWISSIMAGE Hintergrund", JPEG only), is not used either: beyond the border it goes on as satellite images that are not swisstopo's ("Bei den Orthofotos in den Nachbarländern handelt es sich um SPOT 5-Satellitenbilder (Airbus Defence and Space) aus den Jahren 2004 und 2005 mit einer Bodenauflösung von 5 m", its own description; swisstopo's map viewer credits it "CNES, Spot Image, swisstopo, NPOC"), seen over Bregenz, Meran, Como and Munich, and the terms do not say that they cover those. The tiles come through Amazon CloudFront and allow any web page to fetch them (`access-control-allow-origin: *`). *To verify with swisstopo before a public release:* what a map in an app is to send as its name and contact, since a browser's map cannot set a User-Agent, and what the phones' map engine sends was not looked at; whether a phone app counts as a web or desktop application; the "table below" of requests per time unit that the terms speak of and neither their English nor their German page has; whether the Planet statement has to stand on a map |
| Autonome Provinz Bozen - Südtirol (Amt für Landesplanung und Kartografie) | "Orthofoto 2023" (`p_bz-Orthoimagery:Aerial-2023-RGB`): the whole province at 20 cm, made by the province's department 28 with AgEA, the Italian agency for payments in agriculture. `https://geoservices.buergernetz.bz.it/mapproxy/root/wmts/p_bz-Orthoimagery:Aerial-2023-RGB/EPSG_3857/{z}/{x}/{y}.png`, 256 px, a JPEG where the tile is all photograph and a PNG where part of it is transparent, whatever the address ends in; the service's scheme has zoom 0 to 24, and 20 cm is zoom 19. The app shows it from zoom 11 and asks to zoom 19, inside the box the service names for the layer (10.3506° to 12.5246° E, 46.1750° to 47.1414° N) | CC BY 4.0, by the data set's own record in the province's catalogue: "Orthofoto 2023 , Auflösung 20cm. Die Nutzung der Daten unterliegt dem Lizenzmodell Creative Commons BY 4.0", with the licence's address as its one constraint and "keine Beschränkungen für den öffentlichen Zugang" ([record](https://geonetwork1.civis.bz.it/geonetwork/srv/api/records/p_bz:Orthoimagery:Aerial-2023-RGB), dated 2025-09-01; it names the WMTS as the way the data set is given out). The province's open data portal prints the same sentence beside a licence field that says "Creative Commons CCZero" ([data.civis.bz.it](https://data.civis.bz.it/de/dataset/ortofoto-2023)); the stricter of the two is taken. For the service: "Die Geodaten der Landesverwaltung, der Agentur für Bevölkerungsschutz sowie der Gemeinden stehen als Standard-Webdienste frei und kostenlos zur Verfügung", the WMTS among them, and "Falls nicht anders angegeben, werden die Daten mit der Lizenz "Creative Commons Zero" (kurz CC0) ausgegeben" ([Geodienste nutzen](https://natur-raum.provinz.bz.it/de/geodienste-nutzen)); the service describes itself with "Fees: none, AccessConstraints: none" | None is prescribed on any page that was found: the record has no credit, the service none, the province's pages none. Ours, until the province's own is known (*to verify*): "© Autonome Provinz Bozen - Südtirol, Orthofoto 2023, CC BY 4.0", with the province's page on its orthophotos and the licence's text as links | **In use**: the layer `aerial-south-tyrol` in `map/style.json`. No key, no registration, no stated limit and no promise that the service or the layer stays; the address names the year, so a newer orthophoto is another layer. Nothing is said about apps: the province's pages speak of GIS and CAD programs, and an inventory of its cartography of 2016, linked from its page [Download und WebGIS](https://natur-raum.provinz.bz.it/de/download-und-webgis), says of its web services: "Es ist möglich von den eigenen GIS/CAD-Desktopanwendungen auf diese zuzugreifen oder sie in die eigenen Webapplikationen einzubinden." Measured: a tile outside (Cortina, Sillian, Munich) is a PNG of 334 bytes, wholly transparent; on the border the photograph runs on, by more than a kilometre at the Reschen pass, and is transparent beyond (the whole tile at the Brenner is photograph, three fifths of it Austria by the Austrian adapter's outline; a tile at the Reschen pass that is nine tenths Austria is photograph but for a corner). A JPEG is 8 to 22 kB, a PNG on the edge 166 kB. The service tells a cache to keep a tile for 15 days and lets a web page fetch it (it answers `access-control-allow-origin` with the origin that asks). *To verify with the province before a public release:* the credit it wants and whether AgEA is to be named; that a public app may draw the tiles from the service itself, and how many; which of the two licences its pages name holds |

Bavaria's, EOX's, swisstopo's and South Tyrol's servers see the IP address and the map areas of whoever looks at the aerial view there, as the base map's servers do for the drawn one; swisstopo's tiles come through Amazon CloudFront. That is said where the app says what it sends to whom, before a public release.

**Where two countries' photographs meet.** Only Austria's end at the border, because the server cuts them there. Switzerland's and South Tyrol's reach beyond theirs, by a kilometre and more in the tiles that were looked at, and are transparent further out; what was measured of Bavaria's says that they are transparent outside Bavaria and not where exactly they end. So the order of the layers in the style decides what shows in such a strip, the later over the earlier: the world's mosaic, Switzerland, South Tyrol, Austria, Bavaria. Austria's lie over Switzerland's and South Tyrol's, so Austria is Austria's photograph up to the line; an app with no Venti server has no Austrian layer and shows the neighbours' margins there, over the mosaic. Seen on 2026-11-21 in a browser, with the two new layers alone over a plain colour, drawn by MapLibre GL JS from the style's entries around Taufers and the Reschen pass: both drew without an error, the map asked for fewer tiles where a layer's box ends (14 in place of 18 at the Reschen pass), and beyond the two layers' margins the ground of Austria stayed empty, for Austria's own layer to fill.

## Leagues a flight goes to

These are not sources: nothing is fetched from them and nothing of theirs is served. A pilot's own flight file goes from the pilot's device to the league, at the pilot's word. The rule is the same in spirit: only what the league itself publishes for apps is used, and what it publishes was read on its own pages. `crates/leagues` holds what is built; it talks to nobody, and the app sends from the pilot's device. Read on 2026-11-20.

| League | What | What its own pages say | Status |
|---|---|---|---|
| XContest | The page on which a signed-in pilot claims a flight, `https://www.xcontest.org/world/en/flight-claim/` | "How to claim a flight" (`https://www.xcontest.org/world/en/how-to-claim-flight/`) sends the pilot to "Claim flight" in the "My Contest" menu: "All you need now is valid IGC file." To anyone not signed in the address answers "sign in first", so what the page shows was not read | **Use**: a link to the page, and the file handed to the pilot, who uploads it |
| XContest | An upload interface for apps | None on XContest's pages. A document of XContest's from 2012 ("XContest API – Gate Flight"), found only in another project's repository, says that an app needs a key and a secret which XContest gives out on request to `info@xcontest.org`. Whether it is current is not known. No terms for it are published | **Ask** |
| XContest | Reading a pilot's flights or their files | Nothing is published. Flight pages ask for a sign-in, and `robots.txt` shuts out the flight and pilot pages and every address with a query. *To verify, by the owner, who has an account:* whether a signed-in pilot can download the file of their own flight from its page | **Skip** until XContest offers a way. A pilot imports files they downloaded themselves |
| DHV-XC | The upload interface for apps, `POST https://en.dhv-xc.de/api/v1/flights` | "DHV XC Flight Upload Interface Specification", version 2022-04-24, printed in the rules under "Software Spezifikation - für Entwickler" ("Wettbewerbsausschreibung 2026/2027", version 01.2027, `https://www.dhv.de/media/seiten/05_wettbewerb/DHVXC/Ausschreibungen/AusschreibungDHVXC.pdf`). The request carries the pilot's user name, an upload password the pilot sets in the profile ("Upload password as set in the pilot profile"), the file's name and its text, and optionally a glider and whether to publish. No key is asked of the app | **Use**: built in `crates/leagues` and the iPhone app. Nothing has been sent to the DHV-XC by this project yet |

What a bridge has to respect, in the leagues' words:

- **Fourteen days.** XContest's rules, 3.1 (`https://www.xcontest.org/world/en/rules/`): "The deadline for uploading a flight is 14 days after day of the flight. Flights which are claimed after the deadline will be rejected." The DHV-XC's, 6.5: "spätestens 14 Tage (Flugtag x + 14 Tage / 24:00 Uhr UTC)"; later a flight can only go to "Ohne Wertung" or "Privat". *To verify:* by which time zone XContest counts the days; the app counts by UTC for both.
- **The recorder's signature.** XContest's questions and answers (`https://www.xcontest.org/world/en/faq/`): a file whose signature does not validate "can be submitted to XContest, but no points will be assigned for it in categories." The DHV-XC's rules, 6.4: the files are checked on upload for being unedited, "Dies wird durch einen G-Record angezeigt (grün = in Ordnung, rot = wird nicht gewertet)". Neither league publishes a list of the recorders it accepts or a way for a new one to be accepted. **Venti's own recordings carry no signature**, so they are listed and not scored; flights imported from a vario or another app keep theirs, because the logbook keeps the file byte for byte.
- **Whose the file is afterwards.** The DHV-XC's rules, 4.8: with the upload the pilot gives up the sole rights of use in the file, the photos and the comments to the organiser. XContest's, 3.2: an uploaded tracklog "becomes the public property". The app says so before it sends.
- **The upload password is the pilot's.** It is kept in the device's keychain (on Android, sealed under a key in Android's keystore) and sent to the DHV-XC only. It never reaches a Venti server.

Not verified, and said so where it matters: the address of the DHV-XC's check of a user name and password (the specification's heading says `/v1/authcheck`; the app does not use it), and anything about either league that needs a sign-in to see.

## Outreach

Permissions to request, roughly in order of how much they block. AI sessions draft the emails; the owner sends them.

| To | Asking for | Blocks |
|---|---|---|
| Holfuy | Permission and an API key to show and re-serve station data in a free, open-source app | Wind at launch sites |
| DHV | Permission to use and re-serve the site database | Authoritative site data |
| XContest (`info@xcontest.org`) | A key for its upload interface for apps, and the terms for using it; a way for a pilot to fetch their own flights; recognition of the signature of tracks recorded by the app, or the way to it; use of the airspace activation data. Until then the app hands the pilot the file and opens XContest's claim page ([Leagues a flight goes to](#leagues-a-flight-goes-to)) | Sending a flight to XContest with one tap, importing a pilot's history from it, points for flights recorded with the app, airspace activations |
| thermal.kk7.ch | Agreed in person, by the owner's word (2026-11-20). Still wanted in writing: that line, the `src` value an app should send, and what share-alike means for an app that only shows the tiles | Nothing any more; the layer waits for a domain to name in `src` |
| DFS | Written confirmation of the CC BY 4.0 terms. Since 2026-11-19 they are known to be stated on DFS's own page, per data set; whether a letter is still wanted is the owner's decision | German airspace |
| Austro Control | Permission to re-serve airspace and obstacle files | Austrian airspace |
| Land Tirol | Confirmation of licence for the station files and the obstacle dataset; they ask to be notified of use | Tirol stations, cables |
| Avalanche services Bavaria, Salzburg, Vorarlberg, Carinthia | Access to station data | Mountain wind |
| foto-webcam.eu, Panomax | Permission to list and link images | Webcams |
| Transitous | A heads-up before using the public API from an assistant | Transit bootstrap |
| swisstopo (`webgis@swisstopo.ch` for the services, `info@geo.admin.ch` for their limits) | Nothing is needed to show SWISSIMAGE. Before a public release: what a map in an app is to send as its name and contact; whether a phone app is fair use like a web application; the table of requests per time unit; whether the Planet statement has to stand on a map. And whether the satellite images of the layer `ch.swisstopo.swissimage` beyond the border may be shown under swisstopo's terms and with which credit, which would allow its JPEG, a seventh to a ninth of the bytes ([The aerial view's photographs](#the-aerial-views-photographs)) | Nothing now; the weight of Switzerland's photographs on a phone's connection |
| Autonome Provinz Bozen - Südtirol, Amt für Landesplanung und Kartografie (`kartografie@provinz.bz.it`; for the services `service.pab@provinz.bz.it`) | The credit the province wants for its Orthofoto 2023, and whether AgEA is to be named; that a free public app may draw the tiles from the WMTS itself, and what load is welcome; whether CC BY 4.0, as the data set's record says, or CC0, as the open data portal's licence field says, holds | Nothing now; a public release with South Tyrol's photographs |

## Known changes

| Date | Change | Effect |
|---|---|---|
| 2026-01-21 | South Tyrol's weather service has a new site, "Wetter Südtirol"; by the open data portal's notice the datasets stay as they are "for the time being", and changes are announced on the portal | The canary must cover `suedtirol-meteo`. The notice is on the [dataset's page](https://data.civis.bz.it/de/dataset/misure-meteo-e-idrografiche) |
| Before 2026-11-21, date not found | South Tyrol's web service moved to `geoservices.buergernetz.bz.it`; the older addresses answer 404 | The adapter was written for the new address |
| 2026-11-30 | DWD switches off the old ICON-D2 and ICON-EU download paths and regular grids; the new feed is on the native triangular grid only | Nothing to do for the forecasts Venti serves: Open-Meteo reads the new feed already, and the adapter asks for its models on the native grid. A raw ICON adapter has to read the triangular grid. See [what the change means](#what-dwds-change-of-2026-11-30-means). |
| 2026-11-04 | GeoSphere Austria replaced its AROME dataset with C-LAEF 1 km | Use the new dataset identifiers |
| Early 2027 | GeoSphere Austria's 2.5 km endpoint changes again | Canary must cover it |
