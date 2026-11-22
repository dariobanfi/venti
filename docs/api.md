# The server's HTTP API

Last revised: 2026-11-21

The server answers a small JSON API that the phone and web apps use: for the live wind map, for airspace, for flying sites, for forecasts, for a pilot's account and flights, for live tracks, and for the groups in which pilots plan a flying day. Beside the JSON it hands out the map regions that an app downloads to draw its map without a network, and passes on tiles of map imagery, cut where they may not be shown. This document describes it for people. The exact wire format is the Rust crate `venti-api-types`, which the server builds its answers from and the clients decode them with; when this text and that crate disagree, the crate is right and this text has a bug. For a program there is a description it can read, an OpenAPI document that the server hands out itself: [`GET /v1/openapi.json`](#get-v1openapijson). Whoever writes a program that is not one of Venti's apps begins at [Writing a program against Venti](#writing-a-program-against-venti).

Everything here is version 1. It has four parts:

- **Weather stations, airspace, flying sites, forecasts and map regions**, which anyone may read: [health](#get-v1health), [stations](#get-v1stationsbboxwestsoutheastnorth), [observations](#get-v1stationsidobservationsfromto), [airspace](#get-v1airspacebboxwestsoutheastnorth), [sites](#get-v1sitesbboxwestsoutheastnorth), [forecast](#get-v1forecastlatlon), [the thermal map](#get-v1thermals), [the hours of its days](#get-v1thermalshoursdatelayerheight) and [the forecast along a route](#get-v1routeweatherdatepoints), [the map regions to download](#maps) and [their files](#get-and-head-v1mapsregionversionname), and [tiles of map imagery](#get-v1imagerysourcezxy).
- **Accounts and flights**: [accounts and sessions](#accounts-and-sessions), [the sites a pilot keeps as their own](#the-sites-a-pilot-keeps), [alarms](#alarms), [flights](#flights), [pilots](#pilots), [everyone's public flights in one list](#public-flights) with [what changed among them](#get-v1flightspublicchangessincepilotlimit) and [a flight's track as GeoJSON](#get-v1flightsidtrack), which a program of another maker reads as [Following Venti's public flights](#following-ventis-public-flights) says, [the page a shared flight has](#the-page-a-shared-flight-has), and [reporting something to whoever runs the server](#reports), with [what they can then do](#what-whoever-runs-the-server-can-do).
- **Live tracks**, where a glider is while it flies: [what a live track is](#live-tracks), [sending positions](#post-v1livepositions), [who is in the air](#get-v1livebboxwestsoutheastnorth), [one track](#get-v1liveidsinceafterlimit), [deleting one](#delete-v1liveid), and [the page a live track has](#the-page-a-live-track-has).
- **Groups**, which only their members see: [what a group is](#groups), [making one](#post-v1groups), [the groups a pilot is in](#get-v1groups), [coming into one with an invite code](#post-v1groupsjoin), [a group and its members](#get-v1groupsid), [what its keeper can do](#patch-v1groupsid), [leaving](#delete-v1groupsidmembershandle), and [plans for a flying day](#plans-for-a-flying-day): [posting one](#post-v1groupsidplans), [a group's plans](#get-v1groupsidplansfromto), [the plans of all of a pilot's groups](#get-v1groupsplansfromto), and [saying "I am in"](#put-and-delete-v1groupsidplansplanin).

## Writing a program against Venti

For the maker of a program that is not one of Venti's apps: a page, a script, another app, in any language. It needs HTTP and JSON and nothing of this repository's code. This part says where each thing is; the sections it links to say it in full.

**The description.** [`GET /v1/openapi.json`](#get-v1openapijson) is an OpenAPI 3.1 document of everything under `/v1`: every path and method, its parameters, the shape of its answer and of its body, the one shape every error has, and for each request whether it needs nothing, the pilot's own sign-in, or a token with which scope. The same document is in the repository as [openapi.json](openapi.json). A client can be generated from it. What it does not say is here: what a field means, when which error comes, the bounds.

**Reading without a key.** Everything anyone may read needs no account, no key and no registration: [stations](#get-v1stationsbboxwestsoutheastnorth) and [their observations](#get-v1stationsidobservationsfromto), [airspace](#get-v1airspacebboxwestsoutheastnorth), [sites](#get-v1sitesbboxwestsoutheastnorth) with [their history](#get-v1sitesidhistoryafterlimit), [who confirmed them](#get-v1sitesidconfirmations) and [what public flights say about them](#what-public-flights-say-about-a-site), [forecasts](#get-v1forecastlatlon), [the thermal map](#get-v1thermals), [the rain radar](#get-v1radar), [public flights](#public-flights) with [their tracks](#get-v1flightsidtrack), [a pilot's page](#pilots), [the search for pilots](#get-v1pilotsqlimit) and [a pilot's totals](#get-v1pilotshandlestatistics), [the ranking](#ranking), [public live tracks](#live-tracks). Send no token with such a request: an answer to a request with a token is made for one account and is kept by nobody. From a browser, a page on any site may make these reads; [Browsers](#rules-that-hold-for-every-endpoint) says what exactly.

**Acting for a pilot.** A pilot makes a token for the program in their account, limited to scopes, and gives the program its secret; the program sends it as `Authorization: Bearer venti_pat_…`. The program never sees the pilot's password, and the pilot withdraws the token at any time. [Tokens for other programs](#tokens-for-other-programs) has the scopes, which request needs which, and the errors a program meets.

**An assistant acting for a pilot, without a copied secret.** A pilot's own assistant reads the pilot's logbook over the [connector](mcp.md), after the pilot agreed, in the web app where they are signed in already or on a page of the server with handle and password: OAuth 2.1 with PKCE, a token that reads flights and nothing else, withdrawn like any other. [Connecting an assistant to a pilot's logbook](#connecting-an-assistant-to-a-pilots-logbook) has it step by step. Such a token is for the connector and is no token at `/v1`.

**The bounds.** Open does not mean unbounded. An answer `429` carries `Retry-After`: wait that long. Ask again with `If-None-Match` and the `ETag` of the answer you kept, and an unchanged one costs a `304`; `Cache-Control` says for how long an answer may be reused without asking ([Caching](#rules-that-hold-for-every-endpoint)). The bounds that are counted today: a token for a program makes [3,600 requests an hour](#limits), its own budget, and every answer it gets says in `RateLimit` what is left; [600 reads an hour per address](#how-often-an-address-may-read) of public flights as data without a token, said the same way; forecasts of places the server has to fetch, per address ([forecast](#get-v1forecastlatlon)); a box that holds too much is refused with `box_too_large`; a page is 100 entries at most; and everything an account writes is counted per account, which a token shares ([Tokens](#tokens-for-other-programs)). Reads that have no bound today may get one.

**What the licences ask.** The data is not Venti's to give away without conditions, and the conditions travel in the answers:

- `attributions`, in every answer that carries the data of a source: one entry for each source the answer draws on, with the licence's name, the credit line as the provider asks for it, the link, and a notice where the provider wants one said. Whoever shows or passes on the data shows the credit of its source beside it. The sources' licences are not all alike: some ask for the credit only, OpenStreetMap's sites are share-alike, openAIP's airspace is not for commercial use. Read `licence` per source and keep the records of two sources apart, as the answers do ([Sources and credit](#rules-that-hold-for-every-endpoint)). The sites Venti holds itself are two sources of their own, the partners' layer `venti` and the crowdsourced `venti-pilots`, both under CC0 1.0, which asks for no credit; each has a `notice` that is to be shown wherever its sites are ([Sites that Venti's partners and pilots add](#sites-that-ventis-partners-and-pilots-add)).
- `terms`, in every answer that holds flights of pilots: under what terms they may be used. They are **not decided** (`"decided": false`): until they are, the flights are their pilots', shown for reading, and reuse needs the pilot's leave. Read `terms` from each answer; do not keep your own copy of the wording.

**Following changes.** Two kinds of record can be kept as a copy that stays true, by asking what changed since a cursor, removals included: the sites Venti holds itself, with [`GET /v1/sites/changes`](#get-v1siteschangeslayersincelimit) for each of its two layers, `venti` and `venti-pilots`, with a cursor of its own, and public flights, with [`GET /v1/flights/public/changes`](#get-v1flightspublicchangessincepilotlimit), for which [Following Venti's public flights](#following-ventis-public-flights) has the steps. Both work alike: keep `cursor`, ask again with `since=`, at once while `more` is true; `reset` says to begin again. A flight its pilot withdrew is removed from every copy. A copy of the sites need not begin with the first page: each layer is there whole as a file ([Dumps of Venti's own sites](#dumps-of-ventis-own-sites)), which names the cursor to go on from. There are no webhooks; a program asks.

**What is promised, and what is not.** This is `/v1`. Promised within it: a path stays, a field keeps its name, its type and its meaning, and an error keeps its `code`; an identifier is never used for another record. Not promised: that an answer has no more fields tomorrow, that a list of known values (error codes, scopes, kinds) has no more entries, the wording of any `message`, the numbers of the bounds, and how long an answer may be cached. So a client ignores fields it does not know and expects values it does not know. A change that would break the first list needs a `/v2` beside `/v1`; nothing says yet for how long `/v1` would then stay. What the server holds depends on who runs it: `GET /v1/health` says which sources a server has, whether it has forecasts and radar, and whether it takes sites from pilots. The description is held to the server by tests, and it is new (2026-11-21): no client of another maker has been generated from it yet.

### `GET /v1/openapi.json`

The description of this API for programs: an OpenAPI 3.1 document, JSON. No token; the same for everyone; `Cache-Control: public, max-age=3600` and an `ETag`. The repository keeps what the server hands out as [`docs/openapi.json`](openapi.json), and a test holds the two equal, so a change to the API shows as a change of that file.

```bash
curl http://127.0.0.1:8080/v1/openapi.json
```

The server makes it from a table of its operations beside its router and from one schema for every record that goes over the wire ([architecture.md](architecture.md#the-description-of-the-api)). Three things hold it true, all tests: every route the router has is described and the reverse; every example answer of this document, which are the files of `crates/server/tests/golden`, is valid against the schema of its request, with no field the schema lacks; and who may ask is as the table of [scopes](#tokens-for-other-programs) says.

How to read it:

- **Who may ask** is an operation's `security`: empty for what anyone may read; `session` for the token of the app's own sign-in; `token` with the one scope a token for a program needs. Where both are listed, either does. An operation that lists an empty way beside them (a flight by its address) is open for what its pilot shares and needs one of the two for what is private.
- **Errors** have one shape everywhere, `ErrorResponse`, which every operation names as its `default` answer. The codes there are today are listed at the schema `ErrorCode` (`x-venti-values`); [the table above](#rules-that-hold-for-every-endpoint) says which status each comes with. The description does not say which operation answers which code.
- **No schema is closed.** A record may gain fields; `x-venti-values` on a schema lists the values a field has today where more may come, and `enum` those that are all there are.
- **A field that is not required may be left out**, and is never `null`. Where `null` does stand, the schema says so: in the lists of the thermal map that have an entry per cell ([Missing values](#rules-that-hold-for-every-endpoint)).
- **`x-venti-docs`** on an operation is the heading of this document that describes it.
- `HEAD` is answered wherever `GET` is and is not listed. The [share pages](#the-page-a-shared-flight-has) and [the connector](mcp.md) are not under `/v1` and are not in it.

## Try it

```bash
cargo run -p venti-server -- dev
curl 'http://127.0.0.1:8080/v1/stations?bbox=10.9,47.0,11.5,47.5'
```

The development mode serves this API over recorded data: twelve stations, three of each of four sources, as they were on 2026-11-18 at 19:05 UTC, and two more that are made up for the [foehn reading](#get-v1foehn), Klagenfurt's and Salzburg's airfields, with a pressure, a temperature and a light wind (three German and three Austrian as recorded then; three of South Tyrol and three airfields with what their services, asked three days later, gave for 18:50 UTC of that evening), and 192 airspaces recorded from openAIP the day after and served as if fetched at that same moment: 88 of the northern Alps around Munich, Innsbruck and Salzburg, and 104 small ones from the six other countries the first region's box takes in. They are a sample. The development mode says of them what a real server says of its whole set, that they [cover](#get-v1airspacebboxwestsoutheastnorth) the first region, so that a screen can be built and tested in the state a pilot will see; nobody flies with them. The flying sites are not a sample: they are the 502 that OpenStreetMap had for the first region on 2026-11-18, recorded the day after and served as if fetched at the same moment as everything else. Forecasts were recorded for five places around the Tegernsee and in the middle of the region, three days after the wind, and are [served as of that moment](#get-v1forecastlatlon). The rain radar has both of the first region's sources, two frames of each, recorded on 2026-11-21 within two minutes: DWD's of 13:10 and 13:15 UTC and GeoSphere Austria's analysis of 12:45 and 13:00, and a clock of its own that stands at 13:19, four minutes after DWD's second ([`GET /v1/radar`](#get-v1radar)). The mode needs no network and no database, its clock stands still at that moment, and every start gives the same answers. The examples below are its answers; the tests keep them in `crates/server/tests/golden` and check that this document shows them unchanged.

Map regions are the one thing the mode does not carry inside it. It hands out those of a folder it is told, as a real server hands out those of its own, and none otherwise. The repository holds one region, the valley of the sample flight that the apps bundle:

```bash
cargo run -p venti-server -- dev --maps apps/ios/Venti/Resources
curl http://127.0.0.1:8080/v1/maps
```

The mode has read the region's files and checked their hashes before it answers its first request, so a test that asks at once finds the region; [Maps](#maps) has the answer.

It also starts with one account, so that an app can be built and tested against it without creating one first:

| | |
|---|---|
| Handle | `demo` |
| Password | `demo-pilot-password` |
| A session token that always works | `venti_0000000000000000000000000000000000000000000000000000` |
| A [token for another program](#tokens-for-other-programs), "Demo logger", which may read and write the account's flights and nothing else | `venti_pat_0000000000000000000000000000000000000000000000000000` |
| Flights | Three made-up ones: one public (2026-08-15, it lands), one unlisted (2026-07-04, the sample file the apps bundle; its recording ends in the air), one private (2025-06-21, a short one) |
| Live tracks | Three, made from the sample flight and dated by this mode's clock; see the table below |
| Groups | One that it keeps, with the two further pilots in it and three plans; see the tables below |

```bash
curl http://127.0.0.1:8080/v1/flights \
  -H 'Authorization: Bearer venti_0000000000000000000000000000000000000000000000000000'
```

Two further pilots are there as well, so that [the list of everyone's public flights](#public-flights) holds the flights of several. One flight of the first of them was public and is shared by link now, so that [what changed among the public flights](#get-v1flightspublicchangessincepilotlimit) holds eleven flights and one that was withdrawn. Their names are made up, as "Max Mustermann" is. Each signs in with the demo account's password; neither has a token that always works, and neither has a live track.

| Handle | Display name | Flights |
|---|---|---|
| `mara` | Mara Muster | Public: 2026-08-15 at 11:10 UTC (the first 25 minutes of the sample flight) and 2026-08-16 at 12:05 (the whole of it, with a landing). Unlisted: 2026-08-17 |
| `jonas` | Jonas Beispiel | Public: 2026-08-16 at 10:00 UTC (the first 20 minutes) and 2026-09-06 at 13:00 (the whole of it, with a landing). Private: 2026-09-07 |

That makes nine flights, of which five are public: these four and the demo account's. Every one of them is the sample flight, flown on another day and at another time of day, some cut short; a flight that is cut short ends in the air and has no landing. The sample flight is the only flight file the project may ship, so every one of these takes off from the same place, the Wallberg at 47.6642° N, 11.7967° E. The three whole flights score the same 11.98 points, the two that are cut short score less.

Three more made-up pilots have flights that say something about [where pilots take off](#what-public-flights-say-about-a-site) (`dev::add_demo_takeoffs`): the sample flight again, moved to two places in the Lechtal Alps, in the box of the [made-up sites](#sites-that-ventis-partners-and-pilots-add) (`10.85,47.25,10.95,47.32`). One is the made-up take-off "Musterkogel West" (`venti:0000000000000000000000s001`, 47.3° N, 10.9° E); the other, 4 km south-east of it at 47.2612° N, 10.9312° E, is a place where no site stands. Each of them signs in with the demo account's password too, and none has a token that always works or a live track. Every flight of theirs lands.

| Handle | Display name | Flights |
|---|---|---|
| `lena` | Lena Probe | Public: from the Musterkogel on 2026-05-09 (the whole sample flight, 12.01 points) and on 2026-07-11 (the first 25 minutes); from the place without a site on 2026-06-06 (the first 10 minutes) |
| `timo` | Timo Entwurf | Public: from the Musterkogel on 2025-07-19 (the whole of it); from the place without a site on 2026-06-07 (the first 700 seconds) |
| `ines` | Ines Vorlage | Public: from the place without a site on 2026-04-25 (the first 800 seconds). Private: from there on 2026-06-13 |

Each of these starts at 10:30 UTC. So the mode has sixteen flights, of which eleven are public, by six pilots. A `bbox` around the Tegernsee holds the five from the Wallberg, one around the made-up sites the six from the Lechtal Alps. The ranking is, by the sum of each pilot's best three flights: `lena` (28.19 points, three flights), `mara` (23.42), `jonas` (21.0), `timo` (17.83), `demo` (11.98, one flight) and `ines` (6.32, one flight). The Musterkogel has three public flights of two pilots, and the place without a site is a [candidate](#get-v1sitescandidatesbboxwestsoutheastnorth): three flights of three pilots, Ines's private one not counted.

The made-up pilots know each other ([pilots among each other](#pilots-among-each-other)): `demo` follows `mara`, `jonas` and `lena`, and those three follow `demo`; `mara` follows `jonas`. The demo account's public flight has two cheers (`mara`, `lena`) and two comments, `0000000000000000000000c001` by `mara` and `0000000000000000000000c002` by `demo`; the newest public flight of `mara` has two cheers and the comment `0000000000000000000000c003`, the newest of `jonas` one cheer. The demo account was told of four of these. None of it adds a flight or changes one.

```bash
curl http://127.0.0.1:8080/v1/flights/public                          # the eleven public flights, newest first
curl http://127.0.0.1:8080/v1/ranking                                 # their six pilots, by the sum of their best three flights
curl 'http://127.0.0.1:8080/v1/flights/public?bbox=11.6,47.55,12,47.85&sort=score'
curl http://127.0.0.1:8080/v1/pilots/mara
curl 'http://127.0.0.1:8080/v1/pilots?q=le'                           # Lena Probe, by a part of her name
curl http://127.0.0.1:8080/v1/pilots/lena/statistics                  # what her three public flights add up to
curl 'http://127.0.0.1:8080/v1/ranking?month=2026-08'                 # the ranking of one month
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

The demo account and the two further pilots are in a [group](#groups) that the demo account keeps, and the two further pilots have a second one that the demo account is not in: to the demo account it is no group, until the account comes into it with its code. The names are made up, like the pilots': a "Mustergruppe" is a sample group as "Max Mustermann" is a sample man.

| Identifier | Name | Invite code | Keeper and members |
|---|---|---|---|
| `44444444444444444444444444` | Mustergruppe | `4444-4444-4444-4444` | Kept by `demo`. Members in the order in which they came in: `demo`, `mara`, `jonas` |
| `55555555555555555555555555` | Beispielgruppe | `5555-5555-5555-5555` | Kept by `mara`. Members: `mara`, `jonas` |

Their [plans](#plans-for-a-flying-day) are for the day this mode's clock shows, 18 November 2026, and the two days after. The places are made up as well, a sample mountain and an example meadow, and none is taken from the flying sites this mode serves. The one position is where the sample flight takes off.

| Identifier | Group | Day | What it says | Posted by | Who is in |
|---|---|---|---|---|---|
| `66666666666666666666666666` | Mustergruppe | 2026-11-18 | "Musterberg, Startplatz Ost", a position, 10:30 and a note of two lines | `demo` | `demo`, `mara`, `jonas` |
| `77777777777777777777777777` | Mustergruppe | 2026-11-19 | "Beispielwiese", 09:00 and a note; no position | `mara` | `mara`, `jonas` |
| `88888888888888888888888888` | Mustergruppe | 2026-11-20 | "Musterberg" and nothing else | `jonas` | `jonas`, `demo` |
| `99999999999999999999999999` | Beispielgruppe | 2026-11-19 | "Beispielhang, Nordseite" and 13:00 | `jonas` | `jonas`, `mara` |

```bash
token='Authorization: Bearer venti_0000000000000000000000000000000000000000000000000000'
curl http://127.0.0.1:8080/v1/groups -H "$token"                                      # the one group the demo account is in
curl http://127.0.0.1:8080/v1/groups/44444444444444444444444444/plans -H "$token"     # its three plans
curl 'http://127.0.0.1:8080/v1/groups/plans?from=2026-11-18&to=2026-11-18' -H "$token" # the board of this mode's today
curl http://127.0.0.1:8080/v1/groups/55555555555555555555555555 -H "$token"           # 404: not the demo account's to see
curl -X POST http://127.0.0.1:8080/v1/groups/join -H "$token" \
  -H 'Content-Type: application/json' -d '{"code":"5555-5555-5555-5555"}'             # now it is
```

An account that a test creates comes into either group with its code. This mode counts nothing an account does with groups, and takes a plan for any day from 17 November 2026 on, however far ahead: its clock stands still while an app under test may run in a later year.

None of this is a secret, and none of it exists on a real server. Anyone may create further accounts in this mode, without an invite; the limits on attempts are off, and uploads work. [Reports](#reports) are taken and kept, and nobody reads them: no flight of this mode is taken down and no account's publishing is suspended, because the commands that do that work on a real server's database. Whatever a client changes is forgotten when the program stops. The addresses of the flights of all six pilots, the identifiers of the demo live tracks and those of the two groups and their plans, with the groups' invite codes, are the same on every start.

Because its clock stands still, this mode takes live fixes of **any time**, where a real server refuses those from the future and those more than 12 hours old, and it never treats a track as too old to read or to add to. An app under test can therefore send the fixes of a replayed flight of last summer, or of its own clock. Two things stay as on a real server: an answer says when a track would be removed (`expires_at`), and the list of public tracks holds a track only while its newest fix is no older than half an hour by this mode's clock. A track sent with the times of a replayed flight is thus read by its identifier and is not on the list.

## Rules that hold for every endpoint

**Versions.** Every path starts with `/v1`. Within a version the format only grows: an answer may gain fields, so a client ignores fields it does not know. Removing or renaming a field, or changing what one means, needs a `/v2` beside `/v1`.

**Units.** Times are UTC in RFC 3339 (`2026-11-18T18:20:00Z`). Positions are WGS84 degrees. Heights are metres above mean sea level, except the limits of an airspace, which are given as published; see [airspace](#get-v1airspacebboxwestsoutheastnorth). Wind speed is metres per second, and wind direction is the direction the wind comes from, in degrees clockwise from true north. Temperature is degrees Celsius, pressure is hectopascal at the station's own height, humidity is percent.

**Missing values.** A value that is not known is left out of the answer: the field is not there. It is never a placeholder number, and a field is never `null`. A station without a temperature sensor has readings without `temperature_c`. `null` stands in one kind of place only: inside a list that has one entry for every cell of a grid, which are the lists of [the thermal map](#get-v1thermals) and of [its hours](#get-v1thermalshoursdatelayerheight) (`ground_m`, `verdict`, `why`, `thermals`, `climb_ms`, `top_m`, and the hours' wind, cloud and rain). Such a list is as long as the grid and an entry's place says which cell it is, so a cell that has no value cannot be left out and holds `null`: "nothing here". A client reads these lists entry by entry and expects it.

**Sources and credit.** Every station, every airspace, every flying site and every forecast series comes from exactly one data source and names it. A place where two sources measure, such as an airfield by its weather report and by the weather service's own station, is two stations with two identifiers, and nothing of one is copied into the other: data of two sources is never merged into one record. The list of an area holds one of the two; see [the stations of a box](#get-v1stationsbboxwestsoutheastnorth). Every answer that carries data has an `attributions` list with one entry for each source the answer draws on, and only those:

| Field | Meaning |
|---|---|
| `source` | The source's identifier, as records name it |
| `provider` | Who publishes the data |
| `licence` | The licence's name |
| `text` | The credit line, worded as the provider asks |
| `url` | Where the credit line links to |
| `notice` | What the provider asks to be said wherever its data is shown, beside the credit. Left out when it asks for nothing of the kind |

A client that shows the data shows the credit of its source next to it, and the notice if there is one. The licences require that; it is not decoration.

**Caching.** Answers with station data carry `Cache-Control: public, max-age=60` and an `ETag`. A client that asks again with `If-None-Match` gets `304 Not Modified` without a body when nothing changed. Observations arrive every ten minutes, so most repeated requests end that way. Airspace and the flying sites of sources change once a day at most and carry `max-age=3600`; an answer of flying sites that was asked for a layer of Venti's own, as every answer without `layers` is, carries `max-age=60` ([there](#get-v1sitesbboxwestsoutheastnorth)). A forecast carries `max-age=60` like the stations; the server itself keeps one for up to half an hour. The list of [dumps](#dumps-of-ventis-own-sites) and their files carry `public, no-cache`: a copy is checked before every reuse. The list of map regions carries `max-age=60`; a file of a region never changes under its path and may be kept for good, see [Maps](#maps). A tile of map imagery, and the answer that there is none, carry `max-age=604800`, a week; see [Map imagery](#map-imagery). Health answers and errors carry `Cache-Control: no-store`. So does every answer to a request that carried a session token: it was made for one account, and nothing may keep it. What applies to shared flights is said [there](#flights), and what applies to live tracks, which are asked for every few seconds, [there](#live-tracks).

**Compression.** Answers are gzip-compressed when the request says `Accept-Encoding: gzip`. The station list of the whole first region is about 106 kB plain and 15 kB compressed, its airspace about 1,240 kB and 290 kB, its flying sites 110 kB and 17 kB. The forecast of one place is 85 kB and 9 kB. A live track of twenty minutes at a fix a second is 204 kB and 28 kB. The one answer that is never compressed is a file of a map region: [Maps](#maps) says why.

**Bodies.** A request that carries JSON says so: `Content-Type: application/json`, with or without a character set. Anything else, and no type at all, is answered `415` with `unsupported_media_type` before the body is looked at. The one request whose body is not JSON is the upload of a flight file, which takes any type. A JSON body may be 16 kB, and one that carries the fixes of a live track 32 kB; a larger one is answered `413` and not read. The rule is what keeps a page on another site from sending sign-in or registration attempts through a visitor's browser: a browser sends a body typed `text/plain` anywhere without asking, and asks the server first before it sends one typed `application/json`.

**Browsers.** A page on any site may read what anyone may read. A `GET` or `HEAD` under `/v1` that carries no `Authorization` header is answered with `Access-Control-Allow-Origin: *`, whatever the page's origin, and the page may read `ETag`, `Retry-After`, `Accept-Ranges`, `Content-Range`, `RateLimit-Policy` and `RateLimit` of the answer. It may send `If-None-Match`, `Range` and `If-Range` with such a read; the browser's question beforehand (the preflight) is answered for `GET` and `HEAD` with these three headers and nothing else. Everything beyond that is for the origins the server's configuration lists (`cors_origins`), which is where Venti's own web app is served from: a request with a token, a request with a body, every method that writes. To a page of any other origin such a request is not allowed, and the answer to one carries nothing that lets the page read it. Apps that are not browsers are not affected by any of this. The API never tells a browser to send cookies or other credentials of its own along (`Access-Control-Allow-Credentials` is never set), and it has no use for them: an account is acted for with a token that the page's own code puts into the `Authorization` header, and in no other way. The server sets no cookie and reads none. A page on another site therefore cannot make a visitor's browser act for the visitor: it does not have the visitor's token, and an open read is answered with what everybody gets. The pages for people ([a shared flight's](#the-page-a-shared-flight-has), [a live track's](#the-page-a-live-track-has)) and [the connector](mcp.md) are not under `/v1`, and nothing of this applies to them.

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
| 400 | `box_too_large` | The box holds more airspaces, or more flying sites, than one answer carries, or is wider than [candidates](#get-v1sitescandidatesbboxwestsoutheastnorth) are asked for in. Ask for smaller boxes |
| 400 | `invalid_position` | `lat` or `lon` is missing, is not a number, or the two are not a place on the globe |
| 400 | `invalid_station_id` | The identifier in the path is not of the form `source:local` |
| 400 | `invalid_time` | `from`, `to` or `after` is not an RFC 3339 time |
| 400 | `invalid_range` | `from` lies after `to`, or the window of observations is longer than seven days. For the list of public flights: `from` is not earlier than `to` |
| 400 | `invalid_day` | For a list of plans: `from` or `to` is not a date written `YYYY-MM-DD`. For the hours of the thermal map: `date` is missing or is not one |
| 400 | `invalid_sort` | `sort` is not an order the list of public flights has: `newest` or `score` |
| 400 | `invalid_request` | The request could not be read at all: a path that is not valid UTF-8, a body that is not the JSON the endpoint expects. For a body, `message` says where it stops making sense and never repeats what stands there. For the hours of the thermal map: `layer` or `height` is none the map has, and `message` lists those it has |
| 400 | `invalid_handle`, `invalid_display_name`, `invalid_password` | A new account's handle, name or password is not as [described](#accounts-and-sessions) |
| 400 | `invalid_visibility` | `visibility` is not `private`, `unlisted` or `public`; for a live track, not `link` or `public` |
| 400 | `invalid_cursor`, `invalid_limit` | `after` or `since` is not a value the server gave out; `limit` is not a number from 1 to 100, or from 1 to 5000 for the fixes of a live track |
| 400 | `invalid_live_track_id` | The identifier sent for a live track is not 26 characters of its alphabet |
| 400 | `invalid_fixes` | The fixes sent for a live track cannot be used as a whole: none, more than 120, not oldest first, or a value no glider has. `message` names the place of the fix in the request and never one of its values |
| 400 | `invalid_alert` | What a request says about an [alarm](#alarms) cannot be kept. `message` names the field |
| 400 | `invalid_push_subscription` | A [browser's subscription](#web-push) cannot be kept: an endpoint that is not of a push service the server sends to, or keys that are not keys. `message` names the field |
| 400 | `invalid_kept_site` | What a request says about a [site to keep](#the-sites-a-pilot-keeps) cannot be kept: no site's record, another identifier than the path's, or a credit that is another source's or too long. `message` names the field |
| 400 | `invalid_site`, `site_out_of_area` | What a request says about a [site](#sites-that-ventis-partners-and-pilots-add) cannot be kept; the place is in no region this server serves |
| 400 | `invalid_report` | A [report](#reports) gives a reason the server does not know, has a text of more than 1,000 characters, or gives the reason `other` and no text |
| 400 | `invalid_token_name`, `invalid_scopes`, `invalid_expiry` | Making a [token for a program](#post-v1accounttokens): its name, its scopes or its end cannot be used |
| 400 | `invalid_group_name` | The name of a [group](#groups) is empty or has more than 60 characters |
| 400 | `invalid_plan` | A [plan](#plans-for-a-flying-day) is for a day the server takes no plan for, its place is empty or has more than 80 characters, or its note has more than 500 |
| 401 | `unauthorized` | The request needs an account and carries no valid token: no session's, and none that a pilot [gave to a program](#tokens-for-other-programs). The answer has `WWW-Authenticate: Bearer` |
| 403 | `insufficient_scope`, `session_required` | The request carries a good [token for a program](#tokens-for-other-programs) that lacks the scope the request needs, which `scope` in the error names; or it asks for what only the pilot's own sign-in may do |
| 401 | `invalid_credentials` | Signing in: the handle or the password is wrong |
| 403 | `wrong_password` | The password sent to confirm a change to the account is wrong |
| 403 | `registration_closed`, `invalid_invite` | The server takes no new accounts, or takes them by invite and the code is not good |
| 403 | `account_full` | An upload: the account keeps as many flights, or as many megabytes of flight files, as the server gives one account room for |
| 403 | `live_track_full` | The live track holds as many fixes as one track may. Begin a new track with a new identifier |
| 403 | `flight_taken_down` | The pilot asked to share a flight that whoever runs the server [took down](#what-whoever-runs-the-server-can-do). It stays private until they release it |
| 403 | `publishing_suspended` | The pilot asked to make a flight public, and whoever runs the server has [suspended the account's publishing](#what-whoever-runs-the-server-can-do). Sharing by link works |
| 403 | `invalid_group_invite` | The invite code lets nobody into a [group](#groups): mistyped, replaced, ended, or never one. Which of these is not said |
| 403 | `group_full`, `too_many_groups`, `group_plans_full` | The group has as many members as a group may have; the account is in as many groups as an account may be in; the group holds as many plans as a group may hold at a time |
| 403 | `not_group_keeper`, `not_plan_author` | A member of a group asked for what only its keeper may do; a member asked to change a plan they did not post, or to delete one without being its author or the keeper |
| 403 | `sites_closed` | The server takes no [sites and no confirmations from pilots](#sites-that-ventis-partners-and-pilots-add); or a site of a source was to be changed, which is corrected at its source |
| 403 | `sites_for_partners` | The site is of the partners' layer, and the account is not one whoever runs the server named as a partner. A pilot who finds the site wrong reports it |
| 400 | `invalid_layers`, `invalid_confirmation` | `layers` or `layer` names a layer of sites the server does not have; a [confirmation](#put-v1sitesidconfirmation) cannot be kept |
| 404 | `station_not_found` | No station has this identifier |
| 404 | `alert_not_found` | The account has no [alarm](#alarms) with this identifier |
| 404 | `flight_not_found` | No flight has this address, or it is not the caller's to see. The two cannot be told apart |
| 404 | `session_not_found`, `token_not_found`, `pilot_not_found` | The account has no such session; it has no such [token for a program](#tokens-for-other-programs); no pilot has this handle |
| 404 | `authorization_request_not_found` | No [request for a pilot's agreement](#agreeing-in-the-web-app) has this identifier, its end has come, or its program is gone. Which of these is not said. The pilot starts again in the program that asked |
| 410 | `authorization_request_answered` | The [request for a pilot's agreement](#agreeing-in-the-web-app) was answered already. A request is answered once, and nothing of it is read again |
| 404 | `live_track_not_found` | No live track has this identifier: none ever had, its pilot deleted it, or it ended long enough ago to have been removed. The three cannot be told apart |
| 404 | `group_not_found` | No [group](#groups) has this identifier, or the caller is not in it. The two cannot be told apart, on any endpoint about a group |
| 404 | `site_not_found`, `site_revision_not_found` | No [site](#sites-that-ventis-partners-and-pilots-add) has this identifier; the site has no revision with this number |
| 404 | `group_member_not_found`, `plan_not_found` | Said to members of a group only: no member of it has this handle; it has no plan with this identifier, or none any more |
| 404 | `map_not_found` | The server hands out no [map file](#get-and-head-v1mapsregionversionname) under this path: no region with this identifier, not in this version, or no file of this name. The three cannot be told apart |
| 404 | `outside_region` | A forecast was asked for a place outside the area the server has forecasts for |
| 404 | `not_found` | The path is not part of the API. For the hours of the thermal map: the map has no such day, and `message` lists the days it has |
| 405 | `method_not_allowed` | The path exists, but not with this method |
| 408 | `timeout` | An upload: the file stopped arriving, or took too long to arrive |
| 409 | `site_too_close`, `site_changed` | A [site](#sites-that-ventis-partners-and-pilots-add) would stand less than 20 m from another, which the error names in `error.site`; the site was changed since the revision the request names |
| 409 | `handle_taken` | The handle belongs to another account or is not available |
| 501 | `google_not_configured` | The server does not offer [signing in with Google](#sign-in-with-google) |
| 401 | `invalid_google_token`, `invalid_signup` | A Google ID token that is not good for this server; a sign-up of a first sign-in with Google that has expired. Sign in with Google again |
| 503 | `google_unavailable` | The server could not fetch Google's keys to check a Google ID token |
| 409 | `google_taken`, `google_only_sign_in` | Another account is linked to this Google account; an account made with Google cannot be unlinked from it |
| 404 | `google_not_linked` | The account is not linked to a Google account |
| 409 | `too_many_kept_sites` | The account keeps as many [sites as its own](#the-sites-a-pilot-keeps) as it may |
| 409 | `too_many_alerts` | The account has as many [alarms](#alarms) as it may |
| 400 | `invalid_follow` | A pilot does not [follow](#pilots-among-each-other) themselves |
| 409 | `too_many_follows` | The account follows as many pilots as it may: 1,000 |
| 400 | `invalid_cheer` | A pilot does not cheer their own flight |
| 400 | `invalid_comment` | A comment that says nothing, or has more than 500 characters |
| 403 | `comments_off` | The flight's pilot takes no comments on the flight |
| 404 | `comment_not_found` | The flight has no comment with this identifier, or none that is the caller's to remove |
| 409 | `too_many_push_subscriptions` | The account has as many [browsers subscribed to Web Push](#web-push) as it may |
| 409 | `too_many_tokens` | The account holds as many [tokens for programs](#tokens-for-other-programs) as it may |
| 409 | `live_track_taken` | Fixes were sent under the identifier of another account's live track |
| 410 | `site_gone` | The [site](#sites-that-ventis-partners-and-pilots-add) was removed, or taken down. Its identifier is not used again |
| 410 | `live_track_gone` | Fixes were sent under an identifier whose live track was deleted or removed, or that was taken out of use. An identifier is used once: begin a new track with a new one |
| 413 | `payload_too_large` | The request carried a body of more than 16 kB, a batch of live fixes of more than 32 kB, or a flight file of more than 4 MB |
| 415 | `unsupported_media_type` | A request whose body is JSON did not say so with `Content-Type: application/json` |
| 416 | `range_not_satisfiable` | The bytes asked of a [map file](#get-and-head-v1mapsregionversionname) with `Range` begin at or after its end. `Content-Range` says how long the file is |
| 422 | `invalid_flight` | The uploaded file is not a flight file the server can use |
| 422 | `fixes_out_of_time` | None of the live fixes sent has a time the server takes: all lie in the future, or are too old. The device's clock is probably wrong; `message` says what the server's shows |
| 429 | `rate_limited` | Too many attempts. `Retry-After` says when to try again |
| 501 | `no_forecasts` | The server has no forecast service. Asking again does not help |
| 501 | `no_radar` | The server collects no rain radar: none of its regions names a radar source. Asking again does not help |
| 503 | `unavailable` | The server cannot reach its database, is too busy for this request, or has no forecast for a place at hand and cannot fetch one now. `Retry-After` says when to try again |
| 503 | `timeout` | The server could not answer within ten seconds; for [an account's archive](#get-v1accountarchive), within 120 |
| 502 | `unavailable` | A [tile of map imagery](#get-v1imagerysourcezxy): its provider failed or sent what is not a tile. May be kept for a minute |
| 504 | `timeout` | A [tile of map imagery](#get-v1imagerysourcezxy): its provider did not answer within eight seconds. May be kept for a minute |
| 500 | `internal` | The server failed in a way it did not expect; the cause is in its log |

A client must expect codes it does not know.

## Limits

What a client may ask is bounded, and an answer says what is left of the bound it was counted against.

**A token for a program** makes at most **3,600 requests an hour**, of any kind, whatever their answers: a budget of its own, which no other token of the account and no session shares. The hour begins with its first request and ends an hour later. Every answer to a request with such a token, a refusal too, carries two fields:

```
RateLimit-Policy: "token";q=3600;w=3600
RateLimit: "token";a=3599;w=3540
```

`RateLimit-Policy` names the budget (`"token"`), its quota `q` and its window `w` in seconds. `RateLimit` says what is available of it, `a`, and in how many seconds the window closes, `w`. When `a` reached 0 the next request is answered `429` with `rate_limited` and `Retry-After`, which names the same moment as `w`; nothing of the request was done. A token that is not good, withdrawn, ended or never given out, is answered `401` on every path, also where a request without a token would be answered: send no token where none is needed.

**A read without a token** is bounded by the address that asks where it is bounded at all: today the [reads of public flights as data](#how-often-an-address-may-read), 600 an hour, which say the same in the same two fields with the name `"address"`:

```
RateLimit-Policy: "address";q=600;w=3600
RateLimit: "address";a=599;w=3600
```

The same reads with a token for a program are counted against the token's budget and not the address's, so that a program of a club with many pilots behind one address is not held up by the address. With a session's token they are counted for the address, as without one.

**A session of Venti's own apps** spends no such budget, and its answers carry neither field. What every account may write is counted per account, whatever it writes with: [sites](#sites-that-ventis-partners-and-pilots-add), [flights](#post-v1flights), [reports](#reports), [groups](#groups), [an archive](#get-v1accountarchive).

The two fields are those of the IETF's draft "RateLimit header fields for HTTP", [draft-ietf-httpapi-ratelimit-headers-12](https://datatracker.ietf.org/doc/draft-ietf-httpapi-ratelimit-headers/) as it stood on 2026-11-21. It is a draft and not a standard: earlier versions named the fields `RateLimit-Limit`, `RateLimit-Remaining` and `RateLimit-Reset`, and a later one may rename them again, which this API would follow. A client reads `Retry-After` first. The numbers are estimates, counted in the server's memory as its other limits are; [the development mode](#try-it) has none of them.

## `GET /v1/health`

How the server and each of its data sources are doing.

```json
{
  "status": "ok",
  "version": "0.1.0",
  "time": "2026-11-18T19:05:00Z",
  "forecasts": "available",
  "registration": "open",
  "google_sign_in": true,
  "sites_from_pilots": "open",
  "thermal_map": {
    "source": "open-meteo",
    "computed_at": "2026-11-21T09:30:00Z",
    "models": [
      {
        "model": "icon-d2",
        "run": "2026-11-21T06:00:00Z"
      },
      {
        "model": "icon-eu",
        "run": "2026-11-21T06:00:00Z"
      }
    ]
  },
  "radar": [
    {
      "source": "dwd-radar-ry",
      "frames": 2,
      "newest_frame": "2026-11-21T13:15:00Z",
      "newest_frame_age_s": 240,
      "bytes": 9267
    },
    {
      "source": "geosphere-inca-rr",
      "frames": 2,
      "newest_frame": "2026-11-21T13:00:00Z",
      "newest_frame_age_s": 1140,
      "bytes": 10408
    }
  ],
  "sources": [
    {
      "source": "awc-metar",
      "provider": "Aviation Weather Center",
      "cadence_seconds": 600,
      "last_success": "2026-11-18T19:05:00Z",
      "newest_observation": "2026-11-18T18:50:00Z",
      "stale": false,
      "stations": 3
    },
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
      "source": "dwd-radar-ry",
      "provider": "Deutscher Wetterdienst",
      "cadence_seconds": 300,
      "last_success": "2026-11-18T19:05:00Z",
      "stale": false,
      "stations": 0
    },
    {
      "source": "geosphere-inca-rr",
      "provider": "GeoSphere Austria",
      "cadence_seconds": 900,
      "last_success": "2026-11-18T19:05:00Z",
      "stale": false,
      "stations": 0
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
          "run": "2026-11-21T06:00:00Z",
          "available_at": "2026-11-21T07:09:12Z",
          "checked_at": "2026-11-18T19:05:00Z"
        },
        {
          "model": "icon-eu",
          "run": "2026-11-21T06:00:00Z",
          "available_at": "2026-11-21T09:14:18Z",
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
      "source": "osm-cables",
      "provider": "OpenStreetMap contributors",
      "cadence_seconds": 604800,
      "last_success": "2026-11-18T19:05:00Z",
      "stale": false,
      "stations": 0
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
    },
    {
      "source": "suedtirol-meteo",
      "provider": "Autonome Provinz Bozen - Südtirol",
      "cadence_seconds": 600,
      "last_success": "2026-11-18T19:05:00Z",
      "newest_observation": "2026-11-18T18:50:00Z",
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
| `client_addresses` | For whoever runs the server; an app ignores it. `seen` when requests arrive from addresses of their own. `hidden` when, in the last hour, twenty or more requests and at least a tenth of all came from one private address and from no other private one: a proxy or a container network is hiding the visitors, and the server counts them as one ([deploy/README.md](../deploy/README.md#the-visitors-address)). Left out while fewer than twenty requests have arrived |
| `forecasts` | Whether the server answers point forecasts at all: `available`, or `not_configured` for a server that was not told where its forecast service is. Such a server answers `/v1/forecast` with `501` and asks nobody else. Left out by a server older than forecasts, which has none |
| `registration` | Who may create an account here: `open` (anyone), `invite` (whoever has an invite code, which `POST /v1/accounts` then needs) or `closed` (nobody; existing accounts sign in as before). An app shows the field for the invite code, or says that the server takes no new accounts, by this. Left out by a server older than this field, which may ask for a code: an app that reads nothing here offers the field. The development mode says `open` |
| `sites_from_pilots` | Whether the server takes [sites from pilots](#sites-that-ventis-partners-and-pilots-add): `open` (a signed-in pilot can add a site to the crowdsourced layer `venti-pilots`, correct one of it and confirm a site, inside the regions the server serves) or `closed` (every such write is `403` with `sites_closed`). An app offers "add a site", "correct this site" and "I took off here" to a pilot only where this says `open`, so that nobody fills in a form to be refused. It says nothing of the partners' layer, which an account named as a partner writes to whatever this says: [`GET /v1/account`](#get-v1account) tells such an account. Reading the sites of either layer is never off, and an account whose publishing is suspended is refused on an open server too: that is the account's, and is not said here. Left out by a server older than this field (before 2026-11-21), which takes none or does not say: an app that reads nothing here offers neither. The development mode says `open` |
| `thermal_map` | For whoever runs the server; an app asks for the map itself ([`GET /v1/thermals`](#get-v1thermals)). Where the server's thermal map stands. Left out by a server that makes none, which is one without forecasts, and by an older server |
| `thermal_map.source` | The forecast source the map is made from |
| `thermal_map.computed_at` | When the server worked out the map it serves. Left out while it has none: after a first start, until the finest model's part was fetched whole, which is some minutes on a warm forecast service and a look or two longer on one that has just started. `/v1/thermals` answers `503` until then |
| `thermal_map.models[]` | The models the map is made of, the finest first, each with the `run` its part of the map was last made from, every cell of it. `run` is left out until that was done once; the map then has no day of that model. The run the forecast service holds is the model's `run` under `sources[].models[]`: the map follows it some minutes after the server's look sees it. A map that stays behind for more than a few looks waits for something, and the server's log says for what. It does not make the server `degraded` |
| `radar[]` | For whoever runs the server; an app asks for the frames themselves ([`GET /v1/radar`](#get-v1radar)). One entry for each radar source the server collects. Left out by a server that collects none, and by an older server |
| `radar[].frames` | How many frames of the source the server holds: at most 25, two hours of a frame every five minutes |
| `radar[].newest_frame` | The time of the newest frame held: the end of the five minutes it is of. Left out while the server holds none |
| `radar[].newest_frame_age_s` | How old that frame is by the server's clock, in seconds. A source that publishes every five minutes is ordinarily one to six minutes behind; a quarter of an hour and more means the provider has stopped publishing, and the source's entry under `sources` turns `stale` with it. Left out while the server holds none |
| `radar[].bytes` | What the pictures of the held frames weigh in the server's memory. Nothing else is kept of them |
| `sources[].cadence_seconds` | How often the server asks the provider for new data |
| `sources[].last_success` | When a run last delivered its data without trouble. Left out if none ever did |
| `sources[].last_failure` | When a run last failed. For a source of observations: it delivered nothing, or more than a tenth of its requests failed. For a source of airspace or of flying sites: any request failed, because a set with a part missing must not replace a whole one. For a source of rain radar: any request failed, or the provider's newest frame was older than three of its intervals; a run that found nothing new is no failure. Left out if none ever did |
| `sources[].newest_observation` | The time of the newest observation the source has delivered. Left out for a source of anything else |
| `sources[].stale` | `true` when no run has succeeded for too long. For a source the server asks every few minutes: three cadence intervals. For a source it asks once a day or less often, which is airspace and flying sites: two and a quarter intervals, 54 hours for a daily one. A failed daily run is tried again a day later, so one missed day is tolerated and the second is not |
| `sources[].stations` | How many stations the last successful run delivered. `0` for a source of anything else |
| `sources[].airspaces` | How many airspaces the server holds from the source: the set of its last successful run, which is what it serves. Left out for a source that delivers no airspace |
| `sources[].file_date` | The date of the oldest file that set was read from. Left out for a source that delivers no airspace |
| `sources[].sites` | How many flying sites the server holds from the source: the set of its last successful run. Left out for a source that delivers no sites |
| `sources[].sites_as_of` | The date the source's own data is of. Left out for a source that delivers no sites, and before its first set |
| `sources[].models` | For a source of forecasts: the weather models it serves, each with the `run` it was at when the server last looked, when the source had that run ready (`available_at`) and when the server looked (`checked_at`). The server looks every ten minutes, by asking for one forecast. A run older than six hours means the forecast service has stopped fetching new ones. Left out for a source of anything else, and before the first look that worked. In the development mode the runs are dated after `checked_at`: its forecasts were recorded three days after its wind |

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
| `latest.pressure_sea_level_hpa` | The pressure reduced to sea level by the provider's own reduction, hPa. Only from a station whose provider sends it so (South Tyrol's); the [foehn reading](#get-v1foehn) compares it |
| `truncated` | `true` when the box holds more than 5000 stations. The answer then lists the first 5000 by source and identifier; ask for a smaller box |

Stations are sorted by source and then by identifier.

The server does not decide what is too old to show. A station that stopped reporting yesterday is listed with yesterday's reading and its time; the client greys it out or hides it.

**One station for one place.** Where two sources have a station at the same place, the list holds one of the two, since 2026-11-21. A client does nothing for it and sees it only in what is missing:

- *The same place* is two stations of different sources that stand no farther apart than 2.75 km and differ by no more than 50 m in height. A station without `elevation_m` is at the same place as no other, and two stations of one source never are.
- *Listed* is the one whose source the server's regions prefer: in the first region DWD, then GeoSphere Austria, then South Tyrol, then the airfields' reports. With one exception: a station that has no `wind_speed_ms` of the last hour does not stand for one that has, so while a weather service's station on an airfield is silent, the airfield's report is listed in its place.
- *The other is not in the list*, whatever the box: a station at the edge of a box is judged against its neighbour outside it. Nothing of it is copied into the station that is listed, and its source is in `attributions` only if another of its stations is listed. Only in an answer that is `truncated` may both of two show at the box's edge.
- *It is still there.* The server goes on collecting it, and its [observations](#get-v1stationsidobservationsfromto) answer by its identifier as before. An identifier an app has kept stays good. Which of two is listed can change from one answer to the next, when a station falls silent or is heard again; an app that shows a station by an identifier it kept should therefore not conclude from the list that the station is gone.

In the first region this left out the reports of ten airfields among 435 stations on 2026-11-21. The development mode lists all fourteen of its stations: no two of them are one place.

## `GET /v1/stations/{id}/observations?from=&to=`

What one station measured in a window of time, oldest first. Every station the server collects answers here, also one that the [list of a box](#get-v1stationsbboxwestsoutheastnorth) leaves out because another source's station stands for its place.

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

## `GET /v1/cables?bbox=west,south,east,north`

The cables that touch a box: power lines, minor power lines and ropeways, as lines from pylon to pylon, with their kind and, where the source maps one, their height above the ground. A phone asks for the box of the area it will fly in before the flight, keeps the answer, and the cockpit watches for them offline, as it does with airspace.

`bbox` is required and written as for stations. A cable is found by the smallest box around its line. The development mode serves the cables recorded from OpenStreetMap near the Tegernsee (47.66 to 47.74° N, 11.70 to 11.82° E), eight of them.

```bash
curl 'http://127.0.0.1:8080/v1/cables?bbox=11.70,47.66,11.82,47.74'
```

```json
{
  "layers": [
    {
      "source": "osm-cables",
      "fetched_at": "2026-11-18T19:05:00Z",
      "as_of": "2026-11-18T19:05:00Z",
      "covers": [
        { "south": 47.66, "west": 11.7, "north": 47.74, "east": 11.82 }
      ],
      "cables": [
        {
          "source": "osm-cables",
          "local": "way/37016046",
          "kind": "power-line",
          "line": [
            { "lat": 47.7767494, "lon": 11.6832523 },
            "…"
          ],
          "voltage_kv": 110.0,
          "name": "Bad Wiessee - Waakirchen - Bad Tölz;Bad Wiessee - Waakirchen",
          "operator": "E.ON Netz GmbH",
          "url": "https://www.openstreetmap.org/way/37016046"
        }
      ]
    }
  ],
  "attributions": [
    {
      "source": "osm-cables",
      "provider": "OpenStreetMap contributors",
      "licence": "ODbL 1.0",
      "text": "© OpenStreetMap contributors",
      "url": "https://www.openstreetmap.org/copyright",
      "notice": "This data is OpenStreetMap's and is available under the Open Database License, …"
    }
  ]
}
```

The whole answer is the golden file `crates/server/tests/golden/cables.json`; the line above is cut short here.

- `kind` is one of `power-line`, `minor-power-line`, `cable-car`, `gondola`, `chair-lift`, `drag-lift`, `material-ropeway`, `zip-line` and `other`.
- `height_m` is there only where the source maps the cable's height above the ground. Few cables in OpenStreetMap have one; none of the eight recorded has.
- `voltage_kv` is the highest voltage a power line names. `operator` is given for power lines and lifts for people only, which companies run.
- `local` is the source's own key and stays the same from one run to the next.

**No cable in an answer is no statement that there is none.** OpenStreetMap has many power lines and few of the material ropeways and low lines that are the danger; see [data-sources.md](data-sources.md#cables). The layer is never merged with another source's cables, nor with the sites of the same provider.

**Caching, size and failures** as for airspace: `Cache-Control: public, max-age=3600` with an `ETag`; a box that holds more than 5000 cables is refused with `400` and `box_too_large`, to be asked for in smaller boxes; a run that failed changes nothing.

## `GET /v1/sites?bbox=west,south,east,north`

The flying sites inside a box: take-offs, landings, training slopes and winch sites. They come in layers of two kinds that are never merged: a source's sites as the server last fetched them (today one source, OpenStreetMap), and, since 2026-11-21, the sites that Venti holds itself: the partners' layer `venti`, which is in the answer when it has a site in the box, and the crowdsourced layer `venti-pilots`, which is in the answer only when `layers` names it; see [Sites that pilots make](#sites-that-ventis-partners-and-pilots-add). What follows describes a source's layer.

`bbox` is required and written as for stations. Sites on the edge are included.

`layers` is optional: the names of the layers wanted, separated by commas, for example `layers=osm-free-flying,venti,venti-pilots`. Without it the answer holds every source's layer and `venti`, which is what a client from before the crowdsourced layer gets. With it the answer holds the layers named and no other; the word `default` in the list stands for the layers an answer holds without it, so `layers=default,venti-pilots` asks for everything without knowing a source's name. A name of no layer of this server is `400` with `invalid_layers`, and the message lists the names there are.

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
          "url": "https://www.openstreetmap.org/node/5964676196",
          "description": "Startrampe (vermutlich für Drachenflieger)"
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
          "url": "https://www.openstreetmap.org/node/5964676197",
          "description": "Startrampe (vermutlich für Drachenflieger)"
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
          "url": "https://www.openstreetmap.org/way/1080986173",
          "website": "https://www.dhv.de/",
          "fee": "yes",
          "outline": [
            [
              11.9055085,
              47.6789575
            ],
            [
              11.9055532,
              47.6793075
            ],
            [
              11.9048395,
              47.6794382
            ],
            [
              11.9046533,
              47.6788445
            ],
            [
              11.9055085,
              47.6789575
            ]
          ]
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
          "url": "https://www.openstreetmap.org/way/1080988126",
          "website": "https://www.dhv.de/",
          "outline": [
            [
              11.8880464,
              47.6919322
            ],
            [
              11.8875034,
              47.6925871
            ],
            [
              11.8879519,
              47.692816
            ],
            [
              11.8874492,
              47.6933534
            ],
            [
              11.8858179,
              47.6933335
            ],
            [
              11.8850491,
              47.6925639
            ],
            [
              11.8859455,
              47.6914638
            ],
            [
              11.8880464,
              47.6919322
            ]
          ]
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

The box is the valley of the apps' sample flight, the Tegernsee, with the Spitzingsee east of it. The first two records are the ramps on the Wallberg. The last two are areas, each with its outline: the take-off and the landing field of the Jägerkamp.

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
| `position` | Latitude and longitude. For a site that the source draws as an area, the middle of the box around the area: a point in or near it, which need not lie inside it |
| `elevation_m` | Height above mean sea level. Left out if the source gives none |
| `uses` | What the site is used for: any of `takeoff`, `landing`, `towing`, `training`, each once. **May be empty**: the place is tagged for free flying and the source does not say what it is. A client expects values it does not know |
| `wind_directions` | The wind directions the site works in, as spans that run clockwise: `[{"from_deg": 135.0, "to_deg": 225.0}]` is south-east to south-west, and a single direction has both ends equal, `{"from_deg": 270.0, "to_deg": 270.0}`. A direction is where the wind comes from. Left out if the source gives none, **which says nothing about the site**: most sites have none |
| `paragliding`, `hang_gliding` | Whether the source says the site suits that kind of glider. Left out if it does not say |
| `official` | Whether the source calls the site official: legal and looked after. Left out if it does not say |
| `discouraged` | `true` if the source marks the site as unsafe or one of its uses as discouraged. Left out otherwise, which says only that the source marks nothing of the kind |
| `url` | Where the source's record can be looked at, and corrected. Offer it to the pilot: a site that is wrong or missing is fixed there |
| `website` | A web page about the site that the source names: its operator's, or one that says more about it. Always an `http` or `https` address; anything else is not passed on. It is somebody else's page: offer it as a link and take nothing from it. Left out if the source names none |
| `description` | What the source says about the site in words, **as its author wrote it** and in the author's language, 1000 characters at most; it may hold line breaks (`\n`). Show it as a quotation of the source and read nothing out of it. Left out if there is none |
| `fee` | Whether a payment is asked for using the site, as the source writes it: `yes`, `no`, or other words of its author's, 100 characters at most. Left out if the source does not say |
| `guest_guidelines` | `true` if the source says that guests, pilots who do not know the site, have rules for guests to take notice of; `false` if it says they have none. Left out if it does not say. Where the rules are it does not say: the `website`, if there is one, is the place to look |
| `no_flight_time` | A time in which the site may not be used, as the source writes it, 100 characters at most: `01.04-30.09` is its way of writing the first of April to the thirtieth of September. **Show it as written and work nothing out from it**: neither the server nor a client says from this field that a site is open or closed today. Left out if the source names no such time, which does not say that there is none |
| `outline` | For a site that the source draws as an area, a landing field or a take-off meadow: one closed ring of points, each written `[longitude, latitude]` as GeoJSON and an airspace's rings write them, the last point the first again, 4 to 64 points. It is for drawing the field on a map. Where the source has more points the ring is drawn with fewer, and those left out lie within a few metres of the line that stands for them. Left out for a site drawn as a point |

Every text in a record is the source's, and the source is a map that anyone can edit. A client shows `name`, `description`, `fee` and `no_flight_time` as text and never as markup, and opens `website` only as a link the pilot taps.

**A site in the answer is no permission to fly there, and no site in the answer is not "no site".** OpenStreetMap holds what somebody entered. In the first region that is 252 take-offs and 142 landings, a fraction of the sites there are; well-known ones are missing, a fifth of the places do not say what they are used for, and fewer than half of the take-offs carry wind directions that can be read for certain. [data-sources.md](data-sources.md#flying-sites) has the count. `covers` says where the server's set is whole, which means "everything the source has", not "everything there is". Whether a pilot may fly at a site, and by which rules, is the business of whoever runs it.

**Caching.** An answer that holds sources' sites alone, which is one whose `layers` names sources and nothing else, is kept as airspace is: `Cache-Control: public, max-age=3600` and an `ETag`; the server asks its source once a day. Every other answer was asked for a layer of [Venti's own sites](#sites-that-ventis-partners-and-pilots-add) (a request without `layers` asks for the partners'), which changes whenever a partner or a pilot writes: it carries `max-age=60` and an `ETag`, whether it holds a site of such a layer yet or not (since 2026-11-21; an hour before). An app that has just written a site still reads past its own cache for that minute, or shows the site from the answer to its write.

**Size.** One answer carries the whole box; there are no pages. The first region whole is 502 sites, 205 of them with an outline: 168 kB plain and 37 kB with gzip (measured 2026-11-21; 110 kB and 17 kB before a record had outlines). A box that holds more than 5000 sites is refused with `400` and `box_too_large`, as for airspace.

**When a source fails.** A run that could not fetch everything changes nothing: the server keeps serving the set of its last whole run, with that run's `fetched_at`. `/v1/health` then shows the source's `last_failure`, and `stale` once two runs in a row have failed.

## Sites that Venti's partners and pilots add

The server keeps flying sites of its own, beside the sites of its sources and never merged with them, in two layers (the owner, 2026-11-21: take-offs "usually are … really official, and they don't change so much", so the server takes them from "trusted partners"; and beside that "a crowdsourced feature, which is less official … not maybe displayed by default … where someone can say, yes, I started here and it was good").

| Layer | Whose sites | Who writes | In `GET /v1/sites` |
|---|---|---|---|
| `venti` | The partners': official in manner | An account whoever runs the server named as a partner for flying sites, and a token of such an account with `sites:write` | Always, beside the sources' layers |
| `venti-pilots` | The crowdsourced ones: less official | Any signed-in pilot, where the server takes sites from pilots; a partner as well | Only when the request names it: `layers=…,venti-pilots` |

- **Which layer a site is of follows from who added it**, and never changes: a partner's `POST /v1/sites` adds to `venti`, every other pilot's to `venti-pilots`. The request does not choose. Moving a crowdsourced site into the partners' layer is not built.
- **An identifier that stays.** A site's identifier is its layer's name, a colon and 26 characters (`venti:…`, `venti-pilots:…`), made by the server and never used for another site. Its address is `/v1/sites/{id}` for good; a site that was removed answers `410` with `site_gone`.
- **Every change is a revision that is kept.** A revision holds the whole record as it stood from then on, who made it, when, through what it came in (`session`: a sign-in of the pilot's own; `token`: a [token the pilot gave to another program](#tokens-for-other-programs); `operator`: whoever runs the server), and a line saying why. Nothing is overwritten. A change is undone by a revision that restores an earlier one and names it; removing a site is a revision too.
- **The licence: CC0 1.0, for both layers.** What a partner and what a pilot gives as a site is given under [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/), by the owner's decision of 2026-11-21. Every answer that carries a site says so in `attributions`. The licence asks for no credit; the entry's `notice` is to be shown wherever a site of the layer is:
  - `venti`: provider "Venti partners", notice "Added by a partner of Venti and not checked by Venti. A site on the map is not permission to fly there."
  - `venti-pilots`: provider "Venti contributors", notice "Added by pilots: not official and not checked by anyone. A site on the map is not permission to fly there."
- **Who may write.** A partner adds to `venti` and corrects, removes and restores sites of both layers. Any other signed-in pilot adds to `venti-pilots` and corrects, removes and restores sites of that layer, and is refused at a partner's site with `403` `sites_for_partners`: a pilot who finds a partner's site wrong [reports it](#post-v1reports). Nobody whose publishing whoever runs the server has suspended writes anything (`403`, `publishing_suspended`). An account adds at most 20 sites a day and writes at most 100 revisions a day (`429`, `rate_limited`). [`GET /v1/account`](#get-v1account) says `"edits_sites": true` to an account that is a partner, and leaves it out for every other.
- **From pilots only where the server is told to take them.** A server takes sites for `venti-pilots`, and [confirmations](#put-v1sitesidconfirmation), from pilots when its configuration says `sites_from_pilots = true`; otherwise a pilot's write is `403` with `sites_closed`. [`GET /v1/health`](#get-v1health) says which in `sites_from_pilots`, before anything is sent. It is off unless set, because the terms under which a pilot gives a site, which say that it is given under CC0 1.0 and stays when the account goes, are not written yet. The development mode takes them. The partners' layer needs no setting: the trust is its gate, and a partner writes on a server whose setting is off. Reading is never off.
- **Less than 20 m apart.** A site of `venti-pilots` keeps that distance from every standing site of both layers, a site of `venti` from sites of `venti` only. Where a partner's site stands, a place from pilots is refused, when it is added and when a correction or a restored revision would put it there, with `409` `site_too_close`, and the error names the partner's site in `error.site`, to be opened in its place. The other way round nothing is refused: a partner's site is put where a place from pilots stands, and both stand, so that what pilots put on the map keeps no official site off it. Taking the place from pilots over into the partner's site is not built. The check is made only where a revision takes a place: one that moves the site, and one that brings a removed site back. A place from pilots that a partner's site was put beside is corrected where it stands as before.
- **Confirmations.** A signed-in pilot says of a site of either layer "I took off here and it was good", once, with the day; see [below](#put-v1sitesidconfirmation). A site's `standing` says how many pilots did and when last.
- **A pilot's name on a site.** A site and each revision name the account that made them by handle and display name, to everyone. When an account is deleted, its sites and revisions stay and lose that name; its confirmations go with it.
- **Not permission to fly.** A site of either layer is what a partner or a pilot entered, checked by nobody at Venti.

The types and the store still say `pilot_sites` and `PilotSite`, and the apps' core `SiteByPilots`: names from before there were two layers, which stand for a site of either.

The development mode starts with four made-up sites in the Lechtal Alps, in the box `10.85,47.25,10.95,47.32`; each record says that it is made up. Its demo account is a partner; `mara` and `jonas` are not.

| Identifier | What it is |
|---|---|
| `venti:0000000000000000000000s001` | "Musterkogel West", a take-off the demo account added and gave its wind directions. Revision 2. `mara` and `jonas` confirmed it |
| `venti-pilots:0000000000000000000000s002` | "Beispieljoch", a take-off `jonas` added; `mara` gave it a wrong height and the demo account restored revision 1. Revision 3. `mara` confirmed it |
| `venti:0000000000000000000000s003` | "Musterwiese", a landing the demo account added. Revision 1 |
| `venti-pilots:0000000000000000000000s004` | A take-off `jonas` added twice by mistake and removed again. Its address answers `410` |

### The layer in `GET /v1/sites`

```bash
curl 'http://127.0.0.1:8080/v1/sites?bbox=10.85,47.25,10.95,47.32'
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
      "sites": []
    },
    {
      "source": "venti",
      "fetched_at": "2026-11-18T19:05:00Z",
      "as_of": "2026-11-18T19:05:00Z",
      "covers": [],
      "sites": [
        {
          "id": "venti:0000000000000000000000s001",
          "name": "Musterkogel West",
          "position": {
            "lat": 47.3,
            "lon": 10.9
          },
          "elevation_m": 1862.0,
          "uses": [
            "takeoff"
          ],
          "wind_directions": [
            {
              "from_deg": 225.0,
              "to_deg": 315.0
            }
          ],
          "paragliding": true,
          "description": "A made-up site of the development mode. No such place exists."
        },
        {
          "id": "venti:0000000000000000000000s003",
          "name": "Musterwiese",
          "position": {
            "lat": 47.28,
            "lon": 10.89
          },
          "elevation_m": 1010.0,
          "uses": [
            "landing"
          ],
          "paragliding": true,
          "description": "A made-up site of the development mode. No such place exists."
        }
      ],
      "standing": [
        {
          "id": "venti:0000000000000000000000s001",
          "revision": 2,
          "created_at": "2026-10-29T19:05:00Z",
          "created_by": {
            "handle": "demo",
            "display_name": "Demo Pilot"
          },
          "changed_at": "2026-10-31T19:05:00Z",
          "changed_by": {
            "handle": "demo",
            "display_name": "Demo Pilot"
          },
          "confirmed": {
            "pilots": 2,
            "latest_day": "2026-11-16"
          }
        },
        {
          "id": "venti:0000000000000000000000s003",
          "revision": 1,
          "created_at": "2026-11-14T19:05:00Z",
          "created_by": {
            "handle": "demo",
            "display_name": "Demo Pilot"
          },
          "changed_at": "2026-11-14T19:05:00Z",
          "changed_by": {
            "handle": "demo",
            "display_name": "Demo Pilot"
          }
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
    },
    {
      "source": "venti",
      "provider": "Venti partners",
      "licence": "CC0 1.0",
      "text": "Venti partners",
      "url": "https://creativecommons.org/publicdomain/zero/1.0/",
      "notice": "Added by a partner of Venti and not checked by Venti. A site on the map is not permission to fly there."
    }
  ]
}
```

The layer `venti` is in the answer when pilots made a site that stands inside the box, and absent otherwise. Its `sites` are the same canonical records as every layer's, so a client that knows nothing of this layer draws them as it draws any other. Beside them, `standing` has one entry per site in the same order: the `revision` the site stands at, `created_at` and `created_by`, `changed_at` and `changed_by` (each a pilot's `handle` and `display_name`, left out where the account is gone or the revision was the operator's). `fetched_at` and `as_of` are the moment of the answer: the layer is read as it is. `covers` is empty: the layer holds what pilots added and is whole nowhere. A record of this layer never has `official`, `discouraged` or `url`: those are not one pilot's to say, and the record's address is `/v1/sites/{id}`. The sites of both kinds of layer count towards the 5000 of one answer.

**The crowdsourced layer** is asked for by name; here alone:

```bash
curl 'http://127.0.0.1:8080/v1/sites?bbox=10.85,47.25,10.95,47.32&layers=venti-pilots'
```

```json
{
  "layers": [
    {
      "source": "venti-pilots",
      "fetched_at": "2026-11-18T19:05:00Z",
      "as_of": "2026-11-18T19:05:00Z",
      "covers": [],
      "sites": [
        {
          "id": "venti-pilots:0000000000000000000000s002",
          "name": "Beispieljoch",
          "position": {
            "lat": 47.29,
            "lon": 10.91
          },
          "elevation_m": 2104.0,
          "uses": [
            "takeoff"
          ],
          "paragliding": true,
          "description": "A made-up site of the development mode. No such place exists."
        }
      ],
      "standing": [
        {
          "id": "venti-pilots:0000000000000000000000s002",
          "revision": 3,
          "created_at": "2026-11-06T19:05:00Z",
          "created_by": {
            "handle": "jonas",
            "display_name": "Jonas Beispiel"
          },
          "changed_at": "2026-11-13T19:05:00Z",
          "changed_by": {
            "handle": "demo",
            "display_name": "Demo Pilot"
          },
          "confirmed": {
            "pilots": 1,
            "latest_day": "2026-11-12"
          }
        }
      ]
    }
  ],
  "attributions": [
    {
      "source": "venti-pilots",
      "provider": "Venti contributors",
      "licence": "CC0 1.0",
      "text": "Venti contributors",
      "url": "https://creativecommons.org/publicdomain/zero/1.0/",
      "notice": "Added by pilots: not official and not checked by anyone. A site on the map is not permission to fly there."
    }
  ]
}
```

`standing[].confirmed` is how many pilots say they took off at the site and that it was good, with the newest day one of them speaks of; it is left out while nobody has. An app that shows this layer marks its sites as from pilots and shows its `notice`.

### `GET /v1/sites/{id}`

One site by its identifier, of any layer. No token.

```bash
curl 'http://127.0.0.1:8080/v1/sites/venti:0000000000000000000000s001'
```

```json
{
  "site": {
    "id": "venti:0000000000000000000000s001",
    "name": "Musterkogel West",
    "position": {
      "lat": 47.3,
      "lon": 10.9
    },
    "elevation_m": 1862.0,
    "uses": [
      "takeoff"
    ],
    "wind_directions": [
      {
        "from_deg": 225.0,
        "to_deg": 315.0
      }
    ],
    "paragliding": true,
    "description": "A made-up site of the development mode. No such place exists."
  },
  "standing": {
    "id": "venti:0000000000000000000000s001",
    "revision": 2,
    "created_at": "2026-10-29T19:05:00Z",
    "created_by": {
      "handle": "demo",
      "display_name": "Demo Pilot"
    },
    "changed_at": "2026-10-31T19:05:00Z",
    "changed_by": {
      "handle": "demo",
      "display_name": "Demo Pilot"
    },
    "confirmed": {
      "pilots": 2,
      "latest_day": "2026-11-16"
    }
  },
  "lasting": true,
  "attributions": [
    {
      "source": "venti",
      "provider": "Venti partners",
      "licence": "CC0 1.0",
      "text": "Venti partners",
      "url": "https://creativecommons.org/publicdomain/zero/1.0/",
      "notice": "Added by a partner of Venti and not checked by Venti. A site on the map is not permission to fly there."
    }
  ]
}
```

`lasting` says what the address promises. For a site that pilots made it is `true`: the identifier is the server's own, and the address answers the site, or `410` with `site_gone` once it is removed. For a site of a source it is `false`: the identifier is the source's (`osm-free-flying:way/1080986173`, with its `/` as it is), the record is the one in the set the server holds now, there is no `standing`, and when the source no longer has the site the address answers `404` with `site_not_found`:

```bash
curl 'http://127.0.0.1:8080/v1/sites/osm-free-flying:way/1080986173'
```

```json
{
  "site": {
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
    "url": "https://www.openstreetmap.org/way/1080986173",
    "website": "https://www.dhv.de/",
    "fee": "yes",
    "outline": [
      [
        11.9055085,
        47.6789575
      ],
      [
        11.9055532,
        47.6793075
      ],
      [
        11.9048395,
        47.6794382
      ],
      [
        11.9046533,
        47.6788445
      ],
      [
        11.9055085,
        47.6789575
      ]
    ]
  },
  "lasting": false,
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

`404` with `site_not_found` for an identifier no site has, of a source this server does not serve sites of, or of no form at all.

### `GET /v1/sites/{id}/history?after=&limit=`

The revisions of a site that pilots made, the first one first. No token. `limit` is 1 to 100 as everywhere; `after` is the number of the last revision of the page before.

```bash
curl 'http://127.0.0.1:8080/v1/sites/venti:0000000000000000000000s002/history'
```

```json
{
  "id": "venti-pilots:0000000000000000000000s002",
  "revision": 3,
  "removed": false,
  "revisions": [
    {
      "number": 1,
      "at": "2026-11-06T19:05:00Z",
      "by": {
        "handle": "jonas",
        "display_name": "Jonas Beispiel"
      },
      "via": "session",
      "kind": "created",
      "comment": "Made up, for the development mode.",
      "site": {
        "id": "venti-pilots:0000000000000000000000s002",
        "name": "Beispieljoch",
        "position": {
          "lat": 47.29,
          "lon": 10.91
        },
        "elevation_m": 2104.0,
        "uses": [
          "takeoff"
        ],
        "paragliding": true,
        "description": "A made-up site of the development mode. No such place exists."
      }
    },
    {
      "number": 2,
      "at": "2026-11-12T19:05:00Z",
      "by": {
        "handle": "mara",
        "display_name": "Mara Muster"
      },
      "via": "session",
      "kind": "changed",
      "comment": "Height corrected.",
      "site": {
        "id": "venti-pilots:0000000000000000000000s002",
        "name": "Beispieljoch",
        "position": {
          "lat": 47.29,
          "lon": 10.91
        },
        "elevation_m": 1204.0,
        "uses": [
          "takeoff"
        ],
        "paragliding": true,
        "description": "A made-up site of the development mode. No such place exists."
      }
    },
    {
      "number": 3,
      "at": "2026-11-13T19:05:00Z",
      "by": {
        "handle": "demo",
        "display_name": "Demo Pilot"
      },
      "via": "session",
      "kind": "restored",
      "restores": 1,
      "comment": "The height was right before.",
      "site": {
        "id": "venti-pilots:0000000000000000000000s002",
        "name": "Beispieljoch",
        "position": {
          "lat": 47.29,
          "lon": 10.91
        },
        "elevation_m": 2104.0,
        "uses": [
          "takeoff"
        ],
        "paragliding": true,
        "description": "A made-up site of the development mode. No such place exists."
      }
    }
  ],
  "more": false,
  "attributions": [
    {
      "source": "venti-pilots",
      "provider": "Venti contributors",
      "licence": "CC0 1.0",
      "text": "Venti contributors",
      "url": "https://creativecommons.org/publicdomain/zero/1.0/",
      "notice": "Added by pilots: not official and not checked by anyone. A site on the map is not permission to fly there."
    }
  ]
}
```

`kind` is `created`, `changed`, `restored` (with `restores`, the number of the revision it put back) or `removed`; a client expects kinds it does not know. `site` is the whole record as it stood from that revision on, and for a `removed` revision as it stood until then. `more` says that revisions follow. A site that a pilot removed keeps its history (`removed` is `true`), so that it can be restored. A site that whoever runs the server took down answers `410` with `site_gone` here too.

### `GET /v1/sites/changes?layer=&since=&limit=`

What happened in one of the two layers since a program last asked: for whoever keeps a copy of it. No token. `layer` is `venti` (the partners', and what a request that names none gets) or `venti-pilots`; anything else is `400` with `invalid_layers`. A mirror of both follows each with a cursor of its own: the two layers share one count of changes, so a cursor of one is a number the other may never have given out, and is still a place to ask from. A site is listed again when it has a new revision and when a [confirmation](#put-v1sitesidconfirmation) of it is given or taken back: its `standing.confirmed` is then the new count, at the revision it stood at. A confirmation that goes with its account's deletion gives the site no new number (to do): the count is right wherever the site is read, and a mirror has the old one until the site changes again. It works as [`GET /v1/flights/changes`](#get-v1flightschangessincelimit) does: `since` is the `cursor` of the answer before, and none starts at the beginning.

```bash
curl 'http://127.0.0.1:8080/v1/sites/changes'
```

```json
{
  "sites": [
    {
      "site": {
        "id": "venti:0000000000000000000000s003",
        "name": "Musterwiese",
        "position": {
          "lat": 47.28,
          "lon": 10.89
        },
        "elevation_m": 1010.0,
        "uses": [
          "landing"
        ],
        "paragliding": true,
        "description": "A made-up site of the development mode. No such place exists."
      },
      "standing": {
        "id": "venti:0000000000000000000000s003",
        "revision": 1,
        "created_at": "2026-11-14T19:05:00Z",
        "created_by": {
          "handle": "demo",
          "display_name": "Demo Pilot"
        },
        "changed_at": "2026-11-14T19:05:00Z",
        "changed_by": {
          "handle": "demo",
          "display_name": "Demo Pilot"
        }
      }
    },
    {
      "site": {
        "id": "venti:0000000000000000000000s001",
        "name": "Musterkogel West",
        "position": {
          "lat": 47.3,
          "lon": 10.9
        },
        "elevation_m": 1862.0,
        "uses": [
          "takeoff"
        ],
        "wind_directions": [
          {
            "from_deg": 225.0,
            "to_deg": 315.0
          }
        ],
        "paragliding": true,
        "description": "A made-up site of the development mode. No such place exists."
      },
      "standing": {
        "id": "venti:0000000000000000000000s001",
        "revision": 2,
        "created_at": "2026-10-29T19:05:00Z",
        "created_by": {
          "handle": "demo",
          "display_name": "Demo Pilot"
        },
        "changed_at": "2026-10-31T19:05:00Z",
        "changed_by": {
          "handle": "demo",
          "display_name": "Demo Pilot"
        },
        "confirmed": {
          "pilots": 2,
          "latest_day": "2026-11-16"
        }
      }
    }
  ],
  "removed": [],
  "removals": [],
  "cursor": "10",
  "more": false,
  "reset": false,
  "attributions": [
    {
      "source": "venti",
      "provider": "Venti partners",
      "licence": "CC0 1.0",
      "text": "Venti partners",
      "url": "https://creativecommons.org/publicdomain/zero/1.0/",
      "notice": "Added by a partner of Venti and not checked by Venti. A site on the map is not permission to fly there."
    }
  ]
}
```

`sites` are the sites that are new, changed or back, each once and as it stands now, with its `standing`. `removed` are the identifiers of sites that are gone: all a mirror needs to remove them. `removals` are the same sites in the same order, each with what a list of "removed lately" shows, so that an app does not have to ask every site's history. `more` says to ask again at once with `cursor`. `reset` is `true` when the server did not know the cursor and started from the beginning. An account that is deleted takes its name off its sites, and off what it removed, without a change being listed: a copy's author names can be older than the server's.

The crowdsourced layer of the development mode has one site that is gone:

```bash
curl 'http://127.0.0.1:8080/v1/sites/changes?layer=venti-pilots'
```

```json
{
  "sites": [
    {
      "site": {
        "id": "venti-pilots:0000000000000000000000s002",
        "name": "Beispieljoch",
        "position": {
          "lat": 47.29,
          "lon": 10.91
        },
        "elevation_m": 2104.0,
        "uses": [
          "takeoff"
        ],
        "paragliding": true,
        "description": "A made-up site of the development mode. No such place exists."
      },
      "standing": {
        "id": "venti-pilots:0000000000000000000000s002",
        "revision": 3,
        "created_at": "2026-11-06T19:05:00Z",
        "created_by": {
          "handle": "jonas",
          "display_name": "Jonas Beispiel"
        },
        "changed_at": "2026-11-13T19:05:00Z",
        "changed_by": {
          "handle": "demo",
          "display_name": "Demo Pilot"
        },
        "confirmed": {
          "pilots": 1,
          "latest_day": "2026-11-12"
        }
      }
    }
  ],
  "removed": [
    "venti-pilots:0000000000000000000000s004"
  ],
  "removals": [
    {
      "id": "venti-pilots:0000000000000000000000s004",
      "name": "Musterkogel",
      "at": "2026-11-15T19:15:00Z",
      "by": {
        "handle": "jonas",
        "display_name": "Jonas Beispiel"
      }
    }
  ],
  "cursor": "11",
  "more": false,
  "reset": false,
  "attributions": [
    {
      "source": "venti-pilots",
      "provider": "Venti contributors",
      "licence": "CC0 1.0",
      "text": "Venti contributors",
      "url": "https://creativecommons.org/publicdomain/zero/1.0/",
      "notice": "Added by pilots: not official and not checked by anyone. A site on the map is not permission to fly there."
    }
  ]
}
```

| Field of a removal | Meaning |
|---|---|
| `id` | The site, as `removed` names it |
| `name` | Its name as it last stood. Left out for a site that whoever runs the server took down: such a site shows nothing of what it said |
| `at` | When it was removed |
| `by` | Who removed it, by `handle` and `display_name`, as a site's history names a pilot. Left out where whoever runs the server removed it, and where the account is gone |

What the site said, and why it was removed, is in [its history](#get-v1sitesidhistoryafterlimit), which a removed site keeps; a site that was taken down has none to read.

### `POST /v1/sites`

Needs a token. Adds a site, as its first revision: to the partners' layer `venti` when the account is a partner, to the crowdsourced layer `venti-pilots` when it is any other pilot's. The answer's `site.id` begins with the layer's name.

```json
{
  "site": {
    "name": "Beispielkopf Süd",
    "position": {
      "lat": 47.4,
      "lon": 11.2
    },
    "elevation_m": 1540.0,
    "uses": [
      "takeoff"
    ],
    "wind_directions": [
      {
        "from_deg": 135.0,
        "to_deg": 225.0
      }
    ],
    "paragliding": true,
    "description": "Made up, for the documentation. No such place exists."
  },
  "comment": "From where I stood on 14 November."
}
```

`site` says what the site is: `name` (required, one line, 200 characters at most), `position` (required), `uses` (at least one of `takeoff`, `landing`, `towing`, `training`), and if the pilot knows them `elevation_m` (−500 to 9,000), `wind_directions` (at most 16 spans within 0 to 360), `paragliding`, `hang_gliding`, `website` (an `http` or `https` address of plain form, 500 characters at most), `description` (1,000 characters, may have lines), `fee` and `no_flight_time` (one line, 100 characters), `guest_guidelines`, `outline`. They mean what they mean in [a record](#get-v1sitesbboxwestsoutheastnorth), and are held to the same checks as a source's records. `comment` is one line of at most 200 characters, kept with the revision. Text is cleaned as all text is that one person types and another reads: what does not print is dropped.

**A site comes from the pilot's own knowledge**: where they stood, where their own flight began. It is not copied from OpenStreetMap, another database, a map or a guide.

The answer is `201` with the site as [`GET /v1/sites/{id}`](#get-v1sitesid) answers it, under its new identifier, at revision 1.

| Status | Code | When |
|---|---|---|
| 400 | `invalid_site` | A field cannot be kept; `message` names it and repeats nothing that was sent |
| 400 | `site_out_of_area` | The place is in no region this server serves |
| 403 | `sites_closed` | The account is no partner, and this server takes no sites from pilots |
| 403 | `publishing_suspended` | Whoever runs the server has suspended the account's publishing |
| 409 | `site_too_close` | A site stands less than 20 m from the place: for a place from pilots a site of either layer, for a partner's site a partner's ([the rule](#sites-that-ventis-partners-and-pilots-add)). The error names it in `site`: if it is the same place, correct that one, or report it where it is a partner's |
| 429 | `rate_limited` | The account has added 20 sites today |

**The site that stands there** is named in the error, for an app to say which one and to open it for correcting. It is the one error whose body has more than a code and a message. Asked to add a site a few metres from the first of the development mode's:

```json
{
  "error": {
    "code": "site_too_close",
    "message": "the site venti:0000000000000000000000s001 stands less than 20 m from this place; if it is the same place, correct that one",
    "site": {
      "id": "venti:0000000000000000000000s001",
      "name": "Musterkogel West"
    }
  }
}
```

| Field | Meaning |
|---|---|
| `error.site.id` | The identifier of the site that stands less than 20 m from the place |
| `error.site.name` | Its name as it is now. A pilot's text: show it as a name. Left out for a site without one |

`site` is there with `site_too_close` from every request that can answer it (adding, changing, restoring), and with no other code. A server from before 2026-11-21 names the site in `message` only.

### `PUT /v1/sites/{id}`

Needs a token. Replaces what a site of the layer says, as a new revision. `site` is the whole record as it is to stand: a field left out is a field the site no longer has. `revision` is the number of the revision the change was made from.

```json
{
  "site": {
    "name": "Musterkogel West",
    "position": {
      "lat": 47.3,
      "lon": 10.9
    },
    "elevation_m": 1871.0,
    "uses": [
      "takeoff"
    ],
    "wind_directions": [
      {
        "from_deg": 225.0,
        "to_deg": 315.0
      }
    ],
    "paragliding": true,
    "description": "A made-up site of the development mode. No such place exists."
  },
  "revision": 2,
  "comment": "Height from my own GPS."
}
```

The answer is `200` with the site as it now stands:

```json
{
  "site": {
    "id": "venti:0000000000000000000000s001",
    "name": "Musterkogel West",
    "position": {
      "lat": 47.3,
      "lon": 10.9
    },
    "elevation_m": 1871.0,
    "uses": [
      "takeoff"
    ],
    "wind_directions": [
      {
        "from_deg": 225.0,
        "to_deg": 315.0
      }
    ],
    "paragliding": true,
    "description": "A made-up site of the development mode. No such place exists."
  },
  "standing": {
    "id": "venti:0000000000000000000000s001",
    "revision": 3,
    "created_at": "2026-10-29T19:05:00Z",
    "created_by": {
      "handle": "demo",
      "display_name": "Demo Pilot"
    },
    "changed_at": "2026-11-18T19:05:00Z",
    "changed_by": {
      "handle": "demo",
      "display_name": "Demo Pilot"
    },
    "confirmed": {
      "pilots": 2,
      "latest_day": "2026-11-16"
    }
  },
  "lasting": true,
  "attributions": [
    {
      "source": "venti",
      "provider": "Venti partners",
      "licence": "CC0 1.0",
      "text": "Venti partners",
      "url": "https://creativecommons.org/publicdomain/zero/1.0/",
      "notice": "Added by a partner of Venti and not checked by Venti. A site on the map is not permission to fly there."
    }
  ]
}
```

A request that would change nothing writes no revision and answers `200` with the site as it is.

| Status | Code | When |
|---|---|---|
| 400 | `invalid_site`, `site_out_of_area` | As for adding |
| 403 | `sites_closed` | The site is of the crowdsourced layer, the account is no partner, and this server takes no sites from pilots; or the site is a source's, which is corrected at the source (its `url`) |
| 403 | `sites_for_partners` | The site is of the partners' layer and the account is no partner. A pilot who finds it wrong [reports it](#post-v1reports) |
| 404 | `site_not_found` | No site of the layer has this identifier |
| 409 | `site_changed` | The site stands at another revision than `revision`: somebody changed it meanwhile. Read it again |
| 409 | `site_too_close` | At its new place the site would stand less than 20 m from another |
| 410 | `site_gone` | The site is removed. Restoring a revision brings it back |
| 429 | `rate_limited` | The account has written 100 revisions today: changes, restores and removals together |

### `POST /v1/sites/{id}/revisions`

Needs a token. Puts a site back to what an earlier revision said, as a new revision that names it. This is how a change is undone, and how a removed site comes back.

```json
{
  "restore": 2,
  "comment": "The height was right before."
}
```

The answer is `200` with the site as it now stands. `404` with `site_revision_not_found` if the site has no revision with that number; `409` with `site_too_close` if another site stands there by now; `410` with `site_gone` if whoever runs the server took the site down; the others as for changing.

### `DELETE /v1/sites/{id}`

Needs a token. Removes a site of the layer, as a new revision. The body may be left out; one that is there is `{"comment": "…"}`, sent as JSON. The answer is `204` with no body. From then on the site is in no box, its address answers `410`, `GET /v1/sites/changes` lists it as removed, and its history stays, so that any pilot can restore it. `410` with `site_gone` if it is removed already.

Only whoever runs the server takes a site down so that no pilot brings it back; see [deploy/README.md](../deploy/README.md).

### `PUT /v1/sites/{id}/confirmation`

"I took off here and it was good." Needs a token (`sites:write` for a token of another program). A signed-in pilot whose publishing is not suspended confirms a site of either layer, on a server that takes sites from pilots (`sites_from_pilots`); a partner's word counts as any pilot's. One confirmation for an account and a site: sending it again takes the place of the first, which is how its day is moved.

```json
{
  "day": "2026-11-18",
  "note": "  South-west,\n easy launch. "
}
```

| Field | Meaning |
|---|---|
| `day` | The day the pilot speaks of, `YYYY-MM-DD`: today, or one of the 90 days before it. The pilot's own day, so one day ahead of the server's clock is taken |
| `note` | Optional. One line of at most 200 characters, cleaned as a revision's comment is. Everyone reads it |
| `flight` | Optional. The address of one of the pilot's own flights. The server checks that it is theirs and that it took off within 500 m of the site as the site stands now, and keeps that it did (`with_flight`), not the address: a flight's address is the key to its page |

The answer is `200` with the site as [`GET /v1/sites/{id}`](#get-v1sitesid) answers it, its `standing.confirmed` counting this pilot. A confirmation is no revision: the site's `revision` and `changed_at` stay, its history says nothing of it, and [`GET /v1/sites/changes`](#get-v1siteschangeslayersincelimit) lists the site again.

| Status | Code | When |
|---|---|---|
| 400 | `invalid_confirmation` | The day is to come or more than 90 days back; the note is too long; the flight is not this account's, or did not take off within 500 m of the site |
| 403 | `sites_closed` | This server takes no confirmations from pilots; or the site is a source's |
| 403 | `publishing_suspended` | Whoever runs the server has suspended the account's publishing |
| 404 | `site_not_found` | No site has this identifier |
| 410 | `site_gone` | The site is removed or taken down |
| 429 | `rate_limited` | The account gave or took back 50 confirmations today |

### `DELETE /v1/sites/{id}/confirmation`

Needs a token. Takes the caller's own confirmation of the site back. `204` with no body, whether there was one or not. Nobody takes back another pilot's; an account's confirmations go when the account is deleted, where its sites and revisions stay: a confirmation is the pilot's word, not a record given to everyone.

### `GET /v1/sites/{id}/confirmations`

Who says they took off at a site, the newest day first and then by handle: at most the newest 100. No token.

```bash
curl 'http://127.0.0.1:8080/v1/sites/venti:0000000000000000000000s001/confirmations'
```

```json
{
  "id": "venti:0000000000000000000000s001",
  "confirmed": {
    "pilots": 2,
    "latest_day": "2026-11-16"
  },
  "confirmations": [
    {
      "by": {
        "handle": "mara",
        "display_name": "Mara Muster"
      },
      "day": "2026-11-16",
      "note": "North-west at noon, smooth."
    },
    {
      "by": {
        "handle": "jonas",
        "display_name": "Jonas Beispiel"
      },
      "day": "2026-11-09"
    }
  ]
}
```

`confirmed` is the count and the newest day, as the site's `standing` has it: enough for "2 pilots took off here, last on 16 November". `with_flight` is there, and `true`, where the server found a flight of the pilot's that took off at the site; which flight is not said. `410` with `site_gone` for a site that is removed or taken down, `404` for one that never was.

Confirmations cannot be reported by themselves yet (to do): a pilot reports the site, or the pilot's page.

## What public flights say about a site

A pilot says of a place from pilots "I took off here and it was good" ([above](#put-v1sitesidconfirmation)). A flight its pilot made public says the first half of that without anyone typing it. Two requests read it, both open to anyone without a token and to a page of any origin.

**Public flights and nothing else.** Both are made anew for every answer from the flights that are public at that moment. A flight that is private, shared by link only, taken down or deleted is in no count, no month and no list, and a flight that leaves the public is gone from the next answer: nothing is kept between two.

**A day** in these answers is the day at the place, by the sun: the time in UTC moved by four minutes for every degree of longitude. A flight is flown in daylight, so that is the day the pilot had on the calendar, in the Alps and in Australia, without a table of time zones. The months are counted by it.

**The bound.** Both count as reads of public flights for the address that asks, with the list and the tracks: [600 an hour](#how-often-an-address-may-read).

The examples are the answers the server's tests keep, over the [development mode](#try-it). Its made-up flights of `lena`, `timo` and `ines` (`dev::add_demo_takeoffs`: the sample flight, flown from two made-up places) give the made-up take-off "Musterkogel West" its flights and make one candidate; `curl 'http://127.0.0.1:8080/v1/sites/osm-free-flying:node/5964676196/flights'` shows the five public flights of the other three pilots, beside a take-off of OpenStreetMap's.

### `GET /v1/sites/{id}/flights`

What the public flights that began at a site say about it: how many, by how many pilots, when last, in which months of the year, and the best of them. The site is one of any layer, by its identifier: a source's (`osm-free-flying:node/5964676196`, the `/` as it is), the partners' or the crowdsourced one.

```bash
curl 'http://127.0.0.1:8080/v1/sites/venti:0000000000000000000000s001/flights'
```

```json
{
  "id": "venti:0000000000000000000000s001",
  "within_m": 500.0,
  "takeoff": true,
  "flights": 3,
  "pilots": 2,
  "latest_day": "2026-07-11",
  "by_month": [
    0,
    0,
    0,
    0,
    1,
    0,
    2,
    0,
    0,
    0,
    0,
    0
  ],
  "best": [
    {
      "id": "ww5qkxhxaedefvypb3wy3v6m3m",
      "pilot": {
        "handle": "lena",
        "display_name": "Lena Probe"
      },
      "day": "2026-05-09",
      "score": {
        "kind": "free_flight",
        "closed": false,
        "distance_m": 12007.4,
        "points": 12.01
      }
    },
    {
      "id": "kgqqjzfkynh9vgjxby3rx0snd8",
      "pilot": {
        "handle": "timo",
        "display_name": "Timo Entwurf"
      },
      "day": "2025-07-19",
      "score": {
        "kind": "free_flight",
        "closed": false,
        "distance_m": 12007.5,
        "points": 12.01
      }
    },
    {
      "id": "hwj4105bnzcgcavngacrzjt3s4",
      "pilot": {
        "handle": "lena",
        "display_name": "Lena Probe"
      },
      "day": "2026-07-11",
      "score": {
        "kind": "free_flight",
        "closed": false,
        "distance_m": 11472.1,
        "points": 11.47
      }
    }
  ],
  "terms": {
    "text": "These flights are their pilots'. They are shown here for reading. Reusing a flight (copying it elsewhere, publishing it, building on it) needs its pilot's leave until Venti's terms say otherwise, and a flight its pilot has withdrawn is to be removed from every copy.",
    "decided": false
  }
}
```

- `within_m`: a flight began at the site if it took off within this many metres of the site's position, **500**. It is the distance a flight may lie from a site it is named with to [confirm](#put-v1sitesidconfirmation) it: "this flight began at this site" means one thing on the server. A site is one point, a take-off is a meadow, and where a recording first shows the glider flying is some way down the slope. Where a flight took off is what the analysis of its file found, not its first fix. Two sites nearer to each other than that both count a flight that began between them.
- `takeoff`: whether the site is one to take off from by its own record: `takeoff` or `towing` among its `uses`, or no use named. For a site that is not (a landing field, a training slope) nothing is counted, every count is `0` and `best` is empty: the flights that began within 500 m of a landing field are those of a take-off beside it.
- `flights` and `pilots`: how many public flights began there, and how many different pilots flew them.
- `latest_day`: the day of the newest. Left out without a flight.
- `by_month`: twelve numbers, January first: how many of the flights began in each month of the year, all years together. They add up to `flights`.
- `best`: the best of them by points, the best first, as [`sort=score`](#get-v1flightspublicpilotbboxfromtosortafterlimit) orders the list of public flights: at most five, each with its address, its pilot as that list names one, its day and its `score` (the distance and the points). A flight too short to score is not among them. Everything else of a flight is at [`GET /v1/flights/{id}`](#get-v1flightsid).
- `terms`: under what the flights are given ([Flights](#flights)).

The answer names flights and pilots, so it is kept as the list of public flights is: `Cache-Control: public, no-cache` with an `ETag`. A cache may keep it and asks before every reuse, and a flight that is made private is gone from it at once.

`404` with `site_not_found` for a site that never was or that its source no longer delivers, `410` with `site_gone` for one of Venti's own that was removed. The site's own answer ([`GET /v1/sites/{id}`](#get-v1sitesid)) does not carry these numbers: they are asked for here.

All of a site's flights, page by page, are the list of public flights in a box around it ([`bbox`](#get-v1flightspublicpilotbboxfromtosortafterlimit)).

### `GET /v1/sites/candidates?bbox=west,south,east,north`

The places in a box where pilots take off and no site of any layer stands. A candidate is not a site, and the server never makes a site of one: it is something to show a pilot who flew there ("pilots take off here and no site names it: add it?"), who then [adds the site](#post-v1sites) with the place and the height of their own flight, or does not.

```bash
curl 'http://127.0.0.1:8080/v1/sites/candidates?bbox=10.92,47.25,10.95,47.27'
```

```json
{
  "candidates": [
    {
      "position": {
        "lat": 47.2612,
        "lon": 10.9312
      },
      "altitude_m": 1620,
      "flights": 3,
      "pilots": 3,
      "latest_day": "2026-06-07",
      "by_month": [
        0,
        0,
        0,
        1,
        0,
        2,
        0,
        0,
        0,
        0,
        0,
        0
      ]
    }
  ],
  "more": false,
  "min_pilots": 3,
  "cell_deg": 0.0025,
  "clear_of_sites_m": 500.0,
  "terms": {
    "text": "These flights are their pilots'. They are shown here for reading. Reusing a flight (copying it elsewhere, publishing it, building on it) needs its pilot's leave until Venti's terms say otherwise, and a flight its pilot has withdrawn is to be removed from every copy.",
    "decided": false
  }
}
```

**What makes a place a candidate.**

- The public flights of at least `min_pilots` different pilots, **3**, began in one cell of a fixed grid of `cell_deg`, **0.0025** degrees of latitude and of longitude: 278 m from north to south, and 190 m from east to west at 47° north.
- No standing site stands within `clear_of_sites_m`, **500** m, of the middle of those take-offs: no site of any source this server serves, none of the partners' layer and none of the crowdsourced one. Within that distance a flight began at that site ([above](#get-v1sitesidflights)). A site that was removed is in the way of nothing.

**What a candidate says.** `position`: the middle of the take-offs, rounded to 0.0001 degrees (11 m). `altitude_m`: the mean of the heights the flights took off at, satellite altitude, rounded to 10 m. `flights`, `pilots`, `latest_day` and `by_month` as for a site. **Not who:** a candidate names no pilot and no flight.

**Why three pilots.** One pilot's flights say where that pilot takes off, which may be the field behind their house; two may be a pilot and a friend. Three who each chose to make a flight from there public say that pilots take off there. Everything a candidate is made from is in the list of public flights already, with each flight's exact take-off and its pilot's name: a candidate adds no fact about a flight, only the word that the place is one where pilots take off, and the number of pilots is what guards that word. When one of three makes their flight private, the place is no candidate at the next answer.

**At a cell's edge.** The grid is fixed, and an edge of it may run through a take-off. Its flights are then counted in two cells, each by itself: the take-off is offered twice, some hundred metres apart, or not at all where neither part has three pilots alone. The grid never makes a candidate that is none.

**The box.** `bbox` as for [sites](#get-v1sitesbboxwestsoutheastnorth). A cell that the box touches is counted whole, so a place is the same candidate whichever box asks for it; its position may lie just outside the box. The box may span at most **2 degrees** of latitude and of longitude each, some 220 km by 150 km at 47° north: what an answer costs the server grows with the public flights in its box. A wider one is refused with `400` and `box_too_large`. An answer lists at most **200** candidates, the one with the most pilots first, then the one with the most flights; `more` says that the box holds more, and a smaller box shows them.

An answer names nobody and may be reused for a minute: `Cache-Control: public, max-age=60` with an `ETag`. A flight that leaves the public, or a site that is added, shows a minute later at most.

| Status | `code` | When |
|---|---|---|
| 400 | `invalid_bbox` | `bbox` is missing, is not four numbers, or the numbers do not form a box |
| 400 | `box_too_large` | The box spans more than 2 degrees of latitude or of longitude |
| 429 | `rate_limited` | The address has read public flights as often as one may in an hour |

## Dumps of Venti's own sites

Each of Venti's own two layers of sites ([above](#sites-that-ventis-partners-and-pilots-add)) whole, as files: for whoever wants all of it at once and not page by page. No token, no account, and no sign-up of any kind. The addresses stay as they are.

| Address | What it is |
|---|---|
| [`/v1/dumps`](#get-v1dumps) | The list: every file with its licence, when it was made, where the layer stood, its size and its checksum |
| [`/v1/dumps/sites/{layer}.geojson`](#get-v1dumpssiteslayergeojson) | The sites of the layer that stand, as GeoJSON |
| [`/v1/dumps/sites/{layer}.csv`](#get-v1dumpssiteslayercsv) | The same sites as CSV |
| [`/v1/dumps/sites/{layer}.history.jsonl`](#get-v1dumpssiteslayerhistoryjsonl) | Every site the layer ever had, removed ones too, with every revision, as JSON Lines |

`{layer}` is `venti` or `venti-pilots`. Any other name, and any other ending, is `404` with `not_found`.

- **A layer to a file, and nothing else in it.** A file holds the sites of one layer. The two layers are never in one file, and no file holds a record of another source: OpenStreetMap's sites are under another licence and are in no dump.
- **The licence is in the file.** Both layers are given under [CC0 1.0](https://creativecommons.org/publicdomain/zero/1.0/). The GeoJSON file and the history file say so in themselves, with the layer, its `notice` (the crowdsourced layer's: "not official and not checked by anyone"), when the file was made and the `cursor` it stands at. A CSV file holds its lines and nothing else, so that every CSV reader takes it: its licence, time and cursor are in the list, in the entry of its layer, and its last column names the layer in every line.
- **From a dump on with the changes.** `cursor` is the layer's change number the files stand at: everything that happened to the layer up to it is in them. A program that has read a dump asks [`GET /v1/sites/changes?layer=…&since=`](#get-v1siteschangeslayersincelimit) with it and gets what happened since, removals included, and never needs the dump again.
- **What is in a dump is what anyone may read through the API**, and no more. A removed site is in the history file with its revisions and in neither of the other two. Of a site that whoever runs the server took down, the history file says that it is removed and when, and nothing of what it said. Who confirmed a site ("I took off here and it was good") is not in a dump: each site's record has the count and the newest day, as everywhere, and the confirmations themselves are read at [the site](#get-v1sitesidconfirmations). Whether they belong in a file is the owner's to decide. Flights are in no dump: they are their pilots' own.
- **Made when asked for.** The server makes a layer's files from its database when they are first asked for and keeps them. After five minutes the next request makes it look whether the layer has changed, and only then are the files made anew: so a file is at most five minutes behind the layer, and `generated_at` and `cursor` say exactly where it stands. The three files of a layer are made together and agree with each other. The same five minutes are the longest a site that was taken down stays in a file. The development mode looks at every request.
- **Caching and compression.** The list and the files carry `Cache-Control: public, no-cache` and an `ETag`: a copy may be kept and is checked before every reuse, and `If-None-Match` is answered `304` while the file is the same. They are gzip-compressed for a request that says `Accept-Encoding: gzip`. `bytes` and `sha256` in the list are of the file as it is after unpacking.
- **How large a layer may be.** A dump is made of a layer of up to 50,000 sites, standing and removed, with up to 150,000 revisions. Beyond that the list and the files answer `503` with `unavailable`, and the changes read the layer page by page; nothing is ever handed out that is silently short. Both layers together hold four sites in the development mode and none on a new server.

### `GET /v1/dumps`

```bash
curl http://127.0.0.1:8080/v1/dumps
```

```json
{
  "layers": [
    {
      "layer": "venti",
      "provider": "Venti partners",
      "licence": "CC0 1.0",
      "licence_url": "https://creativecommons.org/publicdomain/zero/1.0/",
      "notice": "Added by a partner of Venti and not checked by Venti. A site on the map is not permission to fly there.",
      "generated_at": "2026-11-18T19:05:00Z",
      "cursor": "10",
      "sites": 2,
      "removed": 0,
      "revisions": 3,
      "files": [
        {
          "content": "sites",
          "format": "geojson",
          "path": "/v1/dumps/sites/venti.geojson",
          "media_type": "application/geo+json",
          "bytes": 1705,
          "sha256": "b9c079eb7d8defce4a629ce8842b8c65373f3700d3f6e6d29930ee1135dd3040"
        },
        {
          "content": "sites",
          "format": "csv",
          "path": "/v1/dumps/sites/venti.csv",
          "media_type": "text/csv; charset=utf-8; header=present",
          "bytes": 711,
          "sha256": "957acbcb32dc427f04b5b9c1535c26d6ceecdc39e635fb2e07a2292529b859d3"
        },
        {
          "content": "history",
          "format": "jsonl",
          "path": "/v1/dumps/sites/venti.history.jsonl",
          "media_type": "application/x-ndjson",
          "bytes": 1790,
          "sha256": "ab5f2392279c5d15d6b1fa2169d1428ab01cd65b6972951fba5b0fa97cb7fa09"
        }
      ]
    },
    {
      "layer": "venti-pilots",
      "provider": "Venti contributors",
      "licence": "CC0 1.0",
      "licence_url": "https://creativecommons.org/publicdomain/zero/1.0/",
      "notice": "Added by pilots: not official and not checked by anyone. A site on the map is not permission to fly there.",
      "generated_at": "2026-11-18T19:05:00Z",
      "cursor": "11",
      "sites": 1,
      "removed": 1,
      "revisions": 5,
      "files": [
        {
          "content": "sites",
          "format": "geojson",
          "path": "/v1/dumps/sites/venti-pilots.geojson",
          "media_type": "application/geo+json",
          "bytes": 1061,
          "sha256": "7a724bbab97cafa1cd74dc95be894a1d9344831e97032688192ce5b554a4379b"
        },
        {
          "content": "sites",
          "format": "csv",
          "path": "/v1/dumps/sites/venti-pilots.csv",
          "media_type": "text/csv; charset=utf-8; header=present",
          "bytes": 496,
          "sha256": "466e315480156d854361152e0c2afd8eab7e6ed0ac5e79c8fd85f806e239e750"
        },
        {
          "content": "history",
          "format": "jsonl",
          "path": "/v1/dumps/sites/venti-pilots.history.jsonl",
          "media_type": "application/x-ndjson",
          "bytes": 2831,
          "sha256": "516ddcd7575207ae93f86b90223233f4e8e4f3b75c9f1f7f1c358ba7ad1e76dc"
        }
      ]
    }
  ],
  "attributions": [
    {
      "source": "venti",
      "provider": "Venti partners",
      "licence": "CC0 1.0",
      "text": "Venti partners",
      "url": "https://creativecommons.org/publicdomain/zero/1.0/",
      "notice": "Added by a partner of Venti and not checked by Venti. A site on the map is not permission to fly there."
    },
    {
      "source": "venti-pilots",
      "provider": "Venti contributors",
      "licence": "CC0 1.0",
      "text": "Venti contributors",
      "url": "https://creativecommons.org/publicdomain/zero/1.0/",
      "notice": "Added by pilots: not official and not checked by anyone. A site on the map is not permission to fly there."
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `layers` | One entry per layer, `venti` first |
| `layer`, `provider`, `licence`, `licence_url`, `notice` | Whose records the layer's files hold and under which licence; the same as the layer's entry in `attributions` |
| `generated_at` | When the layer's files were made |
| `cursor` | The layer's change number they stand at; `since` for [`GET /v1/sites/changes`](#get-v1siteschangeslayersincelimit) |
| `sites`, `removed` | How many sites of the layer stand, and how many are removed |
| `revisions` | How many revisions the history file holds |
| `files` | The three files: `content` (`sites` or `history`), `format` (`geojson`, `csv`, `jsonl`), `path`, `media_type`, and `bytes` and `sha256` (lowercase hexadecimal) of the file before any compression on the way |

A newer server may list a `content` or a `format` that is not in this table; a client leaves such a file alone.

### `GET /v1/dumps/sites/{layer}.geojson`

The sites of the layer that stand, sorted by identifier, as one GeoJSON `FeatureCollection` ([RFC 7946](https://www.rfc-editor.org/rfc/rfc7946)), `application/geo+json`. The collection's members beyond GeoJSON's own are the licence and the rest of what the list says of the layer. A feature's `id` is the site's identifier, its geometry is the site's position as a `Point` (longitude first, as GeoJSON has it, and no height: a site's `elevation_m` is above the sea, which is not what GeoJSON's third number means), and its `properties` are the site as [`GET /v1/sites/changes`](#get-v1siteschangeslayersincelimit) lists it: `site`, the record, and `standing`. A site with an outline has it in `site.outline`; the geometry stays the point.

```bash
curl http://127.0.0.1:8080/v1/dumps/sites/venti.geojson
```

The file is one line; here it is set out to be read:

```json
{
  "type": "FeatureCollection",
  "layer": "venti",
  "provider": "Venti partners",
  "licence": "CC0 1.0",
  "licence_url": "https://creativecommons.org/publicdomain/zero/1.0/",
  "notice": "Added by a partner of Venti and not checked by Venti. A site on the map is not permission to fly there.",
  "generated_at": "2026-11-18T19:05:00Z",
  "cursor": "10",
  "features": [
    {
      "type": "Feature",
      "id": "venti:0000000000000000000000s001",
      "geometry": {
        "type": "Point",
        "coordinates": [
          10.9,
          47.3
        ]
      },
      "properties": {
        "site": {
          "id": "venti:0000000000000000000000s001",
          "name": "Musterkogel West",
          "position": {
            "lat": 47.3,
            "lon": 10.9
          },
          "elevation_m": 1862.0,
          "uses": [
            "takeoff"
          ],
          "wind_directions": [
            {
              "from_deg": 225.0,
              "to_deg": 315.0
            }
          ],
          "paragliding": true,
          "description": "A made-up site of the development mode. No such place exists."
        },
        "standing": {
          "id": "venti:0000000000000000000000s001",
          "revision": 2,
          "created_at": "2026-10-29T19:05:00Z",
          "created_by": {
            "handle": "demo",
            "display_name": "Demo Pilot"
          },
          "changed_at": "2026-10-31T19:05:00Z",
          "changed_by": {
            "handle": "demo",
            "display_name": "Demo Pilot"
          },
          "confirmed": {
            "pilots": 2,
            "latest_day": "2026-11-16"
          }
        }
      }
    },
    {
      "type": "Feature",
      "id": "venti:0000000000000000000000s003",
      "geometry": {
        "type": "Point",
        "coordinates": [
          10.89,
          47.28
        ]
      },
      "properties": {
        "site": {
          "id": "venti:0000000000000000000000s003",
          "name": "Musterwiese",
          "position": {
            "lat": 47.28,
            "lon": 10.89
          },
          "elevation_m": 1010.0,
          "uses": [
            "landing"
          ],
          "paragliding": true,
          "description": "A made-up site of the development mode. No such place exists."
        },
        "standing": {
          "id": "venti:0000000000000000000000s003",
          "revision": 1,
          "created_at": "2026-11-14T19:05:00Z",
          "created_by": {
            "handle": "demo",
            "display_name": "Demo Pilot"
          },
          "changed_at": "2026-11-14T19:05:00Z",
          "changed_by": {
            "handle": "demo",
            "display_name": "Demo Pilot"
          }
        }
      }
    }
  ]
}
```

### `GET /v1/dumps/sites/{layer}.csv`

The same sites, a line each, sorted by identifier, as CSV ([RFC 4180](https://www.rfc-editor.org/rfc/rfc4180)): `text/csv; charset=utf-8; header=present`. The first line names the columns; lines end with a carriage return and a line feed; a field that holds a comma, a quotation mark or a line break is between quotation marks, and a quotation mark in it is written twice.

```bash
curl http://127.0.0.1:8080/v1/dumps/sites/venti.csv
```

```csv
id,name,lat,lon,elevation_m,uses,wind_directions,paragliding,hang_gliding,official,discouraged,url,website,description,fee,guest_guidelines,no_flight_time,revision,created_at,created_by,changed_at,changed_by,confirmed_pilots,confirmed_latest_day,layer
venti:0000000000000000000000s001,Musterkogel West,47.3,10.9,1862.0,takeoff,225.0-315.0,true,,,false,,,A made-up site of the development mode. No such place exists.,,,,2,2026-10-29T19:05:00Z,demo,2026-10-31T19:05:00Z,demo,2,2026-11-16,venti
venti:0000000000000000000000s003,Musterwiese,47.28,10.89,1010.0,landing,,true,,,false,,,A made-up site of the development mode. No such place exists.,,,,1,2026-11-14T19:05:00Z,demo,2026-11-14T19:05:00Z,demo,0,,venti
```

| Column | What it holds |
|---|---|
| `id` | The site's identifier |
| `name` | Its name |
| `lat`, `lon` | Its position, WGS84 degrees |
| `elevation_m` | Its height above the sea |
| `uses` | Its uses, separated by a space: `takeoff landing` |
| `wind_directions` | The directions the wind may come from, each as `from-to` in degrees, separated by a space: `225.0-315.0` |
| `paragliding`, `hang_gliding`, `official`, `guest_guidelines` | `true`, `false`, or nothing where the record does not say |
| `discouraged` | `true` or `false` |
| `url`, `website`, `description`, `fee`, `no_flight_time` | The record's texts, as they were written |
| `revision` | The number of the site's latest revision |
| `created_at`, `changed_at` | When the site was added and last changed |
| `created_by`, `changed_by` | The handles of who did; nothing where the account is gone or whoever runs the server did |
| `confirmed_pilots`, `confirmed_latest_day` | How many pilots say they took off there, and the newest day one names |
| `layer` | The layer, the same in every line of a file |

A column that is added later is added at the end. A value that the record does not have is an empty field. A site's outline is not in the CSV file. A text is as its author wrote it: nothing is put before a text that begins with `=`, `+`, `-` or `@`, which some spreadsheets take for a formula. Whoever opens the crowdsourced layer's file in a spreadsheet reads the columns as text.

### `GET /v1/dumps/sites/{layer}.history.jsonl`

Every site the layer ever had with every revision, as JSON Lines: `application/x-ndjson`, one JSON value on every line, each line ended by a line feed. The first line says what the file is; every further line is one site, sorted by identifier.

```bash
curl http://127.0.0.1:8080/v1/dumps/sites/venti-pilots.history.jsonl
```

The first line of the development mode's file, set out to be read:

```json
{
  "layer": "venti-pilots",
  "provider": "Venti contributors",
  "licence": "CC0 1.0",
  "licence_url": "https://creativecommons.org/publicdomain/zero/1.0/",
  "notice": "Added by pilots: not official and not checked by anyone. A site on the map is not permission to fly there.",
  "generated_at": "2026-11-18T19:05:00Z",
  "cursor": "11",
  "sites": 2,
  "revisions": 5
}
```

`sites` is how many lines follow and `revisions` how many revisions they hold. And the line of the site that was removed:

```json
{
  "id": "venti-pilots:0000000000000000000000s004",
  "revision": 2,
  "removed": true,
  "removal": {
    "id": "venti-pilots:0000000000000000000000s004",
    "name": "Musterkogel",
    "at": "2026-11-15T19:15:00Z",
    "by": {
      "handle": "jonas",
      "display_name": "Jonas Beispiel"
    }
  },
  "revisions": [
    {
      "number": 1,
      "at": "2026-11-15T19:05:00Z",
      "by": {
        "handle": "jonas",
        "display_name": "Jonas Beispiel"
      },
      "via": "session",
      "kind": "created",
      "comment": "Made up, for the development mode.",
      "site": {
        "id": "venti-pilots:0000000000000000000000s004",
        "name": "Musterkogel",
        "position": {
          "lat": 47.305,
          "lon": 10.905
        },
        "elevation_m": 1870.0,
        "uses": [
          "takeoff"
        ],
        "paragliding": true,
        "description": "A made-up site of the development mode. No such place exists."
      }
    },
    {
      "number": 2,
      "at": "2026-11-15T19:15:00Z",
      "by": {
        "handle": "jonas",
        "display_name": "Jonas Beispiel"
      },
      "via": "session",
      "kind": "removed",
      "comment": "Added twice: the same take-off as Musterkogel West.",
      "site": {
        "id": "venti-pilots:0000000000000000000000s004",
        "name": "Musterkogel",
        "position": {
          "lat": 47.305,
          "lon": 10.905
        },
        "elevation_m": 1870.0,
        "uses": [
          "takeoff"
        ],
        "paragliding": true,
        "description": "A made-up site of the development mode. No such place exists."
      }
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `id` | The site |
| `revision` | The number of its latest revision |
| `removed` | Whether it is removed |
| `removal` | For a removed site: when it was removed and by whom, as [`GET /v1/sites/changes`](#get-v1siteschangeslayersincelimit) says it in `removals` |
| `revisions` | Every revision by number, each as [the site's history](#get-v1sitesidhistoryafterlimit) has it. Empty for a site that whoever runs the server took down, whose `removal` has no `name` either |

A line can be long: a site with many revisions holds its whole record once for each.

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
  "fetched_at": "2026-11-21T09:30:00Z",
  "layers": [
    {
      "source": "open-meteo",
      "series": [
        {
          "source": "open-meteo",
          "model": "icon-d2",
          "run": "2026-11-21T06:00:00Z",
          "available_at": "2026-11-21T07:09:12Z",
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
              "time": "2026-11-21T07:00:00Z",
              "wind_speed_ms": 2.59,
              "wind_direction_deg": 242.0,
              "wind_gust_ms": 15.5,
              "temperature_c": 1.3,
              "dew_point_c": 0.9,
              "cloud_cover_pct": 100.0,
              "cloud_cover_low_pct": 100.0,
              "cloud_cover_mid_pct": 100.0,
              "cloud_cover_high_pct": 0.0,
              "precipitation_mm": 0.5,
              "cape_j_kg": 0.0,
              "freezing_level_m": 1210.0,
              "updraft_ms": 0.5,
              "heat_to_air_w_m2": 0.0,
              "winds_aloft": [
                {
                  "above_ground_m": 80.0,
                  "wind_speed_ms": 4.3,
                  "wind_direction_deg": 252.0,
                  "temperature_c": 0.6
                },
                {
                  "above_ground_m": 120.0,
                  "wind_speed_ms": 4.83,
                  "wind_direction_deg": 257.0,
                  "temperature_c": 0.3
                },
                {
                  "above_ground_m": 180.0,
                  "wind_speed_ms": 5.56,
                  "wind_direction_deg": 262.0,
                  "temperature_c": -0.1
                },
                {
                  "pressure_hpa": 850.0,
                  "altitude_m": 1474.0,
                  "wind_speed_ms": 8.99,
                  "wind_direction_deg": 280.0,
                  "temperature_c": -1.6
                },
                {
                  "pressure_hpa": 700.0,
                  "altitude_m": 2993.0,
                  "wind_speed_ms": 14.44,
                  "wind_direction_deg": 282.0,
                  "temperature_c": -11.1
                },
                {
                  "pressure_hpa": 600.0,
                  "altitude_m": 4161.0,
                  "wind_speed_ms": 17.08,
                  "wind_direction_deg": 297.0,
                  "temperature_c": -18.6
                },
                {
                  "pressure_hpa": 500.0,
                  "altitude_m": 5495.0,
                  "wind_speed_ms": 16.28,
                  "wind_direction_deg": 305.0,
                  "temperature_c": -27.8
                }
              ]
            }
          ]
        },
        {
          "source": "open-meteo",
          "model": "icon-eu",
          "run": "2026-11-21T06:00:00Z",
          "available_at": "2026-11-21T09:14:18Z",
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
              "time": "2026-11-21T07:00:00Z",
              "wind_speed_ms": 1.24,
              "wind_direction_deg": 256.0,
              "wind_gust_ms": 13.5,
              "temperature_c": 1.4,
              "dew_point_c": 1.1,
              "cloud_cover_pct": 100.0,
              "cloud_cover_low_pct": 100.0,
              "cloud_cover_mid_pct": 98.0,
              "cloud_cover_high_pct": 0.0,
              "precipitation_mm": 1.5,
              "cape_j_kg": 0.0,
              "freezing_level_m": 1310.0,
              "convective_cloud_base_m": 1160.0,
              "convective_cloud_top_m": 1270.0,
              "heat_to_air_w_m2": 0.0,
              "winds_aloft": [
                {
                  "above_ground_m": 80.0,
                  "wind_speed_ms": 2.3,
                  "wind_direction_deg": 268.0,
                  "temperature_c": 0.5
                },
                {
                  "above_ground_m": 120.0,
                  "wind_speed_ms": 2.7,
                  "wind_direction_deg": 272.0,
                  "temperature_c": 0.3
                },
                {
                  "above_ground_m": 180.0,
                  "wind_speed_ms": 3.21,
                  "wind_direction_deg": 275.0,
                  "temperature_c": 0.0
                },
                {
                  "pressure_hpa": 850.0,
                  "altitude_m": 1471.0,
                  "wind_speed_ms": 4.82,
                  "wind_direction_deg": 284.0,
                  "temperature_c": -0.7
                },
                {
                  "pressure_hpa": 700.0,
                  "altitude_m": 2995.0,
                  "wind_speed_ms": 16.36,
                  "wind_direction_deg": 288.0,
                  "temperature_c": -10.3
                },
                {
                  "pressure_hpa": 600.0,
                  "altitude_m": 4164.0,
                  "wind_speed_ms": 19.26,
                  "wind_direction_deg": 289.0,
                  "temperature_c": -18.6
                },
                {
                  "pressure_hpa": 500.0,
                  "altitude_m": 5498.0,
                  "wind_speed_ms": 18.36,
                  "wind_direction_deg": 299.0,
                  "temperature_c": -28.0
                }
              ]
            }
          ]
        }
      ]
    }
  ],
  "nearby": {
    "source": "open-meteo",
    "radius_km": 30.0,
    "cells": 4,
    "hours": [
      {
        "time": "2026-11-21T08:00:00Z",
        "model": "icon-d2",
        "run": "2026-11-21T06:00:00Z",
        "wind": {
          "kmh": 7,
          "at": {
            "lat": 47.7,
            "lon": 11.9
          },
          "from_deg": 250,
          "ground_m": 947
        },
        "gust": {
          "kmh": 45,
          "at": {
            "lat": 47.7,
            "lon": 11.8
          },
          "from_deg": 260,
          "ground_m": 1069
        }
      }
    ]
  },
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

The place is the east shore of the Tegernsee. Each series is cut to its first hour here, and so is `nearby`; in the whole answer ICON-D2 has 48 hours, ICON-EU 78 and `nearby` 26.

**The answer holds magnitudes and no judgement.** Nothing in it says whether a day can be flown, and a client that shows it adds none. The server's own reading of the days, cell by cell, is the thermal map ([`GET /v1/thermals`](#get-v1thermals)).

**Layers and series.** One layer for each source that has something for the place, and in a layer one series for each weather model, sorted by model. Nothing is blended: two models are two series, and a client that draws one line for the day chooses a series and says which. A model that does not reach the place, or whose run the source could not tell, is not listed.

| Field | Meaning |
|---|---|
| `point` | The place the forecast is for: the point of the grid nearest to what was asked |
| `fetched_at` | When the server asked its source. An answer is reused for up to half an hour |
| `layers[].source` | The source of the layer. Its credit and licence are the entry of `attributions` with the same `source` |
| `layers[].series` | One series per model |
| `nearby` | The strongest wind and gust the models have around the place, hour by hour: see [below](#the-wind-around-the-place). Left out where the server has none |

A series is the canonical forecast of the project, `venti_domain::ForecastSeries`:

| Field | Meaning |
|---|---|
| `source`, `model` | Where the values are from: `open-meteo`, and `icon-d2` or `icon-eu`. A client expects models it does not know |
| `run` | When the run of the model that computed these hours began. **Show the pilot the run**: the age of a forecast is the age of its run |
| `available_at` | When the source had that run ready. Left out if the source does not say |
| `point` | The same as the answer's |
| `cell` | The centre of the model's grid cell the values are for. It can lie some kilometres from `point` |
| `cell_elevation_m` | The height of the model's ground in that cell. **Show it beside the values.** A model's ground is smoothed: east of the Tegernsee, whose shore lies at 725 m, ICON-D2's ground is at 1069 m, and "10 m above ground" is 10 m above that. Left out where the cell is one the model counts as water, a cell of the sea or of a lake as large as the model's cells: the source has no height for it. The values are then the model's for the air over the water, and nothing can be placed above or under the ground |
| `hours` | One record per hour, oldest first |

**Only the hours the run computed.** A series holds the hours after its run began, as far as the run reaches: 48 hours for ICON-D2; for ICON-EU 78 hours after a run of 00, 06, 12 or 18 UTC and 30 hours after one in between. The source has older runs' values for other hours and does not say which run they are from, so they are left out. A series therefore never holds the hours of today that are over, and where it ends is the run's doing. ICON-D2's ends tomorrow or on the day after. ICON-EU's ends on the third day after today, at 06, 12 or 18 UTC or with that day, while its newest run is one of 00, 06, 12 or 18 UTC, which is between two and three hours out of every six; after a run in between it ends tomorrow, or in the night after it. So what lies beyond ICON-D2's hours is ICON-EU's alone, and it is there only while ICON-EU's newest run is one of those four. A client shows the hours it has and does not fill the rest.

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
| `freezing_level_m` | Height of the 0 °C level above sea level. A value at or under `cell_elevation_m` means the air is below freezing down to the ground: the model then gives a height under its own ground ([data-sources.md](data-sources.md#the-values)) |
| `updraft_ms` | The strongest rising air the model has anywhere above the cell up to 10 km. ICON-D2 only. **It is not thermal strength**: it is what a model with 2 km cells resolves, which is showers, storms and flow over mountains |
| `convective_cloud_base_m`, `convective_cloud_top_m` | Above sea level. Left out when the model has no convective cloud over the cell at that hour, which is most hours. The two models do not mean the same convection by it; [data-sources.md](data-sources.md#the-values) says what each means |
| `heat_to_air_w_m2` | The heat the ground gives to the air above it, as the mean of the hour before `time`: the model's sensible heat flux, counted upward. Positive when the ground warms the air, which is what thermals are made of; negative when the air warms the ground, as on a clear night. The apps work out the height and strength of thermals from it ([design.md](design.md#thermals)); the server hands on the figure and no estimate |
| `winds_aloft` | The wind higher up, lowest first: see below |

A level of `winds_aloft` is given in one of two ways:

| Field | Meaning |
|---|---|
| `above_ground_m` | A height above the model's ground: 80, 120 and 180 m. These are nominal heights of the model's own levels |
| `pressure_hpa`, `altitude_m` | A surface of equal pressure, 950, 850, 700, 600 or 500 hPa, with the height above sea level the model gives it at this hour. A surface that lies under the model's ground is left out: at Tegernsee that is the one of 950 hPa |
| `wind_speed_ms`, `wind_direction_deg`, `temperature_c` | At the level |

Every value but `time` may be missing, and a missing value is left out, never written as `null` or zero. Between 180 m above the ground and the pressure surfaces, which lie 1.5 km apart, the answer has nothing.

<a id="the-wind-around-the-place"></a>**The wind around the place.** A foehn two valleys away is in the model's cells around a place before it is in the place's own. So the answer carries, in `nearby`, the strongest mean wind and the strongest gust among the cells of the server's thermal map ([`GET /v1/thermals`](#get-v1thermals)) whose centres lie within `radius_km` of `point`, for every hour the map keeps from the first hour of the answer's series on. They are read from what the map already holds, the same values as `GET /v1/thermals/hours?layer=wind` serves: the forecast service is not asked again. Like the rest of the answer they are magnitudes: whether that is much against the place's own wind is the reader's rule (the apps' is in [design.md](design.md#thermals)).

| Field | Meaning |
|---|---|
| `nearby.source` | The source whose models these are; its credit is in `attributions` |
| `nearby.radius_km` | How far from `point` the centre of a cell may lie: 30 km |
| `nearby.cells` | How many cells of the map lie within that distance. Some 34 in the middle of a region, fewer at its edge; four in the development mode, whose map has four |
| `nearby.hours[].time` | The moment the values are for. Daylight hours only, an hour after sunrise to an hour before sunset: the map keeps no others |
| `nearby.hours[].model`, `.run` | The model the map has that hour's day from, and its run. A day of the map is one model's ([below](#get-v1thermals)); it need not be the model a client reads the place's own hour from |
| `nearby.hours[].wind`, `.gust` | The strongest mean wind 10 m above the model's ground among those cells, and the strongest gust. Each is left out when no cell has one in the hour, and an hour with neither is not listed |
| `.kmh` | Its speed in whole km/h, as the map keeps it |
| `.at` | The centre of the cell that has it; of equally strong ones the nearest to `point` |
| `.from_deg` | Where the mean wind of that cell comes from, to the nearest ten degrees. Left out when the model gives none |
| `.ground_m` | The height of the model's ground in that cell. **Show it with the wind**: a cell on a ridge has more wind than one in a valley on any day. Left out for a cell of water |

`nearby` is left out, never empty or `null`: by a server that makes no thermal map, until its map is made after a start, and for a place none of the map's cells lies near. It is as coarse as the map: a cell of the map is 11 by 7.5 km and stands for the one cell of the model nearest its centre, one of some twenty of ICON-D2's, so a narrow valley's wind can be missing from it.

**What it does not have.** No boundary-layer height, no thermal strength, no cloud base for ordinary cumulus. No model the server has computes a climb rate a glider sees, and the answer holds the models' figures only. What the apps and the assistant's `forecast_at` tool say of thermals is an estimate they work out from this answer, from `heat_to_air_w_m2` and the temperatures ([design.md](design.md#thermals)). [data-sources.md](data-sources.md#forecasts) says what the source lacks and where it could come from.

**Credit.** Show the credit of the layer's source next to the forecast, with its link: Open-Meteo asks for that wherever its data is shown.

**Caching.** `Cache-Control: public, max-age=60` and an `ETag`, like the stations. The server itself keeps an answer for up to half an hour, so asking more often than that brings the same answer and a `304`.

**Size.** A whole answer for Tegernsee, two models and 126 hours, of which 78 are of a main run of ICON-EU, is 146 kB plain and 14 kB with gzip (measured 2026-11-21 over the recorded answers). Of that `nearby` is 6.1 kB plain for its 26 hours, some 240 bytes an hour; in summer a day has up to 14 such hours, so four days are some 13 kB plain. After a run of ICON-EU in between, that model's series has 30 hours at most and the answer is smaller.

**When there is no forecast.**

| Status | `error.code` | When |
|---|---|---|
| `400` | `invalid_position` | `lat` or `lon` is missing, is not a number, or the two are not a place on the globe |
| `404` | `outside_region` | The place lies outside the area the server has forecasts for: its regions |
| `429` | `rate_limited` | This address has asked for the forecasts of 12 places within an hour that the server had to fetch, which is as many as one address may; for an IPv6 client the address is its /64 network, and one /48 network together gets 48. `Retry-After` says when the hour is over. Places that are at hand, which is every place anyone asked for in the last half hour, are still answered, and are never counted |
| `501` | `no_forecasts` | The server was not told where its forecast service is. `/v1/health` says the same with `"forecasts": "not_configured"`. Asking again does not help, and the server asks nobody else |
| `503` | `unavailable` | No forecast for the place is at hand and none could be fetched: the forecast service did not answer, the forecast is still being fetched (the first one for a place can take longer than a request waits), or the server has asked its service as often as it may in an hour. `Retry-After` says when to ask again |

**Why a client is counted.** The server may ask its forecast service 400 times an hour, which is some ninety places, for everyone together. The thermal map ([below](#get-v1thermals)) takes 46 of them with each run of ICON-D2 and 46 with each run of ICON-EU, each model a run every three hours, and never more than half of an hour's. A forecast that is at hand costs nothing; one that has to be fetched comes out of that. An app that shows forecasts asks for the place the pilot opens, not for every place on the map. A request that is refused because the server has asked its service often enough (`503`) is not counted against the client.

An answer in which one of the models is missing is still an answer: the server hands on what it has and asks again within two minutes.

**In the development mode** there are recorded forecasts for five places: the east shore of the Tegernsee (47.70, 11.80), the take-off on the Wallberg (47.66, 11.80), the take-off and the landing below the Jägerkamp (47.68, 11.90 and 47.70, 11.88), and the middle of the first region (47.40, 13.30). A place anywhere in the first region is answered with the forecast of the nearest of them, **and `point` says which one that is**; a real server answers for the place asked. The forecasts were recorded on 2026-11-21 at 09:25 UTC, three days after that mode's wind, and the `fetched_at` of every answer is 09:30 UTC. Both models are at their runs of 06:00 UTC of that day, and ICON-EU's is one of its main runs. ICON-D2's hours run from 07:00 UTC on 21 November to 06:00 UTC on the 23rd, ICON-EU's from 07:00 UTC on the 21st to 12:00 UTC on the 24th. By daylight that is the 21st and the 22nd from both models, and from ICON-EU alone the 23rd and the 24th until noon UTC, a day that is not whole. The first day with daylight hours is 21 November.

## `GET /v1/thermals`

The server's thermal map: for every cell of a grid over the area it has forecasts for, whether each day can be flown by the air alone, what kind of day its thermals make, the climb to expect and how high they go. It takes no parameters; the answer is the whole map.

The server works the map out itself, with each run of ICON-D2 and of ICON-EU, from the model's heat, temperatures and wind, by the same fixed rules an app applies to a take-off's forecast (`venti_flight::day`, [design.md](design.md#thermals)): one answer for a region instead of some 1,800 forecasts for one place on every phone. A cell is judged at the height of the model's ground there, a take-off at its own, so the two can differ where a take-off lies far above the valley. **It is a reading of a model for a square some ten kilometres wide, and no statement that any place in it can be flown: show the model and its run with it.** [architecture.md](architecture.md#the-thermal-map) says how the map is made and kept.

```bash
curl 'http://127.0.0.1:8080/v1/thermals'
```

The answer of the development mode, whole:

```json
{
  "source": "open-meteo",
  "model": "icon-d2",
  "run": "2026-11-21T06:00:00Z",
  "computed_at": "2026-11-21T09:30:00Z",
  "grid": {
    "south": 47.6,
    "west": 11.8,
    "step_deg": 0.1,
    "rows": 2,
    "columns": 2
  },
  "ground_m": [
    1118,
    1046,
    1069,
    947
  ],
  "heights_m": [
    1000,
    1500,
    2000,
    2500,
    3000,
    4000
  ],
  "days": [
    {
      "date": "2026-11-21",
      "model": "icon-d2",
      "run": "2026-11-21T06:00:00Z",
      "whole": true,
      "verdict": [
        "not_flyable",
        "not_flyable",
        "not_flyable",
        "not_flyable"
      ],
      "why": [
        "rain",
        "rain",
        "rain",
        "rain"
      ],
      "thermals": [
        null,
        null,
        null,
        null
      ],
      "climb_ms": [
        null,
        null,
        null,
        null
      ],
      "top_m": [
        null,
        null,
        null,
        null
      ],
      "cloud": [
        false,
        false,
        false,
        false
      ]
    },
    {
      "date": "2026-11-22",
      "model": "icon-d2",
      "run": "2026-11-21T06:00:00Z",
      "whole": true,
      "verdict": [
        "flyable",
        "flyable",
        "flyable",
        "flyable"
      ],
      "why": [
        null,
        null,
        null,
        null
      ],
      "thermals": [
        "weak",
        "weak",
        "weak",
        "weak"
      ],
      "climb_ms": [
        0.0,
        0.0,
        0.0,
        0.0
      ],
      "top_m": [
        null,
        null,
        null,
        950
      ],
      "cloud": [
        false,
        false,
        false,
        true
      ]
    },
    {
      "date": "2026-11-23",
      "model": "icon-eu",
      "run": "2026-11-21T06:00:00Z",
      "whole": true,
      "verdict": [
        "flyable",
        "flyable",
        "not_flyable",
        "not_flyable"
      ],
      "why": [
        null,
        null,
        "rain",
        "rain"
      ],
      "thermals": [
        "weak",
        "weak",
        null,
        null
      ],
      "climb_ms": [
        0.0,
        0.0,
        null,
        null
      ],
      "top_m": [
        1900,
        1850,
        null,
        null
      ],
      "cloud": [
        true,
        true,
        false,
        false
      ]
    },
    {
      "date": "2026-11-24",
      "model": "icon-eu",
      "run": "2026-11-21T06:00:00Z",
      "whole": false,
      "verdict": [
        "flyable",
        "flyable",
        "flyable",
        "flyable"
      ],
      "why": [
        null,
        null,
        null,
        null
      ],
      "thermals": [
        "weak",
        "weak",
        "weak",
        "weak"
      ],
      "climb_ms": [
        0.0,
        0.0,
        0.0,
        0.0
      ],
      "top_m": [
        1600,
        1500,
        1350,
        1500
      ],
      "cloud": [
        false,
        false,
        false,
        false
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

| Field | Meaning |
|---|---|
| `source`, `model` | Where the values are from: `open-meteo`, and the finest model the map is read from, `icon-d2`. A day names the model it is read from itself. The credit is the entry of `attributions` with the same `source` |
| `run` | When the newest run of that model began that the map holds hours of. Earlier hours of a day can be an earlier run's: a run says nothing of the hours before it began |
| `computed_at` | When the server worked the map out |
| `grid` | Where the cells are: see below |
| `ground_m` | The height of the model's ground at each cell, whole metres above mean sea level, which is the height the cell is judged at. `null` where the model's answer for the cell is missing, and where the model's cell is water: such a cell is judged by the wind at the ground, the gusts and the rain alone, and has no thermals |
| `heights_m` | The heights above mean sea level at which the hours of the map's days have the wind ([`GET /v1/thermals/hours`](#get-v1thermalshoursdatelayerheight)), whole metres, the lowest first. Since 2026-11-21; a server older than that leaves it out and has no hours to ask for, which is how a client tells |
| `days` | The days the map has, oldest first: from yesterday, while the server has run for a day, to the day after tomorrow and, once a run of ICON-EU of 00, 06, 12 or 18 UTC has reached into its daylight, the day after that, which is ICON-EU's |

**The grid.** Cell number `row * columns + column` is the square of `step_deg` degrees (0.1°: some 11 km north to south and 7.5 km east to west in the Alps) around the point at latitude `south + row * step_deg` and longitude `west + column * step_deg`. The first cell is the south-west one; cells run west to east, rows south to north. The points lie on multiples of the step, inside the area of the server's regions; the model's values for a cell are those of its own cell (about 2 km wide) nearest the point, one sample out of the twenty or so in the square. For the first region the grid has 23 rows of 79 cells, 1,817 in all, from 46.3 N 9.4 E to 48.5 N 17.2 E.

A day holds its values in lists as long as the grid has cells, in the grid's order:

| Field | Meaning |
|---|---|
| `date` | The day, by the sun at the cells: from one midnight by the sun to the next |
| `model` | The model the day is read from: `icon-d2` or `icon-eu`. A day is one model's, never a blend: the finest model's, unless a coarser one has more of the day's hours over the cells, which is the day the finest model's run ends in and the days after it. ICON-D2 reaches 48 hours, so the day after tomorrow is mostly ICON-EU's and the day after that always: a coarser reading, from cells some 7 km wide where ICON-D2's are 2 km. **Show the pilot the day's model and run.** Since 2026-11-21; a server older than that leaves it out, and the day is then the answer's `model`'s |
| `run` | When the newest run of the day's model began that the day holds hours of. Left out where `model` is: the answer's `run` |
| `whole` | Whether every cell that has a verdict was judged with all of the day's daylight hours. `false` on a day the runs at hand do not reach the whole of, which is mostly the last day of the map: a run of ICON-EU of 00, 06 or 12 UTC ends on the third day after today at 06, 12 or 18 UTC, which can be before that day's daylight does, and the run of 18 UTC has all of it. And `false` on today after a start of the server, when the hours before the run began are missing. Such a day is judged by the hours there are, which can make it look worse than it is |
| `verdict` | Per cell `flyable`, `marginal` or `not_flyable`, by the air alone; `null` where the day has no hours for the cell |
| `why` | Per cell what speaks against the day: `rain`, `wind` (the mean wind 10 m above the model's ground), `gusts`, `wind_aloft` (the wind at the model's levels within 1,500 m above the ground), `cloud_base` (convective cloud with its base at or just above the ground) or `storms` (air in which showers or thunderstorms can grow). It is the thing met most often among the hours that are no better than the day's verdict, and of equally frequent ones the earliest. `null` on a flyable day and where the day has no hours. Since 2026-11-21; a server older than that leaves the list out, and a client then shows no reason |
| `thermals` | Per cell the kind of day: `weak`, `local`, `cross_country` or `big`. `null` on a day that is not flyable, and where there is no estimate |
| `climb_ms` | Per cell the climb to expect, m/s to a tenth: the mean over the hours that make the kind of day, or the best hour's on a day of weak thermals. `null` where `thermals` is |
| `top_m` | Per cell how high the thermals go, metres above mean sea level to the nearest fifty. `null` without thermals |
| `cloud` | Per cell whether that height is a base of cumulus; `false` in the blue and without thermals |

A client expects values of `verdict` and `thermals` it does not know, and fields it does not know.

**Across a restart.** The server writes down what it judged of the cells each time it makes a model's part of the map, and reads it back when it starts: the map is there at once, and today keeps the hours before the newest run began. A server that starts with nothing written down (the first start, or the development mode, which has no database) makes the map from the runs its forecast service holds, which takes a minute or two, and today then lacks the hours before those runs began: its `whole` is `false` until the next day. Until the first map is made the endpoint answers `503`.

**Credit.** Show the credit of the source next to the map, with its link.

**Caching.** `Cache-Control: public, max-age=60` and an `ETag`, like a forecast. The map changes with each run of the model, every three hours, so a client that asks again with `If-None-Match` mostly gets a `304`.

**Size.** The map of the first region made from the run of ICON-D2 of 2026-11-21 00:00 UTC, 1,817 cells and two days, was 126 kB plain and 10 kB with gzip (measured 2026-11-21 from answers of an instance for the whole grid). A day more adds about a third: with ICON-EU's day after tomorrow and yesterday, four days, some 250 kB plain (an estimate from that measurement; ICON-EU's grid was not fetched whole). Since 2026-11-21 a map can hold a fifth day, the third after today; a map of five days has not been measured (*to verify* on an installation that has run for a day). The server compresses it for a client that sends `Accept-Encoding: gzip`, as every answer of the API.

**When there is no map.**

| Status | `error.code` | When |
|---|---|---|
| `501` | `no_forecasts` | The server was not told where its forecast service is, so it has no map. Asking again does not help |
| `503` | `unavailable` | The server has a forecast service and no map yet: after its first start, or while the service does not answer. `Retry-After` says when to ask again (60 seconds) |

**In the development mode** the map is made, by the code a real server runs, from answers recorded on 2026-11-21 at 09:25 UTC for four points around the Tegernsee and the Spitzingsee, a grid of two rows of two cells from 47.6 N 11.8 E; the map's `computed_at` is 09:30 UTC. The runs are those of the mode's forecasts for a place: ICON-D2's and ICON-EU's of 06:00 UTC of that day. The map has four days. The 21st and the 22nd are ICON-D2's, whose run reaches 06:00 UTC on the 23rd, and both are whole. The 23rd and the 24th are ICON-EU's, whose run is a main run and reaches 12:00 UTC on the 24th: the 23rd is whole, and the 24th has the hours until noon and is not (`"whole": false`). They are days of late November: not flyable for rain on the 21st, weak thermals on the 22nd, on the 23rd weak thermals in the two southern cells and rain in the two northern, and weak thermals on the 24th. The made-up summer day of the UI tests (`tools/made-up-day/serve.py`) answers this endpoint with a made-up map of all four kinds.

## `GET /v1/thermals/hours?date=&layer=&height=`

One day of the thermal map hour by hour, a layer at a time: for every daylight hour of the day and every cell of the map's grid, the hour's verdict and what its thermals count as, or what the model has in that hour of wind, of cloud or of rain. The day's verdict in [`GET /v1/thermals`](#get-v1thermals) is made of these hours; the wind, the cloud and the rain are the model's own values for the same hours, which the server keeps beside its verdicts since 2026-11-21 ([architecture.md](architecture.md#the-thermal-map)).

| Parameter | Meaning |
|---|---|
| `date` | The day, written `YYYY-MM-DD`: one of `days[].date` of `GET /v1/thermals`. Needed |
| `layer` | What of the hours to answer with: `thermals` (the hours' verdicts and thermals), `wind`, `cloud` or `rain`. Left out: `thermals` |
| `height` | With `layer=wind` only: the height above mean sea level to read the wind at, whole metres, one of the map's `heights_m` (1000, 1500, 2000, 2500, 3000 and 4000; the set is ours). Left out: the wind 10 m above the model's ground, with its gusts |

**The hours are daylight hours, and no others.** The map judges, and keeps, the hours from an hour after sunrise to an hour before sunset, by the sun at each cell (`venti_flight::day::daylight_hours`): the first and the last hour of the sun are left to no judgement by wind and cloud alone. Seven hours in late November at 47° N, fourteen in July. The wind at night, a morning's first hour and an evening's last are not in the answer; `GET /v1/forecast` has every hour for one place.

**It is one cell of the model for a square of 11 by 7.5 km.** The map asks the model at one point per square, and the model answers from its own cell nearest it: one of some twenty cells of ICON-D2 that lie in the square, and for the days ICON-EU is read for, a cell about as wide as the square. A valley's own wind, a pass, a lee: none of it is in a square. Read it as where the model has wind, cloud and rain over a region, and read a take-off's own forecast before going.

```bash
curl 'http://127.0.0.1:8080/v1/thermals/hours?date=2026-11-22&layer=wind&height=1000'
```

The answer of the development mode, whole: the wind at 1000 m above the sea on the 22nd. Three of its four cells have their ground above 1000 m and no wind there.

```json
{
  "source": "open-meteo",
  "date": "2026-11-22",
  "layer": "wind",
  "height_m": 1000,
  "heights_m": [
    1000,
    1500,
    2000,
    2500,
    3000,
    4000
  ],
  "model": "icon-d2",
  "run": "2026-11-21T06:00:00Z",
  "whole": true,
  "computed_at": "2026-11-21T09:30:00Z",
  "grid": {
    "south": 47.6,
    "west": 11.8,
    "step_deg": 0.1,
    "rows": 2,
    "columns": 2
  },
  "ground_m": [
    1118,
    1046,
    1069,
    947
  ],
  "hours": [
    {
      "time": "2026-11-22T08:00:00Z",
      "wind_kmh": [
        null,
        null,
        null,
        2
      ],
      "wind_from_deg": [
        null,
        null,
        null,
        70
      ]
    },
    {
      "time": "2026-11-22T09:00:00Z",
      "wind_kmh": [
        null,
        null,
        null,
        2
      ],
      "wind_from_deg": [
        null,
        null,
        null,
        70
      ]
    },
    {
      "time": "2026-11-22T10:00:00Z",
      "wind_kmh": [
        null,
        null,
        null,
        3
      ],
      "wind_from_deg": [
        null,
        null,
        null,
        50
      ]
    },
    {
      "time": "2026-11-22T11:00:00Z",
      "wind_kmh": [
        null,
        null,
        null,
        4
      ],
      "wind_from_deg": [
        null,
        null,
        null,
        40
      ]
    },
    {
      "time": "2026-11-22T12:00:00Z",
      "wind_kmh": [
        null,
        null,
        null,
        3
      ],
      "wind_from_deg": [
        null,
        null,
        null,
        40
      ]
    },
    {
      "time": "2026-11-22T13:00:00Z",
      "wind_kmh": [
        null,
        null,
        null,
        2
      ],
      "wind_from_deg": [
        null,
        null,
        null,
        20
      ]
    },
    {
      "time": "2026-11-22T14:00:00Z",
      "wind_kmh": [
        null,
        null,
        null,
        3
      ],
      "wind_from_deg": [
        null,
        null,
        null,
        350
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

| Field | Meaning |
|---|---|
| `source` | Where the values are from. The credit is the entry of `attributions` with the same `source` |
| `date` | The day, by the sun at the cells |
| `layer` | The layer the hours hold: `thermals`, `wind`, `cloud` or `rain` |
| `height_m` | For the wind at a height above mean sea level: that height. Left out for the wind 10 m above the model's ground, and for the other layers |
| `heights_m` | The heights the wind can be asked for at, the lowest first: the same in every answer, and in `GET /v1/thermals` |
| `model`, `run` | The model the day is read from, which is the one the day of `GET /v1/thermals` names, and when the newest run of it began that the day holds hours of. Every hour of the answer is that model's, and an earlier hour of the day can be an earlier run's. **Show the pilot both** |
| `whole` | Whether every cell has all of the day's daylight hours, as `days[].whole` of `GET /v1/thermals`. On a day that is not whole the answer ends where the model's run does, or begins where it began |
| `computed_at` | When the server worked the map out: the same moment as the map of the days it belongs to |
| `grid` | Where the cells are, as in [`GET /v1/thermals`](#get-v1thermals) |
| `ground_m` | The height of this model's ground at each cell, whole metres above mean sea level: the height the cell's hours are judged at, 10 m under its wind at the ground, and what a height has to lie above to have a wind. On a day that is ICON-EU's it is ICON-EU's ground, which is not the `ground_m` of `GET /v1/thermals` (that is the finest model's). `null` where the model's cell is water |
| `hours` | The day's daylight hours that any cell has, oldest first. Daylight differs by some minutes across a region, so an hour at the day's edge can be one cell's and not another's |

An hour has its `time` and the lists of the layer that was asked for, each as long as the grid has cells and in the grid's order. The lists of the other layers are left out. A cell has `null` (in `cloud`, `false`) in an hour that is none of its own daylight hours or that the model's runs at hand do not hold for it.

| Layer | Field | Meaning |
|---|---|---|
| all | `time` | The moment the hour's values are for. Rain and gusts are those of the hour before it, as the model gives them |
| `thermals` | `verdict` | Per cell `flyable`, `marginal` or `not_flyable`, by the air alone, for this hour |
| `thermals` | `why` | Per cell the worst thing the air has against the hour, with the names of `GET /v1/thermals`; of two equally bad the first of wind, gusts, wind higher up, cloud base, storms. `null` in a flyable hour |
| `thermals` | `thermals` | Per cell what the hour's thermals count as: `weak`, `local`, `cross_country` or `big`, by their climb, how far above the ground they reach and the wind in them. Never more than `local` in a marginal hour. `null` in an hour that is not flyable, and where there is no estimate |
| `thermals` | `climb_ms`, `top_m`, `cloud` | Per cell the climb to expect in the hour's thermals, m/s to a tenth; how high they go, metres above mean sea level to the nearest fifty; and whether that height is a base of cumulus. As in a day of `GET /v1/thermals`, of the one hour |
| `wind` | `wind_kmh` | Per cell the wind's mean speed, **whole km/h**: 10 m above the model's ground, or at `height_m` |
| `wind` | `wind_from_deg` | Per cell the direction it comes from, degrees **to the nearest ten**, 0 to 350 |
| `wind` | `gust_kmh` | Per cell the strongest gust of the hour 10 m above the model's ground, whole km/h. Left out at a height: the model has gusts at the ground alone |
| `cloud` | `cloud_cover_pct` | Per cell the share of the sky covered by cloud at any height, whole percent. A thin high veil counts as much as a low layer; the cover by height is in `GET /v1/forecast` |
| `rain` | `precipitation_mm` | Per cell the rain, showers and snow of the hour, millimetres of water to a tenth. The hour's verdict is `not_flyable` from 0.2 mm, weighed before the rounding |

**The wind at a height is read between the model's levels.** The model has its wind 10 m above its ground, at 80, 120 and 180 m above it, and on the surfaces of 950, 850, 700, 600 and 500 hPa, whose heights above the sea it gives hour by hour (some 500, 1,500, 3,000, 4,200 and 5,600 m). For a height the server takes the level below and the level above and reads between them: the speed in a straight line by height, the direction along the shorter way round the compass (`venti_flight::aloft`; [architecture.md](architecture.md#the-thermal-map) says why so). Between 850 and 700 hPa that is a reading across some 1,500 m, in which a shear or an inversion's own wind is not seen. Nothing is read beyond the levels: the value is `null` where the height lies under the model's ground (`ground_m` says how high that is), where it lies above the model's highest level, and where the model gives no height of its ground. A surface of pressure that lies under the model's ground is no level. From the ground up to 10 m above it the wind is the one at 10 m.

**Units.** Whole km/h, tens of degrees, whole percent and tenths of a millimetre, where `GET /v1/forecast` has metres per second and the model's own figures: a layer of a region's day is some 25,000 values for each of its lists, and a map shows no more of them than whole numbers.

A client expects values of `layer`, `verdict` and `thermals` it does not know, and fields it does not know.

**Credit.** Show the credit of the source next to the layer, with its link.

**Caching.** `Cache-Control: public, max-age=60` and an `ETag`, as the map of the days. A layer of a day changes when the map does, with a run of the day's model.

**Size, and why a layer at a time.** Not measured on an instance: the development mode has four cells. Measured instead over the first region's grid of 1,817 cells with made-up weather of a July day of fourteen daylight hours, made twice: as fields that change slowly from cell to cell, and with every value drawn by itself from its range, which packs worst (`a_regions_hours_weighed` in `crates/server/src/thermals.rs`). What a model has lies between the two, the wind at the ground nearer the second. *To verify* on the owner's installation.

| Answer | Plain | With gzip, smooth fields | With gzip, every value by itself |
|---|---|---|---|
| `layer=thermals` | 1.0 MB | 42 kB | 86 kB |
| `layer=wind` (the ground, with gusts) | 250 kB | 42 kB | 74 kB |
| `layer=wind&height=` one height | 180 to 230 kB | 26 to 34 kB | 33 to 57 kB |
| `layer=cloud` | 86 kB | 17 kB | 30 kB |
| `layer=rain` | 112 kB | 13 kB | 12 kB |
| everything in one answer for the day (not served) | 2.6 MB | 278 kB | 484 kB |

A day in late November has half the hours and half the size. One answer with every layer of a day was the other choice, and is five to ten times what a pilot looks at in one go; so a client asks for the layer it shows, and for another when the pilot switches. Directions in tens of degrees (0 to 35) in place of degrees would save a twentieth, and were left as degrees.

**When there is no answer.**

| Status | `error.code` | When |
|---|---|---|
| `400` | `invalid_day` | `date` is missing or is not a date written `YYYY-MM-DD` |
| `400` | `invalid_request` | `layer` is none of the four; `height` is none of the map's heights; or `height` is given with a layer other than `wind` |
| `404` | `not_found` | The map has no hours for this day. `message` lists the days it has |
| `501` | `no_forecasts` | The server has no forecast service, and no map |
| `503` | `unavailable` | The server has no map yet. `Retry-After` says when to ask again (60 seconds) |

**In the development mode** the hours are those of the recorded map's four days, by the code a real server runs: seven hours a day, from 08:00 to 14:00 UTC, on the 21st to the 23rd, and five on the 24th, which ends with ICON-EU's run at noon. The made-up summer day of the UI tests (`tools/made-up-day/serve.py`) answers this endpoint for each day of its made-up map, with eleven hours a day and something in every layer.

## `GET /v1/thermals/flown?bbox=&month=&hours=`

Where thermals have been: the cells of a box in which the public flights of several pilots climbed. This is Venti's own map, made from the public flights the server holds and under their terms (`terms`, as in [the public flights](#public-flights)); it is not the forecast of [`GET /v1/thermals`](#get-v1thermals), and it is nobody else's thermal map. Anyone may ask. Since 2026-11-22.

- **A cell needs three pilots.** A cell of the grid of 0.005° (about 560 m by 370 m at 47° north) is handed out only when the climbs of at least `min_pilots`, 3, different pilots lie in it, counted after `month` and `hours` are applied: one pilot's climbs say where that pilot flies. The rule is the one of the [take-off candidates](#get-v1sitescandidatesbboxwestsoutheastnorth). A cell carries no pilot, no flight and no day; its place is the cell's middle and its heights are rounded to 50 m.
- **A climb** is a thermal as the flight analysis finds it (circling for 40 seconds or more) that gained at least 50 m at 0.5 m/s or more on average. It lies in the cell half-way between where the circling began and where it ended.
- **The month and the hour are by the sun** at the place: UTC and four minutes for each degree east, so that noon is when the sun is highest, with no table of time zones. `month=7` is July of any year; `hours=11-15` the climbs that began from 11:00 to before 15:00.
- **Public flights only.** A flight that is made private, taken down or deleted leaves the map with the server's next look at the public flights, five minutes later at most.
- **Kept in memory**, not in the database: after a start the server reads the public flights again, a few at a time, and `flights` says how many it has read; until it has read them all, the map is still filling.

| Parameter | Meaning | Default |
|---|---|---|
| `bbox` | Required; at most 1 degree each way (`invalid_bbox`) | |
| `month` | A month of the year, 1 to 12 (`invalid_time` otherwise) | The whole year |
| `hours` | From an hour of 0 to 23 to a later one, at most 24, by the sun (`invalid_time` otherwise) | The whole day |

**The bound.** It counts as a read of public flights for the address that asks: [600 an hour](#how-often-an-address-may-read). The answer may be kept for a minute.

```bash
curl 'http://127.0.0.1:8080/v1/thermals/flown?bbox=11.6,47.55,12.0,47.85'
```

The development mode's eleven public flights are the sample flight, flown by six made-up pilots; around the Tegernsee three of them climbed in the same two cells:

```json
{
  "cells": [
    {
      "position": {
        "lat": 47.6725,
        "lon": 11.7925
      },
      "climbs": 5,
      "pilots": 3,
      "climb_ms": 2.4,
      "best_climb_ms": 2.4,
      "from_m": 1500,
      "to_m": 2100
    },
    {
      "position": {
        "lat": 47.6875,
        "lon": 11.7525
      },
      "climbs": 5,
      "pilots": 3,
      "climb_ms": 1.5,
      "best_climb_ms": 1.5,
      "from_m": 1700,
      "to_m": 1950
    }
  ],
  "more": false,
  "cell_deg": 0.005,
  "min_pilots": 3,
  "flights": 11,
  "terms": {
    "text": "These flights are their pilots'. They are shown here for reading. Reusing a flight (copying it elsewhere, publishing it, building on it) needs its pilot's leave until Venti's terms say otherwise, and a flight its pilot has withdrawn is to be removed from every copy.",
    "decided": false
  }
}
```

| Field | Meaning |
|---|---|
| `cells[]` | The most pilots first. `climbs` and `pilots`: how many climbs of how many pilots. `climb_ms` and `best_climb_ms`: their average and the best, to a tenth. `from_m`, `to_m`: the heights a climb began and ended at, on average |
| `more` | `true` when the box holds more than 2,000 cells |
| `month`, `hours` | What was asked for; left out for the whole year and the whole day |
| `flights` | How many public flights the server has read for the map |
| `terms` | What the flights it is made from are given under; not decided yet, as for every answer made from public flights |

## `GET /v1/route/weather?date=&points=`

The forecast along a route: for each point of a route and each daylight hour of one day, what the thermal map holds for the cell the point lies in, every layer of [`GET /v1/thermals/hours`](#get-v1thermalshoursdatelayerheight) at once: the hour's verdict and thermals, the wind 10 m above the model's ground with its gusts, the wind at each height the map keeps, cloud and rain. It is read from the hours the map serves, so it says what the map says and costs no request to the forecast's source, however many routes are asked for. What a pilot meets on the way, when, and at which height, the client works out from it: the core does it for the apps (`crates/core/src/route_weather.rs`, [design.md](design.md#plan)).

| Parameter | Meaning |
|---|---|
| `date` | The day, written `YYYY-MM-DD`: one of `days[].date` of `GET /v1/thermals`. Needed |
| `points` | The route's points in the order they are flown, each `lat,lon` in WGS84 degrees, joined by `;`: `47.6642,11.7967;47.62,11.91`. One to **25** points, a bound and not a setting. Needed |

**Why a `GET`.** The answer is the same for everyone who asks for the same points on the same day, so it is cached and validated like the map's own answers; and 25 points written to five decimals, about a metre, are some 500 characters of a query string, well inside what any server and proxy takes. A route of more points would need a body, and a `POST` is not cached.

**It is the map's cell, not the point.** Each point is read in the square of the map's grid it lies in, from the one cell of the model the map asked for that square: what [`GET /v1/thermals/hours`](#get-v1thermalshoursdatelayerheight) says of a square holds here. Two points in the same square have the same hours. A point outside the map has no `cell` and no hours.

```bash
curl 'http://127.0.0.1:8080/v1/route/weather?date=2026-11-22&points=47.6642,11.7967;47.62,11.91;47.6603,11.5197'
```

The answer of the development mode, whole: the Wallberg, a place in the square south-east of it, and the Brauneck, which lies west of the four recorded squares and so has no hours.

<details><summary>The answer</summary>

```json
{
  "source": "open-meteo",
  "date": "2026-11-22",
  "model": "icon-d2",
  "run": "2026-11-21T06:00:00Z",
  "whole": true,
  "computed_at": "2026-11-21T09:30:00Z",
  "heights_m": [
    1000,
    1500,
    2000,
    2500,
    3000,
    4000
  ],
  "points": [
    {
      "at": {
        "lat": 47.6642,
        "lon": 11.7967
      },
      "cell": {
        "lat": 47.7,
        "lon": 11.8
      },
      "ground_m": 1069,
      "hours": [
        {
          "time": "2026-11-22T08:00:00Z",
          "verdict": "flyable",
          "thermals": "weak",
          "climb_ms": 0.0,
          "cloud": false,
          "wind_kmh": 3,
          "wind_from_deg": 90,
          "gust_kmh": 12,
          "aloft": [
            {
              "height_m": 1500,
              "kmh": 5,
              "from_deg": 210
            },
            {
              "height_m": 2000,
              "kmh": 19,
              "from_deg": 230
            },
            {
              "height_m": 2500,
              "kmh": 32,
              "from_deg": 250
            },
            {
              "height_m": 3000,
              "kmh": 45,
              "from_deg": 270
            },
            {
              "height_m": 4000,
              "kmh": 47,
              "from_deg": 280
            }
          ],
          "cloud_cover_pct": 100,
          "precipitation_mm": 0.0
        },
        {
          "time": "2026-11-22T09:00:00Z",
          "verdict": "flyable",
          "thermals": "weak",
          "climb_ms": 0.0,
          "cloud": false,
          "wind_kmh": 0,
          "wind_from_deg": 90,
          "gust_kmh": 11,
          "aloft": [
            {
              "height_m": 1500,
              "kmh": 7,
              "from_deg": 210
            },
            {
              "height_m": 2000,
              "kmh": 19,
              "from_deg": 230
            },
            {
              "height_m": 2500,
              "kmh": 32,
              "from_deg": 250
            },
            {
              "height_m": 3000,
              "kmh": 43,
              "from_deg": 270
            },
            {
              "height_m": 4000,
              "kmh": 42,
              "from_deg": 270
            }
          ],
          "cloud_cover_pct": 100,
          "precipitation_mm": 0.0
        },
        {
          "time": "2026-11-22T10:00:00Z",
          "verdict": "flyable",
          "thermals": "weak",
          "climb_ms": 0.0,
          "cloud": false,
          "wind_kmh": 1,
          "wind_from_deg": 60,
          "gust_kmh": 12,
          "aloft": [
            {
              "height_m": 1500,
              "kmh": 6,
              "from_deg": 190
            },
            {
              "height_m": 2000,
              "kmh": 18,
              "from_deg": 220
            },
            {
              "height_m": 2500,
              "kmh": 30,
              "from_deg": 240
            },
            {
              "height_m": 3000,
              "kmh": 41,
              "from_deg": 260
            },
            {
              "height_m": 4000,
              "kmh": 42,
              "from_deg": 260
            }
          ],
          "cloud_cover_pct": 100,
          "precipitation_mm": 0.0
        },
        {
          "time": "2026-11-22T11:00:00Z",
          "verdict": "flyable",
          "thermals": "weak",
          "climb_ms": 0.0,
          "cloud": false,
          "wind_kmh": 1,
          "wind_from_deg": 320,
          "gust_kmh": 11,
          "aloft": [
            {
              "height_m": 1500,
              "kmh": 6,
              "from_deg": 200
            },
            {
              "height_m": 2000,
              "kmh": 17,
              "from_deg": 220
            },
            {
              "height_m": 2500,
              "kmh": 28,
              "from_deg": 240
            },
            {
              "height_m": 3000,
              "kmh": 37,
              "from_deg": 260
            },
            {
              "height_m": 4000,
              "kmh": 30,
              "from_deg": 250
            }
          ],
          "cloud_cover_pct": 100,
          "precipitation_mm": 0.0
        },
        {
          "time": "2026-11-22T12:00:00Z",
          "verdict": "flyable",
          "thermals": "weak",
          "climb_ms": 0.0,
          "top_m": 1200,
          "cloud": true,
          "wind_kmh": 1,
          "wind_from_deg": 280,
          "gust_kmh": 12,
          "aloft": [
            {
              "height_m": 1500,
              "kmh": 5,
              "from_deg": 220
            },
            {
              "height_m": 2000,
              "kmh": 15,
              "from_deg": 240
            },
            {
              "height_m": 2500,
              "kmh": 24,
              "from_deg": 250
            },
            {
              "height_m": 3000,
              "kmh": 32,
              "from_deg": 260
            },
            {
              "height_m": 4000,
              "kmh": 15,
              "from_deg": 250
            }
          ],
          "cloud_cover_pct": 100,
          "precipitation_mm": 0.0
        },
        {
          "time": "2026-11-22T13:00:00Z",
          "verdict": "flyable",
          "thermals": "weak",
          "climb_ms": 0.0,
          "cloud": false,
          "wind_kmh": 2,
          "wind_from_deg": 290,
          "gust_kmh": 13,
          "aloft": [
            {
              "height_m": 1500,
              "kmh": 5,
              "from_deg": 250
            },
            {
              "height_m": 2000,
              "kmh": 12,
              "from_deg": 250
            },
            {
              "height_m": 2500,
              "kmh": 20,
              "from_deg": 250
            },
            {
              "height_m": 3000,
              "kmh": 25,
              "from_deg": 250
            },
            {
              "height_m": 4000,
              "kmh": 15,
              "from_deg": 220
            }
          ],
          "cloud_cover_pct": 100,
          "precipitation_mm": 0.0
        },
        {
          "time": "2026-11-22T14:00:00Z",
          "verdict": "flyable",
          "thermals": "weak",
          "climb_ms": 0.0,
          "cloud": false,
          "wind_kmh": 3,
          "wind_from_deg": 280,
          "gust_kmh": 17,
          "aloft": [
            {
              "height_m": 1500,
              "kmh": 7,
              "from_deg": 280
            },
            {
              "height_m": 2000,
              "kmh": 11,
              "from_deg": 270
            },
            {
              "height_m": 2500,
              "kmh": 15,
              "from_deg": 260
            },
            {
              "height_m": 3000,
              "kmh": 20,
              "from_deg": 250
            },
            {
              "height_m": 4000,
              "kmh": 31,
              "from_deg": 260
            }
          ],
          "cloud_cover_pct": 100,
          "precipitation_mm": 0.0
        }
      ]
    },
    {
      "at": {
        "lat": 47.62,
        "lon": 11.91
      },
      "cell": {
        "lat": 47.6,
        "lon": 11.9
      },
      "ground_m": 1046,
      "hours": [
        {
          "time": "2026-11-22T08:00:00Z",
          "verdict": "flyable",
          "thermals": "weak",
          "climb_ms": 0.0,
          "cloud": false,
          "wind_kmh": 4,
          "wind_from_deg": 110,
          "gust_kmh": 5,
          "aloft": [
            {
              "height_m": 1500,
              "kmh": 7,
              "from_deg": 160
            },
            {
              "height_m": 2000,
              "kmh": 18,
              "from_deg": 200
            },
            {
              "height_m": 2500,
              "kmh": 29,
              "from_deg": 230
            },
            {
              "height_m": 3000,
              "kmh": 40,
              "from_deg": 270
            },
            {
              "height_m": 4000,
              "kmh": 44,
              "from_deg": 280
            }
          ],
          "cloud_cover_pct": 100,
          "precipitation_mm": 0.0
        },
        {
          "time": "2026-11-22T09:00:00Z",
          "verdict": "flyable",
          "thermals": "weak",
          "climb_ms": 0.0,
          "cloud": false,
          "wind_kmh": 4,
          "wind_from_deg": 110,
          "gust_kmh": 8,
          "aloft": [
            {
              "height_m": 1500,
              "kmh": 8,
              "from_deg": 170
            },
            {
              "height_m": 2000,
              "kmh": 18,
              "from_deg": 200
            },
            {
              "height_m": 2500,
              "kmh": 28,
              "from_deg": 230
            },
            {
              "height_m": 3000,
              "kmh": 38,
              "from_deg": 260
            },
            {
              "height_m": 4000,
              "kmh": 40,
              "from_deg": 270
            }
          ],
          "cloud_cover_pct": 100,
          "precipitation_mm": 0.0
        },
        {
          "time": "2026-11-22T10:00:00Z",
          "verdict": "flyable",
          "thermals": "weak",
          "climb_ms": 0.0,
          "cloud": false,
          "wind_kmh": 3,
          "wind_from_deg": 130,
          "gust_kmh": 8,
          "aloft": [
            {
              "height_m": 1500,
              "kmh": 9,
              "from_deg": 170
            },
            {
              "height_m": 2000,
              "kmh": 18,
              "from_deg": 200
            },
            {
              "height_m": 2500,
              "kmh": 27,
              "from_deg": 230
            },
            {
              "height_m": 3000,
              "kmh": 35,
              "from_deg": 260
            },
            {
              "height_m": 4000,
              "kmh": 40,
              "from_deg": 260
            }
          ],
          "cloud_cover_pct": 100,
          "precipitation_mm": 0.0
        },
        {
          "time": "2026-11-22T11:00:00Z",
          "verdict": "flyable",
          "thermals": "weak",
          "climb_ms": 0.0,
          "top_m": 1150,
          "cloud": false,
          "wind_kmh": 1,
          "wind_from_deg": 90,
          "gust_kmh": 7,
          "aloft": [
            {
              "height_m": 1500,
              "kmh": 8,
              "from_deg": 170
            },
            {
              "height_m": 2000,
              "kmh": 15,
              "from_deg": 200
            },
            {
              "height_m": 2500,
              "kmh": 23,
              "from_deg": 230
            },
            {
              "height_m": 3000,
              "kmh": 29,
              "from_deg": 260
            },
            {
              "height_m": 4000,
              "kmh": 24,
              "from_deg": 250
            }
          ],
          "cloud_cover_pct": 100,
          "precipitation_mm": 0.0
        },
        {
          "time": "2026-11-22T12:00:00Z",
          "verdict": "flyable",
          "thermals": "weak",
          "climb_ms": 0.0,
          "top_m": 1150,
          "cloud": false,
          "wind_kmh": 1,
          "wind_from_deg": 90,
          "gust_kmh": 5,
          "aloft": [
            {
              "height_m": 1500,
              "kmh": 6,
              "from_deg": 160
            },
            {
              "height_m": 2000,
              "kmh": 13,
              "from_deg": 200
            },
            {
              "height_m": 2500,
              "kmh": 19,
              "from_deg": 230
            },
            {
              "height_m": 3000,
              "kmh": 24,
              "from_deg": 260
            },
            {
              "height_m": 4000,
              "kmh": 13,
              "from_deg": 230
            }
          ],
          "cloud_cover_pct": 100,
          "precipitation_mm": 0.0
        },
        {
          "time": "2026-11-22T13:00:00Z",
          "verdict": "flyable",
          "thermals": "weak",
          "climb_ms": 0.0,
          "cloud": false,
          "wind_kmh": 2,
          "wind_from_deg": 50,
          "gust_kmh": 4,
          "aloft": [
            {
              "height_m": 1500,
              "kmh": 2,
              "from_deg": 130
            },
            {
              "height_m": 2000,
              "kmh": 8,
              "from_deg": 170
            },
            {
              "height_m": 2500,
              "kmh": 13,
              "from_deg": 210
            },
            {
              "height_m": 3000,
              "kmh": 18,
              "from_deg": 240
            },
            {
              "height_m": 4000,
              "kmh": 15,
              "from_deg": 220
            }
          ],
          "cloud_cover_pct": 100,
          "precipitation_mm": 0.0
        },
        {
          "time": "2026-11-22T14:00:00Z",
          "verdict": "flyable",
          "thermals": "weak",
          "climb_ms": 0.0,
          "cloud": false,
          "wind_kmh": 2,
          "wind_from_deg": 20,
          "gust_kmh": 4,
          "aloft": [
            {
              "height_m": 1500,
              "kmh": 4,
              "from_deg": 310
            },
            {
              "height_m": 2000,
              "kmh": 8,
              "from_deg": 300
            },
            {
              "height_m": 2500,
              "kmh": 12,
              "from_deg": 280
            },
            {
              "height_m": 3000,
              "kmh": 17,
              "from_deg": 270
            },
            {
              "height_m": 4000,
              "kmh": 41,
              "from_deg": 250
            }
          ],
          "cloud_cover_pct": 100,
          "precipitation_mm": 0.0
        }
      ]
    },
    {
      "at": {
        "lat": 47.6603,
        "lon": 11.5197
      },
      "hours": []
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

</details>

| Field | Meaning |
|---|---|
| `source`, `model`, `run`, `whole`, `computed_at` | As in [`GET /v1/thermals/hours`](#get-v1thermalshoursdatelayerheight): the source, the model the day is read from and its newest run, whether every cell has all of the day's daylight hours, and when the map was worked out. **Show the pilot the model and the run** |
| `date` | The day, by the sun at the cells |
| `heights_m` | The heights above mean sea level the map keeps the wind at, the lowest first: the same as in `GET /v1/thermals` |
| `points` | One entry per point asked for, in the same order |
| `points[].at` | The point as it was asked for |
| `points[].cell` | The middle of the map's square the point lies in. Left out outside the map |
| `points[].ground_m` | The height of the day's model's ground in that square, whole metres above mean sea level: the height its hours are judged at. Left out where the model gives none |
| `points[].hours` | The square's daylight hours of the day that the map holds, oldest first. Empty outside the map |

An hour has the values of every layer for its square, each left out where the model gives none: `time`; `verdict`, `why`, `thermals`, `climb_ms`, `top_m` and `cloud` as the layer `thermals` has them; `wind_kmh`, `wind_from_deg` and `gust_kmh` as the layer `wind` at the ground; `cloud_cover_pct`; `precipitation_mm`. In whole km/h, tens of degrees, whole percent and tenths of a millimetre, as in the hours. `aloft` lists the wind at those of `heights_m` that have one in the hour, the lowest first, each with `height_m`, `kmh` and `from_deg`: a height under the model's ground, or above its highest level, has none and is not listed.

A client expects values of `verdict`, `why` and `thermals` it does not know, and fields it does not know.

**Credit.** Show the credit of the source with what is drawn from it, with its link.

**Caching.** `Cache-Control: public, max-age=60` and an `ETag`, as the hours. The answer changes when the map does, with a run of the day's model.

**When there is no answer.**

| Status | `error.code` | When |
|---|---|---|
| `400` | `invalid_day` | `date` is missing or is not a date written `YYYY-MM-DD` |
| `400` | `invalid_position` | `points` is missing or empty, has more than 25 points, or a point that is not `lat,lon` on the globe; `message` names the point by its number from 1 |
| `404` | `not_found` | The map has no hours for this day. `message` lists the days it has |
| `501` | `no_forecasts` | The server has no forecast service, and no map |
| `503` | `unavailable` | The server has no map yet. `Retry-After` says when to ask again (60 seconds) |

**In the development mode** the points are read in the recorded map's four squares around the Wallberg, on its four days.

## `GET /v1/radar`

The rain radar frames the server holds: what a weather service's radars saw of the rain over the server's regions, a picture every few minutes, for the last two hours. The answer is the list; each frame's picture is at its `path` ([below](#get-v1radarsourcetimepng)).

```json
{
  "time": "2026-11-21T13:19:00Z",
  "steps_mm_h": [
    0.1,
    0.2,
    1.0,
    4.0,
    10.0,
    30.0
  ],
  "unseen": 7,
  "layers": [
    {
      "source": "dwd-radar-ry",
      "west": 9.4,
      "south": 46.291984,
      "east": 17.2,
      "north": 48.5,
      "columns": 624,
      "rows": 261,
      "cadence_seconds": 300,
      "frames": [
        {
          "time": "2026-11-21T13:10:00Z",
          "interval_s": 300,
          "path": "/v1/radar/dwd-radar-ry/20261121T1310Z.png",
          "bytes": 4572
        },
        {
          "time": "2026-11-21T13:15:00Z",
          "interval_s": 300,
          "path": "/v1/radar/dwd-radar-ry/20261121T1315Z.png",
          "bytes": 4695
        }
      ]
    },
    {
      "source": "geosphere-inca-rr",
      "west": 9.4,
      "south": 46.291984,
      "east": 17.2,
      "north": 48.5,
      "columns": 624,
      "rows": 261,
      "cadence_seconds": 900,
      "frames": [
        {
          "time": "2026-11-21T12:45:00Z",
          "interval_s": 900,
          "path": "/v1/radar/geosphere-inca-rr/20261121T1245Z.png",
          "bytes": 5066
        },
        {
          "time": "2026-11-21T13:00:00Z",
          "interval_s": 900,
          "path": "/v1/radar/geosphere-inca-rr/20261121T1300Z.png",
          "bytes": 5342
        }
      ]
    }
  ],
  "attributions": [
    {
      "source": "dwd-radar-ry",
      "provider": "Deutscher Wetterdienst",
      "licence": "CC BY 4.0",
      "text": "Datenbasis: Deutscher Wetterdienst, Rasterdaten bildlich wiedergegeben",
      "url": "https://www.dwd.de/DE/service/rechtliche_hinweise/rechtliche_hinweise_node.html"
    },
    {
      "source": "geosphere-inca-rr",
      "provider": "GeoSphere Austria",
      "licence": "CC BY 4.0",
      "text": "Data source: GeoSphere Austria - https://data.hub.geosphere.at",
      "url": "https://data.hub.geosphere.at"
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `time` | The server's clock when it answered. Count a frame's age against this, not against the device's clock, and write it on the map: a radar picture that is not dated is taken for the rain of now |
| `steps_mm_h` | From how many millimetres an hour each step of rain begins, the first step first. The steps are Venti's and the same for every radar source: 0.1 is the least a product reports, 0.2, 1 and 4 are where the forecast map's light rain, rain and heavy rain begin, 10 and 30 are for the cores of showers and thunderstorms |
| `unseen` | The value of a pixel the radar did not see |
| `layers[]` | One layer for each radar source the server collects, sorted by source: for the first region two, `dwd-radar-ry` and `geosphere-inca-rr`. **Layers are never merged**: two sources are two pictures, each with its own credit, and a client that shows both draws two layers, on the same grid and in the same steps. The order here is by name and says nothing of which to draw on top; the core gives that order. A source the server holds no frame of, after a start or while its provider does not answer, is a layer without frames |
| `layers[].west`, `south`, `east`, `north` | The edges of every picture of the layer, in degrees |
| `layers[].columns`, `rows` | The size of a picture in pixels. The pixels are square in Web Mercator, the projection web maps are drawn in, so a picture lies right on such a map when its four corners are pinned to the four edges: top left `west`, `north`; top right `east`, `north`; bottom right `east`, `south`; bottom left `west`, `south`. The first region's is 624 by 261, a pixel an eightieth of a degree of longitude wide, which is 0.94 km at 47.5° N and as fine as the radar products are. A larger area gets a coarser grid: no side has more than 2048 pixels. `south` lies a little beyond the regions' edge, where the last row ends |
| `layers[].cadence_seconds` | How often the source publishes a frame: 300 for DWD's composite, 900 for GeoSphere Austria's analysis. A frame is too old to be the rain of now after three of these, each source by its own |
| `layers[].frames[]` | The frames held, the oldest first: at most 25, none older than two hours |
| `layers[].frames[].time` | The end of the interval the frame is of |
| `layers[].frames[].interval_s` | How long that interval is: a frame of 13:15 with 300 is the rain between 13:10 and 13:15, one of 13:00 with 900 the rain between 12:45 and 13:00 |
| `layers[].frames[].path` | Where the frame's picture is, on this server |
| `layers[].frames[].bytes` | The size of the picture |
| `attributions[]` | The credit of every source that has a layer in the answer. Show it with the layer: DWD asks for its credit "unmittelbar an der verwendeten DWD-Information", directly at the data |

The list may be kept for a minute (`Cache-Control: public, max-age=60`) and carries an `ETag`. A server that collects no radar answers `501` with `no_radar`.

**What a radar does not see.** DWD's composite is what Germany's 17 radars see, 150 km around each. In the first region that is Bavaria and the north of Tirol and Vorarlberg; east of Passau and south of the main ridge of the Alps it sees nothing, and behind the first chains its beams are blocked by the mountains in front. Such places have the value `unseen` in every picture, which is not "no rain". And a frame is the rain that fell in its five minutes: it says nothing of a shower that has not formed yet.

**The second layer is an analysis, not a radar picture.** `geosphere-inca-rr` is the first step of each run of INCA, GeoSphere Austria's nowcast: the rain of a quarter of an hour as a model puts it together from the Austrian radars and the rain gauges, on a grid of a kilometre, published some 22 minutes after that quarter of an hour. So its newest frame is some 20 to 37 minutes old when all is well, where DWD's is one to six (on 2026-11-21 the run of 13:00 UTC was listed at 13:17, the run of 12:15 between 12:36:51 and 12:37:21). It has a value at every point of its grid: of the first region's box it sees all 162,864 pixels in the recorded frames, which were asked for as the adapter asks, with the box widened by 0.05° (asked for with the plain box, 107 pixels in the middle of the last row had none); DWD's radars see 42.9 % (69,885). *To verify* with the provider: that this first step is an analysis of the rain that fell and not already a forecast.

**In the development mode** the list is the one above: both layers, each with the two frames recorded on 2026-11-21 between 13:17 and 13:19 UTC, an afternoon with light rain in the first region. DWD's frames are of 13:10 and 13:15 UTC, GeoSphere Austria's of 12:45 and 13:00. The radar's clock stands at 13:19 UTC of that day, three days after the clock of the observations: the newest radar frame is four minutes old and the newest analysis nineteen, and neither source is behind. (Until that recording the mode had DWD's layer alone, because the two sources had been recorded an hour apart and no one clock was honest for both.)

### `GET /v1/radar/{source}/{time}.png`

One frame's picture, at the `path` the list gives: `time` is the frame's, written `20261121T1315Z`.

The answer is a PNG with a palette, a byte a pixel, `columns` by `rows`, the first row the northernmost. **A pixel's value is a step, not a colour**: `0` where the radar saw no rain, `1` to `6` for the steps of `steps_mm_h`, `7` (`unseen`) where it did not see. The colours are a client's own. The palette in the file is a plain one, so that the picture can be looked at as it is: no rain is clear, the steps are greys from light to dark, and what was not seen is a faint veil. A client that draws rain in its own colours writes its palette over the file's, which changes two small chunks of the file and none of its pixels (Venti's apps do so in their core), or reads the pixels as numbers.

A picture of the first region is 4 to 9 kB: 4.6 and 4.7 kB for the two recorded frames of DWD, 5.1 and 5.3 kB for the two of GeoSphere Austria, and some 8 kB on 2026-11-19 at 12:35 UTC, with rain over half of what the radar sees there (the first measured by the server, the second with the same grid and steps outside it). Two hours of frames are 0.1 to 0.2 MB.

A frame of a moment does not change, and may be kept for a day (`Cache-Control: public, max-age=86400, immutable`). The server itself drops a frame after two hours.

| Status | `error.code` | When |
|---|---|---|
| `404` | `not_found` | No such source, not the name of a frame, or a frame the server does not hold, or no longer |

## `GET /v1/foehn`

Foehn, read from the air pressure on both sides of the Alps' main ridge: for each pair of stations the difference now, which side is higher, a level, and the differences of the last 24 hours. Public, without an account, and the connector's tool `foehn` answers from it ([mcp.md](mcp.md)). No parameters.

Air is pushed over the ridge from the side with the higher pressure and falls into the valleys of the other side as foehn. With the higher pressure in the south it is south foehn, which blows north of the ridge; the other way it is north foehn. The pairs, the limits and where they are from are in the [decision log](architecture.md#decision-log); the second pair and the limits for south foehn are *to verify with pilots*, which `to_verify` and `basis` say for each pair.

**What it does not say.** A level is a sign, not a measurement: `caution` says so in words to show wherever a level is shown. `none` is "no sign of foehn in the pressure" and `unknown` is "nothing is known"; neither is "no foehn". A pair has a level only from two readings of the last hour (`fresh_for_s`) that are within 30 minutes of each other; else its level is `unknown`, `unknown_because` says why, and it has no difference. The level is of the answer's `time`: a client that shows the answer later judges the two pressures' times against its own clock, as the Venti apps do.

```json
{
  "time": "2026-11-18T19:05:00Z",
  "possible_from_hpa": 3.0,
  "likely_from_hpa": 6.0,
  "fresh_for_s": 3600,
  "pairs": [
    {
      "id": "bolzano-innsbruck",
      "south": {
        "id": "suedtirol-meteo:83200MS",
        "source": "suedtirol-meteo",
        "name": "Bolzano",
        "position": {
          "lat": 46.4977,
          "lon": 11.3128
        },
        "elevation_m": 245.0,
        "pressure": {
          "time": "2026-11-18T18:50:00Z",
          "sea_level_hpa": 1004.4,
          "reduced_by": "provider"
        }
      },
      "north": {
        "id": "geosphere-tawes:11320",
        "source": "geosphere-tawes",
        "name": "Innsbruck",
        "position": {
          "lat": 47.26,
          "lon": 11.3842
        },
        "elevation_m": 578.0,
        "pressure": {
          "time": "2026-11-18T19:00:00Z",
          "sea_level_hpa": 998.2,
          "reduced_by": "venti"
        }
      },
      "level": "likely",
      "difference_hpa": 6.2,
      "difference_time": "2026-11-18T18:50:00Z",
      "higher": "south",
      "foehn": "south",
      "series": [
        {
          "time": "2026-11-17T20:50:00Z",
          "difference_hpa": 1.8
        },
        {
          "time": "2026-11-17T22:50:00Z",
          "difference_hpa": 2.2
        },
        {
          "time": "2026-11-18T00:50:00Z",
          "difference_hpa": 2.6
        },
        {
          "time": "2026-11-18T02:50:00Z",
          "difference_hpa": 3.0
        },
        {
          "time": "2026-11-18T04:50:00Z",
          "difference_hpa": 3.4
        },
        {
          "time": "2026-11-18T06:50:00Z",
          "difference_hpa": 3.8
        },
        {
          "time": "2026-11-18T08:50:00Z",
          "difference_hpa": 4.2
        },
        {
          "time": "2026-11-18T10:50:00Z",
          "difference_hpa": 4.6
        },
        {
          "time": "2026-11-18T12:50:00Z",
          "difference_hpa": 5.0
        },
        {
          "time": "2026-11-18T14:50:00Z",
          "difference_hpa": 5.4
        },
        {
          "time": "2026-11-18T16:50:00Z",
          "difference_hpa": 5.8
        },
        {
          "time": "2026-11-18T18:50:00Z",
          "difference_hpa": 6.2
        }
      ],
      "to_verify": false,
      "basis": "The pair and the limits of 3 and 6 hPa are those of the foehn chart of the weather service of the Province of Bolzano, which gives them for north foehn; for south foehn Venti uses the same limits, which no weather service has given."
    },
    {
      "id": "klagenfurt-salzburg",
      "south": {
        "id": "geosphere-tawes:11331",
        "source": "geosphere-tawes",
        "name": "Klagenfurt",
        "position": {
          "lat": 46.6483,
          "lon": 14.3183
        },
        "elevation_m": 450.0,
        "pressure": {
          "time": "2026-11-18T18:50:00Z",
          "sea_level_hpa": 1001.0,
          "reduced_by": "venti"
        }
      },
      "north": {
        "id": "geosphere-tawes:11150",
        "source": "geosphere-tawes",
        "name": "Salzburg",
        "position": {
          "lat": 47.7894,
          "lon": 13.0086
        },
        "elevation_m": 430.0,
        "pressure": {
          "time": "2026-11-18T18:50:00Z",
          "sea_level_hpa": 1000.2,
          "reduced_by": "venti"
        }
      },
      "level": "none",
      "difference_hpa": 0.8,
      "difference_time": "2026-11-18T18:50:00Z",
      "higher": "south",
      "series": [
        {
          "time": "2026-11-17T20:50:00Z",
          "difference_hpa": 0.7
        },
        {
          "time": "2026-11-17T22:50:00Z",
          "difference_hpa": 0.8
        },
        {
          "time": "2026-11-18T00:50:00Z",
          "difference_hpa": 0.7
        },
        {
          "time": "2026-11-18T02:50:00Z",
          "difference_hpa": 0.8
        },
        {
          "time": "2026-11-18T04:50:00Z",
          "difference_hpa": 0.7
        },
        {
          "time": "2026-11-18T06:50:00Z",
          "difference_hpa": 0.8
        },
        {
          "time": "2026-11-18T08:50:00Z",
          "difference_hpa": 0.7
        },
        {
          "time": "2026-11-18T10:50:00Z",
          "difference_hpa": 0.8
        },
        {
          "time": "2026-11-18T12:50:00Z",
          "difference_hpa": 0.8
        },
        {
          "time": "2026-11-18T14:50:00Z",
          "difference_hpa": 0.8
        },
        {
          "time": "2026-11-18T16:50:00Z",
          "difference_hpa": 0.8
        },
        {
          "time": "2026-11-18T18:50:00Z",
          "difference_hpa": 0.8
        }
      ],
      "to_verify": true,
      "basis": "Venti's own choice of two airfields on either side of the Tauern, with the limits of the pair Bolzano and Innsbruck. No weather service has given either."
    }
  ],
  "caution": "A pressure difference across the main ridge is a sign of foehn, not a measurement of it. Foehn can blow with a small difference, and with a large one it can stay above a valley's cold air for hours and then break through within minutes. It says nothing about one valley or one take-off: read the wind stations, the forecast and the sky, and when in doubt do not fly.",
  "attributions": [
    {
      "source": "geosphere-tawes",
      "provider": "GeoSphere Austria",
      "licence": "CC BY 4.0",
      "text": "Data source: GeoSphere Austria - https://data.hub.geosphere.at",
      "url": "https://data.hub.geosphere.at"
    },
    {
      "source": "suedtirol-meteo",
      "provider": "Autonome Provinz Bozen - Südtirol",
      "licence": "CC0 1.0",
      "text": "Data source: Autonome Provinz Bozen - Südtirol, Agentur für Bevölkerungsschutz",
      "url": "https://data.civis.bz.it/de/dataset/misure-meteo-e-idrografiche"
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `time` | The server's time when it answered: the moment the levels are of |
| `possible_from_hpa`, `likely_from_hpa` | The limits, 3 and 6 hPa: a pair's level is `possible` or `likely` from a difference of that many whole hectopascals, on either side |
| `fresh_for_s` | How old a reading may be to count, 3600 seconds |
| `pairs[].id` | A stable name for the pair |
| `pairs[].south`, `north` | The station south and the station north of the ridge: its `id` (its readings are at [`/v1/stations/{id}/observations`](#get-v1stationsidobservationsfromto)), `source`, the place's `name` as pilots say it, `position`, and `elevation_m` if the server knows the station |
| `pairs[].south.pressure`, `north.pressure` | The station's newest pressure at sea level of the last 24 hours, however old: `time`, `sea_level_hpa` to a tenth, and `reduced_by`. Left out when the server holds none |
| `…pressure.reduced_by` | `provider`: the provider sends the pressure reduced to sea level (South Tyrol). `venti`: the provider sends the pressure at the station, and the server reduced it from the station's height and temperature by the barometric formula, which over a cold valley floor reads a hectopascal or more too high. Only for stations up to 1000 m |
| `pairs[].level` | `unknown`, `none`, `possible` or `likely`, chosen from the difference in whole hectopascals, halves upwards |
| `pairs[].unknown_because` | With `unknown` only: `south_missing`, `north_missing` (no pressure that can be compared), `south_old`, `north_old` (older than an hour), `apart` (the two readings are more than 30 minutes apart) |
| `pairs[].difference_hpa`, `difference_time` | The pressure in the south less the pressure in the north, to a tenth, and the time of the older of the two readings. Left out with `unknown` |
| `pairs[].higher` | `south` or `north`: the side with the higher pressure. Left out when the difference is under half a hectopascal, and with `unknown` |
| `pairs[].foehn` | `south` or `north` foehn, with the levels `possible` and `likely` only |
| `pairs[].series[]` | The last 24 hours, oldest first: `time` and `difference_hpa`, one for each reading of the southern station that has a reading of the northern within 30 minutes |
| `pairs[].to_verify`, `basis` | Whether the pair or its limits wait for pilots to confirm them, and where they are from, in a sentence |
| `caution` | What the reading does not say |
| `attributions[]` | The credit of every source that has a pressure in the answer |

The development mode's pressures are made up, so that one pair stands at `likely` and one at `none`: Bolzano 6.2 hPa above what Innsbruck's recorded 930.9 hPa at 578 m and 7.8 °C reduces to, after rising against it all day; Klagenfurt and Salzburg, two stations the recordings do not have and this mode makes up with a pressure, a temperature and a light wind, 0.8 hPa apart.

## Maps

The map regions a server hands out, for an app to draw its map where there is no network. A region is a few files that the app downloads whole: today a vector base map and the terrain, each a PMTiles archive. Which regions a server has is up to whoever runs it ([deploy/README.md](../deploy/README.md#map-regions)), and a server may have none. Anyone may read both endpoints, without an account: the maps are open data, as the wind is.

### `GET /v1/maps`

The regions, sorted by `id`.

```bash
curl http://127.0.0.1:8080/v1/maps
```

```json
{
  "regions": [
    {
      "id": "tegernsee-demo",
      "name": "Tegernsee valley (demo)",
      "version": "2026-11-18",
      "covers": {
        "south": 47.6,
        "west": 11.64,
        "north": 47.76,
        "east": 11.86
      },
      "files": [
        {
          "role": "basemap",
          "name": "basemap.pmtiles",
          "bytes": 3236717,
          "sha256": "edb16f520a2ba491cd6a977ec755a1f2f5f1b5a5f27b15e10c50985420c4a43c",
          "path": "/v1/maps/tegernsee-demo/2026-11-18/basemap.pmtiles",
          "credits": [
            {
              "text": "© OpenStreetMap contributors",
              "url": "https://www.openstreetmap.org/copyright"
            },
            {
              "text": "© OpenMapTiles",
              "url": "https://openmaptiles.org"
            }
          ]
        },
        {
          "role": "terrain",
          "name": "terrain.pmtiles",
          "bytes": 4239720,
          "sha256": "84dc4b23f4aa57b20a0be523091b636ba09f8c7c4fde035adcb22e6035d9bc6e",
          "path": "/v1/maps/tegernsee-demo/2026-11-18/terrain.pmtiles",
          "credits": [
            {
              "text": "© Mapterhorn and the surveys it combines",
              "url": "https://mapterhorn.com/attribution"
            }
          ]
        }
      ]
    }
  ]
}
```

This is the one region the development mode has when it is started with `--maps apps/ios/Venti/Resources`: the valley of the apps' sample flight, in the files the apps bundle. Without that option, and on a server that was given no maps, the answer is `{"regions":[]}`.

| Field | Meaning |
|---|---|
| `regions[].id` | The region's identifier. It stays when the region's files are renewed |
| `regions[].name` | The region's name for people |
| `regions[].version` | Which edition of the region's files this is; by custom the day they were built. A client that has another version has an older or a newer map. Versions are compared for equality and for nothing else |
| `regions[].covers` | The box the files cover: its four edges in degrees |
| `regions[].files[].role` | What the file is. `basemap`: a PMTiles archive, version 3, of vector tiles in the OpenMapTiles schema. `terrain`: a PMTiles archive, version 3, of WebP tiles with heights in the Terrarium encoding. `other`: something this version of the API has no word for. A client leaves alone a file whose role it does not know |
| `regions[].files[].name` | The file's name |
| `regions[].files[].bytes` | Its size in bytes |
| `regions[].files[].sha256` | The SHA-256 of its bytes, 64 lowercase hexadecimal digits. **Check a copy against it before using the copy** |
| `regions[].files[].path` | Where to `GET` the file; see [below](#get-and-head-v1mapsregionversionname) |
| `regions[].files[].credits` | Whose data the file holds: for each a `text`, worded as the provider asks, and a `url` for the text to link to, left out where there is none. **Show the credits of every file that is drawn, wherever it is drawn.** The licences require that |

An `id`, a `version` and a `name` are lowercase letters and digits with `-`, `_` and `.` between them, 64 characters at most. Each is one step of a path here and can be one step of a folder on the device.

**The files of a region are never merged.** Each holds data under its own licences and names its own credits: in the example the base map is OpenStreetMap's data in the schema of OpenMapTiles, and the terrain is Mapterhorn's. A client keeps the files apart, draws them as separate layers, and shows with each layer the credits of its file.

**A region is listed once the server has read its files.** When the server starts, it reads every file of a region from beginning to end and lists the region only if each file has the SHA-256 that the region's description on the server states. So the hash in the list is the hash of the file that a `GET` sends, and a copy that does not match was damaged on its way. For a region of some gigabytes the reading takes a while, and until it is done the region is not in the list: just after a server started, a region that is missing from the list may be on its way and not gone.

**Caching.** `Cache-Control: public, max-age=60` and an `ETag`, like the stations.

### `GET` and `HEAD /v1/maps/{region}/{version}/{name}`

One file of a region, as it is on the server's disk. The path is the `path` the list gives. The version is part of it, so what a path names never changes: when a region's files are renewed the region gets a new version, its files get new paths, and the old paths answer `404`.

```bash
curl -O http://127.0.0.1:8080/v1/maps/tegernsee-demo/2026-11-18/terrain.pmtiles
curl -C - -O http://127.0.0.1:8080/v1/maps/tegernsee-demo/2026-11-18/terrain.pmtiles   # goes on where a download broke off
curl -I http://127.0.0.1:8080/v1/maps/tegernsee-demo/2026-11-18/terrain.pmtiles        # how large is it?
```

Every answer with a file, or with a part of one, has these headers:

| Header | What it says |
|---|---|
| `Content-Length` | How many bytes this answer sends; for a `HEAD`, how many a `GET` would send. It is always there, so an app can show how far a download is |
| `Accept-Ranges` | `bytes`: a part of the file can be asked for |
| `ETag` | The file's SHA-256 in quotes, the one the list states. A strong validator |
| `Cache-Control` | `public, max-age=31536000, immutable`: the bytes behind a path never change, and a copy may be kept for good |
| `Content-Type` | `application/octet-stream` |

**A part of a file.** `Range: bytes=a-b` asks for the bytes from `a` to `b`, counted from 0 and both included. `bytes=a-` asks for everything from `a` on, which is how a download that broke off goes on. `bytes=-n` asks for the last `n` bytes. The answer is `206` with `Content-Range: bytes a-b/size` and those bytes. A range that ends after the file ends with the file. A range that begins at or after the end of the file is answered `416` with `range_not_satisfiable` and `Content-Range: bytes */size`. Whatever else stands in the header is no range and gets the whole file with `200`, as HTTP allows a server to answer what it does not understand: another unit, several ranges at once, an end before the beginning. A `HEAD` takes no range.

**Going on with the same file.** `If-Range` with the file's `ETag` beside a `Range` means "the part, if this is still that file": the answer is the part if it is, and the whole file with `200` if it is not. A client that goes on with a download sends it, so that it never joins the beginning of one file to the end of another. `If-None-Match` with the file's `ETag` is answered `304` without a body.

**Never compressed.** A file is sent as it is, whatever `Accept-Encoding` a request carries, and no answer about a file has a `Content-Encoding`. A phone asks for gzip with every request; a compressed answer has no `Content-Length` and no ranges, so a download could neither show its progress nor go on where it broke off. The archives are compressed inside already. This is the one exception to the [rule about compression](#rules-that-hold-for-every-endpoint).

**No time limit.** The API's other requests are given up when the server has no answer after a while, ten seconds for most of them. A file has no such bound, and none on how long its bytes take to arrive: a download takes as long as the client's network needs.

**A download can break off**, and not only on the client's side: when the server is stopped, for an update or to renew a region, it ends the downloads that are under way, and the connection closes with bytes outstanding. The client asks for the rest with `Range` and `If-Range` once the server answers again. If the path then answers `404`, the region was renewed in between: the client fetches the list and starts again with the new version.

**What a client does**, in order: ask for the list; download each file of a region to a place of its own, going on by range after every break; compare the size and the SHA-256 of each copy with the list; only then put the files where the map reads them, with the region's `version` beside them. When the list later names another version, there is a newer map to fetch.

**When there is no file.**

| Status | `error.code` | When |
|---|---|---|
| `404` | `map_not_found` | The server hands out no file under this path: no region with this identifier, not in this version, or no file of this name. The three are one answer. The list says what there is |
| `416` | `range_not_satisfiable` | The range begins at or after the end of the file. `Content-Range` says how long the file is |
| `503` | `unavailable` | The server could not open the file at this moment. `Retry-After` says when to try again |

**Browsers.** A page on an origin the server allows may send `Range` and `If-Range` with its request and may read `Content-Range`, `Accept-Ranges` and `ETag` from the answer.

**Size.** The region in the example is 3.2 MB of base map and 4.2 MB of terrain for one valley. A region the size of the server's first one has not been built yet: scaled up by area it would be about 1.6 GB and 2.1 GB, which is arithmetic and not a measurement ([deploy/AGENTS.md](../deploy/AGENTS.md#map-regions)).

## Accounts and sessions

A pilot has an account with a **handle**, a **display name** and a **password**. Signing in on a device makes a **session** and hands the device a **token**; the device sends the token with every request that concerns the account:

```
Authorization: Bearer venti_3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz01
```

That header is the only way to act for an account. The server sets no cookies and reads none, and it takes no token from a query string. A request without a token, or with one that was signed out or has expired, gets `401` with `unauthorized`, on every endpoint marked "needs a token" below.

Another program acts for a pilot with a token of its own kind, which the pilot makes and limits: [Tokens for other programs](#tokens-for-other-programs). Where an endpoint below says "needs a token", such a token does too if it has the scope that section's table names; a session's token always does.

What a client has to know:

- **A handle** is 3 to 20 characters: the letters `a` to `z`, digits and `_`. It starts with a letter, does not end with `_` and has no two `_` in a row. Upper and lower case are the same handle; the server keeps and shows it in lower case. A handle and a display name are public: they stand on every flight the pilot shares.
- **A display name** is 1 to 40 characters in any script, without line breaks or invisible characters.
- **A password** is 15 to 128 characters. Any characters count, spaces included, and there is no other rule; three or four words in a row are easy to remember. The server keeps a hash from which the password cannot be read back (Argon2id).
- **A forgotten password cannot be recovered.** The server knows no email address and has no other way to tell that someone is the account's owner. Say so where the password is chosen. This changes when passkeys or email arrive; see [architecture.md](architecture.md#authentication-and-accounts).
- **A token is shown once**, in the answer to creating an account or signing in. The server keeps only a hash of it. Keep the token where the device keeps secrets (the Keychain, the Keystore) and never in a URL or a log. A browser has no such place: the web app keeps it in the site's own storage, out of reach of other sites but within reach of any script the site itself runs, which is why the site ships none but its own ([apps/web/AGENTS.md](../apps/web/AGENTS.md#where-the-session-token-is-kept)).
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
    "created_at": "2026-11-18T19:05:00Z",
    "edits_sites": true
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

### Sign in with Google

Where `google_sign_in` in [`GET /v1/health`](#get-v1health) is `true`, a pilot signs in with their Google account. The app gets an **ID token** from Google (the web app with Google Identity Services, the Android app with Credential Manager, the iPhone app with Google's authorization code flow; [design.md](design.md) says which) for one of the server's client IDs, and sends it here. The server checks it as Google says to: signed by one of Google's keys, issued by Google, for one of this server's client IDs, not expired (with a minute's grace), for a Google account whose email Google has verified. It needs no client secret, and the app keeps the ID token no longer than this request.

A first sign-in with a Google account makes no account yet: the pilot chooses a handle first. The account then keeps Google's identifier of the Google account and its email; the email is shown to the pilot (`google_email` in `GET /v1/account` and in [the archive](#get-v1accountarchive)) and to nobody else. An account made with Google has no password: where a request asks for the password (deleting the account, making a token for a program) it sends `google_id_token`, a fresh ID token of the same Google account, in its place. Changing the password is not for it.

These requests are for the pilot's own sign-in: no token for a program makes them.

### `POST /v1/auth/google`

Signs this device in with a Google ID token.

```json
{
  "id_token": "eyJhbGciOiJSUzI1NiIsImtpZCI6Ij…",
  "device": "Pixel 9"
}
```

For a Google account an account is linked to, the answer is `201` with the same body as [signing in](#post-v1sessions). For one that no account is linked to it is `200`:

```json
{
  "needs_handle": true,
  "signup": "venti_signup_…",
  "suggested_handle": "anna_alpin",
  "name": "Anna Alpin"
}
```

| Field | Meaning |
|---|---|
| `needs_handle` | Always `true`: what tells this answer from a sign-in |
| `signup` | What carries the Google account on to [`POST /v1/auth/google/account`](#post-v1authgoogleaccount). Sealed: it can be neither read nor changed. Good for 15 minutes, on this server only, until it restarts |
| `suggested_handle` | A handle that follows the rules and is free, made from the email's part before the `@`, or else from the name. Left out when none can be made |
| `name` | The name the Google account gives, for the display name. Left out when it gives none |

| Status | Code | When |
|---|---|---|
| 401 | `invalid_google_token` | The token is not good for this server: not Google's, for another app, expired, or of a Google account whose email is not verified. `message` says which |
| 503 | `google_unavailable` | The server could not fetch Google's keys. Try again later |
| 501 | `google_not_configured` | The server does not offer signing in with Google |

### `POST /v1/auth/google/account`

Creates an account linked to the Google account of a first sign-in, and signs this device in.

```json
{
  "signup": "venti_signup_…",
  "handle": "anna_alpin",
  "display_name": "Anna Alpin",
  "invite": "7k3m-9q2x-4r8t-1v6w",
  "device": "Pixel 9"
}
```

`handle`, `display_name`, `invite` and `device` are as for [`POST /v1/accounts`](#post-v1accounts), and so is who may make an account: `closed` takes nobody, `invite` asks for a code, and attempts are counted against the address as there. The answer is `201` with the same body as signing in.

| Status | Code | When |
|---|---|---|
| 401 | `invalid_signup` | The sign-up has expired or is not this server's. Sign in with Google again |
| 400 | `invalid_handle`, `invalid_display_name` | As for `POST /v1/accounts` |
| 403 | `registration_closed`, `invalid_invite` | As for `POST /v1/accounts` |
| 409 | `handle_taken` | As for `POST /v1/accounts` |
| 409 | `google_taken` | Another account was made for the Google account meanwhile. Sign in with Google to reach it |
| 429 | `rate_limited` | Too many attempts from this address |

### `POST and DELETE /v1/account/google`

Need a session's token. `POST` links the account to the Google account of an ID token, so that the pilot signs in with Google from then on; the password stays. It takes `{"id_token": "…"}` and answers `200` with the account, as `GET /v1/account` does, `google_email` included. An account linked to another Google account is linked to this one in its place. Another account linked to this Google account gives `409` with `google_taken`; a token that is not good, as for `POST /v1/auth/google`.

`DELETE` unlinks it and answers `204`. An account that is linked to none gives `404` with `google_not_linked`. An account made with Google has no password and cannot be unlinked: `409` with `google_only_sign_in`.

### `GET /v1/dev/google-id-token?email=&name=`

**The development mode only**; every other server answers `404` with `not_found`. It is the development mode's own made-up "Google": an ID token, good for an hour of the development mode's clock, for the made-up Google account with this email (the same email is always the same Google account), which the development mode takes at `POST /v1/auth/google`. This is how the apps' UI tests sign in "with Google" without Google:

```bash
curl 'http://127.0.0.1:8080/v1/dev/google-id-token?email=ui.test@example.com&name=UI%20Test'
# {"id_token":"eyJ…"}
```

The key it signs with is a test-only key that is published in the source; a server that is not in the development mode never takes it. `email` is needed; `name` may be left out.

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
    "created_at": "2026-11-18T19:05:00Z",
    "edits_sites": true
  }
}
```

| Field | Meaning |
|---|---|
| `handle`, `display_name` | As the account was created with |
| `created_at` | When it was created |
| `publishing_suspended_at` | Since when the account may publish nothing, because whoever runs the server [suspended its publishing](#what-whoever-runs-the-server-can-do). Left out for every account that may, which is nearly every account |
| `google_email` | The email of the Google account the account [signs in with](#sign-in-with-google), if it is linked to one. Only the account itself sees it. Left out otherwise |
| `edits_sites` | `true` for an account whoever runs the server named as a [partner for flying sites](#sites-that-ventis-partners-and-pilots-add): it adds to the partners' layer `venti` and corrects sites of both layers. Left out for every other account, which is nearly every account. A token with `account:read` reads it too. The demo account of the development mode is one |

The answers to creating an account and to signing in carry the same `account`. This is the demo account after its publishing was suspended:

```json
{
  "account": {
    "handle": "demo",
    "display_name": "Demo Pilot",
    "created_at": "2026-11-18T19:05:00Z",
    "publishing_suspended_at": "2026-11-18T19:05:00Z",
    "edits_sites": true
  }
}
```

### `GET /v1/account/archive`

Everything the account gave, as one ZIP file: for a pilot who wants a copy, or who leaves. **The pilot's own sign-in only**: a session's token. No token for a program gets it, whatever its scopes (`403` with `session_required`); a program reads its pieces through the scopes it has.

```bash
curl -H 'Authorization: Bearer venti_0000000000000000000000000000000000000000000000000000' \
  -o venti-demo.zip 'http://127.0.0.1:8080/v1/account/archive'
```

The answer is `200` with `Content-Type: application/zip` and a name for the file (`Content-Disposition: attachment; filename="venti-demo-2026-11-18.zip"`). It is made for the asking, is kept by nobody (`Cache-Control: no-store`) and is never compressed a second time.

| File | The record it holds | What it is |
|---|---|---|
| `index.json` | `ArchiveIndex`, below | What the archive is: when it was made, every file with the record it holds and its licence, every flight with the name of its file |
| `README.txt` | Text | The same, for a person |
| `account.json` | The answer of [`GET /v1/account`](#get-v1account) | The account |
| `sessions.json` | The answer of [`GET /v1/sessions`](#get-v1sessions) | The devices signed in, by the name each gave |
| `tokens.json` | The answer of [`GET /v1/account/tokens`](#get-v1accounttokens) | The tokens given to other programs: names, scopes, dates |
| `flights.json` | The answer of [`GET /v1/flights`](#get-v1flightsafterlimit), all pages in one, without `next` | Every flight, public or not, newest first |
| `flights/2026-08-15_1030Z_5t1eshyp.igc` | The file as it was uploaded, byte for byte: what [`GET /v1/flights/{id}/igc`](#get-v1flightsidigc) answers | One for each flight, named by the day and the time of its take-off in UTC and the first eight characters of the flight's `id` |
| `groups.json` | The answer of [`GET /v1/groups`](#get-v1groups) | The groups the account is in, each with who keeps it |
| `plans.json` | The answer of [`GET /v1/groups/{id}/plans`](#get-v1groupsidplansfromto), for all groups in one | The plans the account posted, and the plans of others it said it is in on. Plans for days gone by are [no longer kept](#plans-for-a-flying-day) |
| `sites.json` | `{"revisions": […], "confirmations": […]}` | What the account gave to the [sites that pilots make](#sites-that-ventis-partners-and-pilots-add). `revisions`: every revision it wrote, by site and number, each as [a site's history](#get-v1sitesidhistoryafterlimit) has it; the site is `site.id`, and the revision with `number` 1 made the site. `confirmations`: where it says it took off, each with `site`, `day`, `note`, `with_flight` and `at`. Given under CC0 1.0 |
| `kept-sites.json` | The answer of [`GET /v1/account/kept-sites/changes`](#get-v1accountkept-siteschangessincelimit) from the beginning, all pages in one | The sites the account keeps as its own |
| `reports.json` | `{"reports": [{"sent_at": "…", "report": {…}}]}` | The [reports](#reports) the account sent and the server still keeps: `report` is the body of `POST /v1/reports` as it was sent, a site with the revision the server noted |
| `alerts.json` | `{"alerts": […], "fired": […]}` | The account's [alarms](#alarms), each as [`GET /v1/account/alerts`](#get-v1accountalerts) lists it, and what they fired of the last 30 days, each as [`GET /v1/account/alerts/fired`](#get-v1accountalertsfiredsincelimit) hands it out, the oldest first |
| `social.json` | `{"following": […], "cheers": […], "comments": […]}` | What the account gave [among pilots](#pilots-among-each-other): the pilots it follows with since when, the newest first; the flights it cheered, each by its address with the moment, the oldest first, those among them that are not public at the moment; and the comments it wrote, each with its identifier, the address of its flight, its text and its moment. Who follows the account and what others wrote under its flights are theirs and not in it |

Never in it: the password or its fingerprint, and the secret of a session or of a token. `index.json` says in `not_included` what else the account has on the server that the archive does not hold: live tracks, which the server removes 48 hours after their last position; what other pilots put into the groups; and what whoever runs the server noted about a report.

This is `index.json` of the demo account's archive:

```json
{
  "format": 1,
  "made_at": "2026-11-18T19:05:00Z",
  "account": "demo",
  "files": [
    {
      "path": "account.json",
      "holds": "The account: handle, name, since when, and the email of its Google account if it has one.",
      "record": "AccountResponse",
      "entries": 1
    },
    {
      "path": "sessions.json",
      "holds": "The devices signed in to the account, by the name each gave.",
      "record": "SessionsResponse",
      "entries": 1
    },
    {
      "path": "tokens.json",
      "holds": "The tokens given to other programs: names, scopes and dates. No secrets.",
      "record": "AccessTokensResponse",
      "entries": 1
    },
    {
      "path": "flights.json",
      "holds": "Every flight of the account, public or not, newest first: what the server worked out of each file. The files are under flights/.",
      "record": "FlightsResponse",
      "entries": 3
    },
    {
      "path": "groups.json",
      "holds": "The groups the account is in, each with who keeps it.",
      "record": "GroupsResponse",
      "entries": 1
    },
    {
      "path": "plans.json",
      "holds": "The plans the account posted to its groups, and the plans of others it said it is in on. Plans for days gone by are no longer kept.",
      "record": "PlansResponse",
      "entries": 2
    },
    {
      "path": "sites.json",
      "holds": "What the account gave to the take-offs and landings that pilots make: every revision it wrote (number 1 made the site), and where it says it took off. The sites were given to everyone and stay when the account goes.",
      "record": "ArchiveSites",
      "entries": 4,
      "licence": {
        "name": "CC0 1.0",
        "url": "https://creativecommons.org/publicdomain/zero/1.0/"
      }
    },
    {
      "path": "kept-sites.json",
      "holds": "The sites the account keeps as its own, each as it was when kept, and the identifiers of those it forgot lately.",
      "record": "KeptSiteChangesResponse",
      "entries": 0
    },
    {
      "path": "reports.json",
      "holds": "The reports the account sent to whoever runs the server, as it sent them.",
      "record": "ArchiveReports",
      "entries": 0
    },
    {
      "path": "alerts.json",
      "holds": "The account's alarms, and what they fired in the last 30 days.",
      "record": "ArchiveAlerts",
      "entries": 3
    },
    {
      "path": "social.json",
      "holds": "The pilots the account follows, the flights it cheered and the comments it wrote under flights.",
      "record": "ArchiveSocial",
      "entries": 7
    }
  ],
  "flights": [
    {
      "id": "5t1eshypdtvrq7rxw39a18vsrg",
      "file": "flights/2026-08-15_1030Z_5t1eshyp.igc"
    },
    {
      "id": "7cf5ytrw45b0aeb5tcr8wet2km",
      "file": "flights/2026-07-04_1030Z_7cf5ytrw.igc"
    },
    {
      "id": "7fvpx515951prsvkemfbvgkd3w",
      "file": "flights/2025-06-21_1030Z_7fvpx515.igc"
    }
  ],
  "complete": true,
  "not_included": [
    "The password and the secrets of sessions and tokens. The server keeps only a fingerprint of each and cannot hand one out.",
    "Live tracks. The server keeps one for a short time after its last position and then removes it; the flight reaches the logbook as a file.",
    "What other pilots put into the groups: their names as members, and their plans unless this account said it is in on them.",
    "What whoever runs the server noted about a report, and reports others sent.",
    "What this account withdrew: a deleted flight, a forgotten site's record, a confirmation taken back."
  ]
}
```

| Field | Meaning |
|---|---|
| `format` | 1. A later format gets a later number; within one, the index only gains fields |
| `made_at` | When the archive was made. Every file in it carries this moment |
| `account` | The handle of the account |
| `files` | Every file but the flight files, `index.json` and `README.txt`: its `path`, what it `holds` in words, the `record` by the name the [description of the API](#writing-a-program-against-venti) gives it, how many `entries` its list has, and the `licence` (`name`, `url`) its records stand under where the kind has one. Only `sites.json` has one, CC0 1.0: everything else is the pilot's own and was given to nobody |
| `flights` | Every flight, newest first: its `id` and the `file` it has in the archive |
| `complete` | `false` when a flight is listed without a `file`: the archive had no room for it, or the flight was deleted while the archive was made |
| `not_included` | What the account has on the server that the archive does not hold, in words |

**Reading it back.** A flight file uploaded to an account ([`POST /v1/flights`](#post-v1flights)) makes the flight there again, with everything the server works out of it; who may see it is set anew. Nothing else of an archive is read back by the server today.

**The bounds.** None of them is a setting.

- An account asks for its archive at most 4 times a day; after that `429` with `rate_limited` and `Retry-After`.
- The server makes one archive at a time, in its memory. Whoever asks meanwhile gets `503` with `unavailable` and `Retry-After: 60`, and that request is not counted.
- An archive holds at most 512 MB of flight files, counted as they were uploaded, the newest flights first. A flight beyond that is in `flights.json` and in the index without a `file`, `complete` is `false`, and its file is fetched by itself with `GET /v1/flights/{id}/igc`. Hundreds of flights fit; an account with several thousand long ones meets the bound.
- The server gives up after 120 seconds (`503` with `timeout`).

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

An account made with Google sends `{"google_id_token": "…"}`, a fresh ID token of its Google account, in place of the password; another Google account's gives `403` with `invalid_google_token`.

Deletes the account with everything that belongs to it: its sessions, its tokens for programs, [the sites it keeps as its own](#the-sites-a-pilot-keeps), [its alarms](#alarms) and what they fired, its flights and their files, its live tracks with their positions, and what it put into [groups](#groups): the plans it posted and its "I am in" on the plans of others. It leaves every group it is in. A group it kept is kept by the member who has been in it longest, under the name it had; a group it was alone in is deleted. The answer is `204` with no body. It happens at once and cannot be undone; the handle is free again afterwards. A wrong password gives `403` with `wrong_password`, counted as for changing the password.

To take everything along first, ask for [the archive](#get-v1accountarchive).

What becomes of each kind of thing the account gave:

| Kind | What deleting the account does |
|---|---|
| The account, its password, its sessions, its [tokens for programs](#tokens-for-other-programs) | Gone. The handle is free for anyone |
| Flights and their files, private or public | Gone, with the record of the flights it had deleted. A flight that was public is listed as withdrawn in [`GET /v1/flights/public/changes`](#get-v1flightspublicchangessincepilotlimit), by a hash of its address and without a pilot, so that whoever copied it removes the copy |
| Live tracks with their positions | Gone, with the identifiers they used |
| [The sites it keeps as its own](#the-sites-a-pilot-keeps) | Gone |
| Its places in [groups](#groups) | Gone: it leaves every group. A group it kept passes to the member who has been in it longest; a group it was alone in is deleted, with its plans |
| The [plans](#plans-for-a-flying-day) it posted | Gone, with who was in on them |
| Its "I am in" on the plans of others | Gone; the plans stay |
| The [reports](#reports) it sent | Gone, open or closed |
| Reports others sent about it, its flights, its plans or a group it kept | Gone too, with the words they kept of a plan or a group's name |
| The [sites it added and the revisions it wrote](#sites-that-ventis-partners-and-pilots-add) | **They stay**, every revision with its record and its comment: they were given to everyone under CC0 1.0. They name no author from then on: `created_by`, `changed_by` and a revision's `by` are left out, and so is `by` of a site it removed in [`GET /v1/sites/changes`](#get-v1siteschangeslayersincelimit). No change is listed for it, so a copy made earlier still has the name |
| Its confirmations of sites ("I took off here") | Gone: a confirmation is the pilot's word, not a record given to everyone. The site's count is one less; the site is not listed as changed for it |
| The invite code the account was made with | Stays as used, without saying by whom |

Not gone at once are the copies in the server's nightly backups, which the operator keeps for fourteen days unless configured otherwise; see [deploy/README.md](../deploy/README.md#backup). Live tracks are in no backup, and neither are plans; the groups an account was in are.

## Tokens for other programs

A pilot who wants a program that is not Venti's own app to act for them (a logger that uploads flights, a script, another maker's app) makes a **token** for it. The program never learns the pilot's password, can do only what the pilot allowed, and is shut out by the pilot at any time. This section can be read alone by whoever builds such a program.

**How a pilot makes one.** Signed in to their account, the pilot names the program, picks what it may do (the **scopes**), and confirms with the password: [`POST /v1/account/tokens`](#post-v1accounttokens). The answer shows the token's **secret** once. The pilot gives the secret to the program. The apps have no screen for it yet; the request below is all there is to it.

**How a program sends it.** As a session's token is sent, in the one header, with every request:

```
Authorization: Bearer venti_pat_3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz01
```

A secret is `venti_pat_` and 52 characters, 62 in all; a session's token is `venti_` and 52 characters. The prefix is there so that one found in a log or a repository is recognised for what it is. The server keeps a hash of the secret and cannot show it again: a pilot who lost one withdraws the token and makes another. A program keeps the secret where secrets are kept and never in a URL.

**What needs no token.** Everything anyone may read: stations, airspace, sites and their history, forecasts, public flights, pilots' pages, the ranking, public live tracks, a flight or a live track by its link. A request that carries a token is answered `no-store`, so a program that only reads what is public sends none.

**The scopes.** A token has at least one. A scope opens what is the pilot's own, for the one account the token was made for.

| Scope | What it opens |
|---|---|
| `account:read` | The account: handle, display name, since when it exists, whether its publishing is suspended |
| `flights:read` | The pilot's flights, the private ones among them: the list, what changed since, each flight and its file |
| `flights:write` | Uploading flights, changing who may see one, deleting one |
| `live:write` | Sending the positions of a live track, and ending one |
| `sites:write` | Adding, correcting, removing and restoring [sites that pilots make](#sites-that-ventis-partners-and-pilots-add) |
| `groups:read` | The pilot's groups, their members and their plans |
| `groups:write` | Everything a pilot writes in groups: making, renaming and deleting a group, a new invite code, handing the keeping over, removing a member, joining and leaving, plans, and "I am in" |
| `reports:write` | Sending a report |
| `kept-sites:read` | [The sites the pilot keeps as their own](#the-sites-a-pilot-keeps): what changed among them since |
| `kept-sites:write` | Keeping a site as the pilot's own, and forgetting one |
| `alerts:read` | [The pilot's alarms](#alarms), and what they fired |
| `alerts:write` | Setting, changing and removing an alarm |
| `social:read` | What is the pilot's own [among pilots](#pilots-among-each-other): the flights of the pilots they follow, everyone they follow, what they were told, and in a public list whether they follow or cheered |
| `social:write` | Following a pilot, cheering a flight, writing a comment and removing one, in the pilot's name |

**Which request needs which scope.** A session, which Venti's own apps get by signing in, may do all of it.

| Request | Needs |
|---|---|
| `GET /v1/account` | `account:read` |
| `GET /v1/flights` | `flights:read` |
| `GET /v1/flights/changes` | `flights:read` |
| `GET /v1/flights/{id}`, `GET /v1/flights/{id}/igc`, `GET /v1/flights/{id}/track` | Nothing for a flight that is public or shared by link. `flights:read` for the pilot's own private flight |
| `POST /v1/flights` | `flights:write` |
| `PATCH /v1/flights/{id}` | `flights:write` |
| `DELETE /v1/flights/{id}` | `flights:write` |
| `POST /v1/live/positions` | `live:write` |
| `DELETE /v1/live/{id}` | `live:write` |
| `POST /v1/sites` | `sites:write` |
| `PUT /v1/sites/{id}` | `sites:write` |
| `DELETE /v1/sites/{id}` | `sites:write` |
| `POST /v1/sites/{id}/revisions` | `sites:write` |
| `PUT /v1/sites/{id}/confirmation` | `sites:write` |
| `DELETE /v1/sites/{id}/confirmation` | `sites:write` |
| `GET /v1/groups` | `groups:read` |
| `GET /v1/groups/{id}` | `groups:read` |
| `GET /v1/groups/{id}/plans` | `groups:read` |
| `GET /v1/groups/plans` | `groups:read` |
| `POST /v1/groups` | `groups:write` |
| `POST /v1/groups/join` | `groups:write` |
| `PATCH /v1/groups/{id}` | `groups:write` |
| `DELETE /v1/groups/{id}` | `groups:write` |
| `POST /v1/groups/{id}/invite` | `groups:write` |
| `PUT /v1/groups/{id}/keeper` | `groups:write` |
| `DELETE /v1/groups/{id}/members/{handle}` | `groups:write` |
| `POST /v1/groups/{id}/plans` | `groups:write` |
| `PUT /v1/groups/{id}/plans/{plan}` | `groups:write` |
| `DELETE /v1/groups/{id}/plans/{plan}` | `groups:write` |
| `PUT /v1/groups/{id}/plans/{plan}/in` | `groups:write` |
| `DELETE /v1/groups/{id}/plans/{plan}/in` | `groups:write` |
| `POST /v1/reports` | `reports:write` |
| `GET /v1/account/kept-sites/changes` | `kept-sites:read` |
| `PUT /v1/account/kept-sites/{id}` | `kept-sites:write` |
| `DELETE /v1/account/kept-sites/{id}` | `kept-sites:write` |
| `GET /v1/account/alerts` | `alerts:read` |
| `GET /v1/account/alerts/fired` | `alerts:read` |
| `POST /v1/account/alerts` | `alerts:write` |
| `PUT /v1/account/alerts/{id}` | `alerts:write` |
| `DELETE /v1/account/alerts/{id}` | `alerts:write` |
| `POST /v1/account/push` | `alerts:write` |
| `DELETE /v1/account/push` | `alerts:write` |
| `PUT /v1/pilots/{handle}/follow` | `social:write` |
| `DELETE /v1/pilots/{handle}/follow` | `social:write` |
| `GET /v1/account/following` | `social:read` |
| `GET /v1/account/feed` | `social:read` |
| `GET /v1/account/notifications` | `social:read` |
| `PUT /v1/flights/{id}/cheer` | `social:write` |
| `DELETE /v1/flights/{id}/cheer` | `social:write` |
| `POST /v1/flights/{id}/comments` | `social:write` |
| `DELETE /v1/flights/{id}/comments/{comment}` | `social:write` |
| `PATCH /v1/flights/{id}/comments` | `flights:write` |
| `GET /v1/sessions` | the session only |
| `DELETE /v1/sessions` | the session only |
| `DELETE /v1/sessions/{id}` | the session only |
| `PUT /v1/account/password` | the session only |
| `DELETE /v1/account` | the session only |
| `GET /v1/account/archive` | the session only |
| `POST /v1/account/tokens` | the session only |
| `GET /v1/account/tokens` | the session only |
| `DELETE /v1/account/tokens/{id}` | the session only |
| `POST /v1/oauth/requests/{id}` | the session only |
| `POST /v1/account/google` | the session only |
| `DELETE /v1/account/google` | the session only |

"The session only" is what no token may do, whatever its scopes: see or end the pilot's sessions, change the password, delete the account, take [the archive of everything the account gave](#get-v1accountarchive), and make, list or withdraw tokens. A token therefore cannot widen itself or outlive its withdrawal.

**The errors a program meets.**

| Status | `code` | When |
|---|---|---|
| 401 | `unauthorized` | The token was withdrawn, has ended or was never given out. The three are answered alike, and as a session that was signed out is. Nothing the program does mends it: the pilot makes a new token |
| 403 | `insufficient_scope` | The token is good and lacks the scope the request needs. `scope` in the error names it |
| 403 | `session_required` | The token is good, and the request is one of those for the session only |

A token that lacks a scope is told so:

```json
{
  "error": {
    "code": "insufficient_scope",
    "message": "this needs the scope 'groups:read', which the token does not have; the pilot makes a token with it in their account",
    "scope": "groups:read"
  }
}
```

**What a token shares with its account.** Every bound of the API is the account's, and a token has none of its own yet: the uploads a day and at a time, the room for flights, the live tracks and their fixes, the groups, plans and joins a day, the new sites and revisions a day, the reports a day. A program that uses them up uses them up for the pilot's own apps too. An account whose publishing is suspended is suspended for its tokens as well.

**What a write through a token leaves.** A revision of a [site](#get-v1sitesidhistoryafterlimit) says through what it came in: `"via": "token"`. Which token is not said to those who read the history: its name is the pilot's own word for a program of theirs. The server keeps the name with the revision, where whoever runs the server can read it. Flights, live tracks, groups, plans and reports do not record through what they came in.

### `POST /v1/account/tokens`

Needs a session's token, and the password in the body: whoever picks up an unlocked phone cannot leave with a token that outlives signing the phone out. Makes a token. An account made with Google sends `google_id_token`, a fresh ID token of its Google account, in place of `password` ([Sign in with Google](#sign-in-with-google)).

```json
{
  "name": "My logger",
  "scopes": [
    "flights:write",
    "live:write"
  ],
  "password": "demo-pilot-password"
}
```

| Field | Meaning |
|---|---|
| `name` | What the program is, in the pilot's words: 1 to 60 characters in any script, without line breaks or invisible characters. It need not differ from the names of the pilot's other tokens |
| `scopes` | At least one of the scopes above, each once |
| `expires_at` | The moment from which the token is no longer good. Left out for a token that never ends |
| `password` | The account's password |

The answer is `201`:

```json
{
  "token": {
    "id": "3kq9x2m7r4t8v1w6y0a5b3c2d4",
    "name": "My logger",
    "scopes": [
      "flights:write",
      "live:write"
    ],
    "created_at": "2026-11-18T19:05:00Z"
  },
  "secret": "venti_pat_3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz01"
}
```

| Field | Meaning |
|---|---|
| `token` | The token as [the list](#get-v1accounttokens) shows it from now on |
| `secret` | What the program sends. **Shown this once**: say so to the pilot where it is shown, and let them copy it before they leave |

| Status | `code` | When |
|---|---|---|
| 400 | `invalid_token_name` | The name is empty, longer than 60 characters, or holds a line break or an invisible character |
| 400 | `invalid_scopes` | No scope, one named twice, or a name that is no scope's |
| 400 | `invalid_expiry` | `expires_at` is not a moment that is still to come |
| 403 | `wrong_password` | The password is wrong. Counted for the device as for [changing the password](#put-v1accountpassword) |
| 409 | `too_many_tokens` | The account holds 20 tokens, those that have ended among them. No older one makes room: withdraw one first |

### `GET /v1/account/tokens`

Needs a session's token. Every token the account holds, the newest first. This is the demo account's:

```json
{
  "tokens": [
    {
      "id": "00000000000000000000000003",
      "name": "Demo logger",
      "scopes": [
        "flights:read",
        "flights:write"
      ],
      "created_at": "2026-11-18T19:05:00Z"
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `id` | For withdrawing it. It is not the secret and gives no access |
| `name`, `scopes` | As the token was made with. A token cannot be changed: make another and withdraw this one |
| `created_at` | When it was made |
| `last_used_on` | The day, in UTC, on which it was last used. Left out until it is used. The server keeps the day and nothing finer, and no address |
| `expires_at` | From when it is no longer good. Left out for a token that never ends. A token whose end has come stays in the list until it is withdrawn |
| `client` | Only on a token the pilot did not make by hand: it came of the pilot's agreement on the server's page with a program that asked there, an assistant above all ([Connecting an assistant to a pilot's logbook](#connecting-an-assistant-to-a-pilots-logbook)). `id` is the identifier the server gave that program when it registered, `resource` the one address the token is good at, the connector. On such a token `name` is what the program calls itself, which nobody checked; `expires_at` is when it ends unless the program uses it before, and moves on with each use; and it ends by itself then, without staying in the list |

### `DELETE /v1/account/tokens/{id}`

Needs a session's token. Withdraws one token, by the `id` from the list. The answer is `204` with no body, and the token's very next request is answered `401`. A token the account does not have gives `404` with `token_not_found`. A token that came of an agreement with a program is withdrawn the same way: the program's access token is refused at the connector from the next request, and its refresh token gets it no new one.

Changing the password and signing out everywhere are about sessions and leave the tokens as they are. Deleting the account withdraws them all.

## Connecting an assistant to a pilot's logbook

A pilot's own assistant (Claude, ChatGPT, any program that speaks the Model Context Protocol) reads that pilot's logbook over the [connector](mcp.md#the-pilots-own-logbook), the private flights too, without the pilot copying a secret: the assistant sends the pilot to a page of the Venti server, the pilot agrees (in the web app, where they are signed in already, or on the page itself with handle and password), and the assistant is given a token that reads flights and does nothing else, which the pilot withdraws at any time. This is OAuth 2.1 with PKCE, as the protocol's authorization chapter asks for it, and the Venti server is both the resource (the connector) and the authorization server.

Built on the server on 2026-11-22 (owner decision 16), against the protocol's revision 2026-07-28 and the draft after it, the OAuth 2.1 draft (draft-ietf-oauth-v2-1-13), RFC 9728, RFC 8414, RFC 8707, RFC 7636, RFC 7591, RFC 7009 and RFC 9207, each read on that day. **Nothing is deployed and no assistant has been connected: *to verify* with Claude and ChatGPT over HTTPS.** What was run is the server's own tests, which play the program, the pilot's browser and the attacker through the real router (`crates/server/tests/oauth.rs`).

These endpoints are not part of `/v1` and not in [openapi.json](openapi.json): they answer in OAuth's own shapes, and describe themselves in the two documents of steps 2 and 3. The two calls with which the web app reads a request and answers it are part of `/v1` and are described there: [Agreeing in the web app](#agreeing-in-the-web-app). A server that is reached over plain HTTP from another machine offers none of this: every address below answers `404` there, and its connector lists no logbook tools.

**What every public tool and every read of the API keeps:** they stay open without a token, at the same address. Only the three tools of the logbook ask for one.

### How an assistant connects, step by step

1. **It asks for the logbook and is told where a token is got.** A call of `logbook_statistics`, `list_flights` or `get_flight` at `/mcp` without a token is answered:

   ```
   HTTP/1.1 401 Unauthorized
   WWW-Authenticate: Bearer resource_metadata="https://venti.example/.well-known/oauth-protected-resource/mcp", scope="flights:read"
   ```

2. **It reads what the connector is and who gives out tokens for it** (RFC 9728): `GET /.well-known/oauth-protected-resource/mcp`, which is also served at `/.well-known/oauth-protected-resource`.

   ```json
   {
     "resource": "https://venti.example/mcp",
     "authorization_servers": ["https://venti.example"],
     "scopes_supported": ["flights:read"],
     "bearer_methods_supported": ["header"],
     "resource_name": "Venti"
   }
   ```

3. **It reads the authorization server's description** (RFC 8414): `GET /.well-known/oauth-authorization-server`.

   ```json
   {
     "issuer": "https://venti.example",
     "authorization_endpoint": "https://venti.example/oauth/authorize",
     "token_endpoint": "https://venti.example/oauth/token",
     "registration_endpoint": "https://venti.example/oauth/register",
     "revocation_endpoint": "https://venti.example/oauth/revoke",
     "scopes_supported": ["flights:read"],
     "response_types_supported": ["code"],
     "response_modes_supported": ["query"],
     "grant_types_supported": ["authorization_code", "refresh_token"],
     "token_endpoint_auth_methods_supported": ["none"],
     "revocation_endpoint_auth_methods_supported": ["none"],
     "code_challenge_methods_supported": ["S256"],
     "authorization_response_iss_parameter_supported": true,
     "client_id_metadata_document_supported": false
   }
   ```

4. **It registers itself** ([`POST /oauth/register`](#post-oauthregister)) and gets a `client_id` and no secret.
5. **It sends the pilot's browser to the page** ([`GET /oauth/authorize`](#get-oauthauthorize)) with its `client_id`, the address to come back to, a `state` and a PKCE challenge of its own making, and `resource=https://venti.example/mcp`.
6. **The pilot agrees.** The page links into the web app, where the pilot is signed in already and answers with that session, however they signed in, with Google too ([Agreeing in the web app](#agreeing-in-the-web-app)); or the pilot signs in on the page itself with handle and password. Either way the browser is sent back to the program's address with `code`, the program's `state`, and `iss=https://venti.example`, which the program compares with the `issuer` of step 3 (RFC 9207). A pilot who refuses is sent back with `error=access_denied`.
7. **It exchanges the code** ([`POST /oauth/token`](#post-oauthtoken)) within two minutes, with the verifier of its challenge, for an access token (`venti_oat_…`, good for an hour) and a refresh token (`venti_ort_…`).
8. **It calls the connector** with `Authorization: Bearer venti_oat_…` on every request.
9. **When the access token has ended** (`401` with `error="invalid_token"`), it uses the refresh token at the same endpoint and gets a new pair. The refresh token it used is worth nothing from then on.
10. **It ends** when the pilot withdraws the token in their account ([`DELETE /v1/account/tokens/{id}`](#delete-v1accounttokensid)), when the program gives it up ([`POST /oauth/revoke`](#post-oauthrevoke)), or by itself when the program has not used its refresh token for 180 days.

### What is given out

- **One scope, `flights:read`**, which at the connector opens the three tools of the pilot's logbook. No scope that writes is given out: a request that names one is refused, at registration, on the page and at a refresh. A request that names no scope gets `flights:read`.
- **For the connector and nowhere else.** `resource` must be the connector's address as step 2 names it (the scheme and the host may be written in upper case; nothing else may differ), on the page and again at the exchange. The token is bound to that address and to the program. At the API (`/v1`) it is no token at all, whatever its scope: `401`. And a token a pilot made by hand for the API, or a session of Venti's apps, is no token at the connector.
- **Listed among the pilot's tokens** ([`GET /v1/account/tokens`](#get-v1accounttokens)), under the name the program gave itself, with a `client` that says it came this way. An agreement counts as one of the account's 20 tokens. A pilot who agrees again with the same program replaces the earlier agreement and its tokens.
- **Its budget** is a token's: 3,600 requests an hour at the connector, counted for the agreement, across refreshes ([Limits](#limits)).
- **The secrets** are 256 random bits each behind a prefix that says what they are: `venti_oac_` a code, `venti_oat_` an access token, `venti_ort_` a refresh token. The server keeps a hash of each, hashed apart from one another and from sessions and tokens made by hand, and cannot show one again.

| What | Lasts |
|---|---|
| A code | 2 minutes, and one exchange |
| An access token | 1 hour |
| A refresh token | One use. Unused, 180 days |
| The agreement | Until the pilot withdraws it or the program gives it up; else 180 days after the program's latest refresh |
| The page, and the request it links into the web app with | A quarter of an hour from when it was shown |

### `POST /oauth/register`

Dynamic client registration (RFC 7591), for public clients only. The body is JSON:

```json
{
  "client_name": "Assistant",
  "redirect_uris": ["https://assistant.example/oauth/callback"],
  "token_endpoint_auth_method": "none",
  "grant_types": ["authorization_code", "refresh_token"],
  "response_types": ["code"]
}
```

The answer is `201`:

```json
{
  "client_id": "5v8k2m9q3r7t1w6x0y4a2b3c4d",
  "client_id_issued_at": 1795028700,
  "client_name": "Assistant",
  "redirect_uris": ["https://assistant.example/oauth/callback"],
  "token_endpoint_auth_method": "none",
  "grant_types": ["authorization_code", "refresh_token"],
  "response_types": ["code"],
  "scope": "flights:read"
}
```

| Field | Rule |
|---|---|
| `redirect_uris` | One to five addresses, each at most 400 bytes of plain ASCII. `https://` with a host name; or `http://` to `127.0.0.1`, `[::1]` or `localhost`, for a program on the pilot's own device. No other scheme (so no app's own scheme), no user name, no fragment |
| `client_name` | One line of at most 60 characters. Shown to the pilot as the program's own claim, which nobody checked. Left out, the program is called "A program without a name" |
| `token_endpoint_auth_method` | `none`, or left out. A program gets no secret |
| `grant_types`, `response_types` | Nothing but `authorization_code`, `refresh_token` and `code` |
| `scope` | `flights:read`, or left out |
| Anything else (`logo_uri`, `client_uri`, `application_type`, …) | Not read, not kept, and no address in it is ever fetched |

The server fetches nothing when a program registers or afterwards. The newer way of the protocol, a `client_id` that is an HTTPS address of a document the server fetches (client ID metadata documents), is not offered: `client_id_metadata_document_supported` is `false` ([architecture.md](architecture.md#bring-your-own-ai) says what it would need).

Refusals are `400` with RFC 7591's codes, `invalid_redirect_uri` or `invalid_client_metadata`, in OAuth's shape for errors: `{"error": "…", "error_description": "…"}`.

**Bounds.** An address registers 10 programs an hour (an IPv6 /48 network 40): beyond that `429` with `Retry-After`. The server keeps 50,000 programs. When it is full, a new one takes the place of the one that registered longest ago among those no pilot ever agreed to; if there is none, `503`. A program that holds no agreement and has not been used for 30 days is removed. A program whose registration is gone registers again.

### `GET /oauth/authorize`

The page, for the pilot's browser. The query:

| Parameter | Rule |
|---|---|
| `response_type` | `code` |
| `client_id` | From the registration |
| `redirect_uri` | One of the registered addresses, character for character. The one exception is OAuth 2.1's: for an address of the pilot's own device (`http://127.0.0.1…`, `http://[::1]…`, `http://localhost…`) the port may differ, since such a program listens on whichever is free |
| `code_challenge`, `code_challenge_method` | The PKCE challenge of RFC 7636 and `S256`. Without them, or with `plain`, nothing is given out |
| `resource` | The connector's address, `https://venti.example/mcp` (RFC 8707) |
| `scope` | `flights:read`, or left out |
| `state` | The program's own, at most 512 bytes; handed back as it came |

The page says which program asks, by the name it gave itself and marked as its own claim; the address the pilot will be sent back to; what the program may read and what it may not; and for how long. Above its form the page has a link, "Continue in Venti", to the web app's page `/connect/?request=<identifier>`: there the pilot answers with the session they have, without a password ([Agreeing in the web app](#agreeing-in-the-web-app)). That is the way for an account that signs in with Google, which has no password. In the form the pilot types handle and password, and presses "Sign in and agree" or "Refuse".

- **A request that passes every check is kept for a quarter of an hour** under an identifier of 256 random bits (`venti_oar_…`), of which the server keeps a hash. Each showing of the page keeps one of its own. At most 20,000 wait at once; while that many do, the page has its form alone. The link carries the identifier and nothing the program sent.
- **A request that cannot be agreed to is answered on a page, and nobody is redirected by the server.** If the program is not registered or the address to go back to is not one it registered, the page says so and offers no way on. If the address is good and something else is not (`resource`, the challenge, a scope that writes, `response_type`), the page says so and has a link back to the program that carries OAuth's `error`, `error_description`, `state` and `iss`. The pilot's browser is redirected only as the answer to the pilot pressing a button.
- **The password has the sign-in's limits**, counted together with [`POST /v1/sessions`](#post-v1sessions): 10 wrong ones an hour for a handle, 30 for an address. A wrong one shows the page again.
- **The page is shown in no frame** (`X-Frame-Options: DENY`, `frame-ancestors 'none'`), runs no script and loads nothing (`default-src 'none'`, its one style allowed by its hash), is kept by nobody (`no-store`), and its form goes to this server and, by the redirect that answers it, to the origin of the program's address and nowhere else.
- **Its form is taken only from the page itself.** Each showing of the page carries a random value in the form and the same value in a cookie, `__Host-venti-consent` (`HttpOnly`, `SameSite=Strict`, `Secure`, a quarter of an hour). `POST /oauth/authorize` is answered `403` unless both arrive and match, and unless the `Origin` header, where a browser sends one, is the server's own. This is the only cookie the server sets. It names no account and no session: nothing the pilot does elsewhere on Venti is signed in by it.
- **Nothing of a request is written to the log** but the path: no query, no `state`, no name of a program, no code. Of the path of the web app's two calls the identifier is left out.

### Agreeing in the web app

The web app is served at the server's own address and holds the pilot's session, however the pilot signed in. So a pilot agrees there, with the session they have: no password is typed a second time, an account that signs in with Google agrees like any other, and the server sends nobody to Google for it. Built on the server on 2026-11-22, and the web app's page `/connect/` the same day (`837c590e`; what it shows in each state is in [design.md](design.md#account-and-sharing)).

- **The page of [`GET /oauth/authorize`](#get-oauthauthorize) links to `/connect/?request=<identifier>`** once the program's request has passed every check. The server redirects nobody there: the pilot presses the link.
- **Whoever holds the identifier reads the request**; it names no account. **Only a pilot's own session answers it.** A token that a pilot gave to a program gets `403` `session_required` whatever its scopes, and a token the authorization server gave out is no token at `/v1` at all (`401`): a program can never give itself or another program access.
- **The agreement is the account's whose session answers.** Nothing in the answer names an account, a program or an address; a body with any other field than `agree` is refused (`400` `invalid_request`).
- **A request is answered once.** After that both calls say `410` `authorization_request_answered` until the request's end, and hand out nothing of it. From its end on, and for an identifier nobody was given, both say `404` `authorization_request_not_found`.
- **What stands against agreeing is said before the request is used up**, so that the pilot sets it right and answers the same request: `409` `too_many_tokens` when the account holds its 20 tokens (an agreement with the same program is replaced and does not count), `403` `publishing_suspended` for an account whose publishing whoever runs the server [suspended](#what-whoever-runs-the-server-can-do). Such an account may still refuse.
- **No error and no line of the log holds the identifier, a code, or anything the program sent.**

#### `GET /v1/oauth/requests/{id}`

Needs no token. What the pilot must see before answering:

```sh
curl 'https://venti.example/v1/oauth/requests/venti_oar_0000000000000000000000000000000000000000000000000000'
```

```json
{
  "program": "Assistant",
  "returns_to": "assistant.example",
  "on_this_device": false,
  "scopes": [
    "flights:read"
  ],
  "may_read": [
    "Your flights, the private ones and those you share by link too: when and where you flew, how long, how far and how high, the score and your glider."
  ],
  "may_not": "Change, delete or upload anything, or see anything else of your account.",
  "lasts": "Until you withdraw it: it is listed in your Venti account among the programs with access. It also ends by itself when the program has not used it for 180 days.",
  "expires_at": "2026-11-18T19:20:00Z"
}
```

| Field | Meaning |
|---|---|
| `program` | What the program calls itself. **Its own claim**: nobody checked who it is, and whoever shows it says so |
| `returns_to` | The host of the address the browser is sent back to with the answer: one the program registered |
| `on_this_device` | `true` when that address is a program on the device the browser runs on (`http://127.0.0.1…`, `http://[::1]…`, `http://localhost…`), which the server cannot tell from any other program there. Whoever shows the request warns the pilot to agree only if they started this themselves |
| `scopes` | What is asked for. Today always `flights:read` |
| `may_read`, `may_not`, `lasts` | The same in the server's words, as its own page says them: what the program may read, one sentence for each thing; what it may not do; for how long the agreement holds |
| `expires_at` | From this moment the request can no longer be answered |

The program's `state` and its challenge are not handed out. The answer is `no-store`.

#### `POST /v1/oauth/requests/{id}`

Needs a session's token. The body is `{"agree": true}` or `{"agree": false}`:

```sh
curl -X POST -H 'Authorization: Bearer venti_0000000000000000000000000000000000000000000000000000' \
  -H 'Content-Type: application/json' -d '{"agree": true}' \
  'https://venti.example/v1/oauth/requests/venti_oar_0000000000000000000000000000000000000000000000000000'
```

```json
{
  "redirect_to": "https://assistant.example/oauth/callback?code=venti_oac_3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz01&state=state-7f3a%2F%2B%3D%26%3F&iss=https%3A%2F%2Fventi.example"
}
```

`redirect_to` is where the web app sends the browser: the address the program registered, character for character (for a program on the pilot's own device with the port it asked with), followed by `code`, the program's `state` and `iss`; after a refusal by `error=access_denied`, `error_description`, `state` and `iss`. It is made of what the server kept when the request passed its checks and of nothing the web app sends. With a code in it, it is a secret for the two minutes the code lasts: the web app navigates to it and keeps it nowhere. From there on everything is as after the server's own page: step 7 above.

### `POST /oauth/token`

A form (`application/x-www-form-urlencoded`), each parameter once. A program sends its `client_id` and never a secret: a request with `client_secret` or an `Authorization` header is refused.

For a code: `grant_type=authorization_code`, `client_id`, `code`, `code_verifier`, `resource` (the connector's address again), and, if the program likes, the `redirect_uri` it was sent back to. For a refresh: `grant_type=refresh_token`, `client_id`, `refresh_token`, and if it likes `resource` and `scope`, neither of which may ask for more than was agreed to. The answer to both, with `Cache-Control: no-store`:

```json
{
  "access_token": "venti_oat_3x7k9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz01",
  "token_type": "Bearer",
  "expires_in": 3600,
  "refresh_token": "venti_ort_9q2m4r8t1v6w0y5a2b3c4d5e6f7g8h9j0kmnpqrstvwxyz013x7k",
  "scope": "flights:read"
}
```

| Status | `error` | When |
|---|---|---|
| 400 | `invalid_request` | Not a form, a parameter twice, or one that is needed is missing |
| 401 | `invalid_client` | No program is registered under `client_id`, or the request carries a secret |
| 400 | `invalid_grant` | The code or the refresh token is unknown, was used, has ended or belongs to another program; the verifier does not answer the challenge; `redirect_uri` is not the one the code was sent to. One answer for all of them |
| 400 | `invalid_target` | `resource` is missing at an exchange, or is not the connector |
| 400 | `invalid_scope` | A refresh asks for another scope than was agreed to |
| 400 | `unsupported_grant_type` | Any other `grant_type` |
| 429 | `too_many_requests` | Too many requests from the address; with `Retry-After` |

- **A code is exchanged once.** A second exchange with everything else in order gets nothing and withdraws what the first one gave: a code that turns up twice was seen by somebody else. An attempt with a wrong verifier, or by another program, changes nothing: whoever holds a code and not its verifier neither gets tokens nor takes any from the program that does.
- **A refresh token is used once.** Each refresh replaces the access token and the refresh token. A refresh token that is used a second time withdraws the whole agreement, with every token of it: one of its two users is not the program. The pilot then agrees anew.

### `POST /oauth/revoke`

A program gives an agreement up (RFC 7009): a form with `client_id` and `token`, which is its access token or its refresh token. Either ends the whole agreement, and it leaves the pilot's list. The answer is `200` with no body, also for a token that is not good or not this program's, of which nothing is done. `401` `invalid_client` for a `client_id` nobody registered.

**Bounds.** The page, its answer, the web app's two calls, the exchange and giving up are counted together for the address they come from: 60 a minute (an IPv6 /48 network 240), beyond that `429` with `Retry-After`. A body has at most 16 kB, and an answer is given up after 10 seconds. The two documents under `/.well-known` are the same for everyone and may be kept for an hour.

## The sites a pilot keeps

A pilot marks flying sites as their own: the take-offs they fly from. Without an account the list is on the device alone. With one it is on the account too, so that a site kept on the phone is kept in the browser, and it is kept in step as [flights are](#get-v1flightschangessincelimit): a change number per account, "what changed since", and what was forgotten.

- **It is private.** Every request here acts for the account of its token and reads or writes that account's list. Nothing else in the API hands a kept site to anybody: no pilot's page, no list, no connector. [Deleting the account](#delete-v1account) removes the list.
- **A kept site is a copy, not a site of its own.** It is the site's canonical record as its pilot's device had it when it was kept, of whatever source ([OpenStreetMap's](#get-v1sitesbboxwestsoutheastnorth) or [the layer pilots make](#sites-that-pilots-make)), with that source's credit and the date of the source's data, so that the list can be shown without asking for the source's sites. The record stays its source's, under its source's licence: whoever shows a kept site shows its `attribution`. The server does not look the site up in its own sets, because a pilot keeps a site also when its source has since dropped it; it checks that the record is one a site may have and hands it back as it took it.
- **Under the site's own identifier.** The address of a kept site is the site's identifier, written into the path as it is: `/v1/account/kept-sites/osm-free-flying:way/1080986173`.
- **Bounds.** An account keeps at most 200 sites. A body has at most 16 kB, and each text of a credit at most 500 characters. Of the sites an account has forgotten the server remembers the 200 forgotten last.
- **A token for a program** needs `kept-sites:read` to read and `kept-sites:write` to keep and forget ([tokens](#tokens-for-other-programs)).

### `GET /v1/account/kept-sites/changes?since=&limit=`

Needs a token. What happened to the account's kept sites since the device last asked.

| Parameter | Meaning | Default |
|---|---|---|
| `since` | The `cursor` of the answer before | Everything |
| `limit` | How many changes to return, 1 to 100 | 50 |

```sh
curl -H 'Authorization: Bearer venti_0000000000000000000000000000000000000000000000000000' \
  'http://127.0.0.1:8080/v1/account/kept-sites/changes'
```

The development mode's demo account keeps no site when it starts. After the request [below](#put-v1accountkept-sitesid), and after one more site was kept and forgotten again:

```json
{
  "kept": [
    {
      "site": {
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
        "url": "https://www.openstreetmap.org/way/1080986173",
        "website": "https://www.dhv.de/",
        "fee": "yes",
        "outline": [
          [
            11.9055085,
            47.6789575
          ],
          [
            11.9055532,
            47.6793075
          ],
          [
            11.9048395,
            47.6794382
          ],
          [
            11.9046533,
            47.6788445
          ],
          [
            11.9055085,
            47.6789575
          ]
        ]
      },
      "attribution": {
        "source": "osm-free-flying",
        "provider": "OpenStreetMap contributors",
        "licence": "ODbL 1.0",
        "text": "© OpenStreetMap contributors",
        "url": "https://www.openstreetmap.org/copyright",
        "notice": "This data is OpenStreetMap's and is available under the Open Database License, https://opendatacommons.org/licenses/odbl/1-0/. Anyone can correct it: https://www.openstreetmap.org/fixthemap"
      },
      "as_of": "2026-11-18T19:05:00Z",
      "kept_at": "2026-11-18T17:30:00Z"
    }
  ],
  "forgotten": [
    "osm-free-flying:node/5964676196"
  ],
  "cursor": "1795028700000002",
  "more": false,
  "reset": false
}
```

| Field | Meaning |
|---|---|
| `kept` | Sites that were kept, or kept anew, each as it is now, oldest change first. A site appears once |
| `kept[].site` | The site's canonical record, as in [a layer of sites](#get-v1sitesbboxwestsoutheastnorth) |
| `kept[].standing` | For a site that pilots made: who added it and who changed it last, as the server said when it was kept. Left out for a source's site |
| `kept[].attribution` | The credit of the record's source, as in every answer's `attributions`. Left out if the device sent none |
| `kept[].as_of` | The moment the source's data was of when the record was last seen. Left out if the device sent none |
| `kept[].kept_at` | When the pilot kept it |
| `forgotten` | The identifiers of sites that are no longer kept |
| `cursor` | Pass this as `since` next time. Keep it per account |
| `more` | `true` when more changes are waiting: ask again at once with `cursor` |
| `reset` | `true` when the server could not answer from the `since` it was given and started from the beginning. See below |

How a device keeps in step: forget on the server what was forgotten on the device; ask for changes since its stored cursor until `more` is `false`, take each of `kept` and drop each of `forgotten`; then send what was kept on the device; store the cursor.

`reset` is `true` when `since` lies beyond the account's last change (the server was restored from a backup, or the cursor is another account's), and when it lies before the oldest forgetting the server still remembers. The answer then holds everything the account keeps, from the beginning: a site the device holds as the account's and does not find in it is no longer kept. `since` that is no cursor gives `400` `invalid_cursor`.

### `PUT /v1/account/kept-sites/{id}`

Needs a token. Keeps a site as the account's own. The body is the site as the device has it:

```json
{
  "site": {
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
    "url": "https://www.openstreetmap.org/way/1080986173",
    "website": "https://www.dhv.de/",
    "fee": "yes",
    "outline": [
      [
        11.9055085,
        47.6789575
      ],
      [
        11.9055532,
        47.6793075
      ],
      [
        11.9048395,
        47.6794382
      ],
      [
        11.9046533,
        47.6788445
      ],
      [
        11.9055085,
        47.6789575
      ]
    ]
  },
  "attribution": {
    "source": "osm-free-flying",
    "provider": "OpenStreetMap contributors",
    "licence": "ODbL 1.0",
    "text": "© OpenStreetMap contributors",
    "url": "https://www.openstreetmap.org/copyright",
    "notice": "This data is OpenStreetMap's and is available under the Open Database License, https://opendatacommons.org/licenses/odbl/1-0/. Anyone can correct it: https://www.openstreetmap.org/fixthemap"
  },
  "as_of": "2026-11-18T19:05:00Z",
  "kept_at": "2026-11-18T17:30:00Z"
}
```

`site` is required and its `id` is the identifier in the path; `standing`, `attribution` (whose `source` is the site's source), `as_of` and `kept_at` may be left out. The answer is `200` with the site as the account keeps it now:

```json
{
  "kept": {
    "site": {
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
      "url": "https://www.openstreetmap.org/way/1080986173",
      "website": "https://www.dhv.de/",
      "fee": "yes",
      "outline": [
        [
          11.9055085,
          47.6789575
        ],
        [
          11.9055532,
          47.6793075
        ],
        [
          11.9048395,
          47.6794382
        ],
        [
          11.9046533,
          47.6788445
        ],
        [
          11.9055085,
          47.6789575
        ]
      ]
    },
    "attribution": {
      "source": "osm-free-flying",
      "provider": "OpenStreetMap contributors",
      "licence": "ODbL 1.0",
      "text": "© OpenStreetMap contributors",
      "url": "https://www.openstreetmap.org/copyright",
      "notice": "This data is OpenStreetMap's and is available under the Open Database License, https://opendatacommons.org/licenses/odbl/1-0/. Anyone can correct it: https://www.openstreetmap.org/fixthemap"
    },
    "as_of": "2026-11-18T19:05:00Z",
    "kept_at": "2026-11-18T17:30:00Z"
  }
}
```

**Of two records of one site, the one kept later stays.** A site the account does not keep is kept, from `kept_at`, or from the moment the request arrives if it names none. A site the account keeps already is replaced only by a request whose `kept_at` is later than the account's; otherwise the account's record stays, nothing changes, and the answer is that record. So a device that signs in can send everything it holds: what it sends without `kept_at` fills in what the account lacks and replaces nothing. A `kept_at` in the future counts as now. A site the account had forgotten is kept again, whatever its `kept_at`: keeping wins over forgetting.

| Status | `code` | When |
|---|---|---|
| 400 | `invalid_kept_site` | The path does not end in a site's identifier; `site.id` is another; the record holds what no site record may ([the checks of a site](#post-v1sites): lengths, heights, wind sectors, nothing that does not print); `standing.id` is another site's; `attribution.source` is not the site's source, or a text of the credit is longer than 500 characters or does not print |
| 409 | `too_many_kept_sites` | The account keeps 200 sites already. Keeping one of them anew still works |
| 413 | `payload_too_large` | The body is larger than 16 kB |

### `DELETE /v1/account/kept-sites/{id}`

Needs a token. Forgets a site. The answer is `204` with no body, also when the account did not keep the site: what was asked for holds either way. The account's other devices learn of it from `forgotten`.

## Alarms

A pilot sets alarms on the account: "tell me when the wind at the Patscherkofel station is from the north at 10 to 20 km/h", "tell me when a day of the next three is a cross-country day at the Nordkette". An alarm is a fixed rule over data the server has, and the server checks it where the data arrives: a wind alarm against every reading of its station, a day alarm against every run of the [thermal map](#get-v1thermals), a pilots alarm once a minute against the live positions. What fired is kept on the account, and the pilot's apps fetch it ([`GET /v1/account/alerts/fired`](#get-v1accountalertsfiredsincelimit)). A browser the pilot subscribed is also told at once, by [Web Push](#web-push); the phone apps fetch. No mail.

- **It is private.** Every request here acts for the account of its token. [Deleting the account](#delete-v1account) removes its alarms and what they fired.
- **Three kinds**, told apart by `rule.kind`, and more to come: a client keeps a kind it does not know as it is.
  - `wind`: a station (`station`, as [`GET /v1/stations`](#get-v1stationsbboxwestsoutheastnorth) names it, and its `station_name` for the words), and any of: `directions`, the directions the wind comes from, clockwise from `from_deg` to `to_deg`, both included (349 to 56 is north to north-east, half a point either side); `min_kmh` and `max_kmh`, the mean wind, both included; `gusts_max_kmh`, the strongest gust there may be; `hours`, from `from_hour` o'clock to `to_hour` o'clock, both included. At least the directions or one of the two speeds. A reading fits when it fits every part that is set; a reading that lacks what a part asks about (a gust, a direction) does not fit.
  - `day`: a place (`place`, a site's `site` if it is one, and its `place_name` for the words), the days to look at, `days_ahead`, 0 for today to 3, and the least the day is to be, `at_least`: `flyable` (the day's verdict in the thermal map is `flyable`), `local`, `cross_country` or `big` (flyable, with thermals at least of that kind).
  - `pilots` (since 2026-11-22): "a lot of people fly, let me know". A place (`place`, a site's `site` if it is one, and its `place_name` for the words), the fewest pilots in the air, `min_pilots`, 1 to 100 (left out: 3), and for how many hours it rests after it fired, `rest_hours`, 1 to 24 (left out: 6).
- **When a wind alarm fires.** After each run of stations the server takes of each station its newest reading, if it is no older than 60 minutes, and checks the alarms that watch it. An alarm that fired rests: it fires again only for a reading at least 3 hours after the one that fired it. A change to the alarm ends the rest.
- **When a day alarm fires.** After each run of the thermal map, for each day the alarm looks at, in the cell its place lies in: when the day is at least what the alarm asks and no run said so since the last that said otherwise. A run that says less forgets it; a later one that says so again fires again. A cell without a verdict for the day changes nothing. One run can fire one alarm for several days.
- **When a pilots alarm fires.** Once a minute the server counts, for each source of live positions by itself (a network it listens to, Venti's own public live tracks), the paragliders and hang gliders in the air within 5 km of the place, as [`GET /v1/live/sites`](#get-v1livesitesbboxwestsoutheastnorth) counts them. The alarm fires when one source has at least `min_pilots`; the sources are not added up, since a pilot may be in two. Then it rests `rest_hours` hours. Nothing in what fired says who flies.
- **The clock.** The hours of a wind alarm and the "today" of a day alarm are Central European Time, with summer time as the European Union keeps it: Venti's regions lie in it. *To verify* when a region lies beyond it.
- **Bounds.** At most 20 alarms per account. A name has 1 to 60 characters, a station's or place's name 1 to 80. Speeds are at most 250 km/h. What fired is kept 30 days.
- **A token for a program** needs `alerts:read` to read and `alerts:write` to set, change and remove ([tokens](#tokens-for-other-programs)).

### `GET /v1/account/alerts`

Needs a token. The account's alarms, the oldest first. The development mode's demo account has one of each kind:

```json
{
  "alerts": [
    {
      "id": "0000000000000000000000a001",
      "name": "Patscherkofel north",
      "enabled": true,
      "rule": {
        "kind": "wind",
        "station": "geosphere-tawes:11126",
        "station_name": "Patscherkofel",
        "directions": {
          "from_deg": 349,
          "to_deg": 56
        },
        "min_kmh": 10,
        "max_kmh": 20,
        "gusts_max_kmh": 30,
        "hours": {
          "from_hour": 10,
          "to_hour": 16
        }
      },
      "created_at": "2026-11-16T19:05:00Z",
      "changed_at": "2026-11-16T19:05:00Z"
    },
    {
      "id": "0000000000000000000000a002",
      "name": "Nordkette cross-country",
      "enabled": true,
      "rule": {
        "kind": "day",
        "place": {
          "lat": 47.3,
          "lon": 11.4
        },
        "place_name": "Nordkette",
        "days_ahead": [
          0,
          1,
          2
        ],
        "at_least": "cross_country"
      },
      "created_at": "2026-11-16T19:05:00Z",
      "changed_at": "2026-11-16T19:05:00Z"
    }
  ]
}
```

### `POST /v1/account/alerts`

Needs a token. Sets an alarm. The body is `name`, `enabled` (left out: `true`) and `rule`, as [`PUT`](#put-v1accountalertsid) shows. The answer is `201` with the alarm, as `PUT` answers.

| Status | `code` | When |
|---|---|---|
| 400 | `invalid_alert` | A name that is empty, too long or does not print; a sector from and to the same direction; a least wind above the most; no directions and no speeds; hours that do not run forward; no day, a day beyond 3 or one twice; pilots or hours of rest outside their bounds; a kind the server does not know. `message` names the field |
| 409 | `too_many_alerts` | The account has 20 alarms already |

A pilots alarm:

```json
{
  "name": "Wallberg is on",
  "enabled": true,
  "rule": {
    "kind": "pilots",
    "place": {
      "lat": 47.664,
      "lon": 11.748
    },
    "place_name": "Wallberg",
    "min_pilots": 3,
    "rest_hours": 6
  }
}
```

### `PUT /v1/account/alerts/{id}`

Needs a token. Changes an alarm, by the `id` the list gives: its name, whether it is on, its rule. What the server noted of it is forgotten, so a wind alarm may fire with the next reading and a day alarm fires for what the next run says. A request, to the demo's wind alarm:

```json
{
  "name": "Patscherkofel north",
  "enabled": true,
  "rule": {
    "kind": "wind",
    "station": "geosphere-tawes:11126",
    "station_name": "Patscherkofel",
    "directions": {
      "from_deg": 349,
      "to_deg": 56
    },
    "min_kmh": 10,
    "max_kmh": 20,
    "gusts_max_kmh": 30,
    "hours": {
      "from_hour": 9,
      "to_hour": 17
    }
  }
}
```

The answer:

```json
{
  "alert": {
    "id": "0000000000000000000000a001",
    "name": "Patscherkofel north",
    "enabled": true,
    "rule": {
      "kind": "wind",
      "station": "geosphere-tawes:11126",
      "station_name": "Patscherkofel",
      "directions": {
        "from_deg": 349,
        "to_deg": 56
      },
      "min_kmh": 10,
      "max_kmh": 20,
      "gusts_max_kmh": 30,
      "hours": {
        "from_hour": 9,
        "to_hour": 17
      }
    },
    "created_at": "2026-11-16T19:05:00Z",
    "changed_at": "2026-11-18T19:05:00Z"
  }
}
```

The errors of `POST`, and `404` with `alert_not_found` for an alarm the account does not have.

### `DELETE /v1/account/alerts/{id}`

Needs a token. Removes an alarm. The answer is `204` with no body, also when the account did not have it. What it fired stays until it is 30 days old.

### `GET /v1/account/alerts/fired?since=&limit=`

Needs a token. What the account's alarms fired since the device last asked, the oldest first, of the last 30 days.

| Parameter | Meaning | Default |
|---|---|---|
| `since` | The `cursor` of the answer before | Everything kept |
| `limit` | How many to return, 1 to 100 | 50 |

Each entry has the alarm (`alert_id`, its `alert_name` and its `rule` as they were), `fired_at`, and `because`: for a wind alarm the reading (`time`, `speed_kmh`, `gust_kmh`, `direction_deg`, whole numbers), for a day alarm the day (`date`, `verdict`, `thermals`, and the `model` and `run` it is read from), for a pilots alarm the count (`time`, `pilots`, the most one source of live positions had in the air near the place, and `took_off`, how many of them took off there in the last hour). The words a pilot reads are the app's: Venti's apps take them from the core. `reset` is `true` when `since` was not a cursor of this server's; the answer then starts from the oldest kept, and a device shows again what it shows. The demo account's one:

```json
{
  "fired": [
    {
      "alert_id": "0000000000000000000000a001",
      "alert_name": "Patscherkofel north",
      "fired_at": "2026-11-18T10:23:00Z",
      "rule": {
        "kind": "wind",
        "station": "geosphere-tawes:11126",
        "station_name": "Patscherkofel",
        "directions": {
          "from_deg": 349,
          "to_deg": 56
        },
        "min_kmh": 10,
        "max_kmh": 20,
        "gusts_max_kmh": 30,
        "hours": {
          "from_hour": 10,
          "to_hour": 16
        }
      },
      "because": {
        "kind": "wind",
        "time": "2026-11-18T10:20:00Z",
        "speed_kmh": 15,
        "gust_kmh": 22,
        "direction_deg": 10
      }
    }
  ],
  "cursor": "1794997380000000",
  "more": false,
  "reset": false
}
```

What a pilots alarm fired, as a test of the server keeps it:

```json
{
  "fired": [
    {
      "alert_id": "0000000000000000000000a003",
      "alert_name": "Wallberg is on",
      "fired_at": "2026-11-18T19:05:00Z",
      "rule": {
        "kind": "pilots",
        "place": {
          "lat": 47.664,
          "lon": 11.748
        },
        "place_name": "Wallberg",
        "min_pilots": 3,
        "rest_hours": 6
      },
      "because": {
        "kind": "pilots",
        "time": "2026-11-18T19:05:00Z",
        "pilots": 3,
        "took_off": 1
      }
    }
  ],
  "cursor": "1795028700000000",
  "more": false,
  "reset": false
}
```

## Web Push

A browser can be told when an alarm fires even while no Venti page is open, by the Web Push standard: the browser's vendor runs a push service that carries the message (RFC 8030), the server encrypts the message for that one browser (RFC 8291, `aes128gcm`) and signs its request with a key pair of its own (VAPID, RFC 8292). Nothing bills by usage.

- **How a browser subscribes.** The page asks [`GET /v1/push/key`](#get-v1pushkey) for the server's public key, calls `registration.pushManager.subscribe({ userVisibleOnly: true, applicationServerKey })` with it, and sends what it gets, `subscription.toJSON()`, to [`POST /v1/account/push`](#post-v1accountpush). On signing out it sends the endpoint to [`DELETE /v1/account/push`](#delete-v1accountpush) and calls `subscription.unsubscribe()`.
- **What arrives.** When an alarm of the account fires, every browser subscribed on it gets one message for each thing it fired. The browser's service worker gets it in its `push` event, decrypted, as JSON: `title` (the alarm's name), `line` (what happened, in the words the apps show, with the times in Central European Time: "Zugspitze: 15 km/h from the north, gusts 22, at 11:20, as your alarm 'Zugspitze north' asks"), `alert_id` and `fired_at`. The worker shows it with `showNotification(title, { body: line, tag: alert_id })`.
- **For how long.** A push service holds a message 6 hours for a browser that is off (`TTL: 21600`), sent with `Urgency: normal`. What fired stays on the account for 30 days in any case: a page that opens fetches it as before.
- **Which push services.** The server sends only to HTTPS addresses of Google's (`fcm.googleapis.com`), Mozilla's (`updates.push.services.mozilla.com`), Apple's (`*.push.apple.com`) and Microsoft's (`*.notify.windows.com`) push services. A browser that uses another is refused with `invalid_push_subscription` *(to verify, as browsers appear)*.
- **A browser is of one account.** One endpoint is of the account that subscribed it last: a pilot who signs in on a browser that another subscribed takes it over. A push service that answers `404` or `410` has ended the subscription, and the server removes it; a new subscription of the browser is a new `POST`.
- **The server's key** is made when the server first starts with its database and kept there. A new database, or a key removed from it, ends every subscription: the browsers subscribe again with the new key.
- **Bounds.** At most 10 browsers per account; an endpoint of at most 1024 bytes. A token for a program needs `alerts:write`.
- **The phone apps** fetch what fired, as before ([`GET /v1/account/alerts/fired`](#get-v1accountalertsfiredsincelimit)): Google's and Apple's push for the apps is not decided.

### `GET /v1/push/key`

No token. The server's P-256 public key, uncompressed, base64url without padding: the `applicationServerKey` a page passes to `pushManager.subscribe`. The same for everyone; `Cache-Control: public, max-age=3600`.

```json
{
  "public_key": "BP4z9KsN6nGRTbVYI_c7VJSPQTBtkgcy27mlmlMoZIIgDll6e3vCYLocInmYWAmS6TlzAC8wEqKK6PBru3jl7A8"
}
```

### `POST /v1/account/push`

Needs a token. Subscribes a browser: the body is the browser's `PushSubscription` as `toJSON()` gives it, `endpoint` and `keys` with `p256dh` and `auth`; `expirationTime` is read and not kept. The answer is `204` with no body. The same browser again replaces its keys and is not counted twice.

```json
{
  "endpoint": "https://fcm.googleapis.com/fcm/send/dXJxQ:APA91bE",
  "keys": {
    "p256dh": "BPhp5We4GQDXtBrO0GeKUqWoQAgneEEQ63hZe6daNPn1Bn-YKRTZNOHs2Fxs-cfnQeslwbGX7FEThxshVEjNTEY",
    "auth": "BwcHBwcHBwcHBwcHBwcHBw"
  }
}
```

| Status | `code` | When |
|---|---|---|
| 400 | `invalid_push_subscription` | An endpoint that is not HTTPS, not of a push service the server sends to, or longer than 1024 bytes; a `p256dh` that is not a point on P-256; an `auth` that is not 16 bytes. `message` names the field |
| 409 | `too_many_push_subscriptions` | The account has 10 browsers subscribed already |

### `DELETE /v1/account/push`

Needs a token. Removes a browser's subscription, by its endpoint: `{"endpoint": "https://fcm.googleapis.com/fcm/send/dXJxQ:APA91bE"}`. The answer is `204` with no body, also when the account did not have it.

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

The pilot chooses, with two exceptions that whoever runs the server can make by hand: a single flight can be [taken down](#what-whoever-runs-the-server-can-do), which makes it private until they release it, and an account's publishing can be suspended, which leaves `unlisted` as the most it can share with. The flight and the account say so to their pilot (`taken_down_at`, `publishing_suspended_at`), and to nobody else.

**Since when a flight is public** is its `published_at`: the moment its pilot made it public, there while it is public and left out otherwise. A flight that was withdrawn and made public again has the moment of the second time. For a flight that was public before the server kept this (2026-11-21), it is the moment of the upload, which is the earliest it can have been. A flight that stops being public (made private or unlisted, taken down, deleted, or gone with its account) is **withdrawn**, and [the changes among the public flights](#get-v1flightspublicchangessincepilotlimit) say so to whoever kept a copy.

**Under what a flight is given.** Every answer that holds a flight someone other than its pilot may read carries `terms`: the list of public flights, what changed among them, a pilot's page, the ranking, and one flight and its track when the flight is public or shared by link.

```json
"terms": {
  "text": "These flights are their pilots'. They are shown here for reading. Reusing a flight (copying it elsewhere, publishing it, building on it) needs its pilot's leave until Venti's terms say otherwise, and a flight its pilot has withdrawn is to be removed from every copy.",
  "decided": false
}
```

`text` is the statement in words. `licence` names a licence when the flights are given under one; it is left out today, which means that nothing but reading is granted. `decided` is `false`: **under what terms others may reuse a public flight is not decided yet** ([data-platform.md](data-platform.md#for-the-owner)), and the statement above is the one that holds until it is. It is said in one place in the server, so a program reads it from the answer and never keeps its own copy of the words. The flight file itself (`/igc`) is the pilot's file as it was uploaded and carries nothing of this; the same terms apply to it.

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
      "analysis_version": 1,
      "published_at": "2026-11-18T19:05:00Z"
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
    "analysis_version": 1,
    "published_at": "2026-11-18T19:05:00Z"
  },
  "terms": {
    "text": "These flights are their pilots'. They are shown here for reading. Reusing a flight (copying it elsewhere, publishing it, building on it) needs its pilot's leave until Venti's terms say otherwise, and a flight its pilot has withdrawn is to be removed from every copy.",
    "decided": false
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
| `takeoff_site` | The site the glider took off from: `id` (for [`GET /v1/sites/{id}`](#get-v1sitesid)) and `name`. Of the standing sites of every layer the server serves (a source's such as OpenStreetMap's, `venti` and `venti-pilots`) that are take-offs (they use `takeoff` or `towing`, or name no use) and have a name, within 500 m of `takeoff.position`: a partner's (`venti`) before a place from pilots (`venti-pilots`) before a source's, and of one layer the nearest; of two as near, the smaller identifier. The server looks it up with every answer, so it is the site and the name as they stand now: renamed or removed there, so here. Left out when no such site stands, and when there is no `takeoff`. It does not count as a change of the flight: [what changed](#get-v1flightschangessincelimit) lists no flight for a site that was added or renamed. Its `name` is text from a source or from pilots, to show as text. A flight in [the list of a site](#get-v1flightspublicpilotbboxfromtosortafterlimit) (`site=`) may name another site here, a partner's or a nearer one |
| `track_length_m` | Distance along the track |
| `max_altitude_m` | Highest satellite altitude |
| `max_climb_ms` | Best climb averaged over half a minute |
| `score` | How the flight scores as a cross-country flight: `kind` is `free_flight`, `free_triangle` or `fai_triangle`; `closed` says whether a triangle is closed to within 5%; `distance_m` and `points` follow. Left out for a track too short to score. The rules are the World XContest's as this project reads them; the points are ours, not the league's, until they have been compared |
| `glider` | The glider as the file names it. Left out if the file names none. This is text from a file anyone can write: show it as text, never as markup |
| `outline` | The shape of the track for a thumbnail: at most 96 points, each a latitude and a longitude. Circles in thermals are smoothed away, so nothing may be measured on it |
| `file_bytes` | The size of the flight file |
| `analysis_version` | Which version of the server's analysis produced the numbers. When the analysis improves, the server works every flight out again, and the flight shows up in `/v1/flights/changes` |
| `taken_down_at` | Since when the flight is [taken down](#what-whoever-runs-the-server-can-do). Left out for a flight that is not, which is nearly every flight. Only the pilot ever reads it: a flight that is taken down is private |

A client must expect values of `kind` it does not know.

### `GET /v1/flights/{id}/igc`

The flight file, byte for byte as it was uploaded, to whoever may see the flight. The answer has `Content-Type: application/octet-stream` and a `Content-Disposition` that names the file `venti-<date>-<start of id>.igc`.

### `GET /v1/flights/{id}/track`

The flight's track as data, for a program that does not read flight files: a GeoJSON Feature ([RFC 7946](https://www.rfc-editor.org/rfc/rfc7946)) whose geometry is a LineString, with `Content-Type: application/geo+json`. To whoever may see the flight, as its record and its file: anyone for a public or unlisted flight, the pilot (a session, or a token with `flights:read`) for a private one, and `404` with `flight_not_found` otherwise. The flight's summary as JSON is [`GET /v1/flights/{id}`](#get-v1flightsid), at the same lasting address.

```bash
curl http://127.0.0.1:8080/v1/flights/5t1eshypdtvrq7rxw39a18vsrg/track
```

The answer for the demo account's public flight, cut here to its first three positions of 1,835:

```json
{
  "type": "Feature",
  "id": "5t1eshypdtvrq7rxw39a18vsrg",
  "geometry": {
    "type": "LineString",
    "coordinates": [
      [
        11.7967,
        47.6642,
        1620.0
      ],
      [
        11.796617,
        47.664267,
        1619.0
      ],
      [
        11.796517,
        47.66435,
        1618.0
      ]
    ]
  },
  "properties": {
    "start": "2026-08-15T10:30:00Z",
    "times_s": [
      0,
      1,
      2
    ],
    "pressure_altitudes_m": [
      1585,
      1584,
      1583
    ],
    "fixes": 1835,
    "every": 1,
    "takeoff_s": 0,
    "landing_s": 1752
  },
  "terms": {
    "text": "These flights are their pilots'. They are shown here for reading. Reusing a flight (copying it elsewhere, publishing it, building on it) needs its pilot's leave until Venti's terms say otherwise, and a flight its pilot has withdrawn is to be removed from every copy.",
    "decided": false
  }
}
```

| Field | Meaning |
|---|---|
| `geometry.coordinates` | One position per fix of the recording, in its order: **longitude, latitude** in degrees (WGS 84), rounded to six decimals, which keeps all a flight file has, and the satellite altitude in metres as the file gives it |
| `properties.start` | The time of the first position |
| `properties.times_s` | For each position, the seconds from `start` to its fix. The list is as long as `coordinates` |
| `properties.pressure_altitudes_m` | For each position, the pressure altitude in metres as the file gives it. Left out for a recording without a pressure sensor |
| `properties.fixes`, `properties.every` | How many fixes the recording holds, and every how many of them is a position here: `1` when the line holds every fix |
| `properties.takeoff_s`, `properties.landing_s` | The seconds from `start` to the take-off and the landing that the server's analysis found, the ones the flight's record has as `takeoff` and `landing`. Left out where the record has none |
| `id` | The flight's address |
| `terms` | [Under what the track is given](#flights). A member of the server's own, which the format allows; left out for a private flight |

**Everything the recording has, judged by nobody.** The line holds every fix the file has, those before the take-off and after the landing too, and no fix is smoothed or moved. The times and heights are lists beside the line and not a fourth number in each position, because the format's positions hold longitude, latitude and height and nothing more.

**The bound.** A line holds at most 50,000 positions. A recording with more, which is one of more than thirteen hours at a fix a second, is thinned evenly: every second fix, or every third, and so on, as `every` says, and its last fix always. The bound is there so that the answer's size does not depend on the largest file somebody can make (a file may hold 100,000 fixes). The full recording is always the file.

The track is read from the file for every answer, so this request is counted for the address that asks, [as the lists of public flights are](#how-often-an-address-may-read).

### `PATCH /v1/flights/{id}`

Needs a token. Changes who may see a flight of the account.

```json
{
  "visibility": "public"
}
```

The answer is `200` with the flight as it then is, in the shape of `GET /v1/flights/{id}`. A value other than `private`, `unlisted` and `public` gives `400` with `invalid_visibility`. A flight of another pilot is no flight of this account, whoever may see it: `404` with `flight_not_found`.

Two answers say that whoever runs the server stands in the way, and nothing changed:

| Status | Code | When |
|---|---|---|
| 403 | `flight_taken_down` | The flight is taken down, and the request asked for `unlisted` or `public`. Asking for `private`, which it is, is answered `200` |
| 403 | `publishing_suspended` | The account's publishing is suspended, and the request asked for `public`. `unlisted` and `private` work |

Neither goes away by asking again, and neither is a fault in the app: show the pilot what it is, and that the person to talk to is whoever runs the server. A flight that is both gets `flight_taken_down`.

### `DELETE /v1/flights/{id}`

Needs a token. Deletes a flight of the account and its file, at once and for good. The answer is `204` with no body. The server remembers only that the flight was deleted, with its `id` and `identity`, so that the pilot's other devices can be told. Uploading the file again makes the flight again, private, under the same address. A flight that was taken down when it was deleted comes back taken down, since the same moment.

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

Each entry of `flights` is a flight as `GET /v1/flights/{id}` shows it. `after`, `limit` and `next` work as for `GET /v1/flights`. The answer carries [`terms`](#flights), under what the flights are given. A program that wants a pilot's flights as data, narrowed to a place or a time or followed as they change, asks [the list of public flights](#public-flights) with `pilot`.

A pilot has a page once they have made a flight public. A handle nobody has gives `404` with `pilot_not_found`, and so does the handle of a pilot who shares nothing publicly: an account that keeps to itself cannot be found here, and the two answers cannot be told apart. A pilot whose only public flight was taken down, and one whose publishing is suspended, shares nothing publicly and has no page either.

### `GET /v1/pilots?q=&limit=`

The pilots whose handle or display name holds a text: the search of a list of flights or of a ranking. Anyone may ask, without a token, and the answer is the same for everyone.

| Parameter | Meaning | Default |
|---|---|---|
| `q` | A part of a handle or of a display name, 2 to 60 characters; spaces at its ends are not counted. Anything else is `400` `invalid_request` | Required |
| `limit` | How many pilots to return, 1 to 100 | 50 |

```bash
curl 'http://127.0.0.1:8080/v1/pilots?q=le'
## Pilots among each other

A signed-in pilot follows other pilots, reads their public flights as one list, cheers a public flight ("well flown") and writes under it. Since 2026-11-22. What a client has to know:

- **What is public.** That a pilot follows another, who cheered a public flight and what was written under it, each with the handle and the name of the pilot who did it, and the counts. Following, cheering and commenting put a pilot's name where everyone reads it: say so where the button is.
- **A pilot without a page is counted and not named** in the two public lists of following. A pilot has a [page](#pilots) once they have made a flight public; the handle of one who shares nothing is in no list of followers and in no list of followed pilots, and `followers` and `following` count them all the same. A cheer and a comment name their pilot always: both are written onto somebody's flight for everyone to read.
- **What is the account's own.** The feed, the whole list of whom the account follows, and what it was told. They need a session, or a token with `social:read`.
- **Only a public flight has cheers and comments.** For a flight that is private, shared by link or taken down, every request below answers `404` with `flight_not_found`, to its pilot as to everyone. What was given is kept while the flight is, and is back when the flight is public again. A flight that is deleted takes its cheers and comments with it, and they are not back when its file is uploaded again.
- **Who writes.** Any signed-in pilot whose publishing is not suspended; a suspended account gets `403` with `publishing_suspended` when it follows, cheers or comments, and can still take back what it gave. A token for a program needs `social:write`.
- **An account that is deleted takes all of it along**: whom it followed and who followed it, its cheers, its comments, the cheers and comments under its flights, what it was told and what others were told of it. The counts of other pilots' flights and pages go down by what it gave.
- **Bounds.** An account follows at most 1,000 pilots; it follows and cheers at most 1,000 times in a day, taking back not counted, and writes at most 100 comments in a day (`429` with `rate_limited` beyond); a comment has at most 500 characters. The public lists are read within the bound of the [public flights](#public-flights). Pages hold 50 entries, 100 with `limit=100`.
- **A comment is plain text.** The server keeps line breaks and drops control characters and what hides text or turns it round; it makes no link of anything. Show it as text and never as markup.
- **Comments are reported** as [`subject.comment`](#post-v1reports), and whoever runs the server removes one with `venti-server remove-comment`.

### `PUT /v1/pilots/{handle}/follow`

Needs a token. The account follows the pilot. Following a pilot one follows already changes nothing and answers the same. The pilot is [told](#get-v1accountnotificationssincelimit) the first time.

```bash
curl -X PUT http://127.0.0.1:8080/v1/pilots/timo/follow \
  -H 'Authorization: Bearer venti_0000000000000000000000000000000000000000000000000000'
```

```json
{
  "following": true,
  "followers": 1
}
```

`followers` is how many accounts follow the pilot now.

| Status | Code | When |
|---|---|---|
| 400 | `invalid_follow` | The handle is the account's own |
| 403 | `publishing_suspended` | The account may not publish |
| 404 | `pilot_not_found` | Nobody has the handle, or its pilot shares no flight publicly |
| 409 | `too_many_follows` | The account follows 1,000 pilots already |
| 429 | `rate_limited` | The account has followed and cheered 1,000 times today |

### `DELETE /v1/pilots/{handle}/follow`

Needs a token. The account follows the pilot no more: `200` with `"following": false` and the pilot's `followers`, also when it did not follow them. A pilot who shares nothing by now can still be let go of; a handle the account does not follow and that leads to no page gives `404` with `pilot_not_found`.

### `GET /v1/pilots/{handle}/followers?after=&limit=`

Who follows the pilot, the newest first, and how many do. Anyone may ask.

```bash
curl http://127.0.0.1:8080/v1/pilots/demo/followers
```

```json
{
  "pilots": [
    {
      "pilot": {
        "handle": "lena",
        "display_name": "Lena Probe"
      },
      "flights": 3
    }
  ]
}
```

**Who is found.** A pilot with at least one public flight, as on a pilot's page: an account that shares nothing publicly is found by no text, its own handle included. The text is looked for anywhere in the handle and in the name, whatever the case of either (`q=PROBE` finds Lena Probe). Case is folded as Unicode folds it; for letters outside ASCII a database may fold a few differently (to verify against the production database's locale).

**The order.** The pilots of whom the handle or the name begins with the text come first, then the pilot with the most public flights, then by handle. `flights` is the number of the pilot's public flights. The answer has no `next`: a search is narrowed by typing more, not read page by page.

**To list a pilot's flights**, ask [the list of public flights](#public-flights) with `pilot=<handle>`; what they add up to is below.

**The bound.** A search counts as a read of public flights for the address that asks: [600 an hour](#how-often-an-address-may-read). A page that searches as its reader types should wait for a pause in the typing. `Cache-Control: public, no-cache` and an `ETag`, as for a pilot's page.

### `GET /v1/pilots/{handle}/statistics`

What a pilot's public flights add up to, all together and year by year. Anyone may ask, without a token; the answer is the same for everyone, the pilot included, and is made anew from the flights that are public at that moment. A flight that is private or shared by link is in no number of it.

```bash
curl http://127.0.0.1:8080/v1/pilots/lena/statistics
```

```json
{
  "pilot": {
    "handle": "lena",
    "display_name": "Lena Probe"
  },
  "counted": 3,
  "overall": {
    "flights": 3,
    "scored": 3,
    "distance_m": 28191.0,
    "points": 28.19,
    "airtime_s": 3865,
    "longest": {
      "id": "ww5qkxhxaedefvypb3wy3v6m3m",
      "distance_m": 12007.4
    },
    "highest": {
      "id": "hwj4105bnzcgcavngacrzjt3s4",
      "max_altitude_m": 2117
    },
    "sites": [
      {
        "site": {
          "id": "venti:0000000000000000000000s001",
          "name": "Musterkogel West"
        },
        "position": {
          "lat": 47.3,
          "lon": 10.9
        },
        "flights": 2
      }
    ],
    "by_month": [
      0,
      0,
      0,
      0,
      1,
      1,
      1,
      0,
      0,
      0,
      0,
      0
    ]
  },
  "seasons": [
    {
      "year": 2026,
      "totals": {
        "flights": 3,
        "scored": 3,
        "distance_m": 28191.0,
        "points": 28.19,
        "airtime_s": 3865,
        "longest": {
          "id": "ww5qkxhxaedefvypb3wy3v6m3m",
          "distance_m": 12007.4
        },
        "highest": {
          "id": "hwj4105bnzcgcavngacrzjt3s4",
          "max_altitude_m": 2117
        },
        "sites": [
          {
            "site": {
              "id": "venti:0000000000000000000000s001",
              "name": "Musterkogel West"
            },
            "position": {
              "lat": 47.3,
              "lon": 10.9
            },
            "flights": 2
          }
        ],
        "by_month": [
          0,
          0,
          0,
          0,
          1,
          1,
          1,
          0,
          0,
          0,
          0,
          0
        ]
      }
    }
  ],
  "terms": {
    "text": "These flights are their pilots'. They are shown here for reading. Reusing a flight (copying it elsewhere, publishing it, building on it) needs its pilot's leave until Venti's terms say otherwise, and a flight its pilot has withdrawn is to be removed from every copy.",
    "decided": false
  }
}
```

`overall` is every public flight; `seasons` has one entry for each year the pilot has a public flight in, the newest first, with the same numbers for that year in `totals`.

| Field | Meaning |
|---|---|
| `flights` | How many public flights |
| `scored` | How many of them are long enough to score |
| `distance_m` | The scoring distances of those, added up, in metres |
| `points` | The sum of the points of the best `counted` (three) of them, in hundredths: what [the ranking](#ranking) of the same span gives the pilot |
| `airtime_s` | The airtime of all of them, added up, in seconds |
| `longest` | The flight with the longest scoring distance: its `id` and `distance_m`. Left out when no flight scores |
| `highest` | The flight that went highest: its `id` and `max_altitude_m`. Of two as long or as high, the newer is named |
| `sites` | The take-off sites flown from most: each `site` (`id` and `name`, as a flight's [`takeoff_site`](#get-v1flightsid)) with its `position` (`lat` and `lon`, where the site's record has it, for a map or a link) and its `flights`, the most first and by identifier among sites with as many, at most five. A flight that took off where no site with a name stands is in none |
| `by_month` | How many of the flights began in each month of the year, January first, all years together: twelve numbers that add up to `flights` |

**A season is a year by UTC**, and the months are those of UTC, as the ranking's `month` and its periods are. (What flights say about [a site](#what-public-flights-say-about-a-site) counts its months by the day at the site; in the Alps the two never differ.)

**The site of a take-off** is looked up as for a flight, within 500 m, with the take-off taken to the nearest 0.0001° (11 m): a pilot's hundred starts from one meadow are a handful of places to look up. At the very edge of the 500 m a flight may therefore name a site here and none on its own page, or the other way round.

**Who has totals.** A pilot who has a [page](#get-v1pilotshandleafterlimit): one with at least one public flight. Any other handle gives `404` `pilot_not_found`, the same answer as for a handle nobody has.

**The bound.** It counts as a read of public flights for the address that asks: [600 an hour](#how-often-an-address-may-read). `Cache-Control: public, no-cache` and an `ETag`: a flight that is made private is in no number of the next answer.
      "since": "2026-11-18T17:35:00Z"
    },
    {
      "pilot": {
        "handle": "jonas",
        "display_name": "Jonas Beispiel"
      },
      "since": "2026-11-18T09:55:00Z"
    },
    {
      "pilot": {
        "handle": "mara",
        "display_name": "Mara Muster"
      },
      "since": "2026-11-18T09:35:00Z"
    }
  ],
  "followers": 3,
  "following": 3
}
```

| Field | Meaning |
|---|---|
| `pilots` | One page of the followers who have a page, each with `since`, the moment they began to follow |
| `followers`, `following` | How many accounts follow the pilot and how many the pilot follows, those the lists do not name included |
| `you_follow` | Whether the account that asks follows the pilot. Only in an answer to a request with a session, or with a token that has `social:read`; such an answer is made for that account and kept by nobody |
| `next` | Pass it as `after` for the next page. Left out on the last one |

A handle without a page gives `404` with `pilot_not_found`.

### `GET /v1/pilots/{handle}/following?after=&limit=`

Whom the pilot follows, the newest first: the same answer, with the followed pilots who have a page in `pilots`.

### `GET /v1/account/following?after=&limit=`

Needs a token. Everyone the account follows, the newest first, those without a page among them: the answer of the public list without `you_follow`, with the account's own counts.

### `GET /v1/account/feed?before=&limit=`

Needs a token. The public flights of the pilots the account follows, the newest first by their start.

```bash
curl 'http://127.0.0.1:8080/v1/account/feed?limit=3' \
  -H 'Authorization: Bearer venti_0000000000000000000000000000000000000000000000000000'
```

Each entry of `flights` is `flight`, the flight as [`GET /v1/flights/public`](#get-v1flightspublicpilotbboxfromtosortafterlimit) lists it, with its `takeoff_site`; `cheers` and `comments`, how many it has; and `cheered`, whether the account cheered it. `next` is passed as `before` for the flights that started before the last of the page, and is left out on the last page. `terms` says under what the flights are given. A flight that is made private leaves the list at once; the account's own flights are not in it. For someone who is not signed in there is no feed: the list of everyone's public flights is that.

```json
{
  "flights": [
    {
      "flight": { ... },
      "cheers": 1,
      "comments": 0,
      "cheered": true
    },
    ...
  ],
  "next": "1786874400000000_7hn3e0ef6twjgq0g51ckxtxsx8",
  "terms": { ... }
}
```

### `PUT /v1/flights/{id}/cheer`

Needs a token. The account cheers a public flight of another pilot. Cheering it again changes nothing. The flight's pilot is told the first time.

```json
{
  "cheered": true,
  "cheers": 1
}
```

| Status | Code | When |
|---|---|---|
| 400 | `invalid_cheer` | The flight is the account's own |
| 403 | `publishing_suspended` | The account may not publish |
| 404 | `flight_not_found` | No public flight has this address |
| 429 | `rate_limited` | The account has followed and cheered 1,000 times today |

### `DELETE /v1/flights/{id}/cheer`

Needs a token. Takes the account's cheer back: `200` with `"cheered": false` and the flight's `cheers`. A cheer on a flight that is public no more can be taken back too, and the answer then says `"cheers": 0`; without a cheer to take back such a flight gives `404` with `flight_not_found`.

### `GET /v1/flights/{id}/cheers?after=&limit=`

Who cheered a public flight, the newest first. Anyone may ask.

```bash
curl http://127.0.0.1:8080/v1/flights/5t1eshypdtvrq7rxw39a18vsrg/cheers
```

```json
{
  "count": 2,
  "cheers": [
    {
      "pilot": {
        "handle": "lena",
        "display_name": "Lena Probe"
      },
      "at": "2026-11-18T18:20:00Z"
    },
    {
      "pilot": {
        "handle": "mara",
        "display_name": "Mara Muster"
      },
      "at": "2026-11-18T18:05:00Z"
    }
  ]
}
```

`count` is how many pilots cheered. `cheered`, whether the account that asks is one of them, is only in an answer to a request with a session or with a token that has `social:read`.

### `GET /v1/flights/{id}/comments?after=&limit=`

The comments under a public flight, the oldest first. Anyone may ask; the answer is the same for everyone.

```json
{
  "count": 2,
  "open": true,
  "comments": [
    {
      "id": "0000000000000000000000c001",
      "pilot": {
        "handle": "mara",
        "display_name": "Mara Muster"
      },
      "text": "Well flown. How was the air above the ridge?",
      "at": "2026-11-18T18:10:00Z"
    },
    {
      "id": "0000000000000000000000c002",
      "pilot": {
        "handle": "demo",
        "display_name": "Demo Pilot"
      },
      "text": "Rough at first, calm above 2000 m.\nA made-up answer of the development mode.",
      "at": "2026-11-18T18:15:00Z"
    }
  ]
}
```

`open` is `false` once the flight's pilot turned comments off; the comments written before stay and are shown.

### `POST /v1/flights/{id}/comments`

Needs a token. Writes a comment under a public flight; a pilot may write under their own. The body:

```json
{
  "text": "A made-up comment of a test."
}
```

The answer is `201` with the comment as the server keeps it and `count`, how many the flight has now. The flight's pilot is told, unless they wrote it.

```json
{
  "comment": {
    "id": "0000000000000000000000c009",
    "pilot": {
      "handle": "demo",
      "display_name": "Demo Pilot"
    },
    "text": "A made-up comment of a test.",
    "at": "2026-11-18T19:05:00Z"
  },
  "count": 1
}
```

| Status | Code | When |
|---|---|---|
| 400 | `invalid_comment` | Nothing is left of `text` after cleaning, or more than 500 characters |
| 403 | `comments_off` | The flight's pilot takes no comments on it |
| 403 | `publishing_suspended` | The account may not publish |
| 404 | `flight_not_found` | No public flight has this address |
| 429 | `rate_limited` | The account has written 100 comments today |

### `PATCH /v1/flights/{id}/comments`

Needs a token, with `flights:write` for a program: it is a setting of the pilot's own flight, of any visibility. The body says whether the flight takes comments, and the answer repeats it:

```json
{
  "open": false
}
```

A flight takes comments unless its pilot said otherwise. Turning them off removes nothing. A flight that is not the account's gives `404` with `flight_not_found`.

### `DELETE /v1/flights/{id}/comments/{comment}`

Needs a token. Removes a comment: its author may, and the pilot of the flight it is under. `204`. Anything else gives `404` with `comment_not_found`: no such comment under this flight, or one that is neither the account's nor under its flight. Nobody is told.

### `GET /v1/account/notifications?since=&limit=`

Needs a token. What the account was told since the device last asked, the oldest first: a pilot follows it, cheered a flight of its, or commented on one. Kept for 30 days. `since`, `cursor`, `more` and `reset` work as for [`GET /v1/account/alerts/fired`](#get-v1accountalertsfiredsincelimit). The phone apps and the web app fetch this; nothing is pushed.

```bash
curl http://127.0.0.1:8080/v1/account/notifications \
  -H 'Authorization: Bearer venti_0000000000000000000000000000000000000000000000000000'
```

```json
{
  "notifications": [
    {
      "kind": "followed",
      "at": "2026-11-18T17:35:00Z",
      "pilot": {
        "handle": "lena",
        "display_name": "Lena Probe"
      }
    },
    {
      "kind": "cheered",
      "at": "2026-11-18T18:05:00Z",
      "pilot": {
        "handle": "mara",
        "display_name": "Mara Muster"
      },
      "flight": {
        "id": "5t1eshypdtvrq7rxw39a18vsrg",
        "start": "2026-08-15T10:30:00Z"
      }
    },
    {
      "kind": "commented",
      "at": "2026-11-18T18:10:00Z",
      "pilot": {
        "handle": "mara",
        "display_name": "Mara Muster"
      },
      "flight": {
        "id": "5t1eshypdtvrq7rxw39a18vsrg",
        "start": "2026-08-15T10:30:00Z"
      },
      "comment": "0000000000000000000000c001"
    },
    {
      "kind": "cheered",
      "at": "2026-11-18T18:20:00Z",
      "pilot": {
        "handle": "lena",
        "display_name": "Lena Probe"
      },
      "flight": {
        "id": "5t1eshypdtvrq7rxw39a18vsrg",
        "start": "2026-08-15T10:30:00Z"
      }
    }
  ],
  "cursor": "1795026000000000",
  "more": false,
  "reset": false
}
```

| Field | Meaning |
|---|---|
| `kind` | `followed`, `cheered` or `commented`. A client skips a kind it does not know |
| `at` | When |
| `pilot` | Who did it, as they were called then |
| `flight` | For a cheer and a comment, the account's flight: its `id`, its `start` and `takeoff_site`, the name of the site it took off from as it stood then, left out when none was known |
| `comment` | For a comment, its identifier. The words are read where the [comments](#get-v1flightsidcommentsafterlimit) are, and are not there once the comment is removed |

A pilot is told when a follow or a cheer is new: following or cheering again while it stands tells nothing, and one that was taken back and is given again tells again. Nobody is told of what they did themselves. What an account was told goes when the account that did it is deleted.

## Public flights

### `GET /v1/flights/public?pilot=&bbox=&from=&to=&sort=&after=&limit=`

Everyone's public flights in one list: what pilots flew, the newest first. Anyone may ask, without a token, and the answer is the same for everyone. The list holds the flights their pilots made `public` and no others. A flight that is private or unlisted is in no page of it, whatever is asked for, and that includes the asker's own: a pilot's own flights are at [`GET /v1/flights`](#get-v1flightsafterlimit). A flight that was [taken down](#what-whoever-runs-the-server-can-do) is private, and the flights of an account whose publishing is suspended are unlisted at most: neither is here.

| Parameter | Meaning | Default |
|---|---|---|
| `pilot` | A handle: only that pilot's public flights. A pilot who has made no flight public is answered as a handle nobody has, `404` with `pilot_not_found`, as [the pilot's page](#pilots) is | Flights of every pilot |
| `bbox` | `west,south,east,north` in degrees, as for [stations](#get-v1stationsbboxwestsoutheastnorth): only flights that **took off** inside the box, edges included. Where a flight went afterwards does not count. A flight whose recording never shows the glider flying has no take-off and is in no box | Flights from anywhere |
| `from` | An RFC 3339 time: only flights that started at this moment or later | No bound |
| `to` | An RFC 3339 time: only flights that started **before** this moment. It must be later than `from` | No bound |
| `site` | A site's identifier, of any layer: only the flights that **began at** the site, those that took off within 500 m of it as it stands, the flights [what flights say about it](#what-public-flights-say-about-a-site) counts. A site that is no take-off (a landing field, a training slope) has none: an empty list. A site the server does not have is `404` with `site_not_found`, one that pilots removed `410` with `site_gone` | Flights from anywhere |
| `sort` | `newest`: by the start of the flight, the latest first. `score`: by `score.points`, the most first; a flight too short to score is not in that list | `newest` |
| `limit` | How many flights to return, 1 to 100 | 50 |
| `after` | The `next` of the answer before, with every other parameter as it was | The top of the list |

Every parameter narrows the list or orders it. They can all be used together: the best flights of a day in a part of the map are `bbox`, `from`, `to` and `sort=score`, and one pilot's flights from one valley in one season are `pilot`, `bbox`, `from` and `to`.

**The place of a flight** is where it took off, the one place the server keeps in a form it can search by. Where a flight landed is in its record and cannot be asked for; where it went is its [track](#get-v1flightsidtrack).

**By what changed, oldest change first,** the same flights are listed by [`GET /v1/flights/public/changes`](#get-v1flightspublicchangessincepilotlimit), which is the order for a program that keeps a copy.

The answer carries [`terms`](#flights), under what its flights are given.

#### How often an address may read

Seven requests are open to anyone and counted together for the address that asks: a page of this list, a page of [what changed](#get-v1flightspublicchangessincepilotlimit), [a flight's track](#get-v1flightsidtrack), [what flights say about a site](#what-public-flights-say-about-a-site), of one site and of the candidates in a box, [the search for pilots](#get-v1pilotsqlimit) and [a pilot's totals](#get-v1pilotshandlestatistics). An address may make **600 of them in an hour**; for an IPv6 client the address is its /64 network, and one /48 network together gets 2,400. Beyond that the answer is `429` with `rate_limited`, and `Retry-After` says when the hour is over. Every such answer says what the address has left in `RateLimit` ([Limits](#limits)). A session's token does not change the count: it is the address's. A token for a program does: the read is counted against the token's own budget instead. A page holds at most 100 flights or changes ([`limit`](#rules-that-hold-for-every-endpoint)), and a track at most 50,000 positions. One flight by its address, its file and a pilot's page are not counted. The numbers are estimates, counted in the server's memory as its other limits are.

**A day** is asked for with its first moment as `from` and the first moment of the next day as `to`. The server knows no time zone: a client that means the pilots' own day writes its offset into both times (`2026-08-15T00:00:00+02:00`, with the `+` written as `%2B` in a URL). The best flights of 15 August 2026, by UTC:

```bash
curl 'http://127.0.0.1:8080/v1/flights/public?from=2026-08-15T00:00:00Z&to=2026-08-16T00:00:00Z&sort=score'
```

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
      "analysis_version": 1,
      "published_at": "2026-11-18T19:05:00Z"
    },
    {
      "id": "rnwkdftnzbzv3jpcc5v1ajq97w",
      "identity": "57096780ebf49faa9e60b4ed78bfdcf6",
      "pilot": {
        "handle": "mara",
        "display_name": "Mara Muster"
      },
      "visibility": "public",
      "uploaded_at": "2026-11-18T19:05:00Z",
      "start": "2026-08-15T11:10:00Z",
      "airtime_s": 1500,
      "takeoff": {
        "time": "2026-08-15T11:10:00Z",
        "position": {
          "lat": 47.6642,
          "lon": 11.7967
        },
        "altitude_m": 1620
      },
      "track_length_m": 15877.9,
      "max_altitude_m": 2117,
      "max_climb_ms": 2.4,
      "score": {
        "kind": "free_flight",
        "closed": false,
        "distance_m": 11444.3,
        "points": 11.44
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
      "file_bytes": 55650,
      "analysis_version": 1,
      "published_at": "2026-11-18T19:05:00Z"
    }
  ],
  "terms": {
    "text": "These flights are their pilots'. They are shown here for reading. Reusing a flight (copying it elsewhere, publishing it, building on it) needs its pilot's leave until Venti's terms say otherwise, and a flight its pilot has withdrawn is to be removed from every copy.",
    "decided": false
  }
}
```

(The outlines are cut to two points here.)

The newest flights, two to a page:

```bash
curl 'http://127.0.0.1:8080/v1/flights/public?limit=2'
```

```json
{
  "flights": [
    {
      "id": "nfmvgygyr34epnfbcekb04nwqc",
      "identity": "e721900dacb914a34358110db513e503",
      "pilot": {
        "handle": "jonas",
        "display_name": "Jonas Beispiel"
      },
      "visibility": "public",
      "uploaded_at": "2026-11-18T19:05:00Z",
      "start": "2026-09-06T13:00:00Z",
      "airtime_s": 1752,
      "takeoff": {
        "time": "2026-09-06T13:00:00Z",
        "position": {
          "lat": 47.6642,
          "lon": 11.7967
        },
        "altitude_m": 1620
      },
      "landing": {
        "time": "2026-09-06T13:29:12Z",
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
      "analysis_version": 1,
      "published_at": "2026-11-18T19:05:00Z"
    },
    {
      "id": "fbyw03atd9qdt5bfe177ra1k94",
      "identity": "5782b33df19716953603d69bacb63a14",
      "pilot": {
        "handle": "mara",
        "display_name": "Mara Muster"
      },
      "visibility": "public",
      "uploaded_at": "2026-11-18T19:05:00Z",
      "start": "2026-08-16T12:05:00Z",
      "airtime_s": 1752,
      "takeoff": {
        "time": "2026-08-16T12:05:00Z",
        "position": {
          "lat": 47.6642,
          "lon": 11.7967
        },
        "altitude_m": 1620
      },
      "landing": {
        "time": "2026-08-16T12:34:12Z",
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
      "analysis_version": 1,
      "published_at": "2026-11-18T19:05:00Z"
    }
  ],
  "next": "1786881900000000_fbyw03atd9qdt5bfe177ra1k94",
  "terms": {
    "text": "These flights are their pilots'. They are shown here for reading. Reusing a flight (copying it elsewhere, publishing it, building on it) needs its pilot's leave until Venti's terms say otherwise, and a flight its pilot has withdrawn is to be removed from every copy.",
    "decided": false
  }
}
```

(The outlines are cut to two points here.)

| Field | Meaning |
|---|---|
| `flights` | The flights of this page, each as [`GET /v1/flights/{id}`](#get-v1flightsid) shows it, which is also what a [pilot's page](#get-v1pilotshandleafterlimit) lists. `visibility` is `public` in every one |
| `next` | Pass this as `after` to get the next page, with the other parameters unchanged. Left out on the last page. Treat it as an opaque value |

What a row of a list needs is in every flight: `id`, the flight's address, under which its details, its file and [its page](#the-page-a-shared-flight-has) are found; `pilot`, with the handle that leads to the pilot's page; `start`; `airtime_s`; `score`, where `distance_m` is the cross-country distance and `kind` and `points` say how it scores; `takeoff.position`; and `outline` for a thumbnail. The server knows the place a flight took off from by its coordinates only. It has no name for it: the [flying sites](#get-v1sitesbboxwestsoutheastnorth) come from another source under a licence of its own, and nothing of them is put into a flight.

**The order.** Flights that start in the same second, and under `sort=score` flights with the same points, follow each other by `id`, the larger first. That says nothing about the flights; it keeps the pages apart. The points are worked out by this server from the flight's file, for every flight by the same rules ([`score`](#get-v1flightsid)); they are not a league's.

**Reading on.** `next` names the last flight of a page, and the next page begins behind it. A flight that is made public while a client reads, and belongs before that place, is not in the pages that follow; it is there when the client begins again at the top. A flight that stops being public is in no page from that moment, also not in one a client reaches with a `next` it was given earlier.

**Who may keep an answer.** `Cache-Control: public, no-cache` and an `ETag`, like a pilot's page: an answer may be kept, by anyone, and must be checked with `If-None-Match` before every reuse, which ends in `304` without a body while the page has not changed. That is how a flight that is made private leaves every copy of the list at once. A token changes nothing about the answer.

**How large an answer is.** A flight is about 0.6 kB and 20 bytes for each point of its outline, of which it has 96 at most. A page of 50 long flights is therefore up to 130 kB, and one of 100 up to 250 kB *(worked out from the format, not measured on real flights)*. The eleven flights of the development mode, whose outlines have 6 to 13 points, are 10.2 kB, and 2.2 kB with gzip (measured on 2026-11-21).

| Status | Code | When |
|---|---|---|
| 400 | `invalid_bbox` | `bbox` is not four numbers that form a box |
| 400 | `invalid_time` | `from` or `to` is not an RFC 3339 time. `message` names the parameter |
| 400 | `invalid_range` | `from` is not earlier than `to` |
| 400 | `invalid_sort` | `sort` is not `newest` or `score` |
| 400 | `invalid_limit` | `limit` is not a number from 1 to 100 |
| 400 | `invalid_cursor` | `after` is not a `next` the server gave out for this order: a `next` of `sort=newest` is none for `sort=score`, and the other way round |

### `GET /v1/flights/public/changes?since=&pilot=&limit=`

What changed in what the public sees of flights since a program last asked: of everyone's public flights, or with `pilot` of one pilot's. Anyone may ask, without a token. It is for a program that keeps a copy of public flights and wants it to stay true: a flight that its pilot withdrew is named here, so that the copy removes it.

| Parameter | Meaning | Default |
|---|---|---|
| `since` | The `cursor` of the answer before | Everything, from the beginning |
| `pilot` | A handle: only the changes to that pilot's flights. `404` with `pilot_not_found` when the pilot has no public flight, as [the pilot's page](#pilots) answers | The changes to everyone's |
| `limit` | How many changes to return, 1 to 100 | 50 |

The development mode has twelve changes: eleven flights made public and one withdrawn. The first four, read with `limit=4`, end at the cursor `1795028700000003`; the two after them, read with `limit=2`, are a flight and the withdrawal, and six more wait:

```bash
curl 'http://127.0.0.1:8080/v1/flights/public/changes?since=1795028700000003&limit=2'
```

```json
{
  "flights": [
    {
      "id": "nfmvgygyr34epnfbcekb04nwqc",
      "identity": "e721900dacb914a34358110db513e503",
      "pilot": {
        "handle": "jonas",
        "display_name": "Jonas Beispiel"
      },
      "visibility": "public",
      "uploaded_at": "2026-11-18T19:05:00Z",
      "start": "2026-09-06T13:00:00Z",
      "airtime_s": 1752,
      "takeoff": {
        "time": "2026-09-06T13:00:00Z",
        "position": {
          "lat": 47.6642,
          "lon": 11.7967
        },
        "altitude_m": 1620
      },
      "landing": {
        "time": "2026-09-06T13:29:12Z",
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
      "analysis_version": 1,
      "published_at": "2026-11-18T19:05:00Z"
    }
  ],
  "withdrawn": [
    {
      "id_sha256": "f6caf4f4a5a7e862f2a3d419b0df6818653264eb9c2e8914cdb4376313d203b1",
      "withdrawn_at": "2026-11-18T19:05:00Z"
    }
  ],
  "cursor": "1795028700000006",
  "more": true,
  "reset": false,
  "terms": {
    "text": "These flights are their pilots'. They are shown here for reading. Reusing a flight (copying it elsewhere, publishing it, building on it) needs its pilot's leave until Venti's terms say otherwise, and a flight its pilot has withdrawn is to be removed from every copy.",
    "decided": false
  }
}
```

| Field | Meaning |
|---|---|
| `flights` | Flights that became public, or changed while public (the server worked their numbers out again), each once and as it is now, the oldest change first |
| `withdrawn` | Flights that are public no more: made private or shared by link only by their pilot, taken down, deleted, or gone with their pilot's account. Each is named by `id_sha256`, the SHA-256 of the 26 characters of its `id` as 64 lower-case hexadecimal characters, with `withdrawn_at`, which is left out for a flight that went with its account |
| `cursor` | Pass this as `since` next time. It is text; treat it as opaque |
| `more` | Whether more changes are waiting. Ask again with `cursor` at once |
| `reset` | `true` when the server did not know the request's `since`, for example after it was restored from a backup, and started again from the beginning. Whoever keeps a copy then replaces it with what the answers from here on hold |
| `terms` | [Under what the flights are given](#flights) |

A flight is in one of the two lists and never in both: one that was withdrawn and made public again is in `flights`. A flight that was never public is in neither, and that includes every flight shared by link.

**Why a hash and not the address.** A flight that is shared by link is read by whoever has its address. A list of the addresses of withdrawn flights would hand everyone the links to flights whose pilots had just taken them off the public list. Whoever kept a flight has its address and finds it by the hash; whoever did not learns nothing from it.

The same change numbers count for everyone's flights and for one pilot's, so a cursor from one way of asking is a valid place in the other. The request is counted for the address that asks, [as the list is](#how-often-an-address-may-read).

## Following Venti's public flights

This part is for the maker of a program that is not one of Venti's apps: a site that shows flights from a valley, a club's page, a research script. It needs nothing but HTTP and JSON, no account and no token, and can be read without the rest of this document. The server of the examples is the [development mode](#try-it); every answer below is described in full under its own heading.

**What you may do with what you read is not settled.** Every answer carries `terms`, and today they say: the flights are their pilots', they are shown here for reading, and reusing a flight needs its pilot's leave until Venti's terms say otherwise. `"decided": false` marks this as a holding statement. Read `terms` from the answers and act on it as it stands on the day; do not keep your own copy of the wording. Whatever the terms become, one rule will not change: **a flight its pilot has withdrawn is removed from every copy.** A track is a record of where a person was.

**1. Ask for what is public.** Everything, a page at a time:

```bash
curl 'http://127.0.0.1:8080/v1/flights/public/changes'
```

Store each flight of `flights` under its `id`. Remember `cursor`. While `more` is `true`, ask again at once with `?since=<cursor>`.

To follow one pilot, add `pilot=<handle>`. To read a part of the map or a span of time once, without keeping a copy true, use the list: [`GET /v1/flights/public`](#get-v1flightspublicpilotbboxfromtosortafterlimit) with `bbox`, `from`, `to` and `pilot`, and follow its `next`. The changes cannot be narrowed to a place or a time; a program that wants only a valley follows everything and keeps what took off inside its box (`takeoff.position`).

**2. Read a flight.** Its record is in the list already and at its own address; its track is GeoJSON; its file is the IGC file as the pilot uploaded it:

```bash
curl http://127.0.0.1:8080/v1/flights/5t1eshypdtvrq7rxw39a18vsrg
curl http://127.0.0.1:8080/v1/flights/5t1eshypdtvrq7rxw39a18vsrg/track
curl -O -J http://127.0.0.1:8080/v1/flights/5t1eshypdtvrq7rxw39a18vsrg/igc
```

A flight's `id` is its address for good: it is never used for another flight. `published_at` says since when it is public.

**3. Ask again, as often as you need to be right.** With the cursor you remembered:

```bash
curl 'http://127.0.0.1:8080/v1/flights/public/changes?since=1795028700000003'
```

- Every flight of `flights` replaces the one you hold under its `id`, or is new.
- Every entry of `withdrawn` names a flight to **remove**, with everything you derived from it: compare `id_sha256` with the SHA-256 of each `id` you hold (`printf %s "$id" | shasum -a 256`). Keep that hash beside each flight and the comparison is a lookup. An entry for a flight you never held is not for you.
- `reset: true` means your cursor was not this server's: drop what you hold and keep what the answers from here on give you.
- When you follow one pilot and the answer becomes `404` with `pilot_not_found`, the pilot has no public flight any more, or no account: remove everything of theirs.

Once a day is enough for most uses; once a minute is the most that makes sense. How long a copy may lag behind a withdrawal is part of the terms that are not written yet: until they are, ask at least once a day.

**4. Stay within the bound.** An address may make [600 such reads an hour](#how-often-an-address-may-read): pages of the list, pages of the changes and tracks together. A `429` carries `Retry-After`; wait that long. Send `If-None-Match` with the `ETag` of an answer you kept and an unchanged one costs a `304` without a body. A page in a browser on another site may make these reads as any program may ([Browsers](#rules-that-hold-for-every-endpoint)), without a token.

**What is not there:** a file of all public flights, and a call from the server when something changes. Neither is planned before the terms are decided.

## Ranking

### `GET /v1/ranking?bbox=&from=&to=&month=&limit=`

The pilots of everyone's public flights, ranked: a pilot's points are the sum of the points of their best three flights, and the pilot with the most comes first. Anyone may ask, without a token, and the answer is the same for everyone. Only public flights count. A flight that is private or shared by link counts for nobody, whatever is asked for, the asker's own included.

It is asked in the words of the [list of public flights](#public-flights), and means the same flights:

| Parameter | Meaning | Default |
|---|---|---|
| `bbox` | `west,south,east,north` in degrees: only flights that **took off** inside the box count | Flights from anywhere |
| `from` | An RFC 3339 time: only flights that started at this moment or later count | No bound |
| `to` | An RFC 3339 time: only flights that started **before** this moment count. It must be later than `from` | No bound |
| `month` | A calendar month by UTC, written `YYYY-MM`: only flights that started in it count. It is a `from` and a `to` in one word and is not sent with either (`400` `invalid_range`); a month that cannot be read is `400` `invalid_time` | No bound |
| `limit` | How many pilots to return from the top, 1 to 100 | 50 |

**A season** is a span of time the client names, as a day is for the list of flights: the server knows no time zone. The year 2026 by UTC is `from=2026-01-01T00:00:00Z&to=2027-01-01T00:00:00Z`. **A month** by UTC has a word of its own: `month=2026-08` means `from=2026-08-01T00:00:00Z&to=2026-09-01T00:00:00Z`. With `bbox` it is the ranking of the pilots who flew from one mountain, or in one region. The two pilots at the top of August 2026:

```bash
curl 'http://127.0.0.1:8080/v1/ranking?month=2026-08&limit=2'
```

```json
{
  "counted": 3,
  "pilots": [
    {
      "pilot": {
        "handle": "mara",
        "display_name": "Mara Muster"
      },
      "points": 23.42,
      "scored": 2,
      "flights": [
        {
          "id": "fbyw03atd9qdt5bfe177ra1k94",
          "identity": "5782b33df19716953603d69bacb63a14",
          "pilot": {
            "handle": "mara",
            "display_name": "Mara Muster"
          },
          "visibility": "public",
          "uploaded_at": "2026-11-18T19:05:00Z",
          "start": "2026-08-16T12:05:00Z",
          "airtime_s": 1752,
          "takeoff": {
            "time": "2026-08-16T12:05:00Z",
            "position": {
              "lat": 47.6642,
              "lon": 11.7967
            },
            "altitude_m": 1620
          },
          "landing": {
            "time": "2026-08-16T12:34:12Z",
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
          "analysis_version": 1,
          "published_at": "2026-11-18T19:05:00Z"
        },
        {
          "id": "rnwkdftnzbzv3jpcc5v1ajq97w",
          "identity": "57096780ebf49faa9e60b4ed78bfdcf6",
          "pilot": {
            "handle": "mara",
            "display_name": "Mara Muster"
          },
          "visibility": "public",
          "uploaded_at": "2026-11-18T19:05:00Z",
          "start": "2026-08-15T11:10:00Z",
          "airtime_s": 1500,
          "takeoff": {
            "time": "2026-08-15T11:10:00Z",
            "position": {
              "lat": 47.6642,
              "lon": 11.7967
            },
            "altitude_m": 1620
          },
          "track_length_m": 15877.9,
          "max_altitude_m": 2117,
          "max_climb_ms": 2.4,
          "score": {
            "kind": "free_flight",
            "closed": false,
            "distance_m": 11444.3,
            "points": 11.44
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
          "file_bytes": 55650,
          "analysis_version": 1,
          "published_at": "2026-11-18T19:05:00Z"
        }
      ]
    },
    {
      "pilot": {
        "handle": "demo",
        "display_name": "Demo Pilot"
      },
      "points": 11.98,
      "scored": 1,
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
          "analysis_version": 1,
          "published_at": "2026-11-18T19:05:00Z"
        }
      ]
    }
  ],
  "periods": [
    {
      "year": 2026,
      "flights": 10,
      "months": [
        {
          "month": "2026-09",
          "flights": 1
        },
        {
          "month": "2026-08",
          "flights": 4
        },
        {
          "month": "2026-07",
          "flights": 1
        },
        {
          "month": "2026-06",
          "flights": 2
        },
        {
          "month": "2026-05",
          "flights": 1
        },
        {
          "month": "2026-04",
          "flights": 1
        }
      ]
    },
    {
      "year": 2025,
      "flights": 1,
      "months": [
        {
          "month": "2025-07",
          "flights": 1
        }
      ]
    }
  ],
  "from": "2026-08-01T00:00:00Z",
  "to": "2026-09-01T00:00:00Z",
  "month": "2026-08",
  "terms": {
    "text": "These flights are their pilots'. They are shown here for reading. Reusing a flight (copying it elsewhere, publishing it, building on it) needs its pilot's leave until Venti's terms say otherwise, and a flight its pilot has withdrawn is to be removed from every copy.",
    "decided": false
  }
}
```

| Field | Meaning |
|---|---|
| `counted` | How many of a pilot's flights count at most: 3. It is in the answer so that a client words the rule from what the server did |
| `pilots[]` | The pilots, the one with the most points first. Pilots with the same points follow each other by `handle`, compared byte by byte |
| `pilots[].pilot` | The pilot, as in a [flight](#get-v1flightsid) |
| `pilots[].points` | The sum of the points of the flights that count, rounded to hundredths as a flight's points are |
| `pilots[].scored` | How many of the pilot's public flights among those asked for scored at all, the ones that count among them. A client can say "best 3 of 14" |
| `pilots[].flights[]` | The flights that count, the best first, each as the list of public flights shows it. Flights with the same points follow each other by `id`, the larger first |
| `from`, `to` | The span the ranking counts, as `from` and `to` or `month` asked for it: its first moment and the first moment after it. Each is left out when the request had none, so a ranking of all time has neither |
| `month` | The month the ranking is of, written as the parameter takes it, when `month` asked for one. A client names the period from these three and need not keep what it asked |

**Which periods there are.** `periods` names every year in which a public flight that scores started, the newest first, each with how many such flights it has and with its months that have one, the newest first too: `{"year": 2026, "flights": 10, "months": [{"month": "2026-09", "flights": 1}, ...]}`. A `month` is written as the parameter `month` takes it. The periods are those of the `bbox` that was asked for and of no span: they are the same whichever year or month the answer ranks, so a page offers them as the other rankings there are, and offers no empty one. Years and months are those of UTC. An answer of a server that does not say has none.

**Who is ranked.** A pilot with at least one public flight that scores, among the flights asked for. A flight too short to score counts for nothing, and a pilot who has only such flights, or none that is public, is not in the answer. The points are worked out by this server from the flight's file, for every flight by the same rules ([`score`](#get-v1flightsid)); they are not a league's, and neither is this ranking.

**The rule is the server's.** Three flights count, of any kind of route, and nothing weighs a flight by its glider's class, the pilot's country or the day. *The owner has not decided the rule; it is built this way so that there is a ranking to look at.*

**The top only.** The answer holds up to `limit` pilots from the top and has no `next`: a ranking is not read page by page. A server with more than a hundred ranked pilots shows the first hundred.

**Who may keep an answer.** `Cache-Control: public, no-cache` and an `ETag`, as for the list of public flights: a flight that is made private counts no longer with the next question, also for whoever kept the ranking.

| Status | Code | When |
|---|---|---|
| 400 | `invalid_bbox` | `bbox` is not four numbers that form a box |
| 400 | `invalid_time` | `from` or `to` is not an RFC 3339 time. `message` names the parameter |
| 400 | `invalid_range` | `from` is not earlier than `to` |
| 400 | `invalid_limit` | `limit` is not a number from 1 to 100 |

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

**An account whose publishing is suspended has no public track.** Whoever runs the server can [suspend an account's publishing](#what-whoever-runs-the-server-can-do). Its public tracks are `link` from that moment, and every batch it sends afterwards is taken as before and makes a track that is `link`, whatever `visibility` the request names. No batch is refused for it: positions are not lost, and the people who were given the link still see the glider. The answer's `track.visibility` says what the track is, so an app that asked for `public` and reads `link` there knows that the track is not on the map; the account says why (`publishing_suspended_at` in [`GET /v1/account`](#get-v1account)).

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
- Whoever runs the server can [remove a track](#what-whoever-runs-the-server-can-do). To everyone who reads, it then answers as a track that never was. To the app that sends, its identifier is used up: `410` with `live_track_gone`, as after the pilot's own deletion.
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
| 410 | `live_track_gone` | The identifier's track was deleted, was removed 48 hours after its newest fix or by whoever runs the server, or the identifier was taken out of use before a track began |
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

### `GET /v1/live/others?bbox=west,south,east,north`

Who else is in the air: the aircraft that other networks heard inside a box in the last ten minutes, one layer per network. Today the one network is the Open Glider Network (FLARM, FANET, OGN trackers and others; [data-sources.md](data-sources.md#open-glider-network)). Anyone may ask, and the answer is the same for everyone.

These are not Venti's pilots, and an aircraft of a network is never one record with a live track of [`GET /v1/live`](#get-v1livebboxwestsoutheastnorth): the network's data is under the ODbL, a licence of its own. A map that shows both asks both and draws two lists, each with its credit.

`bbox` is required, as for [stations](#get-v1stationsbboxwestsoutheastnorth), and may be at most 10 degrees from west to east and from south to north (`invalid_bbox` otherwise). The answer may be kept for five seconds.

What the network's rules ask is done before anything is listed: a device whose owner chose in the network's device database not to be tracked is never listed, nor one whose line carries the no-tracking flag; one whose owner chose not to be identified, or that the database does not know, is listed without `registration`, `competition_id` and `model`. A server that holds no device database younger than three days lists nobody. Nothing older than 24 hours is kept, and nothing older than ten minutes is listed. No device's address is in an answer: `id` is a name the server makes for the aircraft, the same while it runs.

```bash
curl 'http://127.0.0.1:8080/v1/live/others?bbox=11.6,47.55,12.0,47.85'
```

The development mode answers from lines in the network's form made up over the Tegernsee (not the network's data: it forbids passing on its data older than 24 hours); its clock for them stands just after the newest:

```json
{
  "time": "2026-11-21T19:09:10Z",
  "listed_minutes": 10,
  "layers": [
    {
      "source": "ogn",
      "connected": false,
      "aircraft": [
        {
          "id": "4791c887c4a52d11",
          "aircraft": "paraglider",
          "time": "2026-11-21T19:08:50Z",
          "age_s": 20,
          "position": {
            "lat": 47.664083,
            "lon": 11.7967
          },
          "altitude_m": 2140.0,
          "ground_speed_ms": 7.2,
          "climb_ms": 1.8,
          "course_deg": 210.0,
          "competition_id": "WB",
          "model": "Paraglider"
        },
        {
          "id": "e0eb481b81c2c9cd",
          "aircraft": "paraglider",
          "time": "2026-11-21T19:08:48Z",
          "age_s": 22,
          "position": {
            "lat": 47.7009,
            "lon": 11.761733
          },
          "altitude_m": 1400.0,
          "ground_speed_ms": 8.23,
          "climb_ms": -1.6,
          "course_deg": 300.0
        },
        {
          "id": "87fdab63df5ba895",
          "aircraft": "paraglider",
          "time": "2026-11-21T19:08:45Z",
          "age_s": 25,
          "position": {
            "lat": 47.671667,
            "lon": 11.808333
          },
          "altitude_m": 1800.0,
          "ground_speed_ms": 9.26,
          "climb_ms": -0.5,
          "course_deg": 45.0
        },
        {
          "id": "97cc402b0ebd23a3",
          "aircraft": "hang_glider",
          "time": "2026-11-21T19:08:30Z",
          "age_s": 40,
          "position": {
            "lat": 47.685017,
            "lon": 11.781683
          },
          "altitude_m": 2000.0,
          "ground_speed_ms": 10.29,
          "climb_ms": 0.51,
          "course_deg": 180.0
        },
        {
          "id": "06c45574ebec0149",
          "aircraft": "tow_plane",
          "time": "2026-11-21T19:09:05Z",
          "age_s": 5,
          "position": {
            "lat": 47.72,
            "lon": 11.743333
          },
          "altitude_m": 1300.0,
          "ground_speed_ms": 41.16,
          "climb_ms": 2.54,
          "course_deg": 10.0
        },
        {
          "id": "1f6311db029664cb",
          "aircraft": "glider",
          "time": "2026-11-21T19:09:00Z",
          "age_s": 10,
          "position": {
            "lat": 47.640033,
            "lon": 11.735083
          },
          "altitude_m": 2500.0,
          "ground_speed_ms": 28.29,
          "climb_ms": 0.1,
          "course_deg": 70.0,
          "registration": "D-0000",
          "competition_id": "TG",
          "model": "ASK 21"
        },
        {
          "id": "d5f160ea31beb19d",
          "aircraft": "obstacle",
          "time": "2026-11-21T19:09:00Z",
          "age_s": 10,
          "position": {
            "lat": 47.688117,
            "lon": 11.7608
          }
        }
      ],
      "truncated": false
    }
  ],
  "attributions": [
    {
      "source": "ogn",
      "provider": "Open Glider Network",
      "licence": "ODbL 1.0",
      "text": "Positions: Open Glider Network, glidernet.org (ODbL)",
      "url": "https://www.glidernet.org/ogn-data-usage/",
      "notice": "Positions of the Open Glider Network, available under the Open Database License, https://opendatacommons.org/licenses/odbl/1-0/. Not every aircraft sends its position, and a position may be late or wrong: never use it to avoid a collision."
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `time` | The server's clock when it wrote the answer; `age_s` is counted against it |
| `listed_minutes` | For how long after it was heard an aircraft is listed: 10 |
| `layers` | One per network the server listens to, sorted by `source`; empty on a server that listens to none |
| `layers[].connected` | Whether the server is connected to the network now. When it is not, the list is what was heard before, and ages |
| `layers[].aircraft` | Paragliders and hang gliders first, then the others, each part the most recently heard first; at most 1,000 |
| `aircraft` | `paraglider`, `hang_glider`, `glider`, `tow_plane`, `helicopter`, `parachute`, `drop_plane`, `powered_aircraft`, `jet_aircraft`, `balloon`, `airship`, `drone`, `obstacle` or `unknown`. A word a client does not know is `unknown` |
| `altitude_m`, `ground_speed_ms`, `climb_ms`, `course_deg` | As the network sent them, from feet, knots and feet a minute; left out where a line has none |
| `registration`, `competition_id`, `model` | Only where the owner let the aircraft be identified and gave them |
| `layers[].truncated` | `true` when the box holds more than 1,000; aircraft other than paragliders and hang gliders are left out first |
| `attributions` | The credit of every network in `layers`, with the notice the map shows beside it |

A map asks again every ten to fifteen seconds for the part in view.

### `GET /v1/live/climbs?bbox=west,south,east,north`

Where pilots climb at this moment: the places where paragliders and hang gliders circled and climbed in the last ten minutes. An instrument sees one glider; the server sees all that are heard, and a glider that circles and climbs marks a thermal for everyone. Anyone may ask, and the answer is the same for everyone. Since 2026-11-22.

- **A climb is a place in the air, not a person.** `position` is the middle of a cell of a fixed grid of 0.005° (about 560 m by 370 m at 47° north), the heights are rounded to 50 m, and nothing in the answer names a glider, a pilot, a device or a track. A climb that one pilot works is listed: a glider's position is in [`GET /v1/live`](#get-v1livebboxwestsoutheastnorth) or [`GET /v1/live/others`](#get-v1liveothersbboxwestsoutheastnorth) already, by its pilot's or owner's choice, and exactly.
- **Layers by where the positions come from, never one record.** One layer for each network the server listens to, under that network's credit in `attributions` (the Open Glider Network's positions are under the ODbL, and what is derived from them is credited as they are: [data-sources.md](data-sources.md#open-glider-network)), and one named `venti` from Venti's own live tracks. A thermal that gliders of both work is in both layers.
- **Whose positions.** Of a network: the devices that may be shown, by the rules of [`GET /v1/live/others`](#get-v1liveothersbboxwestsoutheastnorth); nothing is derived from a device whose owner chose not to be tracked, and nothing at all while the server holds no device database it trusts. Of Venti: the live tracks their pilots show to everyone (`public`); a track for those with the link marks nothing, and a track that is deleted or no longer public is out of the answer at once.
- **The rule**, the same for every glider and fixed: positions no more than 12 seconds apart follow one another; the glider is circling where its course over the ground turned at least 120° one way within 20 seconds; circling with no more than 20 seconds between is one climb; it counts when the glider turned a full circle in all, gained at least 30 m and climbed at least 0.5 m/s on average from its first circle to its last. A device heard less often than every 12 seconds marks no climb. The limits are *to verify* against real days.
- **What is kept:** the positions of the last twelve minutes of each glider, one every two seconds at most, in memory only; gone seventy minutes after its newest. Nothing of it is stored, logged or handed out.

`bbox` is required and may be at most 10 degrees each way (`invalid_bbox`). The answer may be kept for five seconds; a map asks again every half minute. An answer reads the gliders in and near its box from memory, and nothing from the database.

```bash
curl 'http://127.0.0.1:8080/v1/live/climbs?bbox=11.6,47.55,12.0,47.85'
```

The development mode has six made-up gliders over the Tegernsee: three circled in one thermal east of the Wallberg until four minutes before its clock, two circle south of the lake now:

```json
{
  "time": "2026-11-21T19:09:10Z",
  "listed_minutes": 10,
  "cell_deg": 0.005,
  "layers": [
    {
      "source": "ogn",
      "climbs": [
        {
          "position": {
            "lat": 47.6275,
            "lon": 11.7425
          },
          "climb_ms": 1.3,
          "from_m": 1350,
          "to_m": 1550,
          "pilots": 2,
          "age_s": 2
        },
        {
          "position": {
            "lat": 47.6675,
            "lon": 11.8025
          },
          "climb_ms": 2.4,
          "from_m": 1600,
          "to_m": 2100,
          "pilots": 3,
          "age_s": 240
        }
      ],
      "truncated": false
    },
    {
      "source": "venti",
      "climbs": [],
      "truncated": false
    }
  ],
  "attributions": [
    {
      "source": "ogn",
      "provider": "Open Glider Network",
      "licence": "ODbL 1.0",
      "text": "Positions: Open Glider Network, glidernet.org (ODbL)",
      "url": "https://www.glidernet.org/ogn-data-usage/",
      "notice": "Positions of the Open Glider Network, available under the Open Database License, https://opendatacommons.org/licenses/odbl/1-0/. Not every aircraft sends its position, and a position may be late or wrong: never use it to avoid a collision."
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `time` | The server's clock when it wrote the answer |
| `listed_minutes`, `cell_deg` | For how long a climb is listed, 10, and the side of a cell, 0.005 |
| `layers[].source` | A network's source, whose credit is in `attributions`, or `venti` |
| `climbs[]` | The newest first. `climb_ms`: the height gained by every pilot who worked it over the time they circled, to a tenth. `from_m`, `to_m`: the lowest height a pilot began to circle at and the greatest one circled to. `pilots`: how many circled in the cell. `age_s`: how long before `time` the last of them was heard circling |
| `layers[].truncated` | `true` when the box holds more than 500 cells with a climb; the newest are listed |

### `GET /v1/live/sites?bbox=west,south,east,north`

How many pilots are in the air at the flying sites of a box, and how many took off there in the last hour. Numbers only. Anyone may ask. Since 2026-11-22.

- **A site is named by its identifier**, as [`GET /v1/sites`](#get-v1sitesbboxwestsoutheastnorth) names it, and by nothing else: the site's record stays in its own layer under its own licence. Every take-off of every layer of sites the server has is looked at; a site that is not listed has nobody.
- **In the air**: a paraglider or hang glider heard in the last 5 minutes within `within_m`, 5 km, of the site, and moving at 3 m/s or more along its path of the last half minute (by the speed it reports where the path is too short to say). One whose sender says it landed is not. A tracker carried up the mountain in a car counts while the car drives: *to verify*.
- **Took off there**: first heard, after at least twenty minutes of silence, within 500 m of the site in the last `took_off_minutes`, no more than 150 m above it where the site has a height, and in the air now or since heard farther away than that.
- **Layers are not added up.** One layer per source of live positions, as for the climbs. A pilot whom a network hears and who sends Venti a live track is in both; a client that wants one number takes the larger, as Venti's apps do.

`bbox` is required, at most 10 degrees each way (`invalid_bbox`), and a box with more than 5,000 sites is refused as [`GET /v1/sites`](#get-v1sitesbboxwestsoutheastnorth) refuses it (`box_too_large`). The answer may be kept for five seconds.

```bash
curl 'http://127.0.0.1:8080/v1/live/sites?bbox=11.6,47.55,12.0,47.85'
```

```json
{
  "time": "2026-11-21T19:09:10Z",
  "within_m": 5000.0,
  "took_off_minutes": 60,
  "layers": [
    {
      "source": "ogn",
      "sites": [
        {
          "site": "osm-free-flying:node/5964676196",
          "airborne": 8,
          "took_off": 2
        },
        {
          "site": "osm-free-flying:node/5964676197",
          "airborne": 8,
          "took_off": 2
        }
      ],
      "truncated": false
    },
    {
      "source": "venti",
      "sites": [],
      "truncated": false
    }
  ],
  "attributions": [
    {
      "source": "ogn",
      "provider": "Open Glider Network",
      "licence": "ODbL 1.0",
      "text": "Positions: Open Glider Network, glidernet.org (ODbL)",
      "url": "https://www.glidernet.org/ogn-data-usage/",
      "notice": "Positions of the Open Glider Network, available under the Open Database License, https://opendatacommons.org/licenses/odbl/1-0/. Not every aircraft sends its position, and a position may be late or wrong: never use it to avoid a collision."
    }
  ]
}
```

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

## Groups

A group is a circle of pilots who plan flying days together. A signed-in pilot makes one and hands its **invite code** to the pilots who are to be in it; whoever has the code and an account comes in. In a group, members post [plans for a flying day](#plans-for-a-flying-day) and say whether they are in. That is all a group does so far. It has no chat, and it shows its members nothing of each other but a handle and a display name: not their flights, not their live tracks.

What a client has to know:

- **A group is seen by its members and by nobody else.** It cannot be found by its name, and it is in no list but its members' own. To a pilot who is not in it, every request about it is answered `404` with `group_not_found`: in status, in every header and in every byte of the body the answer for an identifier that no group has. That holds for every endpoint below, for a pilot who was never in the group, for one who left and for one who was removed.
- **The keeper.** The pilot who makes a group keeps it. The keeper can rename it, make a new invite code, remove a member, hand the keeping to another member, delete the group, and delete any plan in it. The keeper is told nothing about the group that its other members are not told, with one exception: the invite code, at the moment it is made.
- **The invite code** is sixteen characters in four groups, such as `7k3m-9q2x-4r8t-1v6w`: 80 random bits. One code lets in as many pilots as the group has room for, for as long as the code lasts. It is shown once, in the answer to the request that makes it; the server keeps a hash of it and cannot show it again. An app that wants to show the keeper the code again keeps it where the device keeps secrets. Whoever has the code can come in, so treat it like a password: it travels in the body of a request, never in a path or a query string, and it belongs in no log.
- **A code ends** when the keeper makes a new one, when the keeper removes a member, who knew it, and when the group goes. After a member was removed, nobody comes in until the keeper has made a new code. A member who leaves by themselves ends no code, and can come back with it.
- **Who goes takes along what they put in.** A member leaves by removing themselves. The plans they posted in the group go with them, with who was in on those, and so does their "I am in" on the plans of others. The same happens to a member the keeper removes, and in every group of a pilot who [deletes their account](#delete-v1account).
- **When the keeper goes**, by leaving or by deleting their account, the member who has been in the group longest keeps it from then on: the first of the others in the list of members. A keeper who wants someone else to keep it hands it over first. When the last member goes, the group is deleted with everything in it, and its code ends.
- **Limits.** An account is in at most 20 groups, those it keeps included. A group has at most 50 members and holds at most 100 plans at a time. An account makes at most 10 groups a day, tries at most 20 invite codes a day, and posts or changes at most 50 plans a day, whatever becomes of each attempt; after that the answer is `429` with `rate_limited` and a `Retry-After` header. The counts are in the server's memory, like the others, and the development mode has none of the three.
- **Every answer is made for one account**: `Cache-Control: no-store` and no `ETag`, like every answer to a request with a token.
- **Text that pilots type** (a group's name, a plan's place and note) is cleaned before it is kept and never refused for a character in it: control characters go, and so do the characters that hide text or turn its direction round. Show it as text and never as markup. `venti_api_types::v1::parse_group_name`, `parse_plan_place` and `parse_plan_note` do on the device what the server does, for a client that wants to count while the pilot types.
- **A member can report the group's name, or a plan, to whoever runs the server**, with [`POST /v1/reports`](#post-v1reports). Nobody in the group is told. What the person who runs the server can then do in a group, and what that looks like from an app, is said [further down](#what-whoever-runs-the-server-can-do): give the group another name, remove a plan, remove the group. Each looks exactly as if the keeper or the plan's author had done it.

Not built: a link that carries the code, finding a group by name, more than one keeper, chat, events, and anything that tells a member that something happened in a group. A client asks.

A group, in every answer that carries one:

| Field | Meaning |
|---|---|
| `id` | The group's identifier: 26 lower-case letters and digits. It names the group in the paths below. To its members it is no secret; to everyone else it is the identifier of nothing |
| `name` | Its name: one line of 1 to 60 characters |
| `created_at` | When it was made |
| `keeper` | The member who keeps it: `handle` and `display_name` |
| `member_count` | How many members it has, the keeper included |

### `POST /v1/groups`

Needs a token. Makes a group. The asker keeps it and is its only member.

```json
{
  "name": "Zweite Mustergruppe"
}
```

| Field | Meaning |
|---|---|
| `name` | The group's name. A line break or a tab in it becomes a space and several spaces become one; then it must have 1 to 60 characters |

The answer is `201`. This is the development mode's, with the group's identifier and the invite code replaced, because both are different every time:

```json
{
  "group": {
    "id": "0123456789abcdefghjkmnpqrs",
    "name": "Zweite Mustergruppe",
    "created_at": "2026-11-18T19:05:00Z",
    "keeper": {
      "handle": "demo",
      "display_name": "Demo Pilot"
    },
    "member_count": 1
  },
  "invite_code": "7k3m-9q2x-4r8t-1v6w"
}
```

| Field | Meaning |
|---|---|
| `group` | The group |
| `invite_code` | The code that lets others in. It is not shown again: an app shows it to the keeper now, to be handed on |

| Status | Code | When |
|---|---|---|
| 400 | `invalid_group_name` | Nothing is left of `name` once it is cleaned, or more than 60 characters are. `message` says which, and repeats nothing that was sent |
| 403 | `too_many_groups` | The account is in 20 groups. Leaving one makes room |
| 429 | `rate_limited` | The account has made 10 groups within a day. An attempt that was refused for its name counts too |

### `GET /v1/groups`

Needs a token. The groups the asker is in, the one they have been in longest first. An account that is in none gets an empty list.

```json
{
  "groups": [
    {
      "id": "44444444444444444444444444",
      "name": "Mustergruppe",
      "created_at": "2026-10-19T19:05:00Z",
      "keeper": {
        "handle": "demo",
        "display_name": "Demo Pilot"
      },
      "member_count": 3
    }
  ]
}
```

### `POST /v1/groups/join`

Needs a token. Makes the asker a member of the group the code belongs to. The code is the whole request: the asker need not know which group it is, and learns it from the answer.

```json
{
  "code": "5555-5555-5555-5555"
}
```

| Field | Meaning |
|---|---|
| `code` | The invite code, as the keeper handed it on. Hyphens, spaces and upper case do not matter |

The answer is `201` when the request made the asker a member, and `200` when they were one already. This is the demo account coming into the development mode's second group:

```json
{
  "group": {
    "id": "55555555555555555555555555",
    "name": "Beispielgruppe",
    "created_at": "2026-11-08T19:05:00Z",
    "keeper": {
      "handle": "mara",
      "display_name": "Mara Muster"
    },
    "member_count": 3
  },
  "joined": true
}
```

| Field | Meaning |
|---|---|
| `group` | The group the code belongs to |
| `joined` | `true` when this request made the asker a member, `false` when they were one already |

| Status | Code | When |
|---|---|---|
| 403 | `invalid_group_invite` | The code lets nobody in. The answer is the same, in every byte, whatever is wrong with it: mistyped, never made, replaced by a newer one, ended with the removal of a member, its group gone, or not text of a code's form |
| 403 | `group_full` | The group has 50 members |
| 403 | `too_many_groups` | The asker is in 20 groups |
| 429 | `rate_limited` | The account has tried 20 codes within a day, right or wrong |

Whether a code is good is looked at before anything else. `group_full` and `too_many_groups` are therefore only ever said to a request that carries a good code, and a pilot who is in the group already is told so even when it is full.

### `GET /v1/groups/{id}`

Needs a token. A group and its members, for one of them.

```json
{
  "group": {
    "id": "44444444444444444444444444",
    "name": "Mustergruppe",
    "created_at": "2026-10-19T19:05:00Z",
    "keeper": {
      "handle": "demo",
      "display_name": "Demo Pilot"
    },
    "member_count": 3
  },
  "members": [
    {
      "handle": "demo",
      "display_name": "Demo Pilot"
    },
    {
      "handle": "mara",
      "display_name": "Mara Muster"
    },
    {
      "handle": "jonas",
      "display_name": "Jonas Beispiel"
    }
  ]
}
```

| Field | Meaning |
|---|---|
| `group` | The group |
| `members` | Its members, each with `handle` and `display_name`, in the order in which they came in. The first of them who is not the keeper is who keeps the group if the keeper goes. Members who came in within the same second are in an order of the server's own, which stays the same |

To anyone who is not in the group: `404` with `group_not_found`.

### `PATCH /v1/groups/{id}`

Needs a token. The keeper gives the group a new name.

```json
{
  "name": "Mustergruppe, renamed"
}
```

The answer is `200` with the group and its members, as `GET /v1/groups/{id}` gives them.

| Status | Code | When |
|---|---|---|
| 400 | `invalid_group_name` | As for making a group |
| 403 | `not_group_keeper` | The asker is in the group and does not keep it |
| 404 | `group_not_found` | The asker is not in the group, or there is none |

### `DELETE /v1/groups/{id}`

Needs a token. The keeper deletes the group: its members are in no group of that identifier any more, its plans are gone with who was in on them, and its code lets nobody in. The answer is `204` with no body. It happens at once and cannot be undone. `403` with `not_group_keeper` for a member who does not keep it, `404` with `group_not_found` for everyone else.

### `POST /v1/groups/{id}/invite`

Needs a token. No body. The keeper makes a new invite code. The code before it lets nobody in from that moment; who is in the group stays in. The answer is `200`, here with the code replaced:

```json
{
  "invite_code": "2w8x-5c9p-3h7k-6n4d"
}
```

This is also how a keeper closes a group to newcomers: make a new code and hand it to nobody. `403` with `not_group_keeper` for a member who does not keep the group, `404` with `group_not_found` for everyone else.

### `PUT /v1/groups/{id}/keeper`

Needs a token. The keeper hands the keeping to another member.

```json
{
  "handle": "mara"
}
```

The answer is `200` with the group and its members, as `GET /v1/groups/{id}` gives them, now with the other member as `keeper`. Who kept the group is a member like any other from then on, and stays where they were in the list of members. Naming the keeper's own handle changes nothing and is answered the same way.

| Status | Code | When |
|---|---|---|
| 403 | `not_group_keeper` | The asker is in the group and does not keep it |
| 404 | `group_member_not_found` | `handle` is not the handle of a member of the group. A pilot who is not in the group, a handle nobody has and text that is no handle all get this answer |
| 404 | `group_not_found` | The asker is not in the group, or there is none |

### `DELETE /v1/groups/{id}/members/{handle}`

Needs a token. Takes a member out of the group. With the asker's own handle this is **leaving**, which every member may. With another member's handle it is **removing**, which only the keeper may, and which ends the group's invite code. The answer is `204` with no body.

What the member put into the group goes with them, as said above. When the keeper leaves, the keeping passes on; when the last member leaves, the group is deleted.

| Status | Code | When |
|---|---|---|
| 403 | `not_group_keeper` | The asker is in the group, does not keep it, and named someone else |
| 404 | `group_member_not_found` | The keeper named a handle that no member of the group has |
| 404 | `group_not_found` | The asker is not in the group, or there is none. A pilot who has left gets this for their own handle |

## Plans for a flying day

A plan is one member's word to the group: on this day I mean to fly here. The others say that they are in, or take that back. A plan belongs to one group; a pilot who wants to tell two groups posts it twice.

What a client has to know:

- **The day** is a calendar date, `2026-11-21`, as it is at the place where the flying is. The server keeps the date as it was sent and attaches no time zone to it: it does not know where the place is. `meet_at` is a time of day as a clock at the place shows it, `09:30` on a 24-hour clock, and has no zone either. Show both as they are. Do not convert them.
- **Which day is today** is for the client to say. A list of plans is asked for with the days wanted (`from`, `to`), and the board of today is the day's date as both. Without `from` a list starts at the date the server's clock shows in UTC, which away from Greenwich is the pilot's date for only a part of the day. The apps take today's date from the device's own clock where the pilot is: a plan's day is a date on the pilot's calendar, and the device is what knows it. They do not take it from the server's clock, as they do for the age of a wind reading; the two differ only where a device's clock is wrong. Against the development mode, whose clock stands at 19:05 UTC on 18 November 2026, an app therefore asks for the real day, and the mode's own plans lie on the 18th, 19th and 20th of November 2026.
- **The place** is a name the pilot types, one line of at most 80 characters, and a position if the pilot marked one. The server looks neither up and adds nothing to them. A client offers a field to type in and, if it likes, a map to tap on. It does not fill a plan from the [flying sites](#get-v1sitesbboxwestsoutheastnorth) the server serves: their names and positions are OpenStreetMap's under a share-alike licence, a plan is its pilot's own, and data of the two kinds is not put into one record.
- **Who is in** is the list `in`, in the order in which the members said so. Whoever posts a plan is in from the start and can take that back like anyone else; the plan stays.
- **Changing a plan** is its author's alone, and a plan is sent whole: what a change leaves out, the plan no longer has. Who is in stays as it is. Deleting a plan is its author's and the keeper's.
- **A plan is kept for 2 days after its day**, by the server's date in UTC, and then removed: a plan for the 18th is there on the 19th and the 20th and gone on the 21st. From that moment it is in no answer and cannot be changed, whether or not the server has got round to deleting the row, which it does within the hour. Whoever runs a server can set another number of days (`retention.plan_days`). A plan is in no backup the server's own script makes. One thing outlasts it: the place and the note of a plan that a member [reported](#reports), which are kept with the report.
- **A plan is taken for a day** from the day before the server's date to a year after it. The day before, because west of Greenwich that day is not over when the server's is. A later day is refused with `invalid_plan`. The development mode takes any day from 17 November 2026 on.
- **The order of a list**: by day; within a day by `meet_at`, with the plans that have none last; then in the order in which they were posted.

A plan, in every answer that carries one:

| Field | Meaning |
|---|---|
| `id` | The plan's identifier: 26 lower-case letters and digits |
| `group` | The identifier of the group it was posted to |
| `day` | The day it is for, `YYYY-MM-DD` |
| `place` | The place, as its author typed it: one line of 1 to 80 characters |
| `position` | Where the place is, `lat` and `lon` in degrees. Left out if the author marked none |
| `meet_at` | When to meet or to start, `HH:MM`. Left out if the author named no time |
| `note` | What else the author had to say: at most 500 characters, with line breaks. Left out if there is none |
| `author` | The member who posted it: `handle` and `display_name` |
| `created_at` | When it was posted |
| `changed_at` | When its author last changed what it says. The same as `created_at` for a plan that was never changed; who is in does not move it |
| `in` | The members who are in, each with `handle` and `display_name`, in the order in which they said so. May be empty |

### `POST /v1/groups/{id}/plans`

Needs a token. A member posts a plan to the group.

```json
{
  "day": "2026-11-21",
  "place": "Beispielwiese, Parkplatz",
  "position": {
    "lat": 47.6922,
    "lon": 11.7675
  },
  "meet_at": "09:30",
  "note": "A made-up plan. Whoever has room in the car says so here."
}
```

| Field | Meaning |
|---|---|
| `day` | The day, `YYYY-MM-DD` |
| `place` | The place, in the pilot's words. Cleaned to one line like a group's name; then 1 to 80 characters |
| `position` | Where that is: `lat` and `lon` in degrees. May be left out |
| `meet_at` | When to meet or to start: `HH:MM`, two digits each, `00:00` to `23:59`. May be left out |
| `note` | Anything else: at most 500 characters after cleaning, line breaks kept. May be left out |

The answer is `201` with the plan. This is the development mode's, with the plan's identifier replaced:

```json
{
  "plan": {
    "id": "0123456789abcdefghjkmnpqrs",
    "group": "44444444444444444444444444",
    "day": "2026-11-21",
    "place": "Beispielwiese, Parkplatz",
    "position": {
      "lat": 47.6922,
      "lon": 11.7675
    },
    "meet_at": "09:30",
    "note": "A made-up plan. Whoever has room in the car says so here.",
    "author": {
      "handle": "demo",
      "display_name": "Demo Pilot"
    },
    "created_at": "2026-11-18T19:05:00Z",
    "changed_at": "2026-11-18T19:05:00Z",
    "in": [
      {
        "handle": "demo",
        "display_name": "Demo Pilot"
      }
    ]
  }
}
```

| Status | Code | When |
|---|---|---|
| 400 | `invalid_plan` | `day` lies before the server's yesterday or more than a year ahead, `place` is empty or too long once cleaned, or `note` is too long. `message` says which, and for a day names the days that are taken and the server's date; it repeats no text that was sent |
| 400 | `invalid_request` | The body is not JSON of this shape: no `day` or `place`, a `day` that is no date, a `meet_at` that is no time of day, a `position` that is no place on the globe |
| 403 | `group_plans_full` | The group holds 100 plans. Deleting one makes room; so does time, as plans of days gone by are removed |
| 404 | `group_not_found` | The asker is not in the group, or there is none |
| 429 | `rate_limited` | The account has posted or changed 50 plans within a day |

### `GET /v1/groups/{id}/plans?from=&to=`

Needs a token. A group's plans, for one of its members: for today and the days ahead, or for the days asked for.

| Parameter | Meaning |
|---|---|
| `from` | The first day wanted, `YYYY-MM-DD`. Left out: the date the server's clock shows in UTC |
| `to` | The last day wanted. Left out: every day from the first on |

An earlier `from` reaches back to the plans of days gone by for as long as they are kept, and no further. These are the development mode's three plans of the demo account's group, asked for without parameters:

```json
{
  "plans": [
    {
      "id": "66666666666666666666666666",
      "group": "44444444444444444444444444",
      "day": "2026-11-18",
      "place": "Musterberg, Startplatz Ost",
      "position": {
        "lat": 47.6642,
        "lon": 11.7967
      },
      "meet_at": "10:30",
      "note": "A made-up plan of the development mode.\nAt the lift at half past ten, in the air by noon.",
      "author": {
        "handle": "demo",
        "display_name": "Demo Pilot"
      },
      "created_at": "2026-11-17T17:05:00Z",
      "changed_at": "2026-11-17T17:05:00Z",
      "in": [
        {
          "handle": "demo",
          "display_name": "Demo Pilot"
        },
        {
          "handle": "mara",
          "display_name": "Mara Muster"
        },
        {
          "handle": "jonas",
          "display_name": "Jonas Beispiel"
        }
      ]
    },
    {
      "id": "77777777777777777777777777",
      "group": "44444444444444444444444444",
      "day": "2026-11-19",
      "place": "Beispielwiese",
      "meet_at": "09:00",
      "note": "A made-up plan. Ground handling if the wind allows, coffee if not.",
      "author": {
        "handle": "mara",
        "display_name": "Mara Muster"
      },
      "created_at": "2026-11-18T16:05:00Z",
      "changed_at": "2026-11-18T16:05:00Z",
      "in": [
        {
          "handle": "mara",
          "display_name": "Mara Muster"
        },
        {
          "handle": "jonas",
          "display_name": "Jonas Beispiel"
        }
      ]
    },
    {
      "id": "88888888888888888888888888",
      "group": "44444444444444444444444444",
      "day": "2026-11-20",
      "place": "Musterberg",
      "author": {
        "handle": "jonas",
        "display_name": "Jonas Beispiel"
      },
      "created_at": "2026-11-18T18:05:00Z",
      "changed_at": "2026-11-18T18:05:00Z",
      "in": [
        {
          "handle": "jonas",
          "display_name": "Jonas Beispiel"
        },
        {
          "handle": "demo",
          "display_name": "Demo Pilot"
        }
      ]
    }
  ]
}
```

The answer is never cut: a group holds at most 100 plans.

| Status | Code | When |
|---|---|---|
| 400 | `invalid_day` | `from` or `to` is not a date written `YYYY-MM-DD` |
| 400 | `invalid_range` | `from` lies after `to` |
| 404 | `group_not_found` | The asker is not in the group, or there is none |

### `GET /v1/groups/plans?from=&to=`

Needs a token. The plans of every group the asker is in, in one answer, with those groups: what a board of "who flies where today" shows. The parameters and their errors are those of the list for one group. This is the demo account's board for one day, `?from=2026-11-18&to=2026-11-18`:

```json
{
  "groups": [
    {
      "id": "44444444444444444444444444",
      "name": "Mustergruppe",
      "created_at": "2026-10-19T19:05:00Z",
      "keeper": {
        "handle": "demo",
        "display_name": "Demo Pilot"
      },
      "member_count": 3
    }
  ],
  "plans": [
    {
      "id": "66666666666666666666666666",
      "group": "44444444444444444444444444",
      "day": "2026-11-18",
      "place": "Musterberg, Startplatz Ost",
      "position": {
        "lat": 47.6642,
        "lon": 11.7967
      },
      "meet_at": "10:30",
      "note": "A made-up plan of the development mode.\nAt the lift at half past ten, in the air by noon.",
      "author": {
        "handle": "demo",
        "display_name": "Demo Pilot"
      },
      "created_at": "2026-11-17T17:05:00Z",
      "changed_at": "2026-11-17T17:05:00Z",
      "in": [
        {
          "handle": "demo",
          "display_name": "Demo Pilot"
        },
        {
          "handle": "mara",
          "display_name": "Mara Muster"
        },
        {
          "handle": "jonas",
          "display_name": "Jonas Beispiel"
        }
      ]
    }
  ],
  "truncated": false
}
```

| Field | Meaning |
|---|---|
| `groups` | The groups the asker is in, as `GET /v1/groups` lists them: every one, also a group without a plan in this answer, so that a board can say "nothing planned" for it |
| `plans` | The plans of those groups for the days asked for, in the order given above. `group` in each says which group it is of |
| `truncated` | `true` when the groups hold more than 200 plans for those days and the answer has the first 200. Ask for fewer days |

An account that is in no group gets two empty lists. A plan of a group the asker is not in is never in this answer.

### `PUT /v1/groups/{id}/plans/{plan}`

Needs a token. The member who posted a plan replaces what it says. The body is that of posting one, whole; the answer is `200` with the plan as it now is. `changed_at` moves; `in` does not.

| Status | Code | When |
|---|---|---|
| 400 | `invalid_plan`, `invalid_request` | As for posting a plan |
| 403 | `not_plan_author` | The asker is in the group and did not post the plan. The keeper may delete a plan and may not change one |
| 404 | `plan_not_found` | The group has no plan with this identifier: none ever, deleted, gone with its author, a plan of another group, or one whose day lies further back than plans are kept |
| 404 | `group_not_found` | The asker is not in the group, or there is none. An account that is not in the group never learns whether a plan exists |
| 429 | `rate_limited` | The account has posted or changed 50 plans within a day |

### `DELETE /v1/groups/{id}/plans/{plan}`

Needs a token. Deletes a plan, with who was in on it. The member who posted it may, and the keeper of the group may. The answer is `204` with no body. `403` with `not_plan_author` for any other member; `404` with `plan_not_found` or `group_not_found` as for changing a plan.

### `PUT` and `DELETE /v1/groups/{id}/plans/{plan}/in`

Need a token, and take no body. `PUT` says that the asker is in on the plan; `DELETE` takes that back. Both answer `200` with the plan as it now is, and both may be sent again without changing anything: a pilot who is in stays where they are in the list, and a pilot who is not in stays out. Neither is counted. `404` with `plan_not_found` or `group_not_found` as for changing a plan.

## Reports

A signed-in pilot can tell whoever runs the server that something should not be there: a flight, a pilot's page or a live track, in a group the pilot is in, the group's name or one of its plans, and a [site that pilots made](#sites-that-ventis-partners-and-pilots-add). That is all a report does. The server stores it and does nothing else: nothing is hidden, no account is touched, no group is changed, nobody is told. A person reads the reports and decides, with commands on the server's own machine; what those can do is said [below](#what-whoever-runs-the-server-can-do).

What a client has to know:

- **Nobody learns from the server who reported them.** The API has no way to read a report back, to list reports or to count them, not for the reporter and not for the pilot who was reported. No answer to the reported pilot changes in anything when a report arrives or is closed, and none to the keeper or to any member of a group that was reported.
- **Whoever runs the server does see who reported.** The report is kept with the reporter's handle, and the operator reads it beside the report. Say so where the report is written: a report is not anonymous.
- **What can be reported is what the reporter can see**: a public flight, a flight or a live track the reporter has the link to, the page of a pilot who has one, a group the reporter is in and the plans of such a group, and anything of the reporter's own. Everything else gets the answer it gets everywhere, `404` in the same words, so that reporting is no way to find out whether a private flight, a quiet account or a group one is not in exists.
- **One open report about one thing.** While an account has an open report about a thing, a second one about the same thing stores nothing and changes nothing, whatever reason and text it has: `200` with `"created": false`. Once the operator has closed the first, the thing can be reported again.
- **Nothing comes back.** The reporter is not told what was decided, and cannot ask. The server knows no address to write to.
- **Reports are counted**: an account may send 20 in a day. See the errors below.
- **A group and a plan are reported by a member of the group**, any member: the keeper has no say in it and is not told. What is reported of a [group](#groups) is its name, and of a [plan](#plans-for-a-flying-day) its place and its note: the text that other people read. To an account that is not in the group, a report about it or about a plan of it is answered `404` with `group_not_found`, in status, in every header and in every byte the answer for an identifier that no group has; it holds for a pilot who was never in the group, for one who left and for one who was removed.
- **A report about a group or a plan takes the words along.** A name, a place and a note can be changed or deleted a moment after they were reported, so the server keeps, with the report, the group's name or the plan's place and note as they stood when the report was made. Nothing else is copied: not who is in the group, and of a plan neither its day, its time, its position nor who was in on it. The copy is kept as long as the report and goes with it. It is the one place where words of a plan outlast the two days a plan is kept, and the one place where they are in a backup. Say so where the report is written.
- **A report is about an account as well**: the pilot of a flight, a page or a live track, the author of a plan, and for a group the member who keeps it at that moment. A report is removed when that account or the reporter's is deleted.
- **A report about a site is about nobody's account.** A site that pilots made is nobody's: any pilot corrects it, and it stays when the account that added it goes. Such a report names the site and one revision of it, the one the request names or the one the site stands at, and no pilot; it is removed with its reporter's account and with no other. Nothing the site says is copied into it: every revision of a site is kept, and whoever runs the server reads the words there. Only a site of the layer `venti` is reported: a site of a source the server collects is that source's record and is corrected there (`400` with `invalid_report`). A site that a pilot removed can still be reported, because its history can still be read; one that whoever runs the server took down shows nothing and answers `410` with `site_gone`. One open report per account and site, whichever revision each names. Anyone signed in may report a site, as anyone may read it.
- **A server from before 2026-11-21 knows three kinds of subject, or five.** The list of subjects is closed: a server that is sent a kind it does not know cannot read the request and answers `400` with `invalid_request`, and stores nothing. A client that gets this answer to a report about a group, a plan or a site is talking to an older server, and so is one that gets it to a report about a comment: comments, and reports about them, are there since 2026-11-22.

### `POST /v1/reports`

Needs a token.

```bash
curl -X POST http://127.0.0.1:8080/v1/reports \
  -H 'Authorization: Bearer venti_0000000000000000000000000000000000000000000000000000' \
  -H 'Content-Type: application/json' \
  -d '{"subject":{"flight":"fbyw03atd9qdt5bfe177ra1k94"},"reason":"not_a_flight","text":"I flew there that day. This is my track, not hers."}'
```

The body of that request:

```json
{
  "subject": {
    "flight": "fbyw03atd9qdt5bfe177ra1k94"
  },
  "reason": "not_a_flight",
  "text": "I flew there that day. This is my track, not hers."
}
```

| Field | Meaning |
|---|---|
| `subject` | What is reported: an object with exactly one of the seven members below |
| `subject.flight` | A flight, by its address: the `id` of the flight, which is the end of its share link |
| `subject.pilot` | A pilot's page, by the pilot's handle, in upper or lower case |
| `subject.live_track` | A live track, by its identifier, which is the end of its link |
| `subject.group` | A group the reporter is in, by its `id` |
| `subject.plan` | A plan of a group the reporter is in: an object with `group`, the group's `id`, and `id`, the plan's. A plan is named with its group here as in every path about a plan, and the `plan` of every answer carries both |
| `subject.site` | A site that pilots made: an object with `id`, the site's identifier (`venti:` or `venti-pilots:` and 26 characters: a site of either layer is reported the same way), and, if the report is about one revision of it, `revision`, that revision's number. Without `revision` the report is about the site as it stands, and the server notes the revision it stands at |
| `subject.comment` | A [comment](#pilots-among-each-other) under a public flight: an object with `flight`, the address of the flight, and `id`, the comment's. What is reported is its words, which the server keeps with the report as they stood: its author or the flight's pilot can remove it a moment later. The reasons are those of a group and a plan. The report is about the comment's author |
| `reason` | Why; one of the values below |
| `text` | What the reporter wants to add, in their own words: at most 1,000 characters. May be left out, except with the reason `other` |

A report about a group, and one about a plan. The first is sent by `jonas`, the second by the demo account, about the development mode's group and a plan `mara` posted in it:

```json
{
  "subject": {
    "group": "44444444444444444444444444"
  },
  "reason": "impersonation",
  "text": "This is not the club's group. The club has none here."
}
```

```json
{
  "subject": {
    "plan": {
      "group": "44444444444444444444444444",
      "id": "77777777777777777777777777"
    }
  },
  "reason": "spam",
  "text": "An advertisement, not a plan."
}
```

A report about a site that pilots made, as it stands. With `"revision": 2` beside `id` it is about that revision:

```json
{
  "subject": {
    "site": {
      "id": "venti:0000000000000000000000s001"
    }
  },
  "reason": "wrong_or_dangerous",
  "text": "The take-off is on the other side of the ridge."
}
```

| `reason` | Meaning |
|---|---|
| `offensive` | A name, a glider's name or another text that insults, threatens or harasses |
| `impersonation` | The account passes itself off as another person or as an organisation; a group, as a club or an organisation it is not |
| `privacy` | It shows something about a person who did not agree to that: a flight that is someone else's, a name, where someone lives |
| `not_a_flight` | The flight was not flown as it is shown: a track that is made up or altered, or not this pilot's own. Not a reason for a group or a plan |
| `spam` | Advertising, or something that has nothing to do with flying |
| `other` | Anything else. The report must then say what, in `text` |
| `wrong_or_dangerous` | A site is at the wrong place, or says something wrong that can hurt a pilot: a wind direction, a height, a landing that is none. A reason for a site only |
| `not_a_site` | The place is no flying site: nobody takes off or lands there, or nobody may. A reason for a site only |

The list is short on purpose and is the server's: a reason that is not on it is refused, so a client offers the first six for a flight, a page or a live track and no field for another. For a group or a plan it offers five: `not_a_flight` is about a flight, and the server refuses it for words. For a site it offers six, in this order: `wrong_or_dangerous`, `not_a_site`, `offensive`, `privacy`, `spam`, `other`; a site is nobody's account and no flight, so `impersonation` and `not_a_flight` are refused for it, and the two reasons of a site are refused for everything else. There is no reason for "flew through airspace": a report is about what should not be shown, not about how someone flew. `venti_api_types::v1::ReportSubject::reasons` is the list a report about one kind of subject can give, in the order a form shows it.

**The text** is cleaned before it is kept and never refused for what stands in it: line breaks stay, every other control character goes, and so do the characters that hide text or turn its direction round; white space at both ends goes. What is left is counted, and more than 1,000 characters is refused. `venti_api_types::v1::parse_report_text` does the same on the device, for a client that wants to count while the pilot types. The text is read by whoever runs the server and by nobody else.

The answer is `201` when the request stored the report, and `200` when the account had an open report about the same thing already:

```json
{
  "created": true
}
```

| Field | Meaning |
|---|---|
| `created` | `true` when this request stored the report, `false` when the account's earlier report about the same thing is still open. Either way a person will read one |

The answer carries `Cache-Control: no-store`, like every answer to a request with a token.

| Status | Code | When |
|---|---|---|
| 400 | `invalid_report` | `reason` is not one the kind of subject takes: not one of the six, `not_a_flight` for a group, a plan or a comment, a reason of a site for anything else, or `impersonation` or `not_a_flight` for a site; `subject.site` is a site of a source and not one that pilots made; `text` has more than 1,000 characters after cleaning; or `reason` is `other` and nothing is left of `text`. `message` says which, and repeats nothing that was sent. Whether the reason fits is decided by the kind of subject alone, before anything is looked up |
| 400 | `invalid_request` | The body is not JSON of this shape: no `subject`, two subjects in one, a kind of subject the server does not know, a plan without its group, no `reason` |
| 401 | `unauthorized` | No valid token |
| 404 | `flight_not_found` | `subject.flight` is no flight the reporter may see: nobody's address, text that is no address, or another pilot's private flight. The answer is the one `GET /v1/flights/{id}` gives the reporter |
| 404 | `pilot_not_found` | `subject.pilot` is the handle of nobody, or of a pilot who shares nothing publicly and so has no page |
| 404 | `live_track_not_found` | `subject.live_track` leads to no live track: none ever, deleted, or removed for its age |
| 404 | `group_not_found` | `subject.group`, or the `group` of `subject.plan`, is no group the reporter is in: nobody's identifier, text that is no identifier, or a group the reporter is not, or no longer, a member of. The answer is the one `GET /v1/groups/{id}` gives the reporter, and it is given whatever `subject.plan.id` says |
| 404 | `plan_not_found` | The reporter is in the group, and it has no plan with this identifier: none ever, deleted, gone with its author, a plan of another group, or one whose day lies further back than plans are kept |
| 404 | `site_not_found`, `site_revision_not_found` | `subject.site.id` is no site that pilots made here; the site has no revision with the number in `subject.site.revision` |
| 410 | `site_gone` | Whoever runs the server took the site down: nothing of it is shown, and there is nothing to report |
| 413 | `payload_too_large` | The body is larger than 16 kB |
| 415 | `unsupported_media_type` | The request does not say `Content-Type: application/json`. Such a request is not counted |
| 429 | `rate_limited` | The account has sent 20 reports within a day. `Retry-After` says when the day is over, in seconds; asking again in between does not put it off |

**The count.** Every request that says it is JSON and carries a valid token counts as one of the 20, whatever becomes of it: one that is stored, one about a thing that was reported already, one that is refused, one that names nothing. A report is something a person has to read, and trying addresses through this endpoint must not be cheaper than through any other. Twenty is an estimate of more than a pilot who means it sends in a day, and it bounds what one account can put in front of the operator. The count is in the server's memory, like the others, and the development mode has no such limit.

**The development mode** takes reports like a real server and keeps them in its memory until it stops. Nobody reads them there: the operator's commands work on a real server's database.

## What whoever runs the server can do

The person who runs a server reads the reports and can do seven things about what is shared on it, three of them in a group. They do them by hand, with commands on the server's own machine ([deploy/README.md](../deploy/README.md#reports-and-taking-things-down)); the API offers none of them, and the server does none of them by itself. This is what each looks like from an app.

**Take a flight down.** The flight becomes `private` and stays so until the operator releases it.

- To everyone but its pilot it is no flight from that moment: `404` in every answer, on its page, for its file and its preview picture, and it is in no list.
- Its pilot keeps it, with its file, and reads on it since when it is taken down: `taken_down_at`. The flight shows up in [`/v1/flights/changes`](#get-v1flightschangessincelimit) like any change, so the pilot's other devices hear of it.
- The pilot cannot share it again: [`PATCH /v1/flights/{id}`](#patch-v1flightsid) with `unlisted` or `public` is `403` with `flight_taken_down`. Deleting the flight and uploading its file again brings it back taken down.
- The server says **that** the operator did this and since when. It does not say **why**: it does not know. The pilot is not told otherwise either, so an app shows it on the flight.
- Released, the flight is private, without `taken_down_at`, and the pilot's to share again. Nothing is made public by releasing.

This is the demo account's public flight after it was taken down, as its pilot reads it with `GET /v1/flights/{id}` (the `outline` is cut to two points here):

```json
{
  "flight": {
    "id": "5t1eshypdtvrq7rxw39a18vsrg",
    "identity": "0f0d88442e01f52f291d25b2660b8bed",
    "pilot": {
      "handle": "demo",
      "display_name": "Demo Pilot"
    },
    "visibility": "private",
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
    "analysis_version": 1,
    "taken_down_at": "2026-11-18T19:05:00Z"
  }
}
```

**Suspend an account's publishing.** The account can make nothing public until the operator lets it again.

- Its public flights become `unlisted` at that moment, each as a change the pilot's devices hear of: they leave [the pilot's page](#pilots), which is then gone, and [the list of everyone's flights](#public-flights), and whoever has a link to one still opens it. Its public live tracks become `link`.
- The account reads since when: `publishing_suspended_at` in [`GET /v1/account`](#get-v1account) and in the answer to signing in.
- `PATCH /v1/flights/{id}` with `public` is `403` with `publishing_suspended`. Live fixes are taken, and their track is `link` whatever the request asks; see [live tracks](#live-tracks).
- Everything else works as before: signing in, uploading, the logbook, sharing by link, deleting.
- When the suspension is lifted, `publishing_suspended_at` is gone and the account may publish. Nothing becomes public by itself: the flights stay `unlisted` until the pilot changes them, and a live track is public again when the app next says so with a newer fix.

**Remove a live track.** The track and its fixes are deleted and its identifier is taken out of use, exactly as if the pilot had deleted it: `404` for everyone who reads, `410` with `live_track_gone` for the app that sends. An app that follows [the rules for sending](#live-tracks) then goes on under a new identifier, so removing one track takes one link away and does not keep the glider off the map; suspending the account's publishing does that.

**Close a report**, with a note of their own. That changes nothing an app can see.

**Give a group another name.** Every answer that carries the [group](#groups) has the new name from that moment, as after `PATCH /v1/groups/{id}`. Nothing in any answer says that it was not the keeper who renamed it, or when; the keeper can rename it again.

**Remove a plan.** The plan is gone with who was in on it, exactly as if its author or the keeper had deleted it: it is in no list, and a request that names it is `404` with `plan_not_found`. Its author can post it again.

**Remove a group.** The group is gone with its members' places in it, its plans and its invite code, exactly as if its keeper had deleted it: to everyone who was in it, it is `404` with `group_not_found` on every request, the answer for an identifier no group has, and it is in none of their lists. No account is touched.

The three in a group differ from a flight that was taken down in one thing: the server says nothing about them, not even that they happened. A flight that was taken down stays its pilot's, and the pilot runs into a refusal that has to say what stands in the way. A plan or a group that is removed leaves nothing behind that could say anything, the server keeps no record of who was in a group that is gone, and a group tells its members of nothing that happens in it, whoever did it: a client asks. An app that finds a group renamed or gone cannot tell who did it, and should not guess.

None of the seven deletes a flight or an account, and none tells anyone anything beyond what is said here.

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

What is left to build: a map under the line, and one that moves without the page being loaded again. Both need a script and map tiles, and belong to the web app. The web app shows live tracks on its map since 2026-11-20, but it has no address for one track, so the page does not link to it yet.

## Map imagery

Photographs from above that the server passes on to the apps' maps, tile by tile, where a provider's tiles cannot be drawn as they are. There are two sources, both cut along a border so that a map can lay them over other photographs ([architecture.md](architecture.md#the-aerial-view-through-the-server)). Anyone may read them, without an account. The credit goes with the layer wherever it is shown.

| Source | What | Zooms passed on | Box (west, south, east, north) | Credit |
|---|---|---|---|---|
| `basemap-at` | Austria's orthophoto from [basemap.at](https://basemap.at), whose tiles are white beyond Austria's border | 0 to 19 | the provider's: 8.782379, 46.358770, 17.5, 49.037872; Austria's outline lies in 9.5307, 46.3723, 17.1608, 49.0205 | "Datenquelle: basemap.at", with basemap.at as a link to https://basemap.at |
| `ign-ortho` | France's orthophotos from IGN's Géoplateforme (BD ORTHO, the layer "Photographies aériennes"), which reach beyond France's border and are white further out. Metropolitan France with Corsica; not the overseas departments | 13 to 19 | France's outline: -5.1413, 41.3335, 9.56, 51.0892 | "IGN – BD ORTHO®", as a link to https://cartes.gouv.fr/rechercher-une-donnee/dataset/IGNF_BD-ORTHO (ours; IGN words none) |

A map's raster source takes `/v1/imagery/{source}/{z}/{x}/{y}` as its tile address, the box as its `bounds` and 19 as its `maxzoom`; for `ign-ortho` it also takes 13 as its `minzoom`, so that a map asks for no coarser tile. Below zoom 13 IGN's layer shows more than its own photographs of France, so the server passes nothing of it on there and answers `204` ([data-sources.md](data-sources.md#photographs-for-the-rest-of-the-alps)). There is no list of the sources in the API: they are the two above, and an address with any other is `404`.

### `GET /v1/imagery/{source}/{z}/{x}/{y}`

```bash
cargo run -p venti-server -- dev
curl -o innsbruck.jpg http://127.0.0.1:8080/v1/imagery/basemap-at/15/17420/11489
curl -o brenner.png http://127.0.0.1:8080/v1/imagery/basemap-at/15/17431/11524
curl -o annecy.jpg http://127.0.0.1:8080/v1/imagery/ign-ortho/15/16941/11670
curl -o veyrier.png http://127.0.0.1:8080/v1/imagery/ign-ortho/15/16946/11636
curl -i http://127.0.0.1:8080/v1/imagery/ign-ortho/15/17018/11685    # Courmayeur: 204
```

One tile of the Web Mercator scheme every web map uses: zoom `z`, column `x` from the west, row `y` from the north, in the order MapLibre writes `{z}/{x}/{y}`. The fourth of these, the tile at Veyrier below the Salève, is `crates/sources-ign/tests/golden/border_veyrier_15_16946_11636.png`: France's side of it, and Switzerland's transparent.

| Answer | When |
|---|---|
| `200`, `image/jpeg` | The tile lies wholly inside the country: the provider's JPEG as it came |
| `200`, `image/png` | The border runs through the tile or within 50 m of it: the provider's tile cut, with what lies beyond the border transparent |
| `204 No Content` | Nothing here: the tile lies beyond the border, outside the source's box, at a zoom that is not passed on, or was cut to nothing; or the provider has no such tile |
| `502` `unavailable` | The provider failed, or sent what is not a tile |
| `504` `timeout` | The provider did not answer within eight seconds |
| `404` `not_found` | Not the address of a tile: a source the server does not have, or anything but three plain numbers that name a tile (zoom at most 24, column and row below 2^zoom). Also every address, on a server that passes no imagery on |

`200` and `204` carry `Cache-Control: public, max-age=604800`, a week; `502` and `504` carry `public, max-age=60`. A tile is asked of the provider once and then kept on the server's disk. A tile beyond the border is never asked of the provider, and nothing a request says reaches the provider but the three numbers of the tile: the host, the layer and the format are the adapter's. France's border is the land's: at the sea and on Lake Geneva the photographs end within 50 m of the shore. The development mode answers from six tiles of basemap.at recorded on 2026-11-20 and seven of IGN recorded on 2026-11-22 (the five above are among them; `crates/sources-basemapat/tests/fixtures/NOTICE.txt` and `crates/sources-ign/tests/fixtures/NOTICE.txt` list them) and with `204` for every other, unless it is started with `--online-imagery`.

## What is not here yet

Recovering a forgotten password, passkeys, changing a handle or a display name, flights visible to followers only, a push message for what a pilot is [told among pilots](#get-v1accountnotificationssincelimit), blocking a pilot, a name for the place a flight took off from, forecast maps (of the thermal map the server's side is here since 2026-11-21, and no app draws it yet), a second radar source for what DWD's radars do not see (of the rain radar the server's side is here since 2026-11-21, with DWD's composite, and no app draws it yet), pages about a site with its rules and webcams and which airspace is active today are planned and not built; see [plan.md](plan.md). Of live tracks the server's side is here; since 2026-11-20 the iPhone and the Android app send them, and those two and the web app draw the public ones on their maps. Not built for them: a track only followers see, a track shown with a delay, a list of an account's own tracks, the positions of gliders that do not fly with Venti (the Open Glider Network and the trackers pilots carry; [data-sources.md](data-sources.md#live-tracking)). Of reports the server's side is here, and since 2026-11-20 the iPhone, the Android and the web app send them about a flight, a pilot's page and a live track. A group and a plan can be reported since 2026-11-21, on the server and in the apps' shared core; no app offers it yet. Not built for them: an answer to the reporter, a way for a pilot to object to what the operator did, one pilot hiding another's flights from their own view, and anything the server does about a report by itself. Of groups and plans the server's side is here, and since 2026-11-20 the iPhone, the Android and the web app show them. Not built for them: a link that carries an invite code, finding a group by its name, a group that pilots nearby can see, chat, events, a plan posted to several groups at once, members seeing each other's flights or live tracks through a group, and any notice that something happened in a group: a client asks. The connector for a pilot's own assistant is built and is not part of this API: it is the endpoint `/mcp` beside it and has a document of its own, [mcp.md](mcp.md). Of map regions the server's side is here since 2026-11-20. Not built for them: two versions of a region side by side, so that a download of the older one could end while the newer is handed out; a way to fetch only what changed between two versions; and the label fonts, which the apps bundle. Station metadata is planned to move into downloadable region bundles ([architecture.md](architecture.md#api-and-offline-sync)); the station list will keep working when it does.
