# Adding a data source

For somebody who knows a useful source of data in their country and has never seen this project. It says what a source needs before any code, goes step by step through the commonest case, a network of wind and weather stations, says what differs for the other kinds of data, how a source leaves again, and what cannot be plugged in yet.

Venti is one free, open-source, non-commercial paragliding app with one server. A source is brought in by a pull request to this repository; it then serves every pilot in the region it covers. The design behind this page is in [data-sources.md](data-sources.md) (the framework and the catalogue of sources) and [architecture.md](architecture.md) (the crates and the server).

## How a source fits in

- A source is one **adapter**: a small Rust crate, `crates/sources-<name>`, that knows one provider. It opens no connection and reads no clock. It names the addresses it wants, and it turns the bytes that come back into Venti's own records. The server's pipeline does the fetching, the limits, the archive and the validation, the same for every source.
- Every adapter has a **manifest**, `manifest.toml`: who the provider is, the licence, the credit, how often and how much may be asked.
- Every adapter is tested from **recorded answers** of its provider, kept in the crate, against **golden files** of what it makes of them. No test asks a provider.
- Which sources a server collects is **configuration**: a region pack, `regions/<id>/region.toml`, names them. The build contains every adapter; a pack switches one on.
- A new network of stations needs **no change in the apps**: they show the stations the server hands out, each with its source's credit from the manifest. That is not so for every kind of data; the last section says where.

## Before any code

**A stated licence, read on the provider's own page.** Not a summary, not somebody's library that uses the data, not memory. The page, its address and the day it was read go into the manifest. Three outcomes:

- **The page states a licence or terms that allow what Venti does** (fetch, store, show in the apps, pass on through an open API, with a credit): the source can be used, and nobody has to be asked first. A licence that allows non-commercial use only is enough (owner decision 17 in [plan.md](plan.md#owner-decisions)): Venti earns nothing. The manifest then says `commercial = false`, and whoever runs Venti's code commercially may not use that source.
- **The page states no licence or terms, or terms that do not cover the use**: the catalogue calls this **Ask**. No adapter is written until the provider has said yes in writing to the project's owner. Open an issue that names the source and what its pages say; do not write to a provider in the project's name.
- **The terms forbid passing the data on**: not usable. Passing on is all the server does with data.

Three more things to read on the same pages, because the manifest wants them: the credit the provider asks for, word for word; any limit on how often or how much may be asked; and what else the terms require (a link, no suggestion of endorsement, a time after which data may not be passed on).

**For a network of stations, one more rule.** One network Venti uses, Pioupiou, is free only while every other station source in the application may be reused commercially by anybody. A station source that is non-commercial only, or used by permission, cannot stand beside it: read [Pioupiou's condition](data-sources.md#pioupious-condition) first.

**Data of different licences is never mixed into one record.** Each source stays a layer of its own with its own credit. You do not have to do anything for this; it is why an adapter never "improves" another source's records.

## A network of stations, step by step

You need Rust (the version in `rust-toolchain.toml`; `rustup` installs it by itself) and nothing else. The commands run in the repository's root.

### 1. The skeleton

```bash
tools/new-source/new.sh weirdland
```

writes `crates/sources-weirdland`: `Cargo.toml`, `manifest.toml`, `src/lib.rs` (an adapter that reads an imagined answer, to be replaced by yours), `src/fixtures.rs`, `tests/pipeline.rs` and `tests/fixtures/NOTICE.txt`. A second argument sets the identifier region packs will name the source by, if it should differ from the crate's name: `tools/new-source/new.sh weirdland weirdland-10min`. The skeleton compiles; its tests fail and say what is missing, beginning with "record a fixture".

The script prints three lines that join the crate to the build. Add them now:

```toml
# Cargo.toml, under [workspace.dependencies]
venti-sources-weirdland = { path = "crates/sources-weirdland" }

# crates/server/Cargo.toml, under [dependencies]
venti-sources-weirdland.workspace = true
```

```rust
// crates/server/src/adapters.rs, in station_networks()
venti_sources_weirdland::network(),
```

That file is the only one in the workspace that names a provider's crate, and this is the only line a station network adds to it.

**A crate that is there already**, written by hand or before the skeleton existed: the script refuses it ("is there already") and prints nothing, so the lines above are written by hand, with the crate's own name. The line in `adapters.rs` calls a function the crate must have, and an older crate has none. It goes into `src/lib.rs`, beside the adapter's type:

```rust
/// The network as a build takes it in.
pub fn network() -> venti_sources::StationNetwork {
    venti_sources::StationNetwork::new(Weirdland::new())
}
```

with `.with_sample(fixtures::evening_at(), fixtures::evening())` after `new(…)` if the crate has a sample for the development mode (step 7). Such a crate has no test of the contract either: copy `the_adapter_meets_the_contract` from `tools/new-source/template/tests/pipeline.rs` into its `tests/pipeline.rs`, with the answers it recorded. Then go on from the step the crate has not done; for a finished adapter that is step 6.

### 2. The manifest

Fill in every line of `manifest.toml` marked `TO FILL IN`, from the provider's pages. [data-sources.md](data-sources.md#manifest) describes each field. What matters most:

- `id`: lower-case, stable for ever. It is in every record's identifier and in every region pack.
- `short_name`: the name pilots call the network by, at most 14 characters: "MeteoSwiss", "SLF", "ARPA Lombardia". It ends each station's line in a list, where two stations of one name ("Weissfluhjoch") are told apart by it; check it against how the provider calls itself. The manifest is refused without it, and the contract check fails. It never replaces the credit, which goes out in full.
- `[licence]`: the name, whether commercial use and passing on are allowed, the credit in the provider's words, the link, every further condition as a sentence in `constraints`, `verified_on` and `verified_from`.
- `cadence_seconds`: how often the provider has new values. The server asks that often and no more.
- `[rate_limit]`, if the provider publishes one, with its sentence quoted in a comment. The pipeline keeps a run inside it.
- `max_response_kb`: the largest answer you measured, with room. An answer beyond it is refused.

Anything you could not read on a primary source is written as "to verify" in a comment, not guessed.

### 3. Fixtures

Save the provider's real answers into `tests/fixtures/`, as they came:

```bash
curl -sS 'https://the.provider/its/address' -o crates/sources-weirdland/tests/fixtures/stations.json
```

- If an answer is large, cut it down to a handful of stations, and say in `NOTICE.txt` how.
- Copy one answer and damage it by hand (`stations_damaged.json`): a station without a position, a value that is no number, a time that cannot be read. An adapter must lose only what is damaged.
- `NOTICE.txt` says for each file where it is from, when it was recorded (to the minute, UTC), under which licence, and what was cut or changed. Recorded data is the provider's and is in the repository under the provider's licence.
- No key, token or personal data in a fixture or in a recorded address.

### 4. The adapter

Replace the imagined answer in `src/lib.rs` with the provider's form. `crates/sources-slf` (JSON, two requests) and `crates/sources-meteoswiss` (CSV, a list of stations and a file of values) are short, whole examples; `crates/sources-suedtirol` asks one address per kind of sensor.

- `plan` names the first requests of a run, each with a tag of your choosing. `parse` gets the bytes of one answer and its tag, and returns stations, observations, further requests if the provider needs several steps, and what it skipped.
- **Units.** Venti's records are wind in m/s, the direction the wind comes from in degrees, temperature in °C, humidity in percent, pressure in hPa at the station, time in UTC. Write the provider's own words for each value and unit into the crate's first comment, with the page and the date. Each conversion is a function with a unit test that holds one value worked out by hand.
- **Time.** A provider that writes local time without an offset is the commonest mistake: find out which time it is and test it across the change of the clocks.
- **What is not there stays empty.** A height the provider does not give is `None`, never zero: the apps then show no height and compare none. A sensor without a wind still has a station.
- **One bad entry costs that entry.** Push a `Skipped` with what and why, and go on. Return an error only when the whole answer is not what it should be.
- **Stations outside the run's area** (`ctx.area`) are left out.
- No network, no files, no clock, no threads in the crate. `ctx.now` is the time of the run.

### 5. Golden files

```bash
VENTI_UPDATE_GOLDEN=1 cargo test -p venti-sources-weirdland
```

writes `tests/golden/recorded_run.json` and `damaged_run.json`: every station and observation the pipeline made of your fixtures, and the run's report. **Read them** beside the provider's own page for the same hour, station by station: names, positions, heights, a calm station, a windy one, the direction. From then on a test fails whenever the adapter makes anything else of the same bytes. After a change you meant, write them again with the same command and review the diff like any other.

### 6. The check

```bash
tools/new-source/check.sh weirdland
```

One command for "does my adapter meet the contract". It runs the formatter's check and the lints for the crate, the crate's tests, and the server's tests of its list of adapters and of the region packs. Among the tests of a crate that began as the skeleton is the contract every station adapter is held to (`venti_sources::testing::check_station_adapter` and `check_adapter_does_no_io`), run over what the crate recorded; the server's tests run the same contract over the sample of every network that has one, and the search for connections, files and clocks over every adapter crate. A crate with neither that test nor a sample is held by nothing: give it the test (step 1 says where it is). The contract:

- the manifest is valid, allows passing on, and says where its licence was read;
- the adapter asks HTTPS addresses only, and its source uses no connection, file or clock;
- recorded answers in, canonical records out: a healthy run, every station the source's own and inside the area, every observation of one of those stations and not of the future, at least one with a wind;
- nothing in a clean recording is outside the pipeline's limits (0 to 120 m/s, 0 to 360°, −80 to 60 °C, 0 to 100 %, 300 to 1100 hPa): a value outside them is nearly always a unit read wrongly;
- a second run gives the same records;
- each answer cut off in the middle, emptied, or replaced by bytes that are no text is survived without a panic.

The command needs the crate's three lines of step 1: without them it passes, the server has never heard of the adapter, and the command's last line says so. It runs again after step 7, since the server's tests read the packs.

Before a pull request, the whole workspace's checks are the ones in [CLAUDE.md](../CLAUDE.md#commands): `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`. Run them: the check above does not run the server's tests of the API, which is where a network that joined the first region shows (step 7).

### 7. The region pack

A source is collected once a region pack names it. For a new part of the world, write a pack: copy `regions/alps-west/region.toml`, give it an `id`, a `name`, the languages and the box, and list your source:

```toml
[observations]
sources = ["weirdland"]
priority = ["weirdland"]
```

`priority` decides one thing: where two networks have a station at the same place, the station of the network named first is handed out. The same place is within 50 m of height and within 750 m, or within 2.75 km where one of the two is an airfield's report ([data-sources.md](data-sources.md#the-same-station-from-two-sources)). National weather services come first, then other official networks (an avalanche service's), community sensors after them, airfield reports last; among the weather services the order matters only where two of them measure within 750 m of each other, which is rare, so put yours beside its neighbours. A server is told which packs to run by `regions` in its configuration (`crates/server/config.example.toml`).

To add your source to an existing pack, add its name to both lists, at the same place in each, and say in the pack's comment what it brings. A network whose stations lie in two packs' boxes is named in both: it is then collected once, over one box around the packs that name it, and `ctx.area` is that box, so `max_response_kb` and the requests of a run are to be measured for it.

One pack is special: `regions/alps-de-at` is the region the server's development mode runs, over recorded data and without a network, and the three apps' tests are written against it. A network for any other pack needs none of the following. A network named there:

- **needs a sample** for the development mode's clock, 19:05 UTC on 2026-11-18: a few stations inside that pack's box (46.3° to 48.5° N, 9.4° to 17.2° E), returned by `fixtures::evening()` and handed over in `network()` (`src/fixtures.rs` of the skeleton says how; a test fails with a sentence if it is missing).
- **Each station's newest reading in the sample has a wind.** The development mode shows a station's newest reading, and a sample that ends with rows of a thermometer alone puts its stations on the apps' maps without a wind. End the sample at the last time that has one. A test of the server's list of adapters holds this, by station.
- **changes what the server's tests and documents show.** After the pack's lines, write the server's golden files again and read their diff: `VENTI_UPDATE_GOLDEN=1 cargo test -p venti-server`, then the same without the variable. The stations' list (`crates/server/tests/golden/stations.json`) and the health report gain your stations and nothing else should change. `docs/api.md` shows the health report as an example and a test holds it to the golden file: paste the new `health.json` over the old one there, and correct the counts of stations in the paragraph "The development mode serves…" and under "One station for one place". If one of your sample's stations stands at the place of another network's, one of the two is no longer handed out: say which.
- **changes what the apps' tests count**: more stations on the map and in the lists of the development mode. Say so in the pull request, with the names of the stations.

**A network that is in the build and in no pack** is allowed, and costs nothing: nothing fetches it, the health report does not know it, and the server's tests hold both. It is the right state for an adapter that works and whose data is not good enough to show yet (values that arrive hours late, terms that are being asked about). Its catalogue row says "built, listed in no pack" and why.

Try it against the provider, once, by hand:

```bash
cargo run -p venti-server -- ingest regions/<id>/region.toml --source weirdland
```

This fetches for real and stores nothing. It prints how many stations and observations came, what was skipped and what was rejected. Put the numbers into the pull request.

### 8. The catalogue row

Add a row for the source to the catalogue in [data-sources.md](data-sources.md), or correct the row it has: the catalogue lists sources before they are built, in "Catalogue: the rest of the Alps" and in "The adapters to write next", and what was written there before anybody read a whole answer is often wrong in its numbers. The row says what the source provides as the adapter found it, the licence and where it was read, the credit word for word as in the manifest, the status "Built" with the date, the crate and the packs that name it, and anything left "to verify". Three more places name what is built: the table "What is built" in the same document, the list of packages at the top of [architecture.md](architecture.md), and, for a network that joins a pack, a row in the decision log there. The credit itself needs no code: every answer of the API carries the credits of the sources whose records are in it, from the manifests, and the apps show them.

## The other kinds of sources

Each kind has a trait in `crates/sources/src`, a section of the region pack, and an existing adapter to copy. For all of them the steps are the ones above: licence first, a manifest, recorded fixtures, golden files, a line in a pack. What differs is the wiring in `crates/server/src/adapters.rs`, which for these kinds is still by hand, and there is no template yet.

| Kind | Trait and pack section | Copy | Wiring in `adapters.rs` | What to know |
|---|---|---|---|---|
| Airspace | `AirspaceSource`, `[airspace]` | `crates/sources-openaip` | `register_airspace` in `registry_for`; the recorded files in `recorded_payloads` | A run delivers the whole set, which replaces the one before. An adapter never leaves an airspace out silently: what it cannot read is reported. Each source is a layer of its own. Read [airspace.md](airspace.md) first: this is safety-relevant |
| Flying sites | `SiteSource`, `[sites]` | `crates/sources-osm` (`OsmFreeFlying`) | `register_sites`; `recorded_payloads` | A snapshot like airspace. Each source is a layer of its own, never merged with another's sites or with the pilots' own |
| Cables | `CableSource`, `[cables]` | `crates/sources-osm` (`OsmCables`) | `register_cables`; `recorded_payloads` | A snapshot. Answers are large; set `max_response_kb` from a measurement |
| Point forecasts | `ForecastSource`, `[forecasts]` | `crates/sources-openmeteo` | `forecast_sources`, and the service's address in `Providers` and in the server's configuration | Not collected for an area: asked for one place when somebody wants to know. Every series must say which run of which model it is from. The service is one whoever runs the server runs too; see "not pluggable yet" |
| Rain radar | `RadarSource`, `[radar]` | `crates/sources-dwd-radar`, `crates/sources-geosphere-radar` | `radar_sources`, `recorded_radar_payloads`, `radar_recorded_from` | A run delivers frames on the grid the server asks for; the adapter does the projection. Each source is a layer of its own picture |
| Map imagery | `ImagerySource`, no pack section | `crates/sources-basemapat` | `imagery_sources`, `recorded_imagery` | Not collected: the server fetches one tile when a client's map asks, and the adapter says what of it may be shown. See "not pluggable yet" |
| Live positions | `LiveSource`, `[live]` | `crates/sources-ogn` | `live_sources`; made-up lines for the development mode | A connection that stays open, which the server holds; the adapter gives the login and reads the lines. A network's rules on privacy and on how long data may be passed on go into the manifest (`pass_on_hours`) and are applied by the server |

If the provider's address is chosen by whoever runs the server (an instance of a service, not the provider's own host), it becomes a field of `Providers` in `adapters.rs` and a key of the server's configuration; `overpass_url` is the example.

## How a source leaves again

- **Out of a region, or out of a server:** take its name out of the pack's lists (for stations, out of `sources` and `priority`). Nothing else changes: from its next start the server no longer collects the source, and no longer hands out its stations, since the API answers only with records of the sources its packs name. What was stored before stays in the database; whether anything removes it is *to verify* on the first installation. A test holds this for every station network of the build (`a_source_left_out_of_the_packs_is_not_collected_and_nothing_else_changes`). A whole pack leaves by taking its path out of `regions` in the server's configuration.
- **A pack that names a source the build does not have** stops the server at its start, with the file, the region and the source in one sentence. A name spelled wrongly is found the same way.
- **Out of the build:** its line in `station_networks()` (or its lines for another kind), its two lines in the two `Cargo.toml`, the crate's folder, its name in the packs, and its catalogue row marked as removed with the date and the reason. If it was in `regions/alps-de-at`, the development mode loses its stations: write the golden files of `crates/server` again (`VENTI_UPDATE_GOLDEN=1 cargo test -p venti-server`) and tell whoever keeps the apps' tests.
- **When a provider changes its terms:** the manifest's `verified_on` is the day somebody last read them. A source whose terms no longer allow the use leaves the packs the same day; the adapter may stay in the build.

## What is not pluggable yet

Each with what would make it so. None of these is built; they are for the owner to decide.

- **A source needs a build.** An adapter is Rust compiled into the server; nobody adds one to a running server, and an operator cannot load one from a file. *It would take* a declarative adapter for the simple case (a JSON or CSV answer described by a mapping file of field names and units), read at start from a folder.
- **Only station networks have the one-line wiring, the template and the contract check.** The other kinds are wired by hand in `adapters.rs`, with lists and clocks of their own per provider, and the development mode names two provider crates directly (`dev.rs`, for the cables' recorded box). *It would take* the same step for each kind: one function per crate that hands over the adapter and its recording, and a contract check per kind.
- **A map layer of photographs reaches a pilot with an app release.** Aerial imagery is a source and a layer in the shared map style, `map/style.json`, which the three apps bundle, and an app may name a layer in its code as well (the iPhone's settings do, for a credit's wording). So a new provider's photographs are a change to the style and a release of three apps, not a line in a pack. Stations and radar pictures are not like this: the apps name no station network and no radar source (searched on 2026-11-22) and show what the server hands out. *It would take* an endpoint that lists a server's imagery layers (name, credit, area, tile address) and apps that add what it lists to the map.
- **Map imagery is not in the region packs.** Which imagery a server passes on is the list `imagery_sources()` in code. *It would take* an `[imagery]` section in the pack, read like the others.
- **A forecast service is one named key.** The server's configuration knows `open_meteo_url`, and the forecast screens are built on the model ICON-D2 by name. *It would take* a list of forecast services in the configuration, each with its adapter's identifier, and views that take the models a series names rather than ones they know.
- **A new kind of data needs the core and the apps.** Webcams, lightning, protected zones, transit: each wants a canonical record in `venti-domain`, a trait and a run in `venti-sources`, a table and an endpoint in the server, words in the core, and a place on a screen in three apps. *Nothing short of that work* makes a kind; what can be kept small is its first provider, by writing the trait before the adapter.
- **The same station from two sources is one fixed rule.** The distances and the height that make two stations one place are constants: 2.75 km where one is an airfield's report, measured on airfields, and 750 m between two weather networks, an estimate from two pairs. The source of airfield reports is named in the server's code by its identifier, so a second one would be judged as a weather network until it is named there too. A region cannot set its own bounds, a pair cannot be corrected by hand, and the order of the priority is the first pack's for every region. *It would take* the bounds as fields of the pack, a line in the manifest that says what a source's positions are, and a small table of pairs.
- **A network joining the first region is not one line.** It needs a sample for the development mode's clock, which for most providers has to be put together from another answer since they keep no history; it rewrites two golden files of the server and an example in `docs/api.md` by hand; and it changes what three apps' tests count. *It would take* a development mode that names its own networks apart from the first region's pack, and the document's examples written from the golden files by a command.
- **Licence conditions across sources are not checked by code.** That a non-commercial station source must not stand beside Pioupiou is a sentence in the catalogue and a comment in the packs. *It would take* the plan refusing, at start, a set of packs that names `pioupiou` and an observation source whose manifest says `commercial = false`.
- **A provider that needs a key has no place for it.** No adapter built so far sends one, and the fetcher sends no headers but its own. *It would take* a named secret per source in the server's configuration that the fetcher adds and that never reaches a log, the archive or a fixture.
- **A conditional request.** The fetcher keeps nothing between two runs, so a file that rarely changes (MeteoSwiss's station list) is downloaded whole every time. *It would take* the fetcher remembering the `ETag` and the last body per address.
- **Data leaving Venti** is the documented API, the connector and the dumps ([api.md](api.md), [mcp.md](mcp.md), [data-platform.md](data-platform.md)). They are fixed forms: a consumer cannot register a form of its own or be pushed changes. *It would take* the change events that [data-sources.md](data-sources.md#what-is-built) lists as designed and not built.
