# Data sources

Last revised: 2026-11-19

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

On-device instrument drivers follow the same idea in the `sensors` crate: one parser per protocol, fed bytes, tested from recorded streams.

### Manifest

```toml
id = "geosphere-tawes"
provider = "GeoSphere Austria"
kinds = ["observations"]
countries = ["AT"]
cadence_seconds = 600
auth = "none"

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

Each adapter crate has its manifest in `manifest.toml`, compiled into the crate. Unknown fields are rejected, so a typo cannot silently drop a licence term.

The manifest is data, not comments. The server reads it to:

- send the credit of every source with the data it serves: the credit line, the licence's name and the link (`attribution`, `name`, `link`), and the `notice` if the provider asks for one to be shown with its data, as openAIP does;
- enforce rate limits and schedules;
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
sources = ["dwd-10min", "geosphere-tawes"]
priority = ["dwd-10min", "geosphere-tawes"]   # wins when two sources report the same station

[airspace]
sources = ["openaip"]

[sites]
sources = ["osm-free-flying"]
```

Region packs live in `regions/<id>/region.toml`. The area is a bounding box for now; a boundary polygon replaces it when a region needs a shape a box cannot express. Sections for the other kinds (map layers and so on) are added as those kinds are built.

A region may leave `[airspace]` out. Airspace from two sources is never merged, so `priority` there says which layer a client prefers where two cover the same air, not which fields win. A region's airspace sources are checked against the build's adapters like its observation sources: a server that cannot collect a region's airspace does not start.

`[sites]` follows the rules of `[airspace]`: a region may leave it out, sources are never merged, and a server that cannot collect a region's sites does not start. `[forecasts]` is the one section whose sources a server may lack: a forecast comes from a service that whoever runs the server runs beside it, and a server that was not told where that service is starts all the same, says in its log which forecasts it goes without, and reports that it has none.

A manifest needed nothing new for airspace beyond the kind. `cadence_seconds` is how often the provider publishes, a day for openAIP, and the request budget and the staleness rule follow from it as for any source.

### Pipeline

Every payload goes through the same stages, in the `sources` crate:

1. **Fetch.** The server's scheduler runs each source at its cadence and within its rate limit; see [architecture.md](architecture.md#what-the-server-does-today).
2. **Archive.** The raw payload is written to disk and kept for a rolling window. A bug report can be replayed exactly.
3. **Normalise.** The adapter converts to canonical types: SI units, UTC, WGS84, heights above mean sea level.
4. **Merge and validate.** Partial observations for the same station and time are merged (some providers publish wind, gusts and temperature in separate files). Implausible values are removed and counted, so one faulty humidity sensor does not cost a good wind reading. An observation with nothing left is dropped. Nothing unvalidated reaches the store.
5. **Resolve.** The same real-world object from two providers is matched (by position, name and type) and linked in a mapping table. The region pack's priority decides which fields win. Every stored field records its source.
6. **Store and publish.** Canonical records go to the database; changes are published to the live channel and to the region-bundle builder. Storing is built; publishing changes is not.

For airspace, stage 4 has nothing to merge, and the store replaces instead of adding: see the table under [Kinds and traits](#kinds-and-traits). The pipeline also dates every airspace by its file: by what the adapter found in the file, else by the `Last-Modified` the provider's server sent with it, else by the time of the run.

### Licence rules built into the pipeline

- **Different licence families never merge into one record or one bundle.** Non-commercial and share-alike sources (openAIP, thermal.kk7.ch, ParaglidingEarth, recent Sentinel-2 mosaics) stay separate layers with their own licence tag. Resolution links their records to others but does not copy fields across.
- **Constraints are declared and enforced.** For example the Open Glider Network requires honouring pilots' opt-outs and forbids redistributing positions older than 24 hours. Those are manifest constraints that the live service applies; they are not left to each call site.
- **Credentials live in server configuration,** never in the repository.

### Health

Each adapter reports last success, record counts, quarantine counts and data age. The server keeps the outcome of every run and exposes the summary per source at `/v1/health` ([api.md](api.md)). Every data point shown to a pilot carries its age, and a stale source is shown as stale. One failing adapter does not affect the others: each source runs in a task of its own, a failure or even a crash inside an adapter ends that run only, and a source that keeps failing is tried less and less often, down to once an hour.

### Testing

Every adapter crate contains:

- **Fixtures:** real recorded payloads, including at least one malformed one. The healthy ones are also compiled into the crate (`fixtures::recorded()`), where the server's development mode finds them.
- **Golden files:** the expected canonical output for each fixture. CI fails if normalisation changes.
- **A canary:** the `Source canary` workflow runs every region's sources against the live providers once a day (`venti-server ingest`, run from the image a server runs) and fails when a source delivers nothing or reports problems. Providers change formats without notice; two such changes are already dated in [Known changes](#known-changes).

Golden files are rewritten by running the tests with `VENTI_UPDATE_GOLDEN=1`, and the change is reviewed like any other diff.

### Adding a provider

1. Confirm the licence on the provider's own page and record it in the manifest with the date and URL.
2. Create `crates/sources-<provider>` implementing the relevant traits.
3. Record fixtures and golden files.
4. Add the adapter and its recorded payloads to the registry in `crates/server/src/adapters.rs`, and the source to the region packs that should use it.
5. Add the canary.

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
| Region pack loader and the `alps-de-at` pack (`venti-regions`) | Built for observations |
| Store for stations, observations and run history (`venti-store`): PostgreSQL with PostGIS, and in memory | Built |
| Server (`venti-server`): scheduler, raw-payload archive with a rolling window, HTTP API with attributions, health per source, development mode over the recorded fixtures | Built for observations; installed on no machine yet |
| `venti-server ingest`, which runs a region's sources live once and stores nothing (formerly the separate tool `venti-ingest`) | Built |
| Daily canary | Built as a CI workflow |
| Request limits | The per-run budget from the hourly limit and a concurrency cap at the per-second limit are enforced by the pipeline. The scheduler spaces runs by the cadence, also after failures and restarts. A true per-second rate limit is not built; no source that makes many requests publishes one. |
| Resolve stage, change events | Designed, not built. Until resolve exists, a station reported by two sources is two stations. |
| Canonical type for airspace (`venti-domain`), and reading and questioning it (`venti-airspace`) | Built; see [airspace.md](airspace.md) |
| `AirspaceSource`, its registry and its run with stages 1 to 4 (`venti-sources`); the `[airspace]` section of a region pack (`venti-regions`) | Built |
| Adapter: `openaip` | Built, with fixtures recorded 2026-11-19 and golden files |
| Airspace in the server: collecting once a day, storing as one set per source, serving at `GET /v1/airspace`, health, the development mode, the canary | Built; installed on no machine yet |
| Region bundles | Not built. Until they are, the airspace endpoint is how an app gets a region's airspace |
| Canonical types for flying sites and point forecasts (`venti-domain`): `Site`, `ForecastSeries` | Built |
| `SiteSource` and `ForecastSource`, their registries and their runs (`venti-sources`) | Built |
| Adapter: `osm-free-flying` | Built, with an answer recorded 2026-11-19 for the whole first region and golden files |
| Flying sites in the server: collecting once a day, storing as one set per source, serving at `GET /v1/sites`, health, the development mode over the whole first region, `ingest` and with it the canary | Built; installed on no machine yet |
| The `[sites]` and `[forecasts]` sections of a region pack (`venti-regions`); what the store keeps about forecast models | Built |
| Adapter: `open-meteo` | Built, with answers recorded 2026-11-19 for five places from an Open-Meteo instance run for the purpose, and golden files |
| Forecasts in the server: fetched from the operator's Open-Meteo instance when a place is asked for and kept for half an hour, served at `GET /v1/forecast`, a look every ten minutes for the health report, the development mode over the recorded answers, `ingest` | Built; run against no instance on a schedule yet, and installed on no machine |
| Every kind other than observations, airspace, sites and point forecasts | Designed, not built |

A live run of the first region on 2026-11-18 delivered 50 DWD and 267 GeoSphere stations in under six seconds, using 104 and 2 requests.

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
| South Tyrol open data | 95 to 120 stations, 5-minute | CC0 | Open |
| MeteoSwiss open data | SwissMetNet | CC BY 4.0 | Open (for later) |
| METAR (aviationweather.gov) | Airfield reports | Public | Open |
| FANET weather stations through the Open Glider Network | Club stations that broadcast on FANET | ODbL | Open |
| Pioupiou / OpenWindMap | Community wind sensors; little coverage in this region | Community licence with credit | Open |
| **Holfuy** | The stations at most launch sites | No published terms; keys are issued only to station owners | **Ask** |
| Avalanche services Bavaria, Salzburg, Vorarlberg, Carinthia | Mountain stations | No open licence found | Ask |
| winds.mobi | Aggregator; in this region it is essentially Holfuy plus METAR | Code AGPL-3.0; no API terms published | Skip (we ingest the originals) |
| Windy stations, cable-car operators | Various | Custom or none | Skip for now |

### Forecasts

| Source | Provides | Licence | Status |
|---|---|---|---|
| Self-hosted Open-Meteo, `open-meteo` | Point forecasts of DWD's ICON-D2 (about 2 km) and ICON-EU, hour by hour: wind and gusts, wind at heights, temperature, dew point, cloud by layer, precipitation, CAPE, freezing level, convective cloud base and top, and from ICON-D2 the strongest updraft | Data CC BY 4.0 from both Open-Meteo and DWD, verified 2026-11-19; code AGPL-3.0-or-later, run unmodified as a separate service. Needs 8 to 16 GB memory and fast disk | **Built** (ICON-D2 and ICON-EU). ICON-CH and ECMWF through the same instance: open |
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

An instance was started from the image as built on 2026-11-03, in remote-data mode, and asked the adapter's own requests for five places. That is where the recorded fixtures are from. What it showed:

- **It works as documented**, including the two models under their names on the native grid, `dwd_icon_d2_native` and `dwd_icon_eu_native`, which Open-Meteo's bucket holds.
- **The run of a model is at `/data/<model>/static/meta.json`**, and only if the instance has `API_SYNC_APIKEYS` set. Without it the address answers 404, and then the server delivers no forecast: a series must say its run. With it set (to any value), the file is served without a key.
- **A first request for a place is slow in remote-data mode**: 25 to 60 seconds through the slow connection of the machine that tried it, and about 270 reads of 64 kB each from the bucket; a neighbouring place afterwards took under half a second, and a repeated one a few milliseconds. On a server with a direct connection the first request will be faster; *to verify* by how much. Sync mode has no such first request.
- **An answer is 19 kB**, for 72 hours of 44 values. The two answers about the runs are 494 bytes each.
- Open-Meteo's public API answered the same machine "Daily API request limit exceeded. Please try again tomorrow." for every forecast request that day: the address is shared with others. That is the limit working as its terms say, and one more reason not to build on it.

#### What a series holds

Two models, each asked for by itself and delivered as a series of its own. Open-Meteo's "seamless" blends are not used.

| | ICON-D2 | ICON-EU |
|---|---|---|
| Cells | About 2 km, triangles | About 5 km, triangles: two neighbouring cell centres in the recordings lie 5.3 km apart. DWD made the grid finer on 2026-10-06 |
| Runs | Every 3 hours | Every 3 hours |
| Reach of a run | 48 hours | 120 hours from 00, 06, 12 and 18 UTC, of which the first 78 hour by hour; 30 hours from 03, 09, 15 and 21 UTC |
| At the instance | About 70 minutes after the run began (18 UTC run: 19:10) | About 2 hours 50 minutes (15 UTC run: 17:50) |

The reach is from Open-Meteo's downloader, [`IconDownloadDomains.swift`](https://github.com/open-meteo/open-meteo/blob/main/Sources/App/Icon/IconDownloadDomains.swift), read on 2026-11-19; the times at the instance are the two runs that were recorded.

**A series holds only the hours its run computed.** An instance keeps one value per hour, the newest it has. Asked for three days, it answers every hour of them: the hours before the newest run began are older runs', and so are the hours beyond a short run's reach. On 2026-11-19 at 19:25 UTC the instance answered all 72 hours for ICON-EU, whose newest run was the one of 15 UTC that reaches 30 hours; the other hours came from the run of 12 UTC or an older one, and nothing in the answer says so. The adapter delivers the hours after the run began as far as that kind of run reaches, and no others. The hour the run begins at is left out as well: Open-Meteo's downloader skips gusts, precipitation and updraft for it, so those three would be the run before's.

**The gap this leaves.** "Today and the next two days" is not always covered:

- The hours of today before the newest run are never in a series.
- ICON-D2 ends 48 hours after its run, which is somewhere on the day after tomorrow.
- ICON-EU covers all three days after a run of 00, 06, 12 or 18 UTC, and only 30 hours after one in between. So the day after tomorrow has ICON-EU's hours for about three hours out of every six.

Closing it without guessing needs the run of every hour. Two ways are open: Open-Meteo's single-runs API, which answers for one named run (`&run=`), if a self-hosted instance can serve it (*to verify*; it needs each run kept whole); or the server remembering which runs it saw arrive, which it already notes for the health report.

**Between two runs.** Open-Meteo writes a new run into its files variable by variable and updates `meta.json` at the end. For some minutes an answer can hold values of the next run under the name of the one before. The server keeps an answer for half an hour at most. *To verify* how long that window is.

#### The values

All are the model's values for its own cell: the request asks for the nearest cell and switches off Open-Meteo's shift of temperatures to the height of the place (`cell_selection=nearest`, `elevation=nan`). The series carries the centre of the cell and the height of the model's ground there. East of the Tegernsee, whose shore lies at 725 m, ICON-D2's ground is at 1069 m and ICON-EU's at 1100 m.

| Value | From | What it is, and what it is not |
|---|---|---|
| Wind at 10 m, speed and direction | `U_10M`, `V_10M` | Open-Meteo computes speed and direction from the two components |
| Gusts at 10 m | `VMAX_10M` | The strongest in the hour before |
| Temperature at 2 m | `T_2M` | |
| Dew point at 2 m | `T_2M`, `RELHUM_2M` | Computed by Open-Meteo from temperature and relative humidity. ICON's own dew point is not used by it |
| Cloud cover, total and low, mid, high | `CLCT`, `CLCL`, `CLCM`, `CLCH` | Where "low" ends is ICON's definition |
| Precipitation | `TOT_PREC` | Rain, showers and snow of the hour before, as water |
| CAPE | `CAPE_ML` | Of a parcel mixed over the lowest layer. It says whether showers and storms can grow, not how good thermals are |
| Freezing level | `HZEROCL` | Metres above sea level. Where the whole column is below 0 °C, ICON writes the height of its ground: for seven hours of 2026-11-21 the recorded series has 1050 to 1070 m in a cell whose ground is at 1069 m. Open-Meteo keeps it in steps of 10 m |
| Updraft, ICON-D2 only | `W_CTMAX` | The strongest rising air anywhere in the column up to 10 km, in the hour before. It is what a model with 2 km cells resolves: showers, storms, flow over mountains. **It is not thermal strength.** In the recorded two days, with showers and no storm, the values are 0.1 to 2.1 m/s |
| Convective cloud base and top | ICON-D2: `HBAS_SC`, `HTOP_SC`. ICON-EU: `HBAS_CON`, `HTOP_CON` | Not the same thing in the two models. ICON-D2 computes deep convection itself and estimates only shallow convection, and these are the base and top of that. ICON-EU estimates all convection. ICON writes -500 m for "none"; the instance handed that on for ICON-EU and wrote 0 for ICON-D2; the adapter reads both as no value. Whether the heights are above sea level is *to verify* on DWD's description of the fields; the recorded values fit that |
| Wind and temperature at 80, 120 and 180 m | `U`, `V`, `T` on model levels | The third, fourth and fifth level from the ground; the heights are nominal. Open-Meteo's comment gives 78 m and 126 m for the two lowest of ICON-D2 |
| Wind, temperature and height on 950, 850, 700, 600, 500 hPa | `U`, `V`, `T`, `FI` | Both models compute these five surfaces. The height is the geopotential divided by 9.80665. A surface under the model's ground is not delivered: at Tegernsee the 950 hPa surface lay at 436 m and the instance answered it with the wind at 10 m |

Not asked for: the surfaces of 925, 900 and 800 hPa, which ICON-D2 does not compute and Open-Meteo fills from their neighbours; ICON-EU's hours after the 78th of a run, which Open-Meteo fills from values three hours apart; and anything "seamless".

**What a soaring forecast needs and this does not have:**

- **Boundary-layer height.** DWD publishes the mixing layer height (`MH`) for ICON-D2 and ICON-EU, since September 2026 on the native grid. Open-Meteo does not take it. Without it there is no height to which thermals reach.
- **Thermal strength.** No field of either model is a climb rate a glider sees. Deriving one (from the boundary-layer height and the heat flux at the ground) is a model of its own; Venti has none and shows none.
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
| DWD radar composite | 5-minute precipitation, Germany, weak behind the main Alpine ridge | CC BY 4.0 | Open |
| GeoSphere Austria nowcast and INCA | 15-minute analysis and three-hour nowcast | CC BY 4.0 | Open |
| RainViewer | Radar tiles | Free for small community use with credit; limits tightened in 2026 | Open with limits |
| Blitzortung | Lightning | Use in apps needs written permission | Ask |

No open real-time lightning source was found.

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
| OpenStreetMap free-flying tags | Takeoffs and landings, asked for through an Overpass instance: one request a day for a region's box | ODbL 1.0, verified 2026-11-19; share-alike, kept as a separate layer | **Built**. Coverage measured: thin, see below |
| ParaglidingEarth | Worldwide sites, variable quality | CC BY-SA 3.0, newer contributions also ODbL | Open, share-alike, separate layer. Not used |
| **DHV site database** | The authoritative list of approved sites for Germany, also covering Austria; bulk files exist | No terms published | **Ask**. Not used |
| Our own community edits | Site pages improved by pilots | Ours | Grows over time |

**OpenStreetMap, as read on its own pages on 2026-11-19:**

- *Licence.* [openstreetmap.org/copyright](https://www.openstreetmap.org/copyright): "OpenStreetMap is open data, licensed under the Open Data Commons Open Database License (ODbL) by the OpenStreetMap Foundation (OSMF). In summary: You are free to copy, distribute, transmit and adapt our data, as long as you credit OpenStreetMap and its contributors. If you alter or build upon our data, you may distribute the result only under the same license." The last sentence makes it share-alike: sites from OpenStreetMap are a layer of their own and are merged with no other source's records.
- *Credit.* The Foundation's [attribution guidelines](https://osmfoundation.org/wiki/Licence/Attribution_Guidelines): "Attribution must be to 'OpenStreetMap'. Attribution must also make it clear that the data is available under the Open Database License. This may be done by making the text 'OpenStreetMap' a link to openstreetmap.org/copyright"; "The historical forms of attribution '© OpenStreetMap contributors' or '© OpenStreetMap' are acceptable"; the credit must be visible without interaction. For data passed on as data, which is what the API does: "You must include attribution to OpenStreetMap and either the text of the ODbL or a link to it as part of the database". The manifest's credit line is "© OpenStreetMap contributors", linked to the copyright page, and its `notice` names the licence with its address, so both travel with every answer.
- *The tags.* [Tag:sport=free_flying](https://wiki.openstreetmap.org/wiki/Tag:sport%3Dfree_flying), an approved scheme: `free_flying:takeoff`, `:landing`, `:towing`, `:training` with `yes`, `no`, `discouraged` or `permissive`; `direction` for "in which wind directions this site is usable"; `ele` for the height; `free_flying:official`, `:unsafe`, `:paragliding`, `:hanggliding`. The page lists `free_flying:site=takeoff|landing|towing|training` and `free_flying:site_orientation` as deprecated. In the first region nearly every element still carries the deprecated keys: 382 against 25.
- *Directions.* [Key:direction](https://wiki.openstreetmap.org/wiki/Key:direction): degrees from 0, clockwise from north, or one of the sixteen compass points "using up to three uppercase characters [NWSE]"; a range's "sector is calculated clockwise from the given range border values". [Key:ele](https://wiki.openstreetmap.org/wiki/Key:ele): "Elevation of a point above sea level, in metres."

**How the data is fetched.** The adapter sends one query in Overpass QL to an Overpass API instance, as a GET, for the box of the run; the answer for the first region is 165 kB. Which instance is a setting of the server (`overpass_url`), as the operator of the main one recommends: "Die URLs unserer Services sollten nicht in der App fest einprogrammiert werden". The default is that main instance, `overpass-api.de`, run by FOSSGIS e.V. What its operator asks, read on the [OpenStreetMap wiki's list of instances](https://wiki.openstreetmap.org/wiki/Overpass_API#Public_Overpass_API_instances), in the [Overpass manual](https://dev.overpass-api.de/overpass-doc/en/preface/commons.html) and in [FOSSGIS's terms](https://www.fossgis.de/arbeitsgruppen/osm-server/nutzungsbedingungen/):

- "If you set something up that uses the Overpass API regularly, then [...] less than 100 queries fetching less 10 MB of data per day [is] fine." The server sends one query a day per source, and the manifest's budget allows a run two requests.
- "No parallel running of multiple scripts", and scripts "sind auf eine Verbindung zu beschränken". The manifest allows one request at a time.
- A User-Agent that identifies the application. The server sends `venti-server/<version>`. FOSSGIS also asks that an operator's e-mail address be easy to find on the application's website: **for the owner**, with the contact line that is already open in [plan.md](plan.md#needed-from-the-owner).
- "Commercial use should use self-hosted or paid Overpass servers." The project is not commercial.
- A link to `https://www.openstreetmap.org/fixthemap`, "um Ihren Nutzern zu erlauben, Fehler in unseren Daten zu melden". It is in the manifest's `notice`, and every site record carries the address of its own element.
- "Nowadays this server is overloaded [...] do not expect high reliability." A run that fails changes nothing and is tried again a day later.

Regional extracts (Geofabrik) were the alternative. Bavaria and Austria are some 1.5 GB of download a day and need a reader for the PBF format, to keep a few hundred points: not chosen.

**Recorded from another instance.** Neither `overpass-api.de` nor `overpass.private.coffee` accepted a connection from the machine the fixtures were recorded on (2026-11-19: the first reset it, the second timed out). The answer in the repository is from "Overpass Europe", `ovp.dresel.systems`, which the wiki lists as "Free to use, no API key" with Geofabrik's extract of Europe, updated nightly. It took 53 seconds for the first region, and the server's fetcher gives a request 120. *To verify* on the first installation: that the server's own run against `overpass-api.de` works from where the server stands, and how long it takes.

**Coverage, measured for the first region** (46.3 to 48.5° N, 9.4 to 17.2° E; OpenStreetMap as of 2026-11-18 21:21 UTC; the test `the_first_region_holds_what_was_measured` keeps these numbers):

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

That is thin. The first region holds the busiest flying sites of two countries, and well-known ones are missing or bare. Looked for by name on 2026-11-19 and not found as a take-off: Wallberg (two unnamed ramps "vermutlich für Drachenflieger", no paraglider take-off, no landing), Brauneck (three take-offs without a name, found by their position), Tegelberg, Hochries by name (its take-offs are "Startplatz West" and "Startplatz Nord"), Rauschberg, Osterfelder, Hochgrat, Buchenberg, Hörnle, Laber, Herzogstand, Jochberg, Hohe Salve, Zettersfeld, Emberger Alm, Schöckl, Gaisberg, Stoderzinken, Krippenstein, Hochkar, the Nordkette above Innsbruck, Rofan, Penken, Wildkogel. Found: Kampenwand, Unternberg, Nebelhorn, Wank, Breitenberg, Unterberghorn, Bischling, Elfer and Kreuzjoch in the Stubai valley, Choralpe, Gerlitzen, Niedere, Planai. A search by name misses sites that are in the data under another name, so the list of the missing is an upper bound. How many sites there are in truth was not counted; the DHV's database is the reference and may not be used.

What follows: the layer is useful where it has data and must never be shown as complete. A pilot who knows a missing site can add it to OpenStreetMap, and it is in the app a day later. Whether to seed OpenStreetMap ourselves, and what from, is **for the owner**; the DHV's data may not be copied into it either.

*Not read from OpenStreetMap, and why:* `website`, `operator`, `description`, `fee`, `opening_hours`, `wind_speed`, `direction:ideal`, `free_flying:guest-guidelines` are there for some sites and would make a site page; the record has no place for them yet. `free_flying:orientation`, an undocumented spelling on two elements, is not read.

### Thermal maps

| Source | Provides | Licence | Status |
|---|---|---|---|
| thermal.kk7.ch | Thermal hotspots and skyways as map tiles | CC BY-NC-SA 4.0; the author asks to be contacted with expected load | Ask, then use as a separate overlay |
| Our own flights | Hotspots and lines computed from uploaded tracks | Ours | Later |

### Webcams

No webcam network in the region has an open licence. Webcams are **link only**: the app lists them and opens the operator's image or page, with permission requested from the main operators (foto-webcam.eu, Panomax). The Windy webcams API has a free tier with short-lived image links and a required link back.

### Live tracking

| Source | Provides | Licence | Status |
|---|---|---|---|
| Open Glider Network | One feed carrying FLARM, FANET, SafeSky, PureTrack, satellite trackers and others | ODbL; honour opt-outs; nothing older than 24 hours | Open |
| Our own apps | Positions from pilots flying with the app | Ours | |
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
| basemap.at | Austrian official map, terrain and orthophoto | CC BY 4.0 | Open |
| Bavarian survey office open data | DTK25 topographic map, DOP20 orthophoto, DGM1 | CC BY 4.0 | Open |
| basemap.de | German official vector map | CC BY 4.0 | Open |
| swisstopo | Swiss maps and imagery | Free with credit | Open (for later) |
| Sentinel-2 cloudless 2016 | 10 m imagery fallback | CC BY 4.0 (later years are non-commercial share-alike) | Open |
| OpenTopoMap | Raster server shut down in January 2026 | | Skip |

## Outreach

Permissions to request, roughly in order of how much they block. AI sessions draft the emails; the owner sends them.

| To | Asking for | Blocks |
|---|---|---|
| Holfuy | Permission and an API key to show and re-serve station data in a free, open-source app | Wind at launch sites |
| DHV | Permission to use and re-serve the site database | Authoritative site data |
| XContest | Use of the airspace activation data; flight cross-posting; acceptance of tracks recorded by the app | Airspace activations, community adoption |
| thermal.kk7.ch | Use of thermal and skyways tiles, with expected load | Thermal overlay |
| DFS | Written confirmation of the CC BY 4.0 terms. Since 2026-11-19 they are known to be stated on DFS's own page, per data set; whether a letter is still wanted is the owner's decision | German airspace |
| Austro Control | Permission to re-serve airspace and obstacle files | Austrian airspace |
| Land Tirol | Confirmation of licence for the station files and the obstacle dataset; they ask to be notified of use | Tirol stations, cables |
| Avalanche services Bavaria, Salzburg, Vorarlberg, Carinthia | Access to station data | Mountain wind |
| foto-webcam.eu, Panomax | Permission to list and link images | Webcams |
| Transitous | A heads-up before using the public API from an assistant | Transit bootstrap |

## Known changes

| Date | Change | Effect |
|---|---|---|
| 2026-11-30 | DWD switches off the old ICON-D2 and ICON-EU download paths and regular grids; the new feed is on the native triangular grid only | Nothing to do for the forecasts Venti serves: Open-Meteo reads the new feed already, and the adapter asks for its models on the native grid. A raw ICON adapter has to read the triangular grid. See [what the change means](#what-dwds-change-of-2026-11-30-means). |
| 2026-11-04 | GeoSphere Austria replaced its AROME dataset with C-LAEF 1 km | Use the new dataset identifiers |
| Early 2027 | GeoSphere Austria's 2.5 km endpoint changes again | Canary must cover it |
