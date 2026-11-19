# The server's HTTP API

Last revised: 2026-11-19

The server answers a small JSON API that the phone and web apps use: for the live wind map, for airspace, for flying sites, for forecasts, for a pilot's account and flights, and for live tracks. This document describes it for people. The exact wire format is the Rust crate `venti-api-types`, which the server builds its answers from and the clients decode them with; when this text and that crate disagree, the crate is right and this text has a bug.

Everything here is version 1. It has three parts:

- **Weather stations, airspace, flying sites and forecasts**, which anyone may read: [health](#get-v1health), [stations](#get-v1stationsbboxwestsoutheastnorth), [observations](#get-v1stationsidobservationsfromto), [airspace](#get-v1airspacebboxwestsoutheastnorth), [sites](#get-v1sitesbboxwestsoutheastnorth), [forecast](#get-v1forecastlatlon).
- **Accounts and flights**: [accounts and sessions](#accounts-and-sessions), [flights](#flights), [pilots](#pilots), and [the page a shared flight has](#the-page-a-shared-flight-has).
- **Live tracks**, where a glider is while it flies: [what a live track is](#live-tracks), [sending positions](#post-v1livepositions), [who is in the air](#get-v1livebboxwestsoutheastnorth), [one track](#get-v1liveidsinceafterlimit), [deleting one](#delete-v1liveid), and [the page a live track has](#the-page-a-live-track-has).

## Try it

```bash
cargo run -p venti-server -- dev
curl 'http://127.0.0.1:8080/v1/stations?bbox=10.9,47.0,11.5,47.5'
```

The development mode serves this API over recorded data: six stations, three German and three Austrian, as they were on 2026-11-18 at 19:05 UTC, and 192 airspaces recorded from openAIP the day after and served as if fetched at that same moment: 88 of the northern Alps around Munich, Innsbruck and Salzburg, and 104 small ones from the six other countries the first region's box takes in. They are a sample. The development mode says of them what a real server says of its whole set, that they [cover](#get-v1airspacebboxwestsoutheastnorth) the first region, so that a screen can be built and tested in the state a pilot will see; nobody flies with them. The flying sites are not a sample: they are the 502 that OpenStreetMap had for the first region on 2026-11-18, recorded the day after and served as if fetched at the same moment as everything else. Forecasts were recorded for five places around the Tegernsee and in the middle of the region, a day after the wind, and are [served as of that moment](#get-v1forecastlatlon). The mode needs no network and no database, its clock stands still at that moment, and every start gives the same answers. The examples below are its answers; the tests keep them in `crates/server/tests/golden` and check that this document shows them unchanged.

It also starts with one account, so that an app can be built and tested against it without creating one first:

| | |
|---|---|
| Handle | `demo` |
| Password | `demo-pilot-password` |
| A session token that always works | `venti_0000000000000000000000000000000000000000000000000000` |
| Flights | Three made-up ones: one public (2026-08-15, it lands), one unlisted (2026-07-04, the sample file the apps bundle; its recording ends in the air), one private (2025-06-21, a short one) |
| Live tracks | Three, made from the sample flight and dated by this mode's clock; see the table below |

```bash
curl http://127.0.0.1:8080/v1/flights \
  -H 'Authorization: Bearer venti_0000000000000000000000000000000000000000000000000000'
```

The demo account's [live tracks](#live-tracks) are over the Tegernsee valley, where the sample flight is. Their times are fixed against this mode's clock, which stands at 19:05:00 UTC on 2026-11-18, so an app that works out "how old is this position" against the server's clock always gets the same answer:

| Identifier | Who sees it | What it is |
|---|---|---|
| `11111111111111111111111111` | `public` | A glider in the air: the first twenty minutes of the sample flight, 1,201 fixes, one a second from 18:44:56 to 19:04:56 UTC. The newest position is 4 seconds old: 47.6638° N, 11.7432° E at 1,596 m, on a glide towards north-north-east |
| `22222222222222222222222222` | `public` | A glider that landed 9 minutes ago: the last ten minutes of the sample flight and a minute standing on the ground, 660 fixes from 18:45:01 to 18:56:00 UTC, `landed`. It stands at 47.6922° N, 11.7675° E at 786 m |
| `33333333333333333333333333` | `link` | A glider in the air that only those with the link see: the first seven minutes of the sample flight, 421 fixes from 18:57:40 to 19:04:40 UTC. The newest position is 20 seconds old. It is in no list |

```bash
curl 'http://127.0.0.1:8080/v1/live?bbox=11.6,47.55,12.0,47.85'      # the two public ones
curl http://127.0.0.1:8080/v1/live/33333333333333333333333333          # the one shared by link
curl http://127.0.0.1:8080/live/11111111111111111111111111             # the page a link shows; better in a browser
```

Every fix of these tracks but the first of each says how fast the glider moved, how fast it climbed and where it was heading; the first says only where it was, and a glider that stands still has no heading. That is what a real app sends too.

None of this is a secret, and none of it exists on a real server. Anyone may create further accounts in this mode, without an invite; the limits on attempts are off, and uploads work. Whatever a client changes is forgotten when the program stops. The addresses of the demo flights and the identifiers of the demo live tracks are the same on every start.

Because its clock stands still, this mode takes live fixes of **any time**, where a real server refuses those from the future and those more than 12 hours old, and it never treats a track as too old to read or to add to. An app under test can therefore send the fixes of a replayed flight of last summer, or of its own clock. Two things stay as on a real server: an answer says when a track would be removed (`expires_at`), and the list of public tracks holds a track only while its newest fix is no older than half an hour by this mode's clock. A track sent with the times of a replayed flight is thus read by its identifier and is not on the list.

## Rules that hold for every endpoint

**Versions.** Every path starts with `/v1`. Within a version the format only grows: an answer may gain fields, so a client ignores fields it does not know. Removing or renaming a field, or changing what one means, needs a `/v2` beside `/v1`.

**Units.** Times are UTC in RFC 3339 (`2026-11-18T18:20:00Z`). Positions are WGS84 degrees. Heights are metres above mean sea level, except the limits of an airspace, which are given as published; see [airspace](#get-v1airspacebboxwestsoutheastnorth). Wind speed is metres per second, and wind direction is the direction the wind comes from, in degrees clockwise from true north. Temperature is degrees Celsius, pressure is hectopascal at the station's own height, humidity is percent.

**Missing values.** A value that is not known is left out of the answer. It is never `null` and never a placeholder number. A station without a temperature sensor has readings without `temperature_c`.

**Sources and credit.** Every station, every airspace, every flying site and every forecast series comes from exactly one data source and names it. The same physical station reported by two sources appears twice: data under different licences is never merged into one record. Every answer that carries data has an `attributions` list with one entry for each source the answer draws on, and only those:

| Field | Meaning |
|---|---|
| `source` | The source's identifier, as records name it |
| `provider` | Who publishes the data |
| `licence` | The licence's name |
| `text` | The credit line, worded as the provider asks |
| `url` | Where the credit line links to |
| `notice` | What the provider asks to be said wherever its data is shown, beside the credit. Left out when it asks for nothing of the kind |

A client that shows the data shows the credit of its source next to it, and the notice if there is one. The licences require that; it is not decoration.

**Caching.** Answers with station data carry `Cache-Control: public, max-age=60` and an `ETag`. A client that asks again with `If-None-Match` gets `304 Not Modified` without a body when nothing changed. Observations arrive every ten minutes, so most repeated requests end that way. Airspace and flying sites change once a day at most and carry `max-age=3600`. A forecast carries `max-age=60` like the stations; the server itself keeps one for up to half an hour. Health answers and errors carry `Cache-Control: no-store`. So does every answer to a request that carried a session token: it was made for one account, and nothing may keep it. What applies to shared flights is said [there](#flights), and what applies to live tracks, which are asked for every few seconds, [there](#live-tracks).

**Compression.** Answers are gzip-compressed when the request says `Accept-Encoding: gzip`. The station list of the whole first region is about 106 kB plain and 15 kB compressed, its airspace about 1,240 kB and 290 kB, its flying sites 110 kB and 17 kB. The forecast of one place is 85 kB and 9 kB. A live track of twenty minutes at a fix a second is 204 kB and 28 kB.

**Bodies.** A request that carries JSON says so: `Content-Type: application/json`, with or without a character set. Anything else, and no type at all, is answered `415` with `unsupported_media_type` before the body is looked at. The one request whose body is not JSON is the upload of a flight file, which takes any type. A JSON body may be 16 kB, and one that carries the fixes of a live track 32 kB; a larger one is answered `413` and not read. The rule is what keeps a page on another site from sending sign-in or registration attempts through a visitor's browser: a browser sends a body typed `text/plain` anywhere without asking, and asks the server first before it sends one typed `application/json`.

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
| 400 | `box_too_large` | The box holds more airspaces, or more flying sites, than one answer carries. Ask for smaller boxes |
| 400 | `invalid_position` | `lat` or `lon` is missing, is not a number, or the two are not a place on the globe |
| 400 | `invalid_station_id` | The identifier in the path is not of the form `source:local` |
| 400 | `invalid_time` | `from`, `to` or `after` is not an RFC 3339 time |
| 400 | `invalid_range` | `from` lies after `to`, or the window is longer than seven days |
| 400 | `invalid_request` | The request could not be read at all: a path that is not valid UTF-8, a body that is not the JSON the endpoint expects. For a body, `message` says where it stops making sense and never repeats what stands there |
| 400 | `invalid_handle`, `invalid_display_name`, `invalid_password` | A new account's handle, name or password is not as [described](#accounts-and-sessions) |
| 400 | `invalid_visibility` | `visibility` is not `private`, `unlisted` or `public`; for a live track, not `link` or `public` |
| 400 | `invalid_cursor`, `invalid_limit` | `after` or `since` is not a value the server gave out; `limit` is not a number from 1 to 100, or from 1 to 5000 for the fixes of a live track |
| 400 | `invalid_live_track_id` | The identifier sent for a live track is not 26 characters of its alphabet |
| 400 | `invalid_fixes` | The fixes sent for a live track cannot be used as a whole: none, more than 120, not oldest first, or a value no glider has. `message` names the place of the fix in the request and never one of its values |
| 401 | `unauthorized` | The request needs an account and carries no valid session token. The answer has `WWW-Authenticate: Bearer` |
| 401 | `invalid_credentials` | Signing in: the handle or the password is wrong |
| 403 | `wrong_password` | The password sent to confirm a change to the account is wrong |
| 403 | `registration_closed`, `invalid_invite` | The server takes no new accounts, or takes them by invite and the code is not good |
| 403 | `account_full` | An upload: the account keeps as many flights, or as many megabytes of flight files, as the server gives one account room for |
| 403 | `live_track_full` | The live track holds as many fixes as one track may. Begin a new track with a new identifier |
| 404 | `station_not_found` | No station has this identifier |
| 404 | `flight_not_found` | No flight has this address, or it is not the caller's to see. The two cannot be told apart |
| 404 | `session_not_found`, `pilot_not_found` | The account has no such session; no pilot has this handle |
| 404 | `live_track_not_found` | No live track has this identifier: none ever had, its pilot deleted it, or it ended long enough ago to have been removed. The three cannot be told apart |
| 404 | `outside_region` | A forecast was asked for a place outside the area the server has forecasts for |
| 404 | `not_found` | The path is not part of the API |
| 405 | `method_not_allowed` | The path exists, but not with this method |
| 408 | `timeout` | An upload: the file stopped arriving, or took too long to arrive |
| 409 | `handle_taken` | The handle belongs to another account or is not available |
| 409 | `live_track_taken` | Fixes were sent under the identifier of another account's live track |
| 410 | `live_track_gone` | Fixes were sent under an identifier whose live track was deleted or removed, or that was taken out of use. An identifier is used once: begin a new track with a new one |
| 413 | `payload_too_large` | The request carried a body of more than 16 kB, a batch of live fixes of more than 32 kB, or a flight file of more than 4 MB |
| 415 | `unsupported_media_type` | A request whose body is JSON did not say so with `Content-Type: application/json` |
| 422 | `invalid_flight` | The uploaded file is not a flight file the server can use |
| 422 | `fixes_out_of_time` | None of the live fixes sent has a time the server takes: all lie in the future, or are too old. The device's clock is probably wrong; `message` says what the server's shows |
| 429 | `rate_limited` | Too many attempts. `Retry-After` says when to try again |
| 501 | `no_forecasts` | The server has no forecast service. Asking again does not help |
| 503 | `unavailable` | The server cannot reach its database, is too busy for this request, or has no forecast for a place at hand and cannot fetch one now. `Retry-After` says when to try again |
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
  "forecasts": "available",
  "registration": "open",
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
      "source": "open-meteo",
      "provider": "Open-Meteo, with models of Deutscher Wetterdienst",
      "cadence_seconds": 600,
      "last_success": "2026-11-18T19:05:00Z",
      "stale": false,
      "stations": 0,
      "models": [
        {
          "model": "icon-d2",
          "run": "2026-11-19T18:00:00Z",
          "available_at": "2026-11-19T19:10:09Z",
          "checked_at": "2026-11-18T19:05:00Z"
        },
        {
          "model": "icon-eu",
          "run": "2026-11-19T15:00:00Z",
          "available_at": "2026-11-19T17:49:54Z",
          "checked_at": "2026-11-18T19:05:00Z"
        }
      ]
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
    },
    {
      "source": "osm-free-flying",
      "provider": "OpenStreetMap contributors",
      "cadence_seconds": 86400,
      "last_success": "2026-11-18T19:05:00Z",
      "stale": false,
      "stations": 0,
      "sites": 502,
      "sites_as_of": "2026-11-18T19:05:00Z"
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `status` | `ok` when no source is stale, otherwise `degraded` |
| `version` | The server's version |
| `time` | The server's clock. Compute the age of an observation against this, not against the device's clock, which may be wrong |
| `client_addresses` | For whoever runs the server; an app ignores it. `seen` when requests arrive from addresses of their own. `hidden` when, in the last hour, twenty or more requests and at least a tenth of all came from one private address and from no other private one: a proxy or a container network is hiding the visitors, and the server counts them as one ([deploy/README.md](../deploy/README.md#the-visitors-address)). Left out while fewer than twenty requests have arrived |
| `forecasts` | Whether the server answers point forecasts at all: `available`, or `not_configured` for a server that was not told where its forecast service is. Such a server answers `/v1/forecast` with `501` and asks nobody else. Left out by a server older than forecasts, which has none |
| `registration` | Who may create an account here: `open` (anyone), `invite` (whoever has an invite code, which `POST /v1/accounts` then needs) or `closed` (nobody; existing accounts sign in as before). An app shows the field for the invite code, or says that the server takes no new accounts, by this. Left out by a server older than this field, which may ask for a code: an app that reads nothing here offers the field. The development mode says `open` |
| `sources[].cadence_seconds` | How often the server asks the provider for new data |
| `sources[].last_success` | When a run last delivered its data without trouble. Left out if none ever did |
| `sources[].last_failure` | When a run last failed. For a source of observations: it delivered nothing, or more than a tenth of its requests failed. For a source of airspace or of flying sites: any request failed, because a set with a part missing must not replace a whole one. Left out if none ever did |
| `sources[].newest_observation` | The time of the newest observation the source has delivered. Left out for a source of anything else |
| `sources[].stale` | `true` when no run has succeeded for too long. For a source the server asks every few minutes: three cadence intervals. For a source it asks once a day or less often, which is airspace and flying sites: two and a quarter intervals, 54 hours for a daily one. A failed daily run is tried again a day later, so one missed day is tolerated and the second is not |
| `sources[].stations` | How many stations the last successful run delivered. `0` for a source of anything else |
| `sources[].airspaces` | How many airspaces the server holds from the source: the set of its last successful run, which is what it serves. Left out for a source that delivers no airspace |
| `sources[].file_date` | The date of the oldest file that set was read from. Left out for a source that delivers no airspace |
| `sources[].sites` | How many flying sites the server holds from the source: the set of its last successful run. Left out for a source that delivers no sites |
| `sources[].sites_as_of` | The date the source's own data is of. Left out for a source that delivers no sites, and before its first set |
| `sources[].models` | For a source of forecasts: the weather models it serves, each with the `run` it was at when the server last looked, when the source had that run ready (`available_at`) and when the server looked (`checked_at`). The server looks every ten minutes, by asking for one forecast. A run older than six hours means the forecast service has stopped fetching new ones. Left out for a source of anything else, and before the first look that worked. In the development mode the runs are dated after `checked_at`: its forecasts were recorded a day after its wind |

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

## `GET /v1/sites?bbox=west,south,east,north`

The flying sites inside a box: take-offs, landings, training slopes and winch sites, as the server last fetched them from its sources. Today there is one source, OpenStreetMap.

`bbox` is required and written as for stations. Sites on the edge are included.

```bash
curl 'http://127.0.0.1:8080/v1/sites?bbox=11.7,47.6,11.95,47.75'
```

```json
{
  "layers": [
    {
      "source": "osm-free-flying",
      "fetched_at": "2026-11-18T19:05:00Z",
      "as_of": "2026-11-18T19:05:00Z",
      "covers": [
        {
          "south": 46.3,
          "west": 9.4,
          "north": 48.5,
          "east": 17.2
        }
      ],
      "sites": [
        {
          "id": "osm-free-flying:node/5964676196",
          "position": {
            "lat": 47.6652212,
            "lon": 11.7954245
          },
          "uses": [
            "takeoff"
          ],
          "url": "https://www.openstreetmap.org/node/5964676196"
        },
        {
          "id": "osm-free-flying:node/5964676197",
          "position": {
            "lat": 47.6654085,
            "lon": 11.7953273
          },
          "uses": [
            "takeoff"
          ],
          "url": "https://www.openstreetmap.org/node/5964676197"
        },
        {
          "id": "osm-free-flying:node/9908030941",
          "position": {
            "lat": 47.6919322,
            "lon": 11.8880464
          },
          "uses": [
            "landing"
          ],
          "url": "https://www.openstreetmap.org/node/9908030941"
        },
        {
          "id": "osm-free-flying:way/1080986173",
          "name": "Fluggelände Jägerkamp/Nagelspitz",
          "position": {
            "lat": 47.6791413,
            "lon": 11.9051032
          },
          "elevation_m": 1560.0,
          "uses": [
            "takeoff"
          ],
          "paragliding": true,
          "hang_gliding": true,
          "official": true,
          "url": "https://www.openstreetmap.org/way/1080986173"
        },
        {
          "id": "osm-free-flying:way/1080988126",
          "name": "Landeplatz Jägerkamp/Nagelspitz",
          "position": {
            "lat": 47.6924086,
            "lon": 11.8865478
          },
          "uses": [
            "landing"
          ],
          "url": "https://www.openstreetmap.org/way/1080988126"
        }
      ]
    }
  ],
  "attributions": [
    {
      "source": "osm-free-flying",
      "provider": "OpenStreetMap contributors",
      "licence": "ODbL 1.0",
      "text": "© OpenStreetMap contributors",
      "url": "https://www.openstreetmap.org/copyright",
      "notice": "This data is OpenStreetMap's and is available under the Open Database License, https://opendatacommons.org/licenses/odbl/1-0/. Anyone can correct it: https://www.openstreetmap.org/fixthemap"
    }
  ]
}
```

The box is the valley of the apps' sample flight, the Tegernsee, with the Spitzingsee east of it. The first two records are the ramps on the Wallberg.

**Layers.** Sites come in layers like airspace: one for each source the server holds sites of, sorted by source, each in every answer, also when it has nothing in the box. Layers are never merged, and a client that stores or draws sites keeps them apart and shows with each the credit and the notice of its source. OpenStreetMap's licence is share-alike: what is made from its layer and other data may be passed on only under the same licence.

| Field | Meaning |
|---|---|
| `layers[].source` | The source of the layer. Its credit, licence and notice are the entry of `attributions` with the same `source` |
| `layers[].fetched_at` | When the server fetched the set this layer is a part of |
| `layers[].as_of` | The date the source's data is of, by the source's own word: for OpenStreetMap, the moment the copy that answered the server was taken. **Show the pilot how old this is**, measured against the server's clock |
| `layers[].covers` | The boxes inside which the server's set of this source is whole, written as for airspace. Empty, or missing, means the server does not say, and a client takes that as covering nothing |
| `layers[].sites` | The records of the layer inside the box, sorted by `id`. May be empty |

A record is the canonical site of the project, `venti_domain::Site`:

| Field | Meaning |
|---|---|
| `id` | `source:local`. For OpenStreetMap the local part names the element: `node/5964676196` |
| `name` | As the source gives it. Left out for the many sites that have none |
| `position` | Latitude and longitude. For a site that the source draws as an area, a point in or near it |
| `elevation_m` | Height above mean sea level. Left out if the source gives none |
| `uses` | What the site is used for: any of `takeoff`, `landing`, `towing`, `training`, each once. **May be empty**: the place is tagged for free flying and the source does not say what it is. A client expects values it does not know |
| `wind_directions` | The wind directions the site works in, as spans that run clockwise: `[{"from_deg": 135.0, "to_deg": 225.0}]` is south-east to south-west, and a single direction has both ends equal, `{"from_deg": 270.0, "to_deg": 270.0}`. A direction is where the wind comes from. Left out if the source gives none, **which says nothing about the site**: most sites have none |
| `paragliding`, `hang_gliding` | Whether the source says the site suits that kind of glider. Left out if it does not say |
| `official` | Whether the source calls the site official: legal and looked after. Left out if it does not say |
| `discouraged` | `true` if the source marks the site as unsafe or one of its uses as discouraged. Left out otherwise, which says only that the source marks nothing of the kind |
| `url` | Where the source's record can be looked at, and corrected. Offer it to the pilot: a site that is wrong or missing is fixed there |

**A site in the answer is no permission to fly there, and no site in the answer is not "no site".** OpenStreetMap holds what somebody entered. In the first region that is 252 take-offs and 142 landings, a fraction of the sites there are; well-known ones are missing, a fifth of the places do not say what they are used for, and fewer than half of the take-offs carry wind directions that can be read for certain. [data-sources.md](data-sources.md#flying-sites) has the count. `covers` says where the server's set is whole, which means "everything the source has", not "everything there is". Whether a pilot may fly at a site, and by which rules, is the business of whoever runs it.

**Caching.** As for airspace: `Cache-Control: public, max-age=3600` and an `ETag`. The server asks its source once a day.

**Size.** One answer carries the whole box; there are no pages. The first region whole is 502 sites, 110 kB plain and 17 kB with gzip (measured 2026-11-19). A box that holds more than 5000 sites is refused with `400` and `box_too_large`, as for airspace.

**When a source fails.** A run that could not fetch everything changes nothing: the server keeps serving the set of its last whole run, with that run's `fetched_at`. `/v1/health` then shows the source's `last_failure`, and `stale` once two runs in a row have failed.

## `GET /v1/forecast?lat=&lon=`

What the weather models say about one place, hour by hour: wind and gusts, wind higher up, temperature, cloud, precipitation and what the models have about convection. Today there is one source, the server's own Open-Meteo instance, with two models of Deutscher Wetterdienst, ICON-D2 and ICON-EU.

`lat` and `lon` are required, in degrees. The answer is for the nearest point of a grid of 0.02° (about 2 km), which `point` names: everyone who asks inside the same cell of that grid gets the same answer.

```bash
curl 'http://127.0.0.1:8080/v1/forecast?lat=47.7&lon=11.8'
```

```json
{
  "point": {
    "lat": 47.7,
    "lon": 11.8
  },
  "fetched_at": "2026-11-19T19:30:00Z",
  "layers": [
    {
      "source": "open-meteo",
      "series": [
        {
          "source": "open-meteo",
          "model": "icon-d2",
          "run": "2026-11-19T18:00:00Z",
          "available_at": "2026-11-19T19:10:09Z",
          "point": {
            "lat": 47.7,
            "lon": 11.8
          },
          "cell": {
            "lat": 47.70034,
            "lon": 11.799969
          },
          "cell_elevation_m": 1069.0,
          "hours": [
            {
              "time": "2026-11-19T19:00:00Z",
              "wind_speed_ms": 2.4,
              "wind_direction_deg": 225.0,
              "wind_gust_ms": 13.1,
              "temperature_c": 3.2,
              "dew_point_c": -0.1,
              "cloud_cover_pct": 99.0,
              "cloud_cover_low_pct": 83.0,
              "cloud_cover_mid_pct": 98.0,
              "cloud_cover_high_pct": 0.0,
              "precipitation_mm": 0.0,
              "cape_j_kg": 0.0,
              "freezing_level_m": 1800.0,
              "updraft_ms": 1.45,
              "winds_aloft": [
                {
                  "above_ground_m": 80.0,
                  "wind_speed_ms": 5.49,
                  "wind_direction_deg": 237.0,
                  "temperature_c": 4.7
                },
                {
                  "above_ground_m": 120.0,
                  "wind_speed_ms": 6.62,
                  "wind_direction_deg": 241.0,
                  "temperature_c": 4.5
                },
                {
                  "above_ground_m": 180.0,
                  "wind_speed_ms": 7.88,
                  "wind_direction_deg": 246.0,
                  "temperature_c": 4.2
                },
                {
                  "pressure_hpa": 850.0,
                  "altitude_m": 1340.0,
                  "wind_speed_ms": 11.51,
                  "wind_direction_deg": 255.0,
                  "temperature_c": 3.6
                },
                {
                  "pressure_hpa": 700.0,
                  "altitude_m": 2883.0,
                  "wind_speed_ms": 24.66,
                  "wind_direction_deg": 277.0,
                  "temperature_c": -7.6
                },
                {
                  "pressure_hpa": 600.0,
                  "altitude_m": 4066.0,
                  "wind_speed_ms": 21.99,
                  "wind_direction_deg": 288.0,
                  "temperature_c": -15.3
                },
                {
                  "pressure_hpa": 500.0,
                  "altitude_m": 5417.0,
                  "wind_speed_ms": 19.92,
                  "wind_direction_deg": 288.0,
                  "temperature_c": -24.7
                }
              ]
            }
          ]
        },
        {
          "source": "open-meteo",
          "model": "icon-eu",
          "run": "2026-11-19T15:00:00Z",
          "available_at": "2026-11-19T17:49:54Z",
          "point": {
            "lat": 47.7,
            "lon": 11.8
          },
          "cell": {
            "lat": 47.68324,
            "lon": 11.818885
          },
          "cell_elevation_m": 1100.0,
          "hours": [
            {
              "time": "2026-11-19T16:00:00Z",
              "wind_speed_ms": 4.98,
              "wind_direction_deg": 260.0,
              "wind_gust_ms": 25.2,
              "temperature_c": 3.9,
              "dew_point_c": 0.9,
              "cloud_cover_pct": 60.0,
              "cloud_cover_low_pct": 53.0,
              "cloud_cover_mid_pct": 13.0,
              "cloud_cover_high_pct": 4.0,
              "precipitation_mm": 0.0,
              "cape_j_kg": 0.0,
              "freezing_level_m": 1710.0,
              "winds_aloft": [
                {
                  "above_ground_m": 80.0,
                  "wind_speed_ms": 9.72,
                  "wind_direction_deg": 261.0,
                  "temperature_c": 3.9
                },
                {
                  "above_ground_m": 120.0,
                  "wind_speed_ms": 11.4,
                  "wind_direction_deg": 262.0,
                  "temperature_c": 3.5
                },
                {
                  "above_ground_m": 180.0,
                  "wind_speed_ms": 13.19,
                  "wind_direction_deg": 263.0,
                  "temperature_c": 2.9
                },
                {
                  "pressure_hpa": 850.0,
                  "altitude_m": 1324.0,
                  "wind_speed_ms": 13.9,
                  "wind_direction_deg": 263.0,
                  "temperature_c": 2.8
                },
                {
                  "pressure_hpa": 700.0,
                  "altitude_m": 2864.0,
                  "wind_speed_ms": 23.17,
                  "wind_direction_deg": 285.0,
                  "temperature_c": -8.1
                },
                {
                  "pressure_hpa": 600.0,
                  "altitude_m": 4047.0,
                  "wind_speed_ms": 19.85,
                  "wind_direction_deg": 294.0,
                  "temperature_c": -14.7
                },
                {
                  "pressure_hpa": 500.0,
                  "altitude_m": 5401.0,
                  "wind_speed_ms": 18.41,
                  "wind_direction_deg": 305.0,
                  "temperature_c": -24.5
                }
              ]
            }
          ]
        }
      ]
    }
  ],
  "attributions": [
    {
      "source": "open-meteo",
      "provider": "Open-Meteo, with models of Deutscher Wetterdienst",
      "licence": "CC BY 4.0",
      "text": "Weather data by Open-Meteo.com. Datenbasis: Deutscher Wetterdienst, aufbereitet von Open-Meteo",
      "url": "https://open-meteo.com/"
    }
  ]
}
```

The place is the east shore of the Tegernsee. Each series is cut to its first hour here; in the whole answer ICON-D2 has 48 hours and ICON-EU 30.

**The answer holds magnitudes and no judgement.** Nothing in it says whether a day can be flown, and a client that shows it adds none.

**Layers and series.** One layer for each source that has something for the place, and in a layer one series for each weather model, sorted by model. Nothing is blended: two models are two series, and a client that draws one line for the day chooses a series and says which. A model that does not reach the place, or whose run the source could not tell, is not listed.

| Field | Meaning |
|---|---|
| `point` | The place the forecast is for: the point of the grid nearest to what was asked |
| `fetched_at` | When the server asked its source. An answer is reused for up to half an hour |
| `layers[].source` | The source of the layer. Its credit and licence are the entry of `attributions` with the same `source` |
| `layers[].series` | One series per model |

A series is the canonical forecast of the project, `venti_domain::ForecastSeries`:

| Field | Meaning |
|---|---|
| `source`, `model` | Where the values are from: `open-meteo`, and `icon-d2` or `icon-eu`. A client expects models it does not know |
| `run` | When the run of the model that computed these hours began. **Show the pilot the run**: the age of a forecast is the age of its run |
| `available_at` | When the source had that run ready. Left out if the source does not say |
| `point` | The same as the answer's |
| `cell` | The centre of the model's grid cell the values are for. It can lie some kilometres from `point` |
| `cell_elevation_m` | The height of the model's ground in that cell. **Show it beside the values.** A model's ground is smoothed: east of the Tegernsee, whose shore lies at 725 m, ICON-D2's ground is at 1069 m, and "10 m above ground" is 10 m above that |
| `hours` | One record per hour, oldest first |

**Only the hours the run computed.** A series holds the hours after its run began, as far as the run reaches: 48 hours for ICON-D2; for ICON-EU 78 hours after a run of 00, 06, 12 or 18 UTC and 30 hours after one in between. The source has older runs' values for other hours and does not say which run they are from, so they are left out. A series therefore never holds the hours of today that are over, and it can end before the day after tomorrow does: ICON-EU covers that day for about three hours out of every six. A client shows the hours it has and does not fill the rest.

An hour:

| Field | Meaning |
|---|---|
| `time` | The moment the values are for, in UTC |
| `wind_speed_ms`, `wind_direction_deg` | The wind 10 m above the model's ground: m/s, and the direction it comes from |
| `wind_gust_ms` | The strongest gust at 10 m in the hour before `time` |
| `temperature_c`, `dew_point_c` | At 2 m above the model's ground |
| `cloud_cover_pct` | Share of the sky covered, and `cloud_cover_low_pct`, `_mid_pct`, `_high_pct` by layer |
| `precipitation_mm` | Rain, showers and snow in the hour before `time`, as water |
| `cape_j_kg` | Convective available potential energy. High values mean showers and storms can grow, **not that thermals are good** |
| `freezing_level_m` | Height of the 0 °C level above sea level. A value at or near `cell_elevation_m` means the air is below freezing down to the ground |
| `updraft_ms` | The strongest rising air the model has anywhere above the cell up to 10 km. ICON-D2 only. **It is not thermal strength**: it is what a model with 2 km cells resolves, which is showers, storms and flow over mountains |
| `convective_cloud_base_m`, `convective_cloud_top_m` | Above sea level. Left out when the model has no convective cloud over the cell at that hour, which is most hours. The two models do not mean the same convection by it; [data-sources.md](data-sources.md#the-values) says what each means |
| `winds_aloft` | The wind higher up, lowest first: see below |

A level of `winds_aloft` is given in one of two ways:

| Field | Meaning |
|---|---|
| `above_ground_m` | A height above the model's ground: 80, 120 and 180 m. These are nominal heights of the model's own levels |
| `pressure_hpa`, `altitude_m` | A surface of equal pressure, 950, 850, 700, 600 or 500 hPa, with the height above sea level the model gives it at this hour. A surface that lies under the model's ground is left out: at Tegernsee that is the one of 950 hPa |
| `wind_speed_ms`, `wind_direction_deg`, `temperature_c` | At the level |

Every value but `time` may be missing, and a missing value is left out, never written as `null` or zero. Between 180 m above the ground and the pressure surfaces, which lie 1.5 km apart, the answer has nothing.

**What it does not have.** No boundary-layer height, no thermal strength, no cloud base for ordinary cumulus. No model the server has computes a climb rate a glider sees, and nothing here is made up to stand in for one. [data-sources.md](data-sources.md#forecasts) says what the source lacks and where it could come from.

**Credit.** Show the credit of the layer's source next to the forecast, with its link: Open-Meteo asks for that wherever its data is shown.

**Caching.** `Cache-Control: public, max-age=60` and an `ETag`, like the stations. The server itself keeps an answer for up to half an hour, so asking more often than that brings the same answer and a `304`.

**Size.** A whole answer for Tegernsee, two models and 78 hours, is 85 kB plain and 9 kB with gzip (measured 2026-11-19).

**When there is no forecast.**

| Status | `error.code` | When |
|---|---|---|
| `400` | `invalid_position` | `lat` or `lon` is missing, is not a number, or the two are not a place on the globe |
| `404` | `outside_region` | The place lies outside the area the server has forecasts for: its regions |
| `429` | `rate_limited` | This address has asked for the forecasts of 12 places within an hour that the server had to fetch, which is as many as one address may; for an IPv6 client the address is its /64 network, and one /48 network together gets 48. `Retry-After` says when the hour is over. Places that are at hand, which is every place anyone asked for in the last half hour, are still answered, and are never counted |
| `501` | `no_forecasts` | The server was not told where its forecast service is. `/v1/health` says the same with `"forecasts": "not_configured"`. Asking again does not help, and the server asks nobody else |
| `503` | `unavailable` | No forecast for the place is at hand and none could be fetched: the forecast service did not answer, the forecast is still being fetched (the first one for a place can take longer than a request waits), or the server has asked its service as often as it may in an hour. `Retry-After` says when to ask again |

**Why a client is counted.** The server may ask its forecast service 400 times an hour, which is some ninety places, for everyone together. A forecast that is at hand costs nothing; one that has to be fetched comes out of that. An app that shows forecasts asks for the place the pilot opens, not for every place on the map. A request that is refused because the server has asked its service often enough (`503`) is not counted against the client.

An answer in which one of the models is missing is still an answer: the server hands on what it has and asks again within two minutes.

**In the development mode** there are recorded forecasts for five places: the east shore of the Tegernsee (47.70, 11.80), the take-off on the Wallberg (47.66, 11.80), the take-off and the landing below the Jägerkamp (47.68, 11.90 and 47.70, 11.88), and the middle of the first region (47.40, 13.30). A place anywhere in the first region is answered with the forecast of the nearest of them, **and `point` says which one that is**; a real server answers for the place asked. The forecasts were recorded on 2026-11-19 at 19:30 UTC, a day after that mode's wind, and that is the `fetched_at` of every answer: ICON-D2's hours run from 19:00 UTC on 19 November to 18:00 UTC on the 21st, ICON-EU's from 16:00 UTC on the 19th to 21:00 UTC on the 20th. The first day with daylight hours is 20 November.

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
- **A token is shown once**, in the answer to creating an account or signing in. The server keeps only a hash of it. Keep the token where the device keeps secrets (the Keychain, the Keystore) and never in a URL or a log. A browser has no such place: the web app keeps it in the site's own storage, out of reach of other sites but within reach of any script the site itself runs, which is why the site ships none but its own ([apps/web/README.md](../apps/web/README.md), "Where the session is kept").
- **A session ends** when it is signed out, when the password is changed on another device, when the account is deleted, or when it has not been used for 400 days.
- **Attempts are counted.** After 10 wrong passwords for one handle within an hour, or 30 from one address, the answer is `429` with `rate_limited` and a `Retry-After` header, also for the right password. An attempt takes its place among those ten when it arrives and gives it back when its password turns out right, so guesses sent at the same moment count like guesses sent one after another. Creating accounts is limited to 10 attempts an hour from one address. An IPv6 client counts as its /64 network wherever this says "address", and all of one /48 network together get four times as much. When more passwords wait to be checked than the server takes on, the answer is `503` with `unavailable`, at once; such an attempt is not counted. The development mode has none of these limits.

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

The answer is `204` with no body. A wrong `current_password` gives `403` with `wrong_password`. A `new_password` the server would not take gives `400` with `invalid_password`.

Wrong passwords on this request and on `DELETE /v1/account` are counted for the device that sends them, ten an hour for one session, and then answered `429` with `rate_limited`. They are not counted for the handle, and wrong passwords at sign-in are not counted here: whatever strangers do to a handle from outside, a pilot who is signed in can change the password and delete the account.

### `DELETE /v1/account`

Needs a token, and the password in the body: a token alone is not enough for this.

```json
{
  "password": "demo-pilot-password"
}
```

Deletes the account with everything that belongs to it: its sessions, its flights and their files, and its live tracks with their positions. The answer is `204` with no body. It happens at once and cannot be undone; the handle is free again afterwards. A wrong password gives `403` with `wrong_password`, counted as for changing the password.

To take everything along first, list the flights (`GET /v1/flights`) and download each file (`GET /v1/flights/{id}/igc`).

Not gone at once are the copies in the server's nightly backups, which the operator keeps for fourteen days unless configured otherwise; see [deploy/README.md](../deploy/README.md#backup). Live tracks are in no backup.

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
| 408 | `timeout` | The file did not arrive: nothing came for 10 seconds, or not all of it within 100. Send it again; such an upload counts for fifty of the day's 1000 |
| 413 | `payload_too_large` | The file is larger than 4 MB. Twelve hours at one fix a second are about 2 MB |
| 403 | `account_full` | The account has no room for the flight. A server gives an account room for 25,000 flights and 4,096 MB of flight files unless whoever runs it set other numbers; files count as they were uploaded. That is more than twenty years of flying a great deal, so a pilot does not meet it by keeping a logbook. `message` says what the account keeps and what the room is. The same file sent again gets the same answer until flights are deleted; a flight the account already has is answered `200` as always |
| 422 | `invalid_flight` | The file is not an IGC flight file, holds no position, is mostly something other than positions, or could not be analysed. `message` says which. "Mostly something other" means: what is not a position line is larger than the position lines and 64 kB together. Headers, a task, comments and a recorder's signature are a few kilobytes |
| 429 | `rate_limited` | The account is sending more than two files at the same time, or has uploaded 1000 within a day. `Retry-After` says when to try again |
| 503 | `unavailable` | The server is taking in or analysing as many flights as it can right now. `Retry-After` says when to try again |

Send a logbook's flights one after another, or two at a time. A file has 100 seconds to arrive and must not stand still for more than 10 on the way; that lets the largest file through at 40 kB a second.

What counts against the 1000 uploads of a day: every upload counts for one, whatever becomes of it. The time its analysis takes beyond the first second counts as well, in proportion, so that a file which keeps the analysis busy for the ten seconds it is allowed counts for fifty more; a flight is analysed in a fraction of a second and counts for nothing more. A file that did not arrive in time counts for fifty more too.

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

## Live tracks

A signed-in pilot can let other people see where the glider is while it flies: the people who were given a link, or everyone who looks at this server's map. Nothing is shared unless the pilot's app sends it, and an app sends it only for a flight the pilot switched it on for. The app sends the glider's positions in small batches; the server keeps them as a **live track**, hands them to whoever may see it, and removes the track 48 hours after its newest position.

A live track is not a flight. It never reaches the logbook by itself: the logbook's flight is the file the recorder wrote, which the app uploads with `POST /v1/flights` when the pilot wants it kept.

**The identifier.** A live track is known by an `id` that the app makes itself, before it sends the first fix: 26 characters, each drawn at random from `0123456789abcdefghjkmnpqrstvwxyz` with the system's source of random numbers for secrets, which is 130 random bits (`LiveTrackId::from_random` in `venti-api-types` turns 26 random bytes into one). The app makes it, and not the server, for two reasons: a batch sent again after a lost answer adds nothing twice, and the pilot can hand the link on before the first answer has come back through a bad network. Three things follow.

- **Whoever has the identifier can see the track**, with or without an account. For a track shared by link it is the only key. Treat it like one: it belongs in no log, no crash report and no analytics. The server keeps it out of its own log and out of every error message.
- **An identifier is used once.** One account begins a track under it; nobody else can send under it (`409` with `live_track_taken`). Once the track is deleted or removed, the identifier takes nothing more, from anyone (`410` with `live_track_gone`). A new flight gets a new identifier.
- The link to share is the server's public address followed by `/live/` and the identifier; `venti_api_types::v1::live_page_path` builds the path, and the answer to every batch carries the whole address as `page`.

**Who sees a track** is its `visibility`:

| Value | Meaning |
|---|---|
| `link` | Whoever has the identifier, which is to say the link. The track is in no list |
| `public` | Everyone. The track is listed by [`GET /v1/live`](#get-v1livebboxwestsoutheastnorth) for the part of the map it is in, with its pilot's handle and display name |

There is no value for "nobody": a track nobody may see is one the app does not send. A client must expect further values in answers; one for followers is planned. Both kinds show the pilot's handle and display name, as a shared flight does.

Making a public track `link` takes it off the list at once. It does not take it away from someone who copied its identifier while it was listed: the identifier is the key. To cut everyone off, delete the track and begin a new one.

**A fix** is where the glider was at one moment:

| Field | Meaning |
|---|---|
| `time` | When, in whole seconds: the server drops the fraction of a second. A track holds one fix for a second; of two for the same second the first one received stays |
| `position` | Latitude and longitude |
| `altitude_m` | Metres above mean sea level, as the cockpit shows it. A whole number, from -1000 to 20000 |
| `ground_speed_ms` | Speed over the ground, m/s, from 0 to 150. May be left out |
| `climb_ms` | Climb rate, m/s, negative when sinking, from -100 to 100. May be left out |
| `course_deg` | The direction the glider moves in over the ground, degrees clockwise from true north, from 0 to 360. May be left out; a glider that stands still has none |

**When a track ends.** The server never decides that a glider has landed.

- The app says so: `landed` is `true`. A track that says landed and then gets a newer fix that says otherwise is under way again: a pilot who top-lands and flies on is flying.
- Nothing arrives. A track whose newest fix is more than 30 minutes old has gone quiet: it leaves the list of public tracks, and by its identifier it is still there, with the age of its last position for everyone to see. That is all the server knows. Quiet means no network or an empty battery far more often than anything else, and a client must never show it as a landing. `venti_api_types::v1::LIVE_LISTED_MINUTES` is the 30.
- Either way the track goes on when newer fixes arrive.
- **48 hours after its newest fix a track is removed**, with every fix. From that moment it answers as if it had never been, whether or not the server has got round to deleting the rows, which it does within the hour. `expires_at` in every answer says when that moment is; each newer fix moves it. Whoever runs a server can set another number of hours (`retention.live_hours`), which `expires_at` and the page then follow.
- The pilot can delete a track at any time, and deleting the account deletes its tracks.
- Live tracks are not in the server's nightly backups ([deploy/README.md](../deploy/README.md#backup)): what is gone from the server after 48 hours is gone.

**How an app sends.** What the server takes is in the reference below; this is what it is built for.

1. When the pilot switches live tracking on for a flight, make an identifier and keep it with the flight.
2. Record one fix a second at most, and send what has gathered every ten to fifteen seconds in one request. A track holds 43,200 fixes, which is twelve hours at one a second.
3. Keep every fix until a request that carried it was answered `200` or `201`. Then drop it, whatever the counts in the answer say: what the server refused it will refuse again.
4. After a gap in the network, send the **newest** fixes first, so that those who watch see where the glider is, and then the fixes of the gap, in requests of at most 120 fixes. Fixes may arrive in any order.
5. When the glider has landed, say so with the last fixes, or by sending the newest fix once more with `landed: true`.
6. To change who sees the track, send the new `visibility` with the next fixes. To narrow it without waiting for a fix, send the newest fix once more with `visibility: link`.
7. `429` and `503` name a time to wait in `Retry-After`; a request that got no answer at all, or a `500`, is sent again as it was. `401` means that the device is signed out. Every other error is final for that request: `400`, `413` and `415` are faults in the app, `422` with `fixes_out_of_time` means that the device's clock is wrong, and `403`, `409` and `410` mean that this identifier takes no more. The flight then goes on under a new identifier, unless the `410` is the answer to the pilot's own deletion.

**How an app follows a track.** Ask for it once, and then for what arrived since:

1. `GET /v1/live/{id}`. While `more` is `true`, ask again at once with `since` set to the `cursor` of the answer before.
2. From then on ask every five to ten seconds with `since` set to the last `cursor`, and with `If-None-Match` set to the `ETag` of the last answer to that same address. While nothing changes the answer is `304` with no body, which costs the server one small read.
3. Put the fixes of every answer together with the ones held and sort them by `time`: fixes that arrived late lie before others in time, and they are handed out all the same, because `since` goes by arrival and not by time.
4. `track.latest` is the newest position, in every answer. Work out its age against the `time` of the answer, which is the server's clock, and let the device's own clock run on from there until the next answer that is not a `304`.
5. `404` means that the track is gone. Stop asking.

**Who may keep an answer.** The list and a public track carry `Cache-Control: public, no-cache`, a track shared by link `private, no-cache`: an answer may be kept, and must be checked with the server before every reuse, so that a track its pilot deletes or closes is gone at once. The answer to a batch was asked with a token and is `no-store`. The page a link opens is `no-store` as well.

**What one account may do.** The limits are counted in the server's memory, per account, like the uploads of flights.

| Limit | Why this number |
|---|---|
| 300 fixes a minute, sent in whatever becomes of them. A request is taken whole while the account is under 300, so a minute can end up to one request over | Five times one fix a second: ten minutes of fixes that waited for the network are sent in two and a half minutes beside the new ones. Then `429` with `rate_limited`; `Retry-After` is at most 60 |
| 30 live tracks a day, counting identifiers taken out of use without a track | More flights than a day of flying holds, tandem flights included (an estimate), and what bounds how many gliders one account can put on everyone's map. Then `429`; tracks that exist go on |
| 43,200 fixes in one track | Twelve hours at one a second. Then `403` with `live_track_full`; the flight goes on under a new identifier |
| 120 fixes and 32 kB in one request | Two minutes at one a second. More fixes are `400` with `invalid_fixes`, a larger body `413` |
| Fixes no more than 2 minutes ahead of the server's clock and no more than 12 hours old | A phone that takes its time from the network is seconds off; a flight is not longer than twelve hours. Others are left out and counted in the answer |

An answer about one track carries at most 5,000 fixes and the list at most 1,000 tracks. A fix costs the server 165 bytes in its database, both indexes included (measured on 2026-11-19 with PostgreSQL 16): a full track is 7 MB, and what one account can send in a day is 71 MB at 300 fixes a minute and at most 100 MB with every minute one request over, gone again two days later.

### `POST /v1/live/positions`

Needs a token. Adds fixes to a live track of the account, and begins the track if this is the first request for its identifier. The same request sent twice changes nothing the second time.

```bash
curl -X POST http://127.0.0.1:8080/v1/live/positions \
  -H 'Authorization: Bearer venti_0000000000000000000000000000000000000000000000000000' \
  -H 'Content-Type: application/json' \
  --data @positions.json
```

```json
{
  "id": "44444444444444444444444444",
  "visibility": "link",
  "landed": false,
  "fixes": [
    {
      "time": "2026-11-18T19:04:40Z",
      "position": {
        "lat": 47.6642,
        "lon": 11.7967
      },
      "altitude_m": 1620
    },
    {
      "time": "2026-11-18T19:04:50Z",
      "position": {
        "lat": 47.6649,
        "lon": 11.7958
      },
      "altitude_m": 1608,
      "ground_speed_ms": 10.5,
      "climb_ms": -1.2,
      "course_deg": 320.0
    },
    {
      "time": "2026-11-18T19:05:00Z",
      "position": {
        "lat": 47.6656,
        "lon": 11.7949
      },
      "altitude_m": 1596,
      "ground_speed_ms": 10.5,
      "climb_ms": -1.2,
      "course_deg": 320.0
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `id` | The track's identifier, which the app made; see above |
| `visibility` | Who may see the track: `link` or `public` |
| `landed` | Whether the glider has landed. Left out means `false` |
| `fixes` | The fixes, oldest first: at least one, at most 120. A request whose fixes are not in that order is refused whole; fixes with the same second are not an error |

`visibility` and `landed` describe the track as of the newest fix of the request, and the track is what the request with the newest fix says. A request that brings a newer fix than the track had sets both. A request whose newest fix is the one the track already has can only narrow who sees the track (`public` to `link`) and can only say that the glider landed. A request of older fixes, such as one that fills a gap, changes neither. So requests may arrive in any order, and a request that was on its way for a minute does not undo what the pilot chose since.

The answer is `201` when the request began the track and `200` when the track was there; `created` says which. This is the development mode's answer to the request above. Its `page` starts with the address the server was configured with (`public_url`), which here is the one the tests use; without that setting it is the address the server listens on, which nobody else can follow.

```json
{
  "track": {
    "id": "44444444444444444444444444",
    "pilot": {
      "handle": "demo",
      "display_name": "Demo Pilot"
    },
    "visibility": "link",
    "landed": false,
    "started_at": "2026-11-18T19:04:40Z",
    "latest": {
      "time": "2026-11-18T19:05:00Z",
      "position": {
        "lat": 47.6656,
        "lon": 11.7949
      },
      "altitude_m": 1596,
      "ground_speed_ms": 10.5,
      "climb_ms": -1.2,
      "course_deg": 320.0
    },
    "fixes": 3,
    "expires_at": "2026-11-20T19:05:00Z"
  },
  "page": "https://venti.example/live/44444444444444444444444444",
  "created": true,
  "stored": 3,
  "known": 0,
  "refused_future": 0,
  "refused_old": 0,
  "refused_full": 0
}
```

| Field | Meaning |
|---|---|
| `track` | The track as it now is; see [`GET /v1/live/{id}`](#get-v1liveidsinceafterlimit) |
| `page` | The address of [the page to share](#the-page-a-live-track-has) |
| `created` | `true` when this request began the track |
| `stored` | Fixes this request added |
| `known` | Fixes the track already held, or that had the same second as an earlier fix of the request. Nothing changed for them. A request that is sent twice gets all of its fixes back as `known` |
| `refused_future` | Fixes left out because their time lies more than two minutes ahead of the server's clock |
| `refused_old` | Fixes left out because they are more than twelve hours old |
| `refused_full` | Fixes left out because the track holds as many as a track may. The oldest new fixes are taken while there is room |

| Status | Code | When |
|---|---|---|
| 400 | `invalid_live_track_id` | `id` is not 26 characters of the alphabet above |
| 400 | `invalid_visibility` | `visibility` is not `link` or `public` |
| 400 | `invalid_fixes` | No fix, more than 120, a fix older than the one before it, or a value outside what the table of a fix allows. `message` says which fix by its place in the request, counted from one, and which value, and never the value |
| 400 | `invalid_request` | The body is not JSON of this shape. A position that is not a place on the globe, an altitude with a fraction and a time that is not a time end here |
| 403 | `live_track_full` | The track holds 43,200 fixes and none of the request's new ones fits |
| 409 | `live_track_taken` | The identifier is that of another account's track |
| 410 | `live_track_gone` | The identifier's track was deleted, was removed 48 hours after its newest fix, or the identifier was taken out of use before a track began |
| 413 | `payload_too_large` | The body is larger than 32 kB |
| 422 | `fixes_out_of_time` | Every fix of the request was left out for its time. `message` says how many lie in the future and how many are too old, and what the server's clock shows |
| 429 | `rate_limited` | The account has sent 300 fixes within a minute, or has begun 30 live tracks within a day. `Retry-After` says when to try again |

### `GET /v1/live?bbox=west,south,east,north`

Who is in the air: the public tracks whose newest fix lies inside a box and is no older than 30 minutes. Anyone may ask, and the answer is the same for everyone. A track shared by link is never in it.

`bbox` is required, as for [stations](#get-v1stationsbboxwestsoutheastnorth). A track is listed by where its newest fix is, not by where it has been.

```bash
curl 'http://127.0.0.1:8080/v1/live?bbox=11.6,47.55,12.0,47.85'
```

```json
{
  "time": "2026-11-18T19:05:00Z",
  "tracks": [
    {
      "id": "11111111111111111111111111",
      "pilot": {
        "handle": "demo",
        "display_name": "Demo Pilot"
      },
      "visibility": "public",
      "landed": false,
      "started_at": "2026-11-18T18:44:56Z",
      "latest": {
        "time": "2026-11-18T19:04:56Z",
        "position": {
          "lat": 47.663773253525605,
          "lon": 11.743167672151715
        },
        "altitude_m": 1596,
        "ground_speed_ms": 10.5,
        "climb_ms": -1.0,
        "course_deg": 20.0
      },
      "fixes": 1201,
      "expires_at": "2026-11-20T19:04:56Z"
    },
    {
      "id": "22222222222222222222222222",
      "pilot": {
        "handle": "demo",
        "display_name": "Demo Pilot"
      },
      "visibility": "public",
      "landed": true,
      "started_at": "2026-11-18T18:45:01Z",
      "latest": {
        "time": "2026-11-18T18:56:00Z",
        "position": {
          "lat": 47.692168145789914,
          "lon": 11.767486837376403
        },
        "altitude_m": 786,
        "ground_speed_ms": 0.0,
        "climb_ms": 0.0
      },
      "fixes": 660,
      "expires_at": "2026-11-20T18:56:00Z"
    }
  ],
  "truncated": false
}
```

| Field | Meaning |
|---|---|
| `time` | The server's clock when it wrote the answer. Work out the age of a position against this, not against the device's clock. An answer of `304` does not renew it |
| `tracks` | The tracks, sorted by identifier; each as [`GET /v1/live/{id}`](#get-v1liveidsinceafterlimit) shows it. A track that has landed stays on the list until its newest fix is 30 minutes old, with `landed` set |
| `truncated` | `true` when the box holds more than 1,000 tracks. The answer then lists the first 1,000 by identifier, which is a chance selection; ask for a smaller box |

An app that draws the list asks again every ten to fifteen seconds for the part of the map in view, with `If-None-Match`. The answer's `ETag` covers the tracks and not `time`: while no track in the box has changed, the answer is `304`.

### `GET /v1/live/{id}?since=&after=&limit=`

One track and its fixes, for whoever knows its identifier: no token is needed, and the answer is the same with one. It makes no difference whether the track is `public` or `link`.

| Parameter | Meaning | Default |
|---|---|---|
| `since` | The `cursor` of an earlier answer about this track: only fixes that **arrived** after that answer are returned | Every fix |
| `after` | An RFC 3339 time: only fixes whose **time** is later are returned. For an app that draws only the last minutes of each glider on a map | No bound |
| `limit` | How many fixes to return, 1 to 5000 | 5000 |

The two differ when fixes arrive late. `since` never misses a fix, whatever its time; `after` leaves out everything before a moment, also what arrives later for the time before it. They can be used together.

```bash
curl 'http://127.0.0.1:8080/v1/live/11111111111111111111111111?after=2026-11-18T19:04:53Z'
```

```json
{
  "time": "2026-11-18T19:05:00Z",
  "track": {
    "id": "11111111111111111111111111",
    "pilot": {
      "handle": "demo",
      "display_name": "Demo Pilot"
    },
    "visibility": "public",
    "landed": false,
    "started_at": "2026-11-18T18:44:56Z",
    "latest": {
      "time": "2026-11-18T19:04:56Z",
      "position": {
        "lat": 47.663773253525605,
        "lon": 11.743167672151715
      },
      "altitude_m": 1596,
      "ground_speed_ms": 10.5,
      "climb_ms": -1.0,
      "course_deg": 20.0
    },
    "fixes": 1201,
    "expires_at": "2026-11-20T19:04:56Z"
  },
  "fixes": [
    {
      "time": "2026-11-18T19:04:54Z",
      "position": {
        "lat": 47.663595785448955,
        "lon": 11.743071761920355
      },
      "altitude_m": 1598,
      "ground_speed_ms": 10.5,
      "climb_ms": -1.0,
      "course_deg": 20.0
    },
    {
      "time": "2026-11-18T19:04:55Z",
      "position": {
        "lat": 47.66368451948728,
        "lon": 11.743119717036036
      },
      "altitude_m": 1597,
      "ground_speed_ms": 10.5,
      "climb_ms": -1.0,
      "course_deg": 20.0
    },
    {
      "time": "2026-11-18T19:04:56Z",
      "position": {
        "lat": 47.663773253525605,
        "lon": 11.743167672151715
      },
      "altitude_m": 1596,
      "ground_speed_ms": 10.5,
      "climb_ms": -1.0,
      "course_deg": 20.0
    }
  ],
  "cursor": "1201",
  "more": false,
  "reset": false
}
```

| Field | Meaning |
|---|---|
| `time` | The server's clock when it wrote the answer; see the list above |
| `track.id` | The track's identifier |
| `track.pilot` | Handle and display name of the pilot |
| `track.visibility` | Who may see the track |
| `track.landed` | Whether the app has said that the glider landed |
| `track.started_at` | When the track began: the time of its oldest fix. It moves back when older fixes arrive late |
| `track.latest` | The newest fix: where the glider is, or was last heard of |
| `track.fixes` | How many fixes the track holds |
| `track.expires_at` | When the server removes the track, unless a newer fix arrives first: 48 hours after `latest.time` |
| `fixes` | The fixes asked for, in the order in which the server received them. That is the order of their times except where fixes arrived late: sort by `time` before drawing |
| `cursor` | Pass this as `since` next time. Treat it as an opaque value |
| `more` | `true` when more fixes are waiting than `limit` allowed: ask again with `cursor` at once |
| `reset` | `true` when the server did not know the `since` it was given and started again from the track's first fix. Drop the fixes held for the track and keep the ones of this answer |

A track that does not exist, one its pilot deleted and one that was removed for its age all get `404` with `live_track_not_found`, the same in every detail; so does text that has not the form of an identifier. A `since` that is not a whole number from zero up gives `400` with `invalid_cursor`, an `after` that is not a time `invalid_time`, a `limit` outside 1 to 5000 `invalid_limit`.

### `DELETE /v1/live/{id}`

Needs a token. Deletes a live track of the account with all its fixes, at once and for good, and takes its identifier out of use. The answer is `204` with no body.

- Deleting again is `204` again: an app whose first answer was lost can ask twice.
- Fixes that arrive afterwards under the identifier are refused with `410`, also fixes that were sent before the deletion and were held up on the way. A deleted track does not come back.
- That holds for a track that never began as well. An app that deletes an identifier whose first fixes have not arrived yet, because the pilot switched live tracking off again at once, gets `204`, and the fixes begin nothing when they arrive. The server has to remember such an identifier, so this counts as one of the account's 30 live tracks a day, and is answered `429` when those are used up.
- A track of another pilot is no track of this account, whoever may see it: `404` with `live_track_not_found`.

## The page a shared flight has

These two addresses are not part of `/v1`. They answer browsers and the preview fetchers of chat apps, which send no token, so they show public and unlisted flights only. A private flight and an address that belongs to no flight get the same `404` page.

| Address | Answer |
|---|---|
| `/f/{id}` | A page about the flight in HTML: its numbers, its track and altitude profile as drawings, and a link to the flight file. Its head carries Open Graph and Twitter-card lines, from which a chat app builds the preview of a link |
| `/f/{id}/preview.png` | The picture for that preview, 1200 by 630 pixels: the track's shape and the flight's numbers. The server draws it itself |

A share link is the server's public address followed by `/f/` and the flight's `id`; `venti_api_types::v1::share_path` builds the path. The page needs nothing but itself: no script, no font, no other request. The web app will later be linked from it.

The links inside the page start with the address the server was configured with (`public_url`). On a server where that is not set they point at the address it listens on, which a chat app cannot follow.

## The page a live track has

`/live/{id}` is what a link to a live track opens, for people without the app: someone at home, or whoever drives out to fetch the pilot. Like the page of a shared flight it is not part of `/v1`, it asks for no token, and it answers HTML also when something is wrong. It shows any track whose identifier is in its address, `public` or `link`. An identifier that leads to no track, text that is no identifier, a deleted track and one removed for its age all get the same `404` page.

The page says in words, with no script:

- whose track it is: the pilot's display name and handle;
- how the track stands. "… is under way" while fixes arrive and no landing was reported; "… has landed" once the app has said so; "… was last heard of 40 minutes ago" when no landing was reported and the newest position is more than 30 minutes old, with a sentence that says that this is what a phone without a network or with an empty battery looks like, and that it does not say what happened. Silence is never called a landing;
- where the glider is: latitude and longitude, and the time of that position;
- how old that position is, as of the moment the page was made, which the page states;
- the altitude above sea level, and the speed over the ground, the climb and the heading where the app sent them;
- when the track began, how many positions it holds, who can see it, and for how long the server keeps a track.

Below the numbers it draws the end of the track as a line seen from above, north up: the newest 1,800 fixes, which is half an hour at one a second, with a filled dot at the newest position. There is no map under the line.

**It loads itself again.** A page without a script cannot fetch the next position, so it asks the browser to load it again: every 15 seconds while the track is under way, every 60 once it has landed or gone quiet. A link on the page stops that (`/live/{id}?renew=off`), for readers whom a page that reloads disturbs, and another starts it again.

**What a chat app keeps of the link** is the title "Live track of" and the pilot's name, and one sentence that says what the page is. No position, no altitude, no time and no "in the air": a chat keeps its preview for good, and none of those would be true a minute later. For the same reason the page has no preview picture.

**Headers.** `Cache-Control: no-store`: the page says how old a position is, and no copy of it may be handed out later. `X-Robots-Tag: noindex` and the same in the page, also for a public track: where a person is right now is nothing for a search engine. `Referrer-Policy: same-origin`, because the address is the key to the track. A content security policy that allows the page its own style and nothing else: no script, no frame, no form, no request to anywhere.

What is left to build: a map under the line, and one that moves without the page being loaded again. Both need a script and map tiles, and belong to the web app, which the page will link to once the web app shows live tracks.

## What is not here yet

Recovering a forgotten password, passkeys, changing a handle or a display name, flights visible to followers only, follows and a feed, forecast maps, pages about a site with its rules and webcams and which airspace is active today are planned and not built; see [plan.md](plan.md). Of live tracks the server's side is here and no app sends or draws one yet. Not built for them: a track only followers see, a track shown with a delay, a list of an account's own tracks, the positions of gliders that do not fly with Venti (the Open Glider Network and the trackers pilots carry; [data-sources.md](data-sources.md#live-tracking)), and a way to report a track or to take one down, which a server that lets strangers register needs before it lets them put gliders on everyone's map. The connector for a pilot's own assistant is built and is not part of this API: it is the endpoint `/mcp` beside it and has a document of its own, [mcp.md](mcp.md). Station metadata is planned to move into downloadable region bundles ([architecture.md](architecture.md#api-and-offline-sync)); the station list will keep working when it does.
