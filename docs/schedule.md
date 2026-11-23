# What runs by itself

Written by `crates/server/tests/schedule.rs` from the manifests and the server's constants; do not edit. After a change: `VENTI_UPDATE_GOLDEN=1 cargo test -p venti-server --test schedule`.

There is no cron: one server process does all of this, each job in a loop of its own. A wait is from the start of one run to the start of the next. [running-a-server.md](running-a-server.md#what-runs-by-itself) says how to look at it.

## Data sources

A source runs when a region pack names it (`regions/*/region.toml`), first when the server starts unless its latest run was more recent than its wait, then at its cadence. After a failed run it waits the time below, doubling with each further failure; a restart keeps to it. Every wait has up to a tenth of it added at random. The cadence and the limits are set in the source's manifest, the wait after a failure by `rate_limit.retry_failed_run_after_hours` there or else by `crates/server/src/ingest/schedule.rs`.

| Source | What | Every | After a failed run | Provider's limits | Manifest |
| --- | --- | --- | --- | --- | --- |
| `arpa-lombardia` | station readings | 30 min | 30 min, then 1 h | none stated | `crates/sources-arpalombardia/manifest.toml` |
| `arso-ams` | station readings | 4 min | 4 min, 8 min, 16 min, 32 min, then 1 h | none stated | `crates/sources-arso/manifest.toml` |
| `awc-metar` | station readings | 10 min | 10 min, 20 min, 40 min, then 1 h | 1 a second, 6000 an hour, 1000 a run at most | `crates/sources-awc/manifest.toml` |
| `dwd-10min` | station readings | 10 min | 10 min, 20 min, 40 min, then 1 h | none stated | `crates/sources-dwd/manifest.toml` |
| `dwd-radar-ry` | rain radar | 5 min | 5 min, 10 min, 20 min, 40 min, then 1 h | 2 a second | `crates/sources-dwd-radar/manifest.toml` |
| `ffvl-sites` | flying sites | 24 h | 24 h | 1 a second, 4 an hour, 4 a run at most | `crates/sources-ffvl/manifest.toml` |
| `geosphere-inca-rr` | rain radar | 15 min | 15 min, 30 min, then 1 h | 1 a second, 100 an hour, 25 a run at most | `crates/sources-geosphere-radar/manifest.toml` |
| `geosphere-tawes` | station readings | 10 min | 10 min, 20 min, 40 min, then 1 h | 4 a second, 120 an hour, 20 a run at most | `crates/sources-geosphere/manifest.toml` |
| `meteoswiss-smn` | station readings | 10 min | 10 min, 20 min, 40 min, then 1 h | 2400 an hour, 400 a run at most | `crates/sources-meteoswiss/manifest.toml` |
| `meteotrentino` | station readings | 15 min | 15 min, 30 min, then 1 h | 2 a second, 240 an hour, 60 a run at most | `crates/sources-meteotrentino/manifest.toml` |
| `open-meteo` | a look at the forecasts | 10 min | 10 min, 20 min, 40 min, then 1 h | 4 a second, 400 an hour | `crates/sources-openmeteo/manifest.toml` |
| `openaip` | airspace | 24 h | 24 h | none stated | `crates/sources-openaip/manifest.toml` |
| `osm-cables` | cables | 7 days | 2 days | 1 a second, 120 an hour, 120 a run at most, 30 s between two, a refused request asked again after 2 min | `crates/sources-osm/manifest-cables.toml` |
| `osm-free-flying` | flying sites | 24 h | 2 days | 1 a second, 3 an hour, 3 a run at most, a refused request asked again after 2 min | `crates/sources-osm/manifest.toml` |
| `pioupiou` | station readings | 5 min | 5 min, 10 min, 20 min, 40 min, then 1 h | 60 an hour, 5 a run at most | `crates/sources-pioupiou/manifest.toml` |
| `slf-imis` | station readings | 30 min | 30 min, then 1 h | none stated | `crates/sources-slf/manifest.toml` |
| `suedtirol-meteo` | station readings | 10 min | 10 min, 20 min, 40 min, then 1 h | none stated | `crates/sources-suedtirol/manifest.toml` |

Not on the schedule: `basemap-at` (map imagery: fetched when an app asks for a tile); `ign-ortho` (map imagery: fetched when an app asks for a tile); `natural-earth` (boundaries: built into the server, never fetched); `ogn` (live positions: a connection held open, below); `timezone-boundary-builder` (boundaries: built into the server, never fetched).

## The server's own jobs

| Job | When | After a failure | Where it is set |
| --- | --- | --- | --- |
| Makes the thermal map | with each new run of its models, which the look at `open-meteo` notices every 10 min | goes on at the next look, 10 min later | `crates/server/src/thermals.rs` |
| Checks the wind alarms | after each run of a source of station readings | the next run | `crates/server/src/alerts.rs` |
| Checks the day alarms | after each thermal map | the next map | `crates/server/src/alerts.rs` |
| Checks the alarms about pilots in the air | every 1 min | the next check | `AIR_CHECK_EVERY`, `crates/server/src/alerts.rs` |
| Sends a fired alarm to browsers and phones | when an alarm fires | tried again after 30 s and after 5 min, then given up | `PushSettings::new`, `crates/server/src/push/mod.rs`; `PhoneSettings::new`, `crates/server/src/push/phones.rs` |
| Reads the public flights for where thermals were | every 5 min, at once again while new flights wait | 5 min later | `EVERY`, `crates/server/src/flights/flown.rs` |
| Listens to the live network (`ogn`) | a connection held open, a keepalive every 4 min, dropped after 1 min of silence | connects again after 5 s, doubling up to 5 min | `crates/server/src/live/network.rs` |
| Fetches the live network's directory of devices | at the start, then every 24 h | 1 h later; after 3 days without one nothing is listed | `crates/server/src/live/network.rs` |
| Removes old observations and archived payloads | at the start, then every 24 h | the next time, 24 h later | `[retention]` `observation_months`, `archive_days`; `crates/server/src/maintenance.rs` |
| Removes sessions that were not used | at the start, then every 24 h | the next time, 24 h later | unused for 400 days: `Limits::session_idle`, `crates/server/src/accounts/mod.rs` |
| Removes closed reports, and logs how many are open | at the start, then every 24 h | the next time, 24 h later | `[retention]` `report_days` |
| Removes what the authorization server no longer needs | at the start, then every 24 h | the next time, 24 h later | programs unused for 30 days: `CLIENT_IDLE_LIFE`, `crates/server/src/accounts/oauth.rs` |
| Removes fired alarms and notices | at the start, then every 24 h | the next time, 24 h later | alarms fired 30 days ago, notices of 30 days ago: `FIRED_KEPT_DAYS`, `NOTIFICATIONS_KEPT_DAYS`, `crates/api-types` |
| Removes ended live tracks | at the start, then every 1 h | the next time, 1 h later | `[retention]` `live_hours` |
| Removes the plans of days gone by | at the start, then every 1 h | the next time, 1 h later | `[retention]` `plan_days` |
| Removes pilots' positions and ended requests for help | at the start, then every 1 h | the next time, 1 h later | positions older than 1 h, requests ended 24 h ago: `POSITION_KEPT_MINUTES`, `HELP_KEPT_HOURS` |
| Works stored flights out again after the analysis changed | once, at the start | the next start | `crates/server/src/flights/refresh.rs` |
| Reads and checks the map regions | once, at the start | the next start | `maps_dir`; `crates/server/src/maps.rs` |
| Trims the imagery cache | at the start, and when it holds more than its size | the next tile kept | `imagery_cache_mb`; `crates/server/src/imagery.rs` |
