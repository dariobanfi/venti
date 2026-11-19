# The server's HTTP API

Last revised: 2026-11-19

The server answers a small JSON API that the phone and web apps use for the live wind map. This document describes it for people. The exact wire format is the Rust crate `venti-api-types`, which the server builds its answers from and the clients decode them with; when this text and that crate disagree, the crate is right and this text has a bug.

Everything here is version 1. There are no accounts yet: every endpoint is public and read-only.

## Try it

```bash
cargo run -p venti-server -- dev
curl 'http://127.0.0.1:8080/v1/stations?bbox=10.9,47.0,11.5,47.5'
```

The development mode serves this API over recorded data: six stations, three German and three Austrian, as they were on 2026-11-18 at 19:05 UTC. It needs no network and no database, its clock stands still at that moment, and every start gives the same answers. The examples below are its answers; the tests keep them in `crates/server/tests/golden`.

## Rules that hold for every endpoint

**Versions.** Every path starts with `/v1`. Within a version the format only grows: an answer may gain fields, so a client ignores fields it does not know. Removing or renaming a field, or changing what one means, needs a `/v2` beside `/v1`.

**Units.** Times are UTC in RFC 3339 (`2026-11-18T18:20:00Z`). Positions are WGS84 degrees. Heights are metres above mean sea level. Wind speed is metres per second, and wind direction is the direction the wind comes from, in degrees clockwise from true north. Temperature is degrees Celsius, pressure is hectopascal at the station's own height, humidity is percent.

**Missing values.** A value that is not known is left out of the answer. It is never `null` and never a placeholder number. A station without a temperature sensor has readings without `temperature_c`.

**Sources and credit.** Every station comes from exactly one data source and names it in `source`. The same physical station reported by two sources appears twice: data under different licences is never merged into one record. Every answer that carries data has an `attributions` list with one entry for each source that has data in it, and only those:

| Field | Meaning |
|---|---|
| `source` | The source's identifier, as records name it |
| `provider` | Who publishes the data |
| `licence` | The licence's name |
| `text` | The credit line, worded as the provider asks |
| `url` | Where the credit line links to |

A client that shows the data shows the credit of its source next to it. The licences require that; it is not decoration.

**Caching.** Answers with data carry `Cache-Control: public, max-age=60` and an `ETag`. A client that asks again with `If-None-Match` gets `304 Not Modified` without a body when nothing changed. Observations arrive every ten minutes, so most repeated requests end that way. Health answers and errors carry `Cache-Control: no-store`.

**Compression.** Answers are gzip-compressed when the request says `Accept-Encoding: gzip`. The station list of the whole first region is about 106 kB plain and 15 kB compressed.

**Browsers.** A web app on another origin may call the API only if the server's configuration lists that origin (`cors_origins`). Apps that are not browsers are not affected.

**Errors.** Every answer with a 4xx or 5xx status has the same body:

```json
{
  "error": {
    "code": "invalid_bbox",
    "message": "the bbox parameter is missing; give bbox as west,south,east,north in degrees, for example bbox=10.5,47.2,11.9,47.8"
  }
}
```

`code` is stable and meant for programs. `message` explains the problem to the person writing the client; it may change and is not meant for pilots.

| Status | Code | When |
|---|---|---|
| 400 | `invalid_bbox` | `bbox` is missing, is not four numbers, or the numbers do not form a box |
| 400 | `invalid_station_id` | The identifier in the path is not of the form `source:local` |
| 400 | `invalid_time` | `from` or `to` is not an RFC 3339 time |
| 400 | `invalid_range` | `from` lies after `to`, or the window is longer than seven days |
| 400 | `invalid_request` | The request could not be read at all, for example a path that is not valid UTF-8 |
| 404 | `station_not_found` | No station has this identifier |
| 404 | `not_found` | The path is not part of the API |
| 405 | `method_not_allowed` | The path exists, but only for `GET` and `HEAD` |
| 413 | `payload_too_large` | The request carried a body of more than 16 kB; the API expects none |
| 503 | `unavailable` | The server cannot reach its database. `Retry-After` says when to try again |
| 503 | `timeout` | The server could not answer within ten seconds |
| 500 | `internal` | The server failed in a way it did not expect; the cause is in its log |

A client must expect codes it does not know.

## `GET /v1/health`

How the server and each of its data sources are doing.

```json
{
  "status": "ok",
  "version": "0.1.0",
  "time": "2026-11-18T19:05:00Z",
  "sources": [
    {
      "source": "dwd-10min",
      "provider": "Deutscher Wetterdienst",
      "cadence_seconds": 600,
      "last_success": "2026-11-18T19:05:00Z",
      "newest_observation": "2026-11-18T18:20:00Z",
      "stale": false,
      "stations": 3
    },
    {
      "source": "geosphere-tawes",
      "provider": "GeoSphere Austria",
      "cadence_seconds": 600,
      "last_success": "2026-11-18T19:05:00Z",
      "newest_observation": "2026-11-18T19:00:00Z",
      "stale": false,
      "stations": 3
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `status` | `ok` when no source is stale, otherwise `degraded` |
| `version` | The server's version |
| `time` | The server's clock. Compute the age of an observation against this, not against the device's clock, which may be wrong |
| `sources[].cadence_seconds` | How often the server asks the provider for new data |
| `sources[].last_success` | When a run last delivered observations without trouble. Left out if none ever did |
| `sources[].last_failure` | When a run last delivered nothing, or had more than a tenth of its requests fail. Left out if none ever did |
| `sources[].newest_observation` | The time of the newest observation the source has delivered |
| `sources[].stale` | `true` when no run has succeeded within three cadence intervals |
| `sources[].stations` | How many stations the last successful run delivered |

The status code is `200` whenever the server can read its database, also when sources are stale: a monitor that only looks at the code learns whether the server works, and the body says whether its data is fresh. When the database cannot be reached the answer is `503` with the error body.

`newest_observation` can lag behind `last_success`. A run succeeds when the provider answers, and some providers publish late: on 2026-11-19 the German values were 25 to 44 minutes behind real time when fetched.

## `GET /v1/stations?bbox=west,south,east,north`

The stations inside a box, each with its newest observation.

`bbox` is required: four numbers in degrees, in the order west, south, east, north. Stations on the edge are included. A box that crosses the 180th meridian is not supported.

```bash
curl 'http://127.0.0.1:8080/v1/stations?bbox=10.9,47.4,11.0,47.5'
```

```json
{
  "stations": [
    {
      "id": "dwd-10min:05792",
      "source": "dwd-10min",
      "name": "Zugspitze",
      "position": {
        "lat": 47.421,
        "lon": 10.9848
      },
      "elevation_m": 2956.0,
      "latest": {
        "time": "2026-11-18T18:20:00Z",
        "wind_speed_ms": 14.9,
        "wind_gust_ms": 25.7,
        "wind_direction_deg": 260.0,
        "gust_direction_deg": 260.0,
        "temperature_c": -7.1,
        "humidity_pct": 91.8,
        "pressure_hpa": 693.4
      }
    }
  ],
  "truncated": false,
  "attributions": [
    {
      "source": "dwd-10min",
      "provider": "Deutscher Wetterdienst",
      "licence": "CC BY 4.0",
      "text": "Quelle: Deutscher Wetterdienst",
      "url": "https://www.dwd.de/DE/service/rechtliche_hinweise/rechtliche_hinweise_node.html"
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `stations[].id` | The station's identifier, `source:local`. Use it in the observations path |
| `stations[].source` | The source that reports the station |
| `stations[].name` | The name as the provider publishes it |
| `stations[].position` | Latitude and longitude |
| `stations[].elevation_m` | Height above mean sea level. Left out if the provider gives none |
| `stations[].latest` | The newest observation the server holds, however old. Left out if it holds none |
| `latest.time` | End of the measuring interval |
| `latest.wind_speed_ms`, `wind_gust_ms` | Mean wind and strongest gust of the interval |
| `latest.wind_direction_deg`, `gust_direction_deg` | Where the wind and the strongest gust came from |
| `latest.temperature_c`, `humidity_pct`, `pressure_hpa` | Air temperature, relative humidity, pressure at the station |
| `truncated` | `true` when the box holds more than 5000 stations. The answer then lists the first 5000 by source and identifier; ask for a smaller box |

Stations are sorted by source and then by identifier.

The server does not decide what is too old to show. A station that stopped reporting yesterday is listed with yesterday's reading and its time; the client greys it out or hides it.

## `GET /v1/stations/{id}/observations?from=&to=`

What one station measured in a window of time, oldest first.

| Parameter | Meaning | Default |
|---|---|---|
| `id` | A station identifier from the station list. Characters outside letters, digits and `-._~:` must be percent-encoded | |
| `to` | End of the window, included | The server's current time |
| `from` | Start of the window, included | 24 hours before `to` |

A window may be at most seven days long. Write times with `Z`, as in `2026-11-18T18:00:00Z`: the `+` of an offset such as `+01:00` means a space in a URL and must be written `%2B`.

```bash
curl 'http://127.0.0.1:8080/v1/stations/dwd-10min:05792/observations?from=2026-11-18T18:00:00Z&to=2026-11-18T19:05:00Z'
```

```json
{
  "station": {
    "id": "dwd-10min:05792",
    "source": "dwd-10min",
    "name": "Zugspitze",
    "position": {
      "lat": 47.421,
      "lon": 10.9848
    },
    "elevation_m": 2956.0
  },
  "observations": [
    {
      "time": "2026-11-18T18:00:00Z",
      "wind_speed_ms": 11.9,
      "wind_gust_ms": 27.3,
      "wind_direction_deg": 300.0,
      "gust_direction_deg": 260.0,
      "temperature_c": -7.2,
      "humidity_pct": 91.5,
      "pressure_hpa": 693.3
    },
    {
      "time": "2026-11-18T18:10:00Z",
      "wind_speed_ms": 13.5,
      "wind_gust_ms": 28.1,
      "wind_direction_deg": 270.0,
      "gust_direction_deg": 260.0,
      "temperature_c": -7.2,
      "humidity_pct": 92.0,
      "pressure_hpa": 693.4
    },
    {
      "time": "2026-11-18T18:20:00Z",
      "wind_speed_ms": 14.9,
      "wind_gust_ms": 25.7,
      "wind_direction_deg": 260.0,
      "gust_direction_deg": 260.0,
      "temperature_c": -7.1,
      "humidity_pct": 91.8,
      "pressure_hpa": 693.4
    }
  ],
  "attributions": [
    {
      "source": "dwd-10min",
      "provider": "Deutscher Wetterdienst",
      "licence": "CC BY 4.0",
      "text": "Quelle: Deutscher Wetterdienst",
      "url": "https://www.dwd.de/DE/service/rechtliche_hinweise/rechtliche_hinweise_node.html"
    }
  ]
}
```

A station that exists but measured nothing in the window gives `200` with an empty `observations` list. A station the server does not know gives `404` with `station_not_found`.

How much history there is depends on the source. The German stations deliver the last hours with every run. The Austrian network delivers only its newest values, so its history grows by one reading per run from the moment the server first collected it. The server keeps observations for thirteen whole months by default.

## What is not here yet

Accounts, flight upload, forecasts, sites, airspace, live positions and the assistant connector are planned and not built; see [plan.md](plan.md). Station metadata is planned to move into downloadable region bundles ([architecture.md](architecture.md#api-and-offline-sync)); the station list will keep working when it does.
