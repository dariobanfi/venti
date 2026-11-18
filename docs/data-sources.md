# Data sources

Last revised: 2026-11-18

The app depends on dozens of external data providers, and the set differs by country. This document defines how they plug in, and lists what is available for the first region.

Status words used in the catalogue:

- **Open:** licence allows our use and re-serving. Build the adapter.
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

The traits are small. Sketch:

```rust
#[async_trait]
pub trait ObservationSource: Send + Sync {
    fn manifest(&self) -> &Manifest;
    /// Stations this provider has inside the given area.
    async fn stations(&self, area: &Area, ctx: &FetchCtx) -> Result<Vec<RawStation>, SourceError>;
    /// Observations newer than `since`.
    async fn observations(&self, since: Timestamp, ctx: &FetchCtx) -> Result<Vec<RawObservation>, SourceError>;
}
```

`FetchCtx` supplies the HTTP client, credentials, the clock and the rate limiter. Adapters never create their own, which is what makes them testable from recorded payloads.

On-device instrument drivers follow the same idea in the `sensors` crate: one parser per protocol, fed bytes, tested from recorded streams.

### Manifest

```toml
id          = "geosphere-tawes"
provider    = "GeoSphere Austria"
kinds       = ["observations"]
coverage    = { countries = ["AT"] }
cadence     = "10min"
rate_limit  = { per_hour = 240 }
auth        = "none"

[licence]
name            = "CC BY 4.0"
commercial      = true
redistribution  = true
share_alike     = false
attribution     = "Datenquelle: GeoSphere Austria"
constraints     = []
verified_on     = "2026-11-18"
verified_from   = "https://data.hub.geosphere.at/en/legal"
```

The manifest is data, not comments. The server reads it to:

- build the attribution screen and the map credit line;
- enforce rate limits and schedules;
- apply licence constraints automatically (see below);
- exclude non-commercial sources from a build with one switch, should the project ever take money.

### Region pack

```toml
id        = "alps-de-at"
name      = "Germany and Austria"
boundary  = "boundary.geojson"
languages = ["de", "en"]

[observations]
sources  = ["dwd-10min", "geosphere-tawes", "lwd-tirol", "southtyrol-meteo", "metar", "ogn-fanet-stations"]
priority = ["dwd-10min", "geosphere-tawes"]   # wins when two sources report the same station

[airspace]
sources  = ["dfs-aixm", "austrocontrol-openair"]
fallback = ["openaip"]

[map]
elevation = "mapterhorn"
layers    = ["osm-alpine", "basemap-at", "bayern-dtk25"]
imagery   = ["basemap-at-ortho", "bayern-dop20", "s2cloudless-2016"]
```

### Pipeline

Every payload goes through the same stages, in the `sources` crate:

1. **Fetch.** The scheduler calls the adapter within its rate limit and cadence.
2. **Archive.** The raw payload is written to disk and kept for a rolling window. A bug report can be replayed exactly.
3. **Normalise.** The adapter converts to canonical types: SI units, UTC, WGS84, heights above mean sea level.
4. **Validate.** Schema and range checks. Bad records are quarantined and counted; they never reach the store.
5. **Resolve.** The same real-world object from two providers is matched (by position, name and type) and linked in a mapping table. The region pack's priority decides which fields win. Every stored field records its source.
6. **Store and publish.** Canonical records go to the database; changes are published to the live channel and to the region-bundle builder.

### Licence rules built into the pipeline

- **Different licence families never merge into one record or one bundle.** Non-commercial and share-alike sources (openAIP, thermal.kk7.ch, ParaglidingEarth, recent Sentinel-2 mosaics) stay separate layers with their own licence tag. Resolution links their records to others but does not copy fields across.
- **Constraints are declared and enforced.** For example the Open Glider Network requires honouring pilots' opt-outs and forbids redistributing positions older than 24 hours. Those are manifest constraints that the live service applies; they are not left to each call site.
- **Credentials live in server configuration,** never in the repository.

### Health

Each adapter reports last success, record counts, quarantine counts and data age. The server exposes a health page. Every data point shown to a pilot carries its age, and a stale source is shown as stale. One failing adapter is isolated by a circuit breaker and does not affect the others.

### Testing

Every adapter crate contains:

- **Fixtures:** real recorded payloads, including at least one malformed one.
- **Golden files:** the expected canonical output for each fixture. CI fails if normalisation changes.
- **A canary:** a scheduled job that calls the live endpoint and alerts when the response no longer parses. Providers change formats without notice; two such changes are already dated in [Known changes](#known-changes).

### Adding a provider

1. Confirm the licence on the provider's own page and record it in the manifest with the date and URL.
2. Create `crates/sources-<provider>` implementing the relevant traits.
3. Record fixtures and golden files.
4. Add the adapter to the registry and to the region packs that should use it.
5. Add the canary.

### Adding a region

1. List the providers for each kind and their licences.
2. Add the missing adapters.
3. Write the region pack and its boundary.
4. Build the map and terrain extracts for the area.

## Catalogue: Germany and Austria

### Live wind and weather stations

| Source | Provides | Licence | Status |
|---|---|---|---|
| DWD open data, 10-minute observations | Wind, gusts, temperature across Germany | CC BY 4.0 | Open |
| GeoSphere Austria, TAWES 10-minute | Austrian station network, real time | CC BY 4.0; 240 requests an hour, so fetch all stations in one call | Open |
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
| DFS AIP data sets | Official German airspace as AIXM, per AIRAC cycle, no key | Reported as CC BY 4.0 since November 2026 *(to verify in writing)* | Open after confirmation |
| DAeC OpenAir file | Germany including glider sectors | No licence stated | Ask |
| Austro Control | Austrian airspace as OpenAir and KML | Own terms; re-serving not addressed | Ask |
| openAIP | Worldwide airspace, airports, obstacles, hotspots | CC BY-NC 4.0 | Open, non-commercial, kept as a separate layer |
| XContest airspace service | Airspace curated for paragliding with activation status | No published terms | Ask |

**Activation times are the gap.** There is no open machine-readable feed of which restricted areas are active today, in either country. XContest maintains one by hand. Austria publishes a daily plan as a PDF. Until this is solved, the app shows restricted areas as potentially active and says so.

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
| DFS | Written confirmation of the CC BY 4.0 terms | German airspace |
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
