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
| Point and grid forecasts | `ForecastSource` | `ForecastSeries`, `ForecastField` |
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
```

Region packs live in `regions/<id>/region.toml`. The area is a bounding box for now; a boundary polygon replaces it when a region needs a shape a box cannot express. Sections for the other kinds (map layers and so on) are added as those kinds are built.

A region may leave `[airspace]` out. Airspace from two sources is never merged, so `priority` there says which layer a client prefers where two cover the same air, not which fields win. A region's airspace sources are checked against the build's adapters like its observation sources: a server that cannot collect a region's airspace does not start.

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
| Every kind other than observations and airspace | Designed, not built |

A live run of the first region on 2026-11-18 delivered 50 DWD and 267 GeoSphere stations in under six seconds, using 104 and 2 requests.

On 2026-11-19 openAIP's export files for Germany and Austria were fetched by hand from the address the adapter uses, without a key: 756 and 262 airspaces, 2.0 MB and 0.8 MB, both written that night. The adapter and the pipeline read both whole files from disk: 1,018 airspaces, none skipped, none rejected, no type the adapter does not know. Of them the cockpit's warning table marks 637 to avoid, 134 as a caution and 247 as free. The OpenAir exports of the same two countries, read by the OpenAir reader of `venti-airspace`, give the same name, class, type and limits for every one of the 1,018. `VENTI_OPENAIP_DIR=<folder> cargo test -p venti-sources-openaip --test pipeline whole -- --nocapture` repeats the check on freshly fetched files.

Later that day the server's own fetcher ran the adapter against openAIP (`venti-server ingest regions/alps-de-at/region.toml --source openaip`): two requests, no failure, 344 airspaces in the first region's box, 257 of Austria dated 01:12 UTC and 87 of Germany dated 01:57 UTC by openAIP's file store. The same files, read whole by the server and served at `/v1/airspace`, are 1.7 MB as JSON and 369 kB with gzip for both countries; see [api.md](api.md#get-v1airspacebboxwestsoutheastnorth). The server has not yet run the source on its schedule against a real database for a day: *to verify* on the first installation.

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
| Self-hosted Open-Meteo | One API over ICON-D2 (about 2 km), ICON-EU, ICON-CH and ECMWF. Soaring-relevant fields include CAPE, updraft, convective cloud base and top, freezing level, cloud layers, wind at several heights | Data CC BY 4.0; code AGPL-3.0, run unmodified as a separate service. Needs 8 to 16 GB memory and at least 100 GB fast disk | Open |
| DWD ICON-D2 raw GRIB | Model levels, mixing height, maximum updraft | CC BY 4.0 | Open (later, for fields Open-Meteo lacks) |
| GeoSphere Austria C-LAEF 1 km | Alpine high-resolution model, hourly | CC BY 4.0 | Open (later) |
| ECMWF open data | Global 0.25° | CC BY 4.0 | Open |
| Soaringmeteo | WRF thermal forecasts for the Alps | Code GPL-3.0; data licence not stated | Ask |

Open-Meteo does not expose boundary-layer height or model-level winds for ICON. A proper thermal forecast will need our own pipeline on raw ICON-D2 data.

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
| openAIP | Worldwide airspace, airports, obstacles, hotspots. Airspace per country as GeoJSON and OpenAir files, rebuilt daily, no key | CC BY-NC 4.0, verified 2026-11-19, with a share-alike condition in openAIP's own terms; see below | **Built** for the airspace of Germany and Austria. Non-commercial, kept as a separate layer |
| XContest airspace service | Airspace curated for paragliding with activation status | No published terms | Ask |

**Activation times are the gap.** There is no open machine-readable feed of which restricted areas are active today, in either country. XContest maintains one by hand. Austria publishes a daily plan as a PDF. Until this is solved, the app shows restricted areas as potentially active and says so.

**openAIP, as read on its own pages on 2026-11-19:**

- *Licence.* The home page and the footer of every page: "OpenAIP data is licensed under the Attribution-NonCommercial 4.0 International (CC BY-NC 4.0) license." The legal page ([openaip.net/legal](https://www.openaip.net/legal)) adds terms of use: the data must be attributed so that "it must be clearly visible to any user that the used data is openAIP data or is based on openAIP data"; it may not be sold on its own but "you may ship openAIP data with paid/commercial applications as long as you don't exclusively sell openAIP data"; and "If you alter, transform, or build upon openAIP Data, you may distribute the resulting work only under the same or similar license to this one." That last sentence is a share-alike condition, which CC BY-NC 4.0 itself does not contain. The manifest declares the stricter reading. **For the owner to decide:** whether to ask openAIP which of the two holds.
- *Credit.* The wording openAIP offers, used as the manifest's credit line: "Data used comes from openAIP and is licensed under the Creative Commons Attribution-NonCommercial 4.0 International License. Visit https://www.openaip.net and contribute to better aviation data, free for everyone to use and share."
- *Warranty.* "OpenAIP data is not certified and must not be used for primary navigation or flight planning. Never rely on openAIP data! OpenAIP data contains errors. Using openAIP data may result in serious injury or death." (read again on 2026-11-19, in the script that draws [openaip.net/legal](https://www.openaip.net/legal); the page has no text without it). The first three sentences are the manifest's `notice`. The server sends it with every answer that carries openAIP's airspace, and the app must show it wherever it shows this data.
- *Access without a key.* The documentation ([openaip.net/docs](https://www.openaip.net/docs)): "The export files are stored in the public S3 object storage bucket openaip-system-exports and can be downloaded anonymously via HTTPS from: https://storage.openaip.net/openaip-system-exports/ ... The export files are created on a daily basis ... without authentication or credentials. Each file is available at `https://storage.openaip.net/openaip-system-exports/<filename>`, where the filename follows the pattern `<country>_<type>.<extension>`." Only the API needs an account and a key; the adapter does not use it. The owner has nothing to sign up for.
- *Format.* The adapter reads `<country>_asp.geojson`. It carries more than the OpenAir exports (hours, NOTAM and on-request flags, the period of temporary areas, a second floor, remarks, change times, a stable identifier) and gives corners in decimal degrees where the OpenAir exports round to whole seconds. The numbers in it are explained by openAIP's [schema](https://api.core.openaip.net/api/schemas/response/airspace/airspace-schema.json).
- *To verify:* whether the hours of operation are UTC or local; what tells "on demand" from "on request".
- *Gap.* Only Germany and Austria are fetched. The first region's box also takes in parts of Switzerland, Liechtenstein, Italy, Slovenia, Hungary, Slovakia and Czechia. Their airspace is missing until their codes are added to the manifest, which is one line and a recorded fixture each; the warning table has rules by class and type for any country and special ones only for Germany and Austria.
- *Gap.* The server keeps the airspace that reaches into its regions' box, not whole countries: German airspace north of 48.5° N is not in it. The API's answer does not say which area it covers. An app must not read "no airspace here" out of an empty answer.

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
| OpenStreetMap free-flying tags | Takeoffs and landings | ODbL | Open; coverage to be measured |
| ParaglidingEarth | Worldwide sites, variable quality | CC BY-SA 3.0, newer contributions also ODbL | Open, share-alike, separate layer |
| **DHV site database** | The authoritative list of approved sites for Germany, also covering Austria; bulk files exist | No terms published | **Ask** |
| Our own community edits | Site pages improved by pilots | Ours | Grows over time |

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
| 2026-11-30 | DWD switches off the old ICON-D2 and ICON-EU download paths and regular grids; the new feed is on the native triangular grid only | A raw ICON adapter needs grid remapping. Self-hosted Open-Meteo has already migrated. |
| 2026-11-04 | GeoSphere Austria replaced its AROME dataset with C-LAEF 1 km | Use the new dataset identifiers |
| Early 2027 | GeoSphere Austria's 2.5 km endpoint changes again | Canary must cover it |
