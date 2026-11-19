# The server's HTTP API

Last revised: 2026-11-19

The server answers a small JSON API that the phone and web apps use: for the live wind map, for airspace, and for a pilot's account and flights. This document describes it for people. The exact wire format is the Rust crate `venti-api-types`, which the server builds its answers from and the clients decode them with; when this text and that crate disagree, the crate is right and this text has a bug.

Everything here is version 1. It has two parts:

- **Weather stations and airspace**, which anyone may read: [health](#get-v1health), [stations](#get-v1stationsbboxwestsoutheastnorth), [observations](#get-v1stationsidobservationsfromto), [airspace](#get-v1airspacebboxwestsoutheastnorth).
- **Accounts and flights**: [accounts and sessions](#accounts-and-sessions), [flights](#flights), [pilots](#pilots), and [the page a shared flight has](#the-page-a-shared-flight-has).

## Try it

```bash
cargo run -p venti-server -- dev
curl 'http://127.0.0.1:8080/v1/stations?bbox=10.9,47.0,11.5,47.5'
```

The development mode serves this API over recorded data: six stations, three German and three Austrian, as they were on 2026-11-18 at 19:05 UTC, and 192 airspaces recorded from openAIP the day after and served as if fetched at that same moment: 88 of the northern Alps around Munich, Innsbruck and Salzburg, and 104 small ones from the six other countries the first region's box takes in. They are a sample. The development mode says of them what a real server says of its whole set, that they [cover](#get-v1airspacebboxwestsoutheastnorth) the first region, so that a screen can be built and tested in the state a pilot will see; nobody flies with them. It needs no network and no database, its clock stands still at that moment, and every start gives the same answers. The examples below are its answers; the tests keep them in `crates/server/tests/golden` and check that this document shows them unchanged.

It also starts with one account, so that an app can be built and tested against it without creating one first:

| | |
|---|---|
| Handle | `demo` |
| Password | `demo-pilot-password` |
| A session token that always works | `venti_0000000000000000000000000000000000000000000000000000` |
| Flights | Three made-up ones: one public (2026-08-15, it lands), one unlisted (2026-07-04, the sample file the apps bundle; its recording ends in the air), one private (2025-06-21, a short one) |

```bash
curl http://127.0.0.1:8080/v1/flights \
  -H 'Authorization: Bearer venti_0000000000000000000000000000000000000000000000000000'
```

None of this is a secret, and none of it exists on a real server. Anyone may create further accounts in this mode, without an invite; the limits on attempts are off, and uploads work. Whatever a client changes is forgotten when the program stops. The addresses of the demo flights are the same on every start.

## Rules that hold for every endpoint

**Versions.** Every path starts with `/v1`. Within a version the format only grows: an answer may gain fields, so a client ignores fields it does not know. Removing or renaming a field, or changing what one means, needs a `/v2` beside `/v1`.

**Units.** Times are UTC in RFC 3339 (`2026-11-18T18:20:00Z`). Positions are WGS84 degrees. Heights are metres above mean sea level, except the limits of an airspace, which are given as published; see [airspace](#get-v1airspacebboxwestsoutheastnorth). Wind speed is metres per second, and wind direction is the direction the wind comes from, in degrees clockwise from true north. Temperature is degrees Celsius, pressure is hectopascal at the station's own height, humidity is percent.

**Missing values.** A value that is not known is left out of the answer. It is never `null` and never a placeholder number. A station without a temperature sensor has readings without `temperature_c`.

**Sources and credit.** Every station and every airspace comes from exactly one data source and names it. The same physical station reported by two sources appears twice: data under different licences is never merged into one record. Every answer that carries data has an `attributions` list with one entry for each source the answer draws on, and only those:

| Field | Meaning |
|---|---|
| `source` | The source's identifier, as records name it |
| `provider` | Who publishes the data |
| `licence` | The licence's name |
| `text` | The credit line, worded as the provider asks |
| `url` | Where the credit line links to |
| `notice` | What the provider asks to be said wherever its data is shown, beside the credit. Left out when it asks for nothing of the kind |

A client that shows the data shows the credit of its source next to it, and the notice if there is one. The licences require that; it is not decoration.

**Caching.** Answers with station data carry `Cache-Control: public, max-age=60` and an `ETag`. A client that asks again with `If-None-Match` gets `304 Not Modified` without a body when nothing changed. Observations arrive every ten minutes, so most repeated requests end that way. Airspace changes once a day at most and carries `max-age=3600`. Health answers and errors carry `Cache-Control: no-store`. So does every answer to a request that carried a session token: it was made for one account, and nothing may keep it. What applies to shared flights is said [there](#flights).

**Compression.** Answers are gzip-compressed when the request says `Accept-Encoding: gzip`. The station list of the whole first region is about 106 kB plain and 15 kB compressed, its airspace about 1,240 kB and 290 kB.

**Browsers.** A web app on another origin may call the API only if the server's configuration lists that origin (`cors_origins`). Apps that are not browsers are not affected. The API never tells a browser to send cookies or other credentials of its own along (`Access-Control-Allow-Credentials` is never set), and it has no use for them: an account is acted for with a token that the page's own code puts into the `Authorization` header, and in no other way. A page on another site therefore cannot make a visitor's browser act for the visitor.

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
| 400 | `box_too_large` | The box holds more airspaces than one answer carries. Ask for smaller boxes |
| 400 | `invalid_station_id` | The identifier in the path is not of the form `source:local` |
| 400 | `invalid_time` | `from` or `to` is not an RFC 3339 time |
| 400 | `invalid_range` | `from` lies after `to`, or the window is longer than seven days |
| 400 | `invalid_request` | The request could not be read at all: a path that is not valid UTF-8, a body that is not the JSON the endpoint expects. For a body, `message` says where it stops making sense and never repeats what stands there |
| 400 | `invalid_handle`, `invalid_display_name`, `invalid_password` | A new account's handle, name or password is not as [described](#accounts-and-sessions) |
| 400 | `invalid_visibility` | `visibility` is not `private`, `unlisted` or `public` |
| 400 | `invalid_cursor`, `invalid_limit` | `after` or `since` is not a value the server gave out; `limit` is not a number from 1 to 100 |
| 401 | `unauthorized` | The request needs an account and carries no valid session token. The answer has `WWW-Authenticate: Bearer` |
| 401 | `invalid_credentials` | Signing in: the handle or the password is wrong |
| 403 | `wrong_password` | The password sent to confirm a change to the account is wrong |
| 403 | `registration_closed`, `invalid_invite` | The server takes no new accounts, or takes them by invite and the code is not good |
| 404 | `station_not_found` | No station has this identifier |
| 404 | `flight_not_found` | No flight has this address, or it is not the caller's to see. The two cannot be told apart |
| 404 | `session_not_found`, `pilot_not_found` | The account has no such session; no pilot has this handle |
| 404 | `not_found` | The path is not part of the API |
| 405 | `method_not_allowed` | The path exists, but not with this method |
| 409 | `handle_taken` | The handle belongs to another account or is not available |
| 413 | `payload_too_large` | The request carried a body of more than 16 kB, or a flight file of more than 4 MB |
| 422 | `invalid_flight` | The uploaded file is not a flight file the server can use |
| 429 | `rate_limited` | Too many attempts. `Retry-After` says when to try again |
| 503 | `unavailable` | The server cannot reach its database, or is too busy for this request. `Retry-After` says when to try again |
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
    },
    {
      "source": "openaip",
      "provider": "openAIP",
      "cadence_seconds": 86400,
      "last_success": "2026-11-18T19:05:00Z",
      "stale": false,
      "stations": 0,
      "airspaces": 192,
      "file_date": "2026-11-18T19:05:00Z"
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
| `sources[].last_success` | When a run last delivered its data without trouble. Left out if none ever did |
| `sources[].last_failure` | When a run last failed. For a source of observations: it delivered nothing, or more than a tenth of its requests failed. For a source of airspace: any request failed, because a set with a file missing must not replace a whole one. Left out if none ever did |
| `sources[].newest_observation` | The time of the newest observation the source has delivered. Left out for a source of airspace |
| `sources[].stale` | `true` when no run has succeeded for too long. For a source the server asks every few minutes: three cadence intervals. For a source it asks once a day or less often, which is airspace: two and a quarter intervals, 54 hours for a daily one. A failed daily run is tried again a day later, so one missed day is tolerated and the second is not |
| `sources[].stations` | How many stations the last successful run delivered. `0` for a source of airspace |
| `sources[].airspaces` | How many airspaces the server holds from the source: the set of its last successful run, which is what it serves. Left out for a source that delivers no airspace |
| `sources[].file_date` | The date of the oldest file that set was read from. Left out for a source that delivers no airspace |

The status code is `200` whenever the server can read its database, also when sources are stale: a monitor that only looks at the code learns whether the server works, and the body says whether its data is fresh. When the database cannot be reached the answer is `503` with the error body.

`newest_observation` can lag behind `last_success`. A run succeeds when the provider answers, and some providers publish late: on 2026-11-19 the German values were 25 to 44 minutes behind real time when fetched. `file_date` can lag in the same way: a run succeeds also when the provider hands out a file it has not renewed. openAIP's files of 2026-11-19 were written at 01:12 and 01:57 UTC.

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

## `GET /v1/airspace?bbox=west,south,east,north`

The airspace that touches a box, as the server last fetched it from its sources. A phone asks for the box of its whole region once, keeps the answer, and flies with it offline.

`bbox` is required and written as for stations. An airspace is found by the smallest box around its outline: one that curves past the requested box may be in the answer, and none that reaches into it is ever missing.

```bash
curl 'http://127.0.0.1:8080/v1/airspace?bbox=12.65,47.48,12.66,47.49'
```

```json
{
  "layers": [
    {
      "source": "openaip",
      "fetched_at": "2026-11-18T19:05:00Z",
      "file_date": "2026-11-18T19:05:00Z",
      "newest_change": "2026-11-18T10:29:12.552Z",
      "covers": [
        {
          "south": 46.3,
          "west": 9.4,
          "north": 48.5,
          "east": 17.2
        }
      ],
      "countries": [
        "DE",
        "AT",
        "CH",
        "IT",
        "SI",
        "HR",
        "HU",
        "SK"
      ],
      "airspaces": [
        {
          "id": "openaip:677d59be13e31af480aca6f6",
          "name": "LOD22 (HOCHFILZEN)",
          "class": "unclassified",
          "airspace_type": "danger",
          "country": "AT",
          "lower": {
            "reference": "ground",
            "metres": 0.0
          },
          "upper": {
            "reference": "standard",
            "value": 145.0,
            "unit": "flight-level",
            "metres": 4419.6
          },
          "geometry": {
            "exterior": [
              [
                12.6,
                47.525
              ],
              [
                12.625,
                47.475
              ],
              [
                12.69167,
                47.45
              ],
              [
                12.72222,
                47.5
              ],
              [
                12.6,
                47.525
              ]
            ]
          },
          "activity": {
            "by_notam": false,
            "on_request": false,
            "on_demand": false,
            "special_agreement": false
          },
          "validity": {
            "file_date": "2026-11-18T19:05:00Z",
            "updated": "2025-01-07T16:45:06.169Z"
          }
        },
        {
          "id": "openaip:677d59be13e31af480aca726",
          "name": "LOR8 (HOCHFILZEN)",
          "class": "unclassified",
          "airspace_type": "restricted",
          "country": "AT",
          "lower": {
            "reference": "ground",
            "metres": 0.0
          },
          "upper": {
            "reference": "standard",
            "value": 125.0,
            "unit": "flight-level",
            "metres": 3810.0
          },
          "geometry": {
            "exterior": [
              [
                12.6,
                47.525
              ],
              [
                12.625,
                47.475
              ],
              [
                12.69167,
                47.45
              ],
              [
                12.72222,
                47.5
              ],
              [
                12.6,
                47.525
              ]
            ]
          },
          "activity": {
            "by_notam": false,
            "on_request": false,
            "on_demand": false,
            "special_agreement": false
          },
          "validity": {
            "file_date": "2026-11-18T19:05:00Z",
            "updated": "2025-01-07T16:45:08.433Z"
          }
        }
      ]
    }
  ],
  "attributions": [
    {
      "source": "openaip",
      "provider": "openAIP",
      "licence": "CC BY-NC 4.0",
      "text": "Data used comes from openAIP and is licensed under the Creative Commons Attribution-NonCommercial 4.0 International License. Visit https://www.openaip.net and contribute to better aviation data, free for everyone to use and share.",
      "url": "https://www.openaip.net",
      "notice": "OpenAIP data is not certified and must not be used for primary navigation or flight planning. Never rely on openAIP data! OpenAIP data contains errors."
    }
  ]
}
```

**Layers.** Airspace comes in layers, one for each source the server holds airspace of, sorted by source. Every such source is in every answer, also when nothing of it touches the box: its layer then has no airspaces and still says how old the set is and [where it is whole](#where-a-layer-is-whole). Layers are never merged: each source has its own licence, its own credit and its own date. A client that stores or draws airspace keeps them apart in the same way, and shows with each layer the credit and the notice of its source. The same airspace published by two sources is two records in two layers.

| Field | Meaning |
|---|---|
| `layers[].source` | The source of the layer. Its credit, licence and notice are the entry of `attributions` with the same `source` |
| `layers[].fetched_at` | When the server fetched the set this layer is a part of. **Show the pilot how old this is**, measured against the server's clock (`time` of `/v1/health`) |
| `layers[].file_date` | The date of the oldest file the source's set was read from, by the source's own word. Where a source puts no date on its files, it is the time of the fetch. Left out if no record says |
| `layers[].newest_change` | When the source last changed any airspace of its set. Left out if the source does not say |
| `layers[].covers` | The boxes inside which the server's set of this source is whole; see [below](#where-a-layer-is-whole). A box is written with its four edges by name, in degrees: `{"south": 46.3, "west": 9.4, "north": 48.5, "east": 17.2}`. Empty, or missing in an answer of an older server, means the server does not say, and a client takes that as covering nothing |
| `layers[].countries` | The countries whose airspace the layer holds, as ISO 3166-1 alpha-2 codes. Empty if the server does not say |
| `layers[].airspaces` | The records of the layer that touch the box, sorted by `id`. May be empty |
| `attributions[].notice` | What the source asks to be said wherever its data is shown. For openAIP: that the data is not certified and must not be used for navigation. **Show it with the airspace.** Left out for a source that asks for nothing of the kind |

A record is the canonical airspace record of the project, the same that the apps read from an OpenAir file: `venti_domain::Airspace`. [airspace.md](airspace.md#the-record) explains every field. What a client needs to know here:

| Field | Meaning |
|---|---|
| `id` | `source:local`. The part before the colon is the layer's source |
| `name` | As published. May be empty |
| `class` | `A` to `G`, or `unclassified` |
| `airspace_type` | The kind: `ctr`, `tma`, `restricted`, `danger`, `glider-sector` and some forty more, or `{"other": "..."}` with the source's own word |
| `country` | ISO code. Left out if the source gives none |
| `lower`, `upper` | The floor and the ceiling as published: `reference` (`ground`, `above-ground`, `mean-sea-level`, `standard`, `unlimited`), `value` and `unit` (`feet`, `metres`, `flight-level`), and `metres`, the same height in metres above the same reference. These are the one exception to "heights are metres above mean sea level": a limit above ground or on standard pressure cannot be turned into one without the terrain or the air pressure |
| `lower_at_least`, `upper_at_most` | A second floor, and the height the ceiling may be raised to. Left out for most |
| `geometry.exterior` | The outline: corners as `[longitude, latitude]`, the first repeated at the end. `geometry.holes` and `geometry.circle` are left out for most |
| `activity` | What the source publishes about use: by NOTAM, on request, hours. It is background, not the state of today |
| `frequencies`, `remarks` | As published. Left out when there are none |
| `validity.file_date` | The date of the file this record was read from |
| `validity.updated` | When the source last changed this record. Left out if it does not say |

**No answer says what is active today.** The server knows no activations and no NOTAMs. A client treats every airspace as potentially active.

### Where a layer is whole

**No airspace in the answer is not "no airspace".** The server does not hold the world's airspace. It holds what reaches into the area it collects for, of the countries it fetches, and a layer says both:

- `covers` is the area the server collected the set for: one box around its regions. Inside it the layer holds every airspace the server could read from what the source publishes for the countries in `countries`. Outside it the layer holds nothing, or only what happens to reach out of it.
- `countries` are the countries that were fetched. Today those are Germany, Austria, Switzerland, Italy, Slovenia, Croatia, Hungary and Slovakia: every country the first region's box takes in that openAIP has a file for.

Both describe the server's set, not the request: they are the same whatever box was asked for. A client that wants to know whether a place is covered asks whether the place lies in one of the boxes of `covers`. It shows a pilot "no airspace near" only for a place inside; anywhere else it says that its airspace does not reach there. A glider at 49° N is outside the first region's box, which ends at 48.5° N, and must not be told its air is free because the answer had no airspace in it.

**A box can take in more than the countries, and the answer cannot say where a border runs.** For the first region this is settled as far as the source allows: all eight countries its box reaches into are fetched. (Czechia begins 6 km north of the box.) One country is left, Liechtenstein, for which openAIP publishes no file. The records that lie over it are in the Swiss file: the Swiss flight information region, the control area above the Alps and a glider area. Whether openAIP lacks anything over Liechtenstein is *to verify*. On a server whose regions reach into a country that is not in `countries`, the layer holds nothing of that country's airspace although the place is inside `covers`; a client therefore shows the pilot the countries with the airspace.

**Whole means every record the server could read.** A record the source publishes in a form the server cannot read, or one that fails the server's checks (a ceiling below its floor), is left out and named in the server's log. The run still counts, and the answer does not say that something is missing. On 2026-11-19 none of the 506 airspaces in the first region's box was left out. One Hungarian record far east of the box was: its ceiling is at the ground.

**What a warning means differs by country, and the app's table was written for two of them.** See [airspace.md](airspace.md#outside-germany-and-austria).

A layer without airspaces, for a quiet box inside the area:

```bash
curl 'http://127.0.0.1:8080/v1/airspace?bbox=14.90,48.20,14.91,48.21'
```

```json
{
  "layers": [
    {
      "source": "openaip",
      "fetched_at": "2026-11-18T19:05:00Z",
      "file_date": "2026-11-18T19:05:00Z",
      "newest_change": "2026-11-18T10:29:12.552Z",
      "covers": [
        {
          "south": 46.3,
          "west": 9.4,
          "north": 48.5,
          "east": 17.2
        }
      ],
      "countries": [
        "DE",
        "AT",
        "CH",
        "IT",
        "SI",
        "HR",
        "HU",
        "SK"
      ],
      "airspaces": []
    }
  ],
  "attributions": [
    {
      "source": "openaip",
      "provider": "openAIP",
      "licence": "CC BY-NC 4.0",
      "text": "Data used comes from openAIP and is licensed under the Creative Commons Attribution-NonCommercial 4.0 International License. Visit https://www.openaip.net and contribute to better aviation data, free for everyone to use and share.",
      "url": "https://www.openaip.net",
      "notice": "OpenAIP data is not certified and must not be used for primary navigation or flight planning. Never rely on openAIP data! OpenAIP data contains errors."
    }
  ]
}
```

The credit and the notice come with it: that the source has nothing there is a statement from its data, and no more certified than the rest.

**Caching.** The answer carries `Cache-Control: public, max-age=3600` and an `ETag`. The server fetches its sources once a day, so an answer changes once a day at most; a client that asks again with `If-None-Match` gets `304` until the server has fetched a new set.

**Size.** One answer carries the whole box; there are no pages. Measured on 2026-11-19 with openAIP's files of that day:

| Box | Airspaces | Corners | Plain | With gzip |
|---|---|---|---|---|
| The eight countries whole | 3,575 | 154,407 | 4,800 kB | 1,020 kB |
| The first region (46.3 to 48.5° N, 9.4 to 17.2° E) | 506 | 49,888 | 1,237 kB | 291 kB |
| The first region when only Germany and Austria were fetched | 344 | 31,438 | 798 kB | 187 kB |

A box that holds more than 5000 airspaces is refused with `400` and `box_too_large`. The first region is a tenth of that; the eight countries whole would be 3,575, of which Italy alone has 1,557. It is not cut short: a client could take a cut answer for the whole airspace of the box, and a pilot is not warned about airspace that is missing. Ask for such a box in several smaller ones.

**When a source fails.** A run that could not fetch every file of a source changes nothing: the server keeps serving the set of its last complete run, with that run's `fetched_at`. `/v1/health` then shows the source's `last_failure`, and `stale` once two runs in a row have failed.

## Accounts and sessions

A pilot has an account with a **handle**, a **display name** and a **password**. Signing in on a device makes a **session** and hands the device a **token**; the device sends the token with every request that concerns the account:

```
Authorization: Bearer venti_3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz01
```

That header is the only way to act for an account. The server sets no cookies and reads none, and it takes no token from a query string. A request without a token, or with one that was signed out or has expired, gets `401` with `unauthorized`, on every endpoint marked "needs a token" below.

What a client has to know:

- **A handle** is 3 to 20 characters: the letters `a` to `z`, digits and `_`. It starts with a letter, does not end with `_` and has no two `_` in a row. Upper and lower case are the same handle; the server keeps and shows it in lower case. A handle and a display name are public: they stand on every flight the pilot shares.
- **A display name** is 1 to 40 characters in any script, without line breaks or invisible characters.
- **A password** is 15 to 128 characters. Any characters count, spaces included, and there is no other rule; three or four words in a row are easy to remember. The server keeps a hash from which the password cannot be read back (Argon2id).
- **A forgotten password cannot be recovered.** The server knows no email address and has no other way to tell that someone is the account's owner. Say so where the password is chosen. This changes when passkeys or email arrive; see [architecture.md](architecture.md#authentication-and-accounts).
- **A token is shown once**, in the answer to creating an account or signing in. The server keeps only a hash of it. Keep the token where the device keeps secrets (the Keychain, the Keystore) and never in a URL or a log.
- **A session ends** when it is signed out, when the password is changed on another device, when the account is deleted, or when it has not been used for 400 days.
- **Attempts are counted.** After 10 wrong passwords for one handle within an hour, or 30 from one address, the answer is `429` with `rate_limited` and a `Retry-After` header, also for the right password. Creating accounts is limited to 10 attempts an hour from one address. The development mode has none of these limits.

### `POST /v1/accounts`

Creates an account and signs this device in.

```json
{
  "handle": "dario_b",
  "display_name": "Dario B.",
  "password": "correct horse battery staple",
  "invite": "7k3m-9q2x-4r8t-1v6w",
  "device": "iPhone 17"
}
```

| Field | Meaning |
|---|---|
| `handle`, `display_name`, `password` | As described above |
| `invite` | An invite code. Needed on a server that takes accounts by invite, which is the default; ignored elsewhere. Hyphens, spaces and upper case do not matter |
| `device` | What to call this device in the list of sessions. May be left out |

The answer is `201` with the same body as signing in, below. The development mode takes anyone and asks for no invite.

| Status | Code | When |
|---|---|---|
| 400 | `invalid_handle`, `invalid_display_name`, `invalid_password` | The value is not as described above. `message` says what is wrong with it |
| 403 | `registration_closed` | The server takes no new accounts |
| 403 | `invalid_invite` | The server takes accounts by invite, and the code is missing, mistyped, used or expired |
| 409 | `handle_taken` | Another account has the handle, or it is one the server keeps back, such as `admin` |
| 429 | `rate_limited` | Too many attempts from this address |

On a server that takes accounts by invite, `handle_taken` is only ever said to a request with a good invite. Without one the answer is `invalid_invite` whatever the handle, so nobody can find out which handles exist by trying to register them.

### `POST /v1/sessions`

Signs this device in.

```json
{
  "handle": "demo",
  "password": "demo-pilot-password",
  "device": "iPhone 17"
}
```

The answer is `201`. This is the development mode's, with the token and the session's identifier replaced, because both are different on every sign-in:

```json
{
  "account": {
    "handle": "demo",
    "display_name": "Demo Pilot",
    "created_at": "2026-11-18T19:05:00Z"
  },
  "session": {
    "id": "3kq9x2m7r4t8v1w6y0a5b3c2d4",
    "created_at": "2026-11-18T19:05:00Z",
    "last_used_at": "2026-11-18T19:05:00Z",
    "current": true
  },
  "token": "venti_3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz01"
}
```

| Field | Meaning |
|---|---|
| `account` | The account; see `GET /v1/account` |
| `session` | The session that was made; see `GET /v1/sessions` |
| `token` | The token to send from now on. It is not shown again |

A wrong password and a handle nobody has get the same answer after the same time: `401` with `invalid_credentials`. The server does not say which of the two was wrong.

### `GET /v1/sessions`

Needs a token. The devices signed in to the account, most recently used first.

```json
{
  "sessions": [
    {
      "id": "00000000000000000000000002",
      "device": "Development token",
      "created_at": "2026-11-18T19:05:00Z",
      "last_used_at": "2026-11-18T19:05:00Z",
      "current": true
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `id` | The session's identifier, for signing it out. It is not the token and gives no access |
| `device` | What the device called itself. Left out if it gave no name |
| `created_at` | When the device signed in |
| `last_used_at` | When the session was last used, to within five minutes |
| `current` | `true` for the session this request was made with |

### `DELETE /v1/sessions/{id}` and `DELETE /v1/sessions`

Need a token. The first signs one device out, by the `id` from the list; this is how a pilot signs out a phone they lost. The second signs out every device, the asking one included. Both answer `204` with no body. A session the account does not have gives `404` with `session_not_found`.

To sign the asking device out, delete the session whose `id` the sign-in answer gave.

### `GET /v1/account`

Needs a token. The account the token belongs to.

```json
{
  "account": {
    "handle": "demo",
    "display_name": "Demo Pilot",
    "created_at": "2026-11-18T19:05:00Z"
  }
}
```

### `PUT /v1/account/password`

Needs a token. Changes the password, and signs out every device except the asking one: whoever knew the old password is out.

```json
{
  "current_password": "demo-pilot-password",
  "new_password": "a new and longer password"
}
```

The answer is `204` with no body. A wrong `current_password` gives `403` with `wrong_password`, and counts as a wrong password for the handle. A `new_password` the server would not take gives `400` with `invalid_password`.

### `DELETE /v1/account`

Needs a token, and the password in the body: a token alone is not enough for this.

```json
{
  "password": "demo-pilot-password"
}
```

Deletes the account with everything that belongs to it: its sessions, its flights and their files. The answer is `204` with no body. It happens at once and cannot be undone; the handle is free again afterwards. A wrong password gives `403` with `wrong_password`.

To take everything along first, list the flights (`GET /v1/flights`) and download each file (`GET /v1/flights/{id}/igc`).

Not gone at once are the copies in the server's nightly backups, which the operator keeps for fourteen days unless configured otherwise; see [deploy/README.md](../deploy/README.md#backup).

## Flights

The server keeps a pilot's flight files exactly as they were uploaded, and works out everything else it says about a flight from the file, with the same code the apps use on the device. The numbers describe the flown part of the recording, from take-off to landing.

**Two identifiers.** A flight has an `identity`: a hash of its fixes, 32 hexadecimal characters, the identifier the apps' logbooks already use. The same recording has the same identity on every device, whatever its file's headers say. And it has an `id`, its address on the server: 26 lower-case letters and digits, used in every path below and in share links. One account holds a flight once, so uploading it again changes nothing. Two accounts that upload the same recording hold two flights with two addresses. An address cannot be worked out from the file: a link to an unlisted flight is known only to those it was given to.

**Who sees a flight** is its `visibility`:

| Value | Meaning |
|---|---|
| `private` | Only its pilot. Every flight starts like this |
| `unlisted` | Anyone who has its link. It is listed nowhere |
| `public` | Anyone. It is listed on its pilot's page |

To everyone but its pilot, a private flight does not exist: the answer is the one for an address that belongs to no flight, `404` with `flight_not_found`, the same in every detail. A client must expect further values of `visibility` in answers; one for followers is planned.

**Caching.** An answer to a request that carried a token is made for that account: `Cache-Control: no-store`, no `ETag`. An answer about a public or unlisted flight to a request without a token carries an `ETag` and `Cache-Control: public, no-cache` or `private, no-cache`: it may be kept, and must be checked with `If-None-Match` before every reuse, which is how a flight that was made private disappears at once.

### `POST /v1/flights`

Needs a token. Stores a flight file. The body of the request is the file itself, as bytes, not JSON and not a form:

```bash
curl -X POST http://127.0.0.1:8080/v1/flights \
  -H 'Authorization: Bearer venti_0000000000000000000000000000000000000000000000000000' \
  --data-binary @flight.igc
```

The answer is `201` when the upload created the flight, and `200` when the account already had it. Both are a success; `created` says which. The flight that was already there is returned as it is: its visibility stays, and the first file received stays the file.

```json
{
  "flight": {
    "id": "wkvydb8am8wmdcxzds9vkrw5kr",
    "identity": "d76b1d1a62211781e7fac7e5db815546",
    "pilot": {
      "handle": "demo",
      "display_name": "Demo Pilot"
    },
    "visibility": "private",
    "uploaded_at": "2026-11-18T19:05:00Z",
    "start": "2026-09-01T10:30:00Z",
    "airtime_s": 1744,
    "takeoff": {
      "time": "2026-09-01T10:30:00Z",
      "position": {
        "lat": 47.6642,
        "lon": 11.7967
      },
      "altitude_m": 1620
    },
    "track_length_m": 18402.3,
    "max_altitude_m": 2117,
    "max_climb_ms": 2.4,
    "score": {
      "kind": "free_flight",
      "closed": false,
      "distance_m": 11975.7,
      "points": 11.98
    },
    "glider": "Sample Wing",
    "outline": [
      [
        47.6642,
        11.7967
      ],
      [
        47.6697,
        11.78986
      ]
    ],
    "file_bytes": 64678,
    "analysis_version": 1
  },
  "created": true
}
```

(The `outline` is cut to two points here.)

| Status | Code | When |
|---|---|---|
| 413 | `payload_too_large` | The file is larger than 4 MB. Twelve hours at one fix a second are about 2 MB |
| 422 | `invalid_flight` | The file is not an IGC flight file, holds no position, or could not be analysed. `message` says which |
| 429 | `rate_limited` | The account is sending more than two files at the same time, or has uploaded 1000 within a day. `Retry-After` says when to try again |
| 503 | `unavailable` | The server is taking in or analysing as many flights as it can right now. `Retry-After` says when to try again |

Send a logbook's flights one after another, or two at a time. An upload may take up to two minutes on a slow connection before the server gives up on it.

### `GET /v1/flights?after=&limit=`

Needs a token. The account's flights, newest first by the start of the flight.

| Parameter | Meaning | Default |
|---|---|---|
| `limit` | How many flights to return, 1 to 100 | 50 |
| `after` | The `next` of the answer before | The newest flight |

```json
{
  "flights": [ ... ],
  "next": "1783161000000000_7cf5ytrw45b0aeb5tcr8wet2km"
}
```

Each entry of `flights` is a flight as `GET /v1/flights/{id}` shows it. `next` is left out on the last page. Treat it as an opaque value: pass it back as it came. A value that did not come from the server gives `400` with `invalid_cursor`, a bad `limit` gives `invalid_limit`.

### `GET /v1/flights/changes?since=&limit=`

Needs a token. What happened to the account's flights since the device last asked. This is what keeps the logbooks of a pilot's devices the same.

| Parameter | Meaning | Default |
|---|---|---|
| `since` | The `cursor` of the answer before | Everything |
| `limit` | How many changes to return, 1 to 100 | 50 |

```json
{
  "flights": [
    {
      "id": "5t1eshypdtvrq7rxw39a18vsrg",
      "identity": "0f0d88442e01f52f291d25b2660b8bed",
      "pilot": {
        "handle": "demo",
        "display_name": "Demo Pilot"
      },
      "visibility": "public",
      "uploaded_at": "2026-11-18T19:05:00Z",
      "start": "2026-08-15T10:30:00Z",
      "airtime_s": 1752,
      "takeoff": {
        "time": "2026-08-15T10:30:00Z",
        "position": {
          "lat": 47.6642,
          "lon": 11.7967
        },
        "altitude_m": 1620
      },
      "landing": {
        "time": "2026-08-15T10:59:12Z",
        "position": {
          "lat": 47.692167,
          "lon": 11.767483
        },
        "altitude_m": 786
      },
      "track_length_m": 18402.3,
      "max_altitude_m": 2117,
      "max_climb_ms": 2.4,
      "score": {
        "kind": "free_flight",
        "closed": false,
        "distance_m": 11975.7,
        "points": 11.98
      },
      "glider": "Sample Wing",
      "outline": [
        [
          47.6642,
          11.7967
        ],
        [
          47.6697,
          11.78986
        ]
      ],
      "file_bytes": 68008,
      "analysis_version": 1
    },
    {
      "id": "7cf5ytrw45b0aeb5tcr8wet2km",
      "identity": "6c55247b80a34b03fa0bb3bd38173c59",
      "pilot": {
        "handle": "demo",
        "display_name": "Demo Pilot"
      },
      "visibility": "unlisted",
      "uploaded_at": "2026-11-18T19:05:00Z",
      "start": "2026-07-04T10:30:00Z",
      "airtime_s": 1744,
      "takeoff": {
        "time": "2026-07-04T10:30:00Z",
        "position": {
          "lat": 47.6642,
          "lon": 11.7967
        },
        "altitude_m": 1620
      },
      "track_length_m": 18402.3,
      "max_altitude_m": 2117,
      "max_climb_ms": 2.4,
      "score": {
        "kind": "free_flight",
        "closed": false,
        "distance_m": 11975.7,
        "points": 11.98
      },
      "glider": "Sample Wing",
      "outline": [
        [
          47.6642,
          11.7967
        ],
        [
          47.6697,
          11.78986
        ]
      ],
      "file_bytes": 64678,
      "analysis_version": 1
    }
  ],
  "deleted": [],
  "cursor": "1795028700000003",
  "more": true,
  "reset": false
}
```

(The outlines are cut to two points here.)

| Field | Meaning |
|---|---|
| `flights` | Flights that are new or changed, each as it is now, oldest change first. A flight that changed several times appears once |
| `deleted` | Flights that were deleted, with the `id` they had and their `identity` |
| `cursor` | Pass this as `since` next time. Keep it per account |
| `more` | `true` when more changes are waiting: ask again at once with `cursor` |
| `reset` | `true` when the server did not know the `since` it was given and started from the beginning. See below |

How a device syncs:

1. Ask for changes since its stored cursor, until `more` is `false`. For each entry of `flights` whose `identity` its logbook lacks, download the file and import it. For each entry of `deleted`, remove the flight with that `identity`.
2. Then upload every flight of its logbook that the server does not have.
3. Store the new cursor.

Asking before uploading matters: a device that uploads first brings back flights the pilot deleted elsewhere.

`reset` is `true` when `since` lies beyond the account's last change. That happens when the server was restored from a backup, or when a cursor of another account is sent. The answer then holds everything from the beginning. The device should compare its whole logbook with it, and upload what the server lost, instead of only applying changes. A careful client does that comparison now and then anyway.

### `GET /v1/flights/{id}`

One flight. Its pilot sees it always and needs a token for it. A public or unlisted flight is answered to anyone, with or without a token. A token that is no longer valid gets `401` here as everywhere, not the answer for nobody: a device should learn that it is signed out.

```bash
curl http://127.0.0.1:8080/v1/flights/5t1eshypdtvrq7rxw39a18vsrg
```

```json
{
  "flight": {
    "id": "5t1eshypdtvrq7rxw39a18vsrg",
    "identity": "0f0d88442e01f52f291d25b2660b8bed",
    "pilot": {
      "handle": "demo",
      "display_name": "Demo Pilot"
    },
    "visibility": "public",
    "uploaded_at": "2026-11-18T19:05:00Z",
    "start": "2026-08-15T10:30:00Z",
    "airtime_s": 1752,
    "takeoff": {
      "time": "2026-08-15T10:30:00Z",
      "position": {
        "lat": 47.6642,
        "lon": 11.7967
      },
      "altitude_m": 1620
    },
    "landing": {
      "time": "2026-08-15T10:59:12Z",
      "position": {
        "lat": 47.692167,
        "lon": 11.767483
      },
      "altitude_m": 786
    },
    "track_length_m": 18402.3,
    "max_altitude_m": 2117,
    "max_climb_ms": 2.4,
    "score": {
      "kind": "free_flight",
      "closed": false,
      "distance_m": 11975.7,
      "points": 11.98
    },
    "glider": "Sample Wing",
    "outline": [
      [
        47.6642,
        11.7967
      ],
      [
        47.6697,
        11.78986
      ],
      [
        47.67054,
        11.78942
      ],
      [
        47.67075,
        11.79003
      ],
      [
        47.67073,
        11.79875
      ],
      [
        47.68876,
        11.75225
      ],
      [
        47.68935,
        11.75136
      ],
      [
        47.68954,
        11.75173
      ],
      [
        47.68932,
        11.75721
      ],
      [
        47.66357,
        11.74306
      ],
      [
        47.69144,
        11.75818
      ],
      [
        47.6922,
        11.75985
      ],
      [
        47.69217,
        11.76748
      ]
    ],
    "file_bytes": 68008,
    "analysis_version": 1
  }
}
```

| Field | Meaning |
|---|---|
| `id` | The flight's address on the server |
| `identity` | The hash of its fixes: what the apps' logbooks know the flight by |
| `pilot` | Handle and display name of the pilot |
| `visibility` | Who may see the flight |
| `uploaded_at` | When the server first received it |
| `start` | When the flown part begins: the take-off, or the first fix of a recording that never shows the glider flying |
| `airtime_s` | Seconds from take-off to landing, or to the end of a recording that stops in the air. For a recording that never shows the glider flying, the length of the recording |
| `takeoff`, `landing` | When and where, with the satellite altitude. `takeoff` is left out when the recording never shows the glider flying, `landing` when the recording ends in the air |
| `track_length_m` | Distance along the track |
| `max_altitude_m` | Highest satellite altitude |
| `max_climb_ms` | Best climb averaged over half a minute |
| `score` | How the flight scores as a cross-country flight: `kind` is `free_flight`, `free_triangle` or `fai_triangle`; `closed` says whether a triangle is closed to within 5%; `distance_m` and `points` follow. Left out for a track too short to score. The rules are the World XContest's as this project reads them; the points are ours, not the league's, until they have been compared |
| `glider` | The glider as the file names it. Left out if the file names none. This is text from a file anyone can write: show it as text, never as markup |
| `outline` | The shape of the track for a thumbnail: at most 96 points, each a latitude and a longitude. Circles in thermals are smoothed away, so nothing may be measured on it |
| `file_bytes` | The size of the flight file |
| `analysis_version` | Which version of the server's analysis produced the numbers. When the analysis improves, the server works every flight out again, and the flight shows up in `/v1/flights/changes` |

A client must expect values of `kind` it does not know.

### `GET /v1/flights/{id}/igc`

The flight file, byte for byte as it was uploaded, to whoever may see the flight. The answer has `Content-Type: application/octet-stream` and a `Content-Disposition` that names the file `venti-<date>-<start of id>.igc`.

### `PATCH /v1/flights/{id}`

Needs a token. Changes who may see a flight of the account.

```json
{
  "visibility": "public"
}
```

The answer is `200` with the flight as it then is, in the shape of `GET /v1/flights/{id}`. A value other than `private`, `unlisted` and `public` gives `400` with `invalid_visibility`. A flight of another pilot is no flight of this account, whoever may see it: `404` with `flight_not_found`.

### `DELETE /v1/flights/{id}`

Needs a token. Deletes a flight of the account and its file, at once and for good. The answer is `204` with no body. The server remembers only that the flight was deleted, with its `id` and `identity`, so that the pilot's other devices can be told. Uploading the file again makes the flight again, private, under the same address.

## Pilots

### `GET /v1/pilots/{handle}?after=&limit=`

A pilot and their public flights, newest first. Anyone may ask; the answer is the same for everyone, the pilot included.

```bash
curl http://127.0.0.1:8080/v1/pilots/demo
```

```json
{
  "pilot": {
    "handle": "demo",
    "display_name": "Demo Pilot"
  },
  "flights": [ ... ]
}
```

Each entry of `flights` is a flight as `GET /v1/flights/{id}` shows it. `after`, `limit` and `next` work as for `GET /v1/flights`.

A pilot has a page once they have made a flight public. A handle nobody has gives `404` with `pilot_not_found`, and so does the handle of a pilot who shares nothing publicly: an account that keeps to itself cannot be found here, and the two answers cannot be told apart.

## The page a shared flight has

These two addresses are not part of `/v1`. They answer browsers and the preview fetchers of chat apps, which send no token, so they show public and unlisted flights only. A private flight and an address that belongs to no flight get the same `404` page.

| Address | Answer |
|---|---|
| `/f/{id}` | A page about the flight in HTML: its numbers, its track and altitude profile as drawings, and a link to the flight file. Its head carries Open Graph and Twitter-card lines, from which a chat app builds the preview of a link |
| `/f/{id}/preview.png` | The picture for that preview, 1200 by 630 pixels: the track's shape and the flight's numbers. The server draws it itself |

A share link is the server's public address followed by `/f/` and the flight's `id`; `venti_api_types::v1::share_path` builds the path. The page needs nothing but itself: no script, no font, no other request. The web app will later be linked from it.

The links inside the page start with the address the server was configured with (`public_url`). On a server where that is not set they point at the address it listens on, which a chat app cannot follow.

## What is not here yet

Recovering a forgotten password, passkeys, changing a handle or a display name, flights visible to followers only, follows and a feed, forecasts, sites, which airspace is active today, live positions and the assistant connector are planned and not built; see [plan.md](plan.md). Station metadata is planned to move into downloadable region bundles ([architecture.md](architecture.md#api-and-offline-sync)); the station list will keep working when it does.
