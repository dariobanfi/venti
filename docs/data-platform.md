# The server as a data platform

Last revised: 2026-11-21

What owner decision 14 of [plan.md](plan.md#owner-decisions) asks of the server, measured against the server as it stands on that day: what it holds, every way in and out, what lives on a device and should not, a design for take-offs that pilots make, the pieces that are missing in the order to build them, and what in today's server works against the direction. It is a review of the code and of the documents. Every claim about the code names its file; what was not checked is marked *to verify*. Nothing in here is built unless it says "today" or names the day it was built. The first of the missing pieces, sites that pilots make, was built on the same day, after the review: [what stands of it](#what-stands-of-it-2026-11-21) is said with its files, and the tables below have it.

## The direction

The owner, 2026-11-21:

> Make sure that when building the server, you have to think about it as a data platform first, because we want this server to become the center point of paragliding, and people can use it to create takeoffs, track their flights, any kind of community data. It will become the central hub for it in the future. That's my vision, at least. So it needs to be very open and supporting of that, and allow people to put their data in there and query it.

It stands beside the decisions that bind it: free and open source (1), one central platform on the owner's own server and nothing that bills by use (5), every external source behind an adapter (6), AGPL-3.0-or-later for the code (12), many users (13).

## Principles

A new endpoint or a new table is judged by these. Each is a question with a yes or a no.

1. **A record, not a screen's state.** Does the table hold the thing itself (a take-off, a flight, a plan), so that a screen nobody has thought of yet could be built from it? An answer may be shaped for a screen only beside an answer that is not.
2. **Author, times, origin.** Does every record a pilot made say who made it, when it was made and when it last changed, and through what it came in (which app or token)? Does a record that others may correct keep every earlier state and who made each change?
3. **A licence on everything handed out.** Does every answer and every file say under which licence its records are given, per source, as `attributions` does today for the data of others (`crates/api-types/src/v1/attribution.rs`)? The community's own data is a source like any other, with a licence of its own.
4. **Others' data and the community's stay apart.** Is the community's data in tables, layers and files of its own, so that it can be handed out whole without a record of another licence family in it? ([data-sources.md](data-sources.md#licence-rules-built-into-the-pipeline))
5. **Who may read is written down, in one place per kind.** Is the visibility of a record a column with named values, held by the database, and the same rule for the API, the connector, a share page and a dump? Today's model for it is `flights.visibility` with its constraint (`crates/store/migrations/0002_accounts_sessions_flights.sql`, `0009_reports_and_takedowns.sql`).
6. **Asked for as a program asks.** Can what is open be asked for by identifier, by place, by time and by pilot, page after page with an opaque cursor, and "what changed since", deletions included? Today's model is `GET /v1/flights/changes` ([api.md](api.md#get-v1flightschangessincelimit)).
7. **Documented is enough.** Can a program that is not Venti's app, written in any language, use it from the documents alone, without the Rust core? Nothing is readable only by `crates/core`.
8. **What a pilot may write, a program may write for them with their leave.** Does the write take a token that the pilot gave to another maker's app, limited to what the pilot allowed and withdrawn by the pilot at any time? It never needs the pilot's password.
9. **An identifier stays.** Is the record's identifier never used for another record, and its address the same for good? A record that is gone answers that it is gone.
10. **On the server unless it is the pilot's private matter.** If it lives on the device alone: is it private, and can the pilot move it to their account when they choose?
11. **Theirs to take and to remove.** Can a pilot get everything they gave in formats other programs read, and remove it? Where a contribution was given under an open licence and stays, do the terms say so before it is given?
12. **Open does not mean unbounded.** Is every read and write bounded per token or per address, and is the bound in the documents?

## What the server holds today

### Data of others, through adapters

One table family per kind in `crates/store/migrations`; the licence is each source's `manifest.toml`, verified as [data-sources.md](data-sources.md) records.

| Kind | Tables | Sources and their licence | How it is kept |
|---|---|---|---|
| Wind stations and observations | `stations`, `observations` (by month), `source_runs` (`0001`) | DWD and GeoSphere Austria: CC BY 4.0. South Tyrol: CC0 1.0. Airfield reports through the AWC: public domain | A row per station and per reading, each with its source; thirteen months |
| Airspace | `airspace_sets`, `airspaces` (`0003`, `0004`) | openAIP: CC BY-NC 4.0 with a share-alike condition | The latest whole set of each source |
| Flying sites | `site_sets`, `sites` (`0005`) | OpenStreetMap: ODbL 1.0, share-alike | The latest whole set of each source; a site is its record as JSON text with a position (`crates/store/src/site.rs`) |
| Forecasts | `forecast_models` (`0006`); series are fetched on demand and kept in memory | Open-Meteo with DWD's ICON-D2 and ICON-EU: CC BY 4.0 | Not stored beyond half an hour |
| The thermal map | `thermal_map_parts` (`0011`) | Venti's own estimate from the forecast above | Remade with every model run |
| Rain radar | In memory (`crates/server/src/radar.rs`) | DWD: CC BY 4.0 | The newest frames |
| Aerial photographs of Austria | Tiles on disk (`crates/server/src/imagery.rs`) | basemap.at: CC BY 4.0 | Passed on, cut at the border |
| Map regions | Files in a folder (`crates/server/src/maps.rs`) | OpenStreetMap and the terrain's sources | Handed out as files |

Three licence families are in the store: attribution only (CC BY, CC0, public domain), share-alike (ODbL), and non-commercial (openAIP). They never share a table row, a layer or an answer's record.

### What pilots make

| Kind | Tables | Kept with each record | Not kept |
|---|---|---|---|
| Account | `accounts`, `credentials`, `sessions`, `invites` (`0002`) | Handle, display name, `created_at`, `publishing_suspended_at`; a session's device name, `created_at`, `last_used_at` | Nothing else about the person, on purpose. No terms the pilot agreed to and no version of them; no identifier but the handle that can be handed out |
| Flight | `flights`, `flight_deletions` (`0002`, `0008`, `0009`) | The owner (`account_id`), the file byte for byte, `identity`, `visibility`, `uploaded_at`, a change number per account, everything worked out from the file with `analysis_version`, `taken_down_at` | When the visibility changed and so when it became public; through what it came in; a licence or terms for those who read a public flight; anything the pilot wrote (a title, a note, a named take-off); a change number across accounts, so nobody can follow the public flights |
| Live track | `live_tracks`, `live_fixes`, `live_retired` (`0007`) | The owner, `visibility` (`link`, `public`), `started_at`, each fix with its time and sequence | Removed 48 hours after the newest fix, on purpose. No list of an account's own tracks ([api.md](api.md#what-is-not-here-yet)) |
| Group, membership | `groups`, `group_members` (`0010`) | Name, `created_at`, keeper, `joined_at` | Who made the group, once the keeping has passed on |
| Plan for a flying day | `group_plans`, `group_plan_pilots` (`0010`) | Author, `created_at`, `changed_at`, day, place as text, position, who is in and since when | Removed two days after its day, on purpose; private to the group |
| Report | `reports` (`0009`, `0012`) | Reporter, the account it is about, kind, subject, reason, text, `created_at`, closing note | Read by the operator only |
| Flying site a pilot made, in the store since 2026-11-21 | `pilot_sites`, `pilot_site_revisions`, `pilot_site_change` (`0013`) | The site as it stands: the canonical record as JSON text with its position, the number of the revision it stands at, `created_at` and `created_by`, `changed_at` and `changed_by`, `removed_at`, `taken_down`, and the change number of the layer that its latest revision took. Every revision: its number, author, time, through what it came in (`via`: `session` or `operator`), its kind (`created`, `changed`, `restored` with the number it restored, `removed`), a line saying why, and the whole record as it stood from then on | Which app or token a revision came through: `via` says `session` for every pilot's write until tokens exist (piece 3). Photos. The OpenStreetMap element a site is the same place as. Which version of which terms the pilot agreed to (piece 13). No row of the three tables is ever deleted; an account that is deleted leaves its sites and revisions there without an author (`ON DELETE SET NULL`) |

There is no pilot-made waypoint, route, photo, comment or note on the server. `0010` is the model of a pilot's record done as principle 2 asks: author and both times in the table, held by the database; `0013` is the first that also keeps every earlier state.

## Every way in and out today

### Reading

| What | Who may ask | Asked by | Pages | Usable from the documents alone |
|---|---|---|---|---|
| `GET /v1/stations` | Anyone | Box | No; the whole box | Yes. Hands out one of two stations that stand at the same place (`crates/server/src/api/same_place.rs`); the other can be read by its identifier only if it is known |
| `GET /v1/stations/{id}/observations` | Anyone | Identifier and a span of seven days at most | By the span | Yes |
| `GET /v1/airspace` | Anyone | Box | No; `box_too_large` beyond a bound | Yes |
| `GET /v1/sites` | Anyone | Box | No; `box_too_large` beyond a bound | Yes. One layer per source. Since 2026-11-21: the sites pilots made as the layer `venti`, in the answer when it has a site in the box, which also says who made each site and when it last changed (`standing` in `SiteLayer`, `crates/api-types/src/v1/sites.rs`) |
| `GET /v1/sites/{id}` (since 2026-11-21) | Anyone | Identifier, of either layer | No | Yes. `lasting` in the answer says what the address promises. A pilot-made site's address is for good, and answers `site_gone` once the site is removed. An OpenStreetMap site's identifier is the source's: the answer is the record of the set the server holds now, and `site_not_found` once a run no longer delivers it |
| `GET /v1/sites/{id}/history` (since 2026-11-21) | Anyone, for a pilot-made site that is not taken down | Identifier; revisions after a number | By revision number | Yes |
| `GET /v1/sites/changes` (since 2026-11-21) | Anyone | Since a cursor | Cursor | Yes. The layer `venti` only: sites that are new, changed or back, and those that are gone |
| `GET /v1/forecast` | Anyone; new places are counted per address | Place | No | Yes |
| `GET /v1/thermals`, `/v1/thermals/hours`, `GET /v1/radar` and its frames | Anyone | The whole area; a day and a layer | No | Yes; a judged map and a picture, shaped for the apps' layers |
| `GET /v1/maps` and files, `GET /v1/imagery/…` | Anyone | Region; tile | Byte ranges | Yes |
| `GET /v1/flights/public` | Anyone | Box of the take-off, span of time, order | Cursor, 100 a page | Yes. Not by pilot (that is the next row), not by what changed since |
| `GET /v1/pilots/{handle}` | Anyone, once the pilot has a public flight | Pilot | Cursor | Yes |
| `GET /v1/ranking` | Anyone | Box, span of time | `limit` only | Yes; a rule (best three) is in the answer's making (`RANKING_COUNTED`) |
| `GET /v1/flights/{id}`, `/igc` | Anyone for a public or unlisted flight; the pilot for a private one | Identifier | No | Yes. The track as data is the IGC file only; no GeoJSON |
| `GET /v1/flights`, `/v1/flights/changes` | The pilot, with a session token | The account; since a cursor | Cursor | Yes |
| `GET /v1/live`, `/v1/live/{id}` | Anyone for public tracks; whoever has the identifier for a track by link | Box; identifier, since a sequence | `limit` up to 5,000 fixes | Yes |
| `GET /v1/groups…`, plans, the board | Members, with a session token | Group; span of days | No | Yes |
| `GET /v1/account`, `/v1/sessions` | The pilot | | No | Yes |
| The connector, `/mcp` | Anyone, no token looked at | Three tools: `wind_stations`, `station_history`, `airspace_at` ([mcp.md](mcp.md#the-tools)) | | Yes. Sites and public flights are tools of the app's own assistant (`crates/tools/src/sites.rs`, `flights.rs`) and not of the connector |
| `/f/{id}`, `/f/{id}/preview.png`, `/live/{id}` | Anyone with the address | Identifier | | HTML for people; no data in them that the API lacks |

From a browser, a page of another maker can read none of this, public or not: the server answers cross-origin requests only for the origins its configuration lists (`cors_origins`; `Cors::Origins` in `crates/server/src/api/mod.rs`; every origin only in the development mode, `crates/server/src/dev.rs`).

The description of all of it is prose, [api.md](api.md), checked against recorded answers in `crates/server/tests/golden`, and the Rust crate `venti-api-types`. There is no description a program can read (no OpenAPI, no JSON Schema; searched for in `crates/`, `docs/` and `Cargo.toml` on 2026-11-21).

### Writing

| What | Who may write | With what | What the server checks |
|---|---|---|---|
| `POST /v1/accounts` | Whoever has an invite code (the default setting), anyone, or nobody | An invite | Handle, name, password; 10 attempts an hour per address |
| `POST /v1/sessions` | The pilot | Handle and password | Counted per handle and per address |
| `POST /v1/flights`, `PATCH`, `DELETE` | The pilot | Session token | The file is analysed by the server itself; 4 MB; 1,000 uploads a day and two at a time per account; room per account; taken down and suspended states |
| `POST /v1/live/positions`, `DELETE /v1/live/{id}` | The pilot | Session token | Times, order, values, bounds per track and account |
| Groups, plans, "I am in" | Members, keeper, author as [api.md](api.md#groups) says | Session token | Lengths, days, counts per account and group |
| `POST /v1/reports` | Any signed-in pilot | Session token | Reasons, length, counted per account |
| `POST /v1/sites`, `PUT /v1/sites/{id}`, `DELETE /v1/sites/{id}`, `POST /v1/sites/{id}/revisions` (since 2026-11-21) | Any signed-in pilot whose publishing is not suspended, on a server that was told to take sites from pilots (`sites_from_pilots`; off unless set, on in the development mode) | Session token | What every site record is checked for ([below](#what-stands-of-it-2026-11-21)); a name and a use; the place inside a region the server serves; 20 m to every other standing site of the layer; the revision a change was made from; 20 new sites and 100 revisions a day per account |
| `PUT /v1/account/password`, `DELETE /v1/account` | The pilot | Session token and the password | |
| The operator's commands: `ingest`, `invite`, `moderate` with `reports`, `close-report`, `take-down`, `release`, `remove-live-track`, `suspend-publishing`, `resume-publishing`, `group`, `rename-group`, `remove-plan`, `remove-group`, and since 2026-11-21 `site`, `revert-site`, `remove-site` (`crates/server/src/cli.rs`, `crates/server/src/moderation.rs`) | Whoever can run the program on the server's machine | The database | |

**One kind of credential.** Every write, and every read of what is a pilot's own, takes the session token of the app's own sign-in and nothing else (`crates/server/src/api/auth.rs`: `Signed` is an account and a session, with no scope). A session is got with the password. So today an app of another maker can act for a pilot only by asking for the pilot's password, and then it can do everything the pilot's own app can, short of the two requests that ask for the password again. The connector takes no token at all.

## What lives only on a device

| What | Where | Private, or the platform's |
|---|---|---|
| The pilot's kept take-offs | `crates/core/src/kept_sites.rs`: copies of the server's site records under `sites-kept/`, one file per source. "Nothing of this goes to the server" | **Which** sites a pilot keeps is private; it belongs on the account, so that it follows the pilot to their other devices, and stays private there. It is no contribution: the records are OpenStreetMap's. A take-off that is in no source is what the next section is about |
| Waypoints | `crates/core/src/waypoints.rs`: one SeeYou CUP file | Private by default, on the account when the pilot chooses. A named waypoint at a take-off or a landing is the raw material of a pilot-made site: offering "make this a take-off on Venti" is the bridge, and it is the pilot's act, never the server's |
| The task or route | `crates/core/src/task.rs` (one XCTSK file at a time), `crates/core/src/route_plan.rs` | Private by default. Shared routes ("common cross-country lines" in [plan.md](plan.md#today-and-plan)) are community data and want a record of their own on the server; not designed here |
| Airspace files the pilot brought | `crates/core/src/airspace.rs`: OpenAir files as they came | Private, and not the pilot's to publish: the files are somebody else's work under terms the server cannot know. They stay on the device |
| Airspace kept from the server | `crates/core/src/kept_airspace.rs` | A cache of the server's answer. Nothing to move |
| "How you fly" and the statistics | `crates/core/src/tendencies.rs`, `statistics.rs`, with each flight's figures in the logbook's index | Private. Worked out from the files, which the server has; the server could work out the same for a pilot, and the comparison "beside the pilots around" that [plan.md](plan.md#review) asks for can only be made there, from public flights |
| The logbook of a pilot without an account | `crates/core/src/logbook.rs` | Private until the pilot signs in; then the files are on the server (`crates/core/src/account.rs`) |
| A league's upload password, the assistant's key | `crates/core/src/leagues.rs`; the shells' secret stores | Private, and never the server's |
| Live sharing's choice, offline map regions | `crates/core/src/live.rs`, `maps.rs` | Settings and caches |

## Take-offs and landings made by pilots

Until 2026-11-21 every flying site was OpenStreetMap's, read-only, and thin: 252 take-offs in the first region, 93 of them with wind directions, and well-known mountains missing ([data-sources.md](data-sources.md#flying-sites)). The catalogue there had the row this section fills: "Our own community edits". The subsections after the first are the design as the review wrote it, brought up to date where the piece decided something; the first says what stands.

### What stands of it (2026-11-21)

Piece 1 of [the build order](#what-is-missing-in-build-order), on the server. The core reads the layer for the apps, which cannot yet write to it.

**What was run.** All of it is in the code and its tests ran on 2026-11-21 against the in-memory store: the store's suite (`crates/store/tests/suite/pilot_sites.rs`), the API through the real router with golden answers that [api.md](api.md#sites-that-pilots-make) shows (`crates/server/tests/sites.rs`), and the operator's commands (`crates/server/tests/moderation.rs`). The migration and the queries for PostgreSQL were compiled and not run against a database: *to verify* on the first installation.

- **The record and its history.** A site a pilot made has an identifier of the server's own, `venti:` and 26 characters, that is never used for another site; no row is ever deleted (`crates/store/migrations/0013_pilot_sites.sql`). Every change is a revision that is kept, with the whole record as it stood from then on, its author, its time, through what it came in and a line of up to 200 characters saying why (`crates/store/src/pilot_site.rs`). Both stores decide what a revision comes to in one function, `decide`, in that file.
- **Undo and removal.** Putting a site back to an earlier revision is a new revision that names it: `POST /v1/sites/{id}/revisions` with `{"restore": n}`. Removing is a revision too. The store keeps the two ways a site can be gone apart (`removed_at`, `taken_down`), and holds a site that is taken down against every revision but the operator's. In the API (`crates/server/src/api/sites.rs`): a site a pilot removed answers `410` `site_gone` at its address, keeps a readable history, and comes back when any signed-in pilot restores a revision; a site that whoever runs the server took down (`remove-site`) answers the same at its address, its history is closed as well, and only the operator brings it back (`revert-site`).
- **Reading.** The layer `venti` in `GET /v1/sites`; one site of either layer by its address; a pilot-made site's history; what changed in the layer since a cursor, removals included. Every answer carries the layer's credit and licence in `attributions`: source `venti`, provider "Venti contributors", licence "CC0 1.0".
- **Writing.** Adding, correcting, removing and restoring, with a session token. Who may write is one function, `may_write` in `crates/server/src/pilot_sites.rs`: any signed-in pilot whose publishing is not suspended. A change names the revision it was made from, and is refused with `409` `site_changed` when somebody else changed the site meanwhile.
- **What the server checks.** What every site record passes, the same code as for OpenStreetMap's (`check_site` in `crates/sources/src/sites.rs`): a name of up to 200 characters, a description of up to 1,000, the words about a fee and about a time without flying up to 100 each, a web address of plain `http` or `https` form up to 500, a height from −500 to 9,000 m, at most 16 wind sectors within 0° to 360°, and nothing that does not print. Beyond that: a name and at least one use; the place inside the bounds of a region the server serves; no standing site of the layer within 20 m (`409` `site_too_close`, which names the other one); 20 new sites and 100 revisions a day per account, counted in the process's memory as the other limits are.
- **The licence in one place.** `LICENCE` in `crates/server/src/pilot_sites.rs`: CC0 1.0, [the owner's decision](#the-licence-cc0-10-decided-by-the-owner). The source `venti` has no `manifest.toml`: it is no adapter and fetches nothing, so the constant stands where a manifest's licence would.
- **A setting that lets pilots write.** `sites_from_pilots` (`VENTI_SITES_FROM_PILOTS`), off unless set and on in the development mode. With it off every write answers `403` `sites_closed`; reading is always on. The reason is [the terms for contributors](#what-the-terms-for-contributors-must-say), which do not exist yet: before a public server takes a site from a pilot, the pilot has to have been told that it is given under CC0 1.0 and stays when the account goes.
- **The operator's commands.** `site` (the record with its history), `revert-site` (restores a revision, also of a site that is taken down) and `remove-site` (takes a site down): [deploy/README.md](../deploy/README.md).
- **The development mode.** It serves four made-up sites of its three pilots, with their histories, one of them removed (`DEMO_SITES` in `crates/server/src/dev.rs`).
- **The core reads the layer** through the calls the apps already make (`crates/core/src/sites.rs`, `kept_sites.rs`), so that the three apps show the community's sites beside OpenStreetMap's without a change of their own; no app was built and looked at with it on 2026-11-21, *to verify*. Adding and correcting from the apps is piece 2.

Not built in this piece:

- **Reports about a site.** A report names two accounts that both exist (`reports.about_account_id` is `NOT NULL` and goes with its account, `0009`), and a site outlives its author. A kind `site` among the reports needs that rule changed first, in a migration, and reasons that fit a place in the core.
- **Hiding one revision.** A revision whose text should not be read stays in the history of a standing site; the operator can only take the whole site down.
- **The link to an OpenStreetMap element** that is the same place, and a rule for which of two layers a map draws there.
- **Photos**; tokens with scopes (piece 3); a dump of the layer (piece 10); the layer in a pilot's archive (piece 6).
- **An account's deletion takes no change number**: a mirror that follows `GET /v1/sites/changes` does not learn from it that a site lost its author's name.

### The record

The same canonical record the apps already draw, filter and keep, `venti_domain::Site` (`crates/domain/src/site.rs`), so that a pilot-made site needs no second code path in the core's filter (`site_filter`), Today (`kept_sites`) or the tools:

- name; position; height above sea level; uses (take-off, landing, winch, training); wind directions as sectors; whether for paragliders and for hang gliders; a web page; a description; whether a fee is asked; whether guests have rules to read; a time without flying; an outline for a landing field.
- Not taken from pilots as a plain field: `official`. Who says a site is approved is a question of authority (a club, the DHV), not of one pilot's knowledge; until there is an answer the field is left out of pilot-made records. *For the owner.* As built, the body a pilot sends (`SiteDraft` in `crates/api-types/src/v1/sites.rs`) has neither `official` nor `discouraged`, nor the record's `url`, which for a source's site is the address of the source's own record.
- Photos come later, as records of their own that point at a site, each with its author and its licence. They need a place for files, which [architecture.md](architecture.md#storage-1) plans ("media under content-hash paths") and nothing builds yet.

Beside the record, what principle 2 asks: an identifier of the server's own making that is never reused (`venti:` and 26 characters, as flights and groups have); `created_at` and the author; `changed_at` and who changed it last; a revision number; the licence, said once for the layer.

### Who may add, who may correct, history and undo

Proposed, as OpenStreetMap does it and for the same reason, that a site outlives its first author:

- **Any signed-in pilot may add** a site, and **any signed-in pilot may correct** one. *For the owner* (the other way: only the author and the operator change a record, others propose). Built as proposed, with the rule in one function (`may_write`, `crates/server/src/pilot_sites.rs`).
- **Every change is a revision that is kept:** a table of revisions (site, number, author, time, what came in through, the whole record as it then stood, a line saying why) beside the table of sites as they stand. Nothing is overwritten.
- **Undo is a new revision** that restores an earlier state and names it. **Removing is a revision too:** the site is marked as gone and stays answerable by its identifier as gone, so that those who mirror learn of it (principle 9).
- **Bounds per account**, ours and to set: so many new sites and so many changes a day; an account whose publishing is suspended (`accounts.publishing_suspended_at`) writes none. Built with 20 new sites and 100 revisions a day (`Limits` in `crates/server/src/pilot_sites.rs`); both numbers are estimates.
- **An account that is deleted** takes its flights with it today (`ON DELETE CASCADE` on every table of `0002` to `0010`). A site given under an open licence cannot go that way without tearing holes into what others built on it. Proposed: the site and its history stay, and the revisions lose their author. The terms must say so before the first one is given. *For the owner.* Built as proposed (`ON DELETE SET NULL` on the three columns that name an account, `0013`).

### Beside OpenStreetMap's sites, never in one record

The answer of `GET /v1/sites` is already a list of layers, one per source, each with its own credit and licence (`SiteLayer` in `crates/api-types/src/v1/sites.rs`), and the core keeps kept sites in one file per source. Venti's own sites are **one more layer** with a source of its own (`venti` in what follows), never mixed into OpenStreetMap's.

- A pilot who corrects a site that is OpenStreetMap's is sent to OpenStreetMap, as today (every record carries its element's address). Venti does not keep corrections to another source's record.
- Where both layers have the same take-off, both are handed out. A pilot-made site may **name** the OpenStreetMap element it is the same place as (an identifier, no copied field: "Resolution links their records to others but does not copy fields across", [data-sources.md](data-sources.md#licence-rules-built-into-the-pipeline)); not built. Which of two a map draws is the core's rule, as `same_place.rs` is the server's for stations; not built either, so both are drawn. *For the owner:* which wins on the map. *To verify:* that storing another database's identifier beside a record makes nothing a derived database.
- **A pilot-made site must come from the pilot's own knowledge**: their own position, their own flights, what they see on the hill. An app must not offer "copy this OpenStreetMap site into Venti", and the terms must rule out copying from OpenStreetMap, the DHV's database, ParaglidingEarth or a printed guide. Otherwise the layer's licence is a promise Venti cannot keep.
- *To verify before an app lets a pilot set the pin on the map:* whether a point placed by eye over a map drawn from OpenStreetMap, or over an orthophoto under CC BY 4.0, is free of those works' licences. The OpenStreetMap Foundation has guidelines on this (trivial and horizontal layers); they were not read for this review. A position from the phone's own GPS, or from where the pilot's own flights began, has no such question.

### The licence: CC0 1.0, decided by the owner

**Decided on 2026-11-21.** Asked under which licence pilots give their take-offs to Venti, the owner answered: "CC0, public domain". A site a pilot makes on Venti, with everything in its record and the description among it, and every revision of it, is given under CC0 1.0 Universal. The code names it in one place (`LICENCE` in `crates/server/src/pilot_sites.rs`), from which every answer that carries such a record takes its `attributions`.

**What CC0 1.0 says**, from its deed at <https://creativecommons.org/publicdomain/zero/1.0/>, read on 2026-11-21: whoever puts a work under it has dedicated the work to the public domain, by waiving every right they have in it under copyright law anywhere in the world, related and neighbouring rights included, as far as the law allows. Anyone may then copy, change, distribute and perform the work, for commercial ends too, "all without asking permission". The deed adds what it leaves alone: nobody's patent or trademark rights are affected; rights other people may have in the work or in its use, such as publicity or privacy rights, are not affected; whoever gave the work gives no warranty and takes no liability, as far as the law allows; and whoever uses the work must not suggest that its author endorses them. The deed is a summary. The legal code it stands for is at <https://creativecommons.org/publicdomain/zero/1.0/legalcode> and was not read: *to verify* there, before the terms are written, what happens where a country's law does not let a person give up their rights, and that the waiver covers the rights in a database.

What the choice had to serve: handing the community's data out freely (decision 14), keeping it apart from other families (principle 4), and the way back into OpenStreetMap. The three that were weighed, as the review laid them out. Of CC BY 4.0 and ODbL 1.0 no text was read; every statement about those two, and about what the OpenStreetMap Foundation takes, is *to verify*.

| | CC0 1.0, **decided** | CC BY 4.0 | ODbL 1.0 |
|---|---|---|---|
| What a reader must do | Nothing | Name the source | Name the source; pass on a changed or combined database under ODbL |
| Handed out freely | Most: any use, any combination | Yes, with a credit line that every later user must carry | With strings: whoever builds a database on it must open theirs. That keeps a closed app from taking without giving, and keeps some open ones from taking at all |
| The same family as OpenStreetMap's sites | No; stays a layer of its own | No; stays a layer of its own | The same licence. The rule would still keep the two apart as sources, but nothing in the licence would forbid one bundle |
| Into OpenStreetMap | Yes, by anyone, with nothing to ask | Only with a written waiver, by what the OpenStreetMap Foundation has said of CC BY | Not simply: OpenStreetMap takes contributions under its own contributor terms, and data that is merely ODbL from a third party is an import to be agreed, case by case |
| A site's name beside a flight or in a plan | Free. This answers the two questions [plan.md](plan.md#needed-from-the-owner) has open about OpenStreetMap's names ("Public flights" (2), "Groups and plans" (10)) for every site that is in Venti's own layer | Free with the credit | Open again: the same share-alike question as today |
| Credit to pilots | None owed; Venti still shows who made each revision | Owed, to "Venti contributors" or by name | Owed, to the database |

**Why CC0.** It is the only one of the three that lets the data go anywhere, OpenStreetMap included, and it removes the question of a site's name beside a flight. What it costs: anyone, a commercial app too, may take the whole layer and give nothing back, and a pilot is owed no credit outside Venti.

**What the decision leaves.**

- **Who holds the rights** was a second question under the first: whether each pilot licenses their contribution directly to the world, or grants Venti a broad licence and Venti publishes the collection. With CC0 it nearly disappears: the pilot gives up their rights, and there is nothing left for Venti to hold or to change later. The terms still have to say who "Venti" is. *For the owner.*
- **A dedication is for good.** The deed's waiver has no way back, so a site and its revisions stay when their pilot leaves, without the pilot's name. That is built ([above](#what-stands-of-it-2026-11-21)) and has to be said to the pilot before the first site is given. Until the [terms for contributors](#what-the-terms-for-contributors-must-say) say it, a server takes no site from a pilot unless its operator set `sites_from_pilots`.
- **CC0 is the pilot's promise about their own rights and says nothing of anyone else's.** The deed leaves the rights of others untouched: a site copied from OpenStreetMap, from the DHV's database or from a guide is not made free by the pilot's dedication. The rule that a site comes from the pilot's own knowledge ([above](#beside-openstreetmaps-sites-never-in-one-record)) carries the layer's licence, and the terms have to state it.
- **The credit line** that travels with the layer is "Venti contributors". CC0 asks for none; it says where the data is from.
- **Photos**, when they come, are creative works and want their own choice (CC BY 4.0 is the usual one). Not decided.

### Moderation

What exists carries over: a report is a row with a kind and a subject (`reports`, `crates/core/src/report.rs`), and the operator acts by command. Needed: a kind `site` among the report's kinds (a migration of the constraint, as `0012` did for groups), reasons that fit a place (does not exist, wrong place, private ground or closed, offensive text, copied from elsewhere), and commands `site` (the record with its history), `revert-site` and `remove-site`. The three commands are built (2026-11-21); the report's kind is not, because a report is about an account that exists and a site may have none ([above](#what-stands-of-it-2026-11-21)). A history that anyone can read and any pilot can undo is the first line; the operator is the second. What a site says about whether one may fly there is the hard part and no code decides it: every pilot-made site is to be shown as "added by pilots, not verified", as OpenStreetMap's are shown as not being permission to fly (`crates/core/src/sites.rs`).

### What flights already say about a take-off

- The server has the place every stored flight began, as a point with a spatial index for the public ones (`flights.takeoff`, `flights_public_takeoff_idx`, `0008`), and its height (`takeoff_altitude_m`).
- `GET /v1/flights/public?bbox=` already answers "the flights from here", and the web app has a page for a place built on it ([architecture.md](architecture.md#everyones-flights-in-the-browser)).
- The core counts a pilot's own logbook by take-off (`site_statistics` in `crates/core/src/statistics.rs`), naming a place by the pilot's own waypoints.

What follows for the design: (1) a site's page can say how many public flights began there, in which months, how far they went, from the server's own data, with no licence question once the site is Venti's; (2) places where the public flights of several pilots began and no site of any layer stands are **candidates**, offered to a pilot who flew there as "add this take-off", with the position and height from their own flight; never added by the server itself, and never from private or unlisted flights; (3) a flight could name the Venti site it began at, by identifier.

## What is missing, in build order

Sizes: small is a day's piece for one session, medium several, large a piece with parts on every app.

| # | Piece | Size | Depends on | Where to begin | Done when |
|---|---|---|---|---|---|
| 1 | **Pilot-made sites on the server.** Built on 2026-11-21: [what stands](#what-stands-of-it-2026-11-21). Tables for sites and their revisions; the layer `venti` in `GET /v1/sites`; `GET /v1/sites/{id}` for every layer; a site's history; `GET /v1/sites/changes?since=` for the layer, removals included; `POST`, `PUT` and `DELETE` that write revisions, and a `POST` that restores one; the operator's three commands; bounds per account. **Left:** reports about a site; hiding a single revision; the first run of the migration and of the queries against PostgreSQL | Large | The licence is decided (CC0 1.0). Before a public server takes sites from pilots: the terms for contributors (13), and the owner's word on who may correct and on what an account's deletion leaves, both built as recommended | For reports: a migration that lets a report be about a site whose author is gone (`reports.about_account_id`, `0009`), `crates/store/src/report.rs`, `crates/api-types/src/v1/report.rs`, `crates/server/src/api/reports.rs` and the core's form (`crates/core/src/report.rs`). The source has no manifest: it is no adapter, and its licence is the constant `LICENCE` in `crates/server/src/pilot_sites.rs` | The development mode serves a made-up `venti` layer; a script with a token adds, corrects, undoes and removes a site and reads each step back, the history and the changes since; `api.md` has it with golden answers; a pilot reports a site and the operator reads the report |
| 2 | **Pilot-made sites in the core and the three apps.** Add a take-off or landing (from where one stands, from one's own flight, from one's own waypoint), correct, see the history, report; the layer drawn beside OpenStreetMap's with its own mark and credit | Large | 1; the question of the pin on the map | `crates/core/src/sites.rs`, `report.rs`; [design.md](design.md) for the screens, which are the owner's design language | Each app adds and corrects a site against the development mode, at every state of [design/AGENTS.md](../design/AGENTS.md)'s list |
| 3 | **Tokens a pilot gives to another program.** A pilot makes a token with a name and scopes (read my flights, write flights, send live positions, write sites, read my groups), sees it among their sessions and withdraws it; every endpoint names the scope it needs; a session of the app's own sign-in has them all | Medium | Nothing. It is the part of OAuth that does not need a consent page, and the scopes are what OAuth will hand out later | `crates/server/src/api/auth.rs` (`Signed` gains the scopes); a table beside `sessions`; `crates/api-types/src/v1/account.rs` | A script with a token for "write flights" uploads a flight and is refused the groups; the pilot withdraws it and the next request is `401` |
| 4 | **A description a program can read, and reads open to every origin.** An OpenAPI document served at an address of `/v1`, made from `venti-api-types` or held to it by a test that checks every golden answer against it; `GET` requests that carry no token answered to any origin | Medium | Nothing. *To verify:* which crate makes the schemas, its licence under `cargo deny`, and that the owner's registry mirror has it | `crates/api-types`; `crates/server/tests/golden`; `Cors` in `crates/server/src/api/mod.rs` | A client generated from the document in another language reads stations, sites and public flights; a page on another origin reads a public flight |
| 5 | **Public flights as data.** The track as GeoJSON beside the IGC file; the public list by pilot as well as by place and time; `GET /v1/flights/public/changes?since=` that says what became public, changed, or is public no more; `published_at` on a flight | Medium | The terms under which a public flight may be reused (below). A change number across accounts (today's is per account: `accounts.flight_change`) | `crates/store/src/flight.rs`, `crates/server/src/api/feed.rs`, `flights.rs` | A mirror that follows the changes holds exactly the public flights, a day after one was made private too |
| 6 | **Everything a pilot gave, and its removal.** One request that hands the pilot an archive: the account, every flight file with its record, groups and plans, sites and revisions they made, reports they sent; and what deleting the account does to each kind, said in `api.md` | Medium | 1 for the sites' part | `crates/server/src/api/account.rs`; the `zip` crate is in the workspace already (`Cargo.toml`) | The archive of the demo account holds its three flights and reads back into a fresh account |
| 7 | **The pilot's kept take-offs, waypoints and routes on the account**, private, in step across devices as flights are | Medium | Nothing; the pattern is `flights/changes` | `crates/core/src/kept_sites.rs`, `waypoints.rs`, `account.rs` | A site kept on the phone is kept in the browser after a sync |
| 8 | **What flights say about a site**: counts by site and month from public flights, a site's best flights, and the candidates | Medium | 1 | `crates/server/src/api/feed.rs`, `ranking.rs` | A site's answer says how many public flights began within a stated distance; a place with flights of several pilots and no site is offered |
| 9 | **The connector reads what the API reads**: tools for sites and public flights, which `crates/tools` already defines | Small | Nothing. With a token (3) or OAuth (12): the pilot's own logbook, and adding a site | `crates/server/src/api/mcp.rs`, `crates/mcp` | An assistant answers "which take-offs near … face west" from the connector |
| 10 | **Dumps of the community's own data**, made on a schedule: the `venti` sites with their history as GeoJSON and CSV, with the licence in the file, at a stable address, apart from every other source | Small | 1; the licence is decided, CC0 1.0. Flights are not in a dump (below) | `crates/server/src/maintenance.rs` for the schedule; files served as map regions are (`crates/server/src/maps.rs`) | Last night's file can be fetched without an account and names its licence |
| 11 | **Bounds per token**, and bounds on reads without one | Medium | 3 | `crates/server/src/accounts/throttle.rs`, whose counters are keyed by address, handle or account and live in the process | A token has its own budget, said in headers; an address without one has a smaller one |
| 12 | **The authorization server** (OAuth 2.1 with PKCE): another maker's app sends the pilot to Venti, the pilot agrees to scopes, the app gets a token. The same server lets the connector act for a pilot | Large | 3; the sign-in with Apple or Google, since the page on which the pilot agrees is a sign-in ([architecture.md](architecture.md#authentication-and-accounts), [architecture.md](architecture.md#bring-your-own-ai)) | The steps are listed under "What signing in will need" in architecture.md | Claude adds the connector, the pilot agrees, and it reads their logbook |
| 13 | **Terms for contributors**, in words a pilot reads (below) | The owner's | The licence of sites is decided (CC0 1.0). A decision on who runs Venti by name, and on the row "Reports and taking down" (7) of [plan.md](plan.md#needed-from-the-owner) | | Shown before the first contribution; the account records which version was agreed to |

**Not yet: webhooks.** A program that wants to know of changes asks "since" (pieces 1 and 5). Calls from the server to addresses that strangers name are a new way out of the server, which today has none but its adapters ([mcp.md](mcp.md#what-a-request-may-cost)). To look at again when a mirror says that asking is not enough.

**Open registration** is not a piece of code (`registration = "open"` exists) and is what a hub needs most. It waits on moderation that holds for strangers, on the terms, and on the sign-in.

### What the terms for contributors must say

Named here, not written: under which licence each kind of contribution is given and handed out (sites under CC0 1.0, decided 2026-11-21; later photos, notes), and that it is given for good: a site and its history stay when the account goes, without the pilot's name; that a pilot gives only what is theirs to give, from their own knowledge and flights, and copies from no map, database or guide; what happens to each kind when the account is deleted, and that openly licensed contributions stay without their author's name; who can see what (the values of each visibility); under what terms others may reuse a flight a pilot made public; that another maker's app acts for a pilot only with a token the pilot gave and can withdraw; that whoever runs Venti may remove a contribution and on what grounds; that nothing on Venti is permission to fly anywhere; who is responsible for the data, by name and with an address, as data protection law asks (*to verify* what exactly: tracks and positions are personal data).

## What in today's server works against it

In the order of how much it would hurt to leave.

1. **One credential that does everything, got with the password** (`crates/server/src/api/auth.rs`). No other maker's app can act for a pilot without the pilot's password, and no token can be limited. Every endpoint added before scopes exist is one more to sort later. Piece 3.
2. **Sites are a source's whole set, replaced with each run** (`crates/store/src/site.rs`, `0005`): a record is text, without author, time or history, and goes when its set is replaced. A pilot-made site cannot live in that table. The answer's shape, layers by source, is right and stays. Answered on 2026-11-21 by piece 1: pilots' sites have tables of their own (`0013`, `crates/store/src/pilot_site.rs`) and are one more layer of the same answer; a source's sites are as they were, and get an address that promises only what a whole set can ([`lasting`](#reading)).
3. **A flight has no life as public data.** No moment it became public, no terms for reuse, no change number across accounts, no way to learn that a flight is public no more except by asking for it, and the list is ordered by the flight's start only (`0002`, `0008`, `crates/server/src/api/feed.rs`). A mirror cannot be kept true, so anyone who copies public flights will keep flights their pilots withdrew. Piece 5.
4. **Everything a pilot made goes with the account** (`ON DELETE CASCADE` on `account_id` throughout). Right for flights and plans; wrong for data given to everyone. The rule has to be chosen per kind before the first such kind exists. For sites it is chosen as recommended, the owner's to confirm: they stay, without their author (`0013`).
5. **Browsers of others are shut out of public data** (`cors_origins`). Piece 4.
6. **The documented API is prose and a Rust crate.** [api.md](api.md) is exact and tested, and 4,300 lines long; a maker in another language writes every type by hand. Piece 4.
7. **Answers shaped for one screen with no plain answer beside them.** `GET /v1/stations` leaves out one of two stations at the same place and offers no way to ask for both (`same_place.rs`); a flight's `outline` is "for a thumbnail" and the track as data is the IGC file only; the ranking holds its rule as a constant (`RANKING_COUNTED`); the public list knows a flight by where it began and not where it went. Each is right for the apps; a platform also gives the unjudged data.
8. **The pilot has one public identifier, the handle**, and changing a handle is on the list of what is to come ([api.md](api.md#what-is-not-here-yet)). The account's own identifier is never handed out. Before handles can change, either they are declared permanent or a pilot gets an identifier that is. *For the owner.*
9. **Bounds are per address or per account, and in the process's memory** (`crates/server/src/accounts/throttle.rs`, [mcp.md](mcp.md#what-a-request-may-cost)), and most reads have none. One address with many pilots behind it (a club's app, an assistant's provider) shares one budget; nothing tells a good client from a flood. Piece 11.
10. **A new kind of community data costs a migration of the reports' constraint** and a new form in the core (`0012`, `crates/core/src/report.rs`). Bearable; said so that the next kind plans for it. The first such kind showed a second cost: a report is about an account, and a site can be without one, which is why piece 1 has no reports yet.
11. **The pilot's own places are on the device by decision** (`kept_sites.rs`, `waypoints.rs`, `task.rs`). Piece 7.

## For the owner

Decided on 2026-11-21 and no longer in this list: the licence of pilot-made sites, [CC0 1.0](#the-licence-cc0-10-decided-by-the-owner).

1. **Who may correct a site.** Recommended: any signed-in pilot, with every change kept and undoable. Built so; the rule is one function (`may_write`, `crates/server/src/pilot_sites.rs`).
2. **What an account's deletion leaves of open contributions.** Recommended: the sites and their history stay, without the author. Built so (`0013`); with CC0 the dedication cannot be taken back in any case, so what is left to decide is whether the server goes on showing them.
3. **Whether a pilot may mark a site as official.** Recommended: no; leave the field to a later answer about clubs. Built so: the body a pilot sends has no such field (`SiteDraft`).
4. **Which of two layers a map draws where both have the same take-off.** Recommended: Venti's own, with OpenStreetMap's reachable from its page. Not built: both are drawn.
5. **Under what terms a public flight may be reused by others.** Recommended: readable by anyone as today; reuse with the pilot's name (CC BY 4.0) for flights made public after the terms exist; no file of all public flights, because a track is personal data and a file outlives a withdrawal.
6. **Whether a handle is permanent.** Recommended: yes, until a second, lasting identifier for a pilot exists. A site's history now names its pilots by handle, which makes the question more pressing.
7. **Whether the server works out a pilot's statistics and "How you fly" from the files it holds.** Recommended: yes for the comparison with others, from public flights only; a pilot's own figures stay the core's.
8. **Who "Venti" is in the terms**, and the terms themselves. Until they exist, no public server should take sites from pilots (`sites_from_pilots` stays off).
9. **The numbers of piece 1**, all estimates: 20 m between two sites of the layer, 20 new sites and 100 revisions a day per account, 200 characters for a revision's line.

## Not checked

- Of the licences, the deed of CC0 1.0 was read on 2026-11-21, after the owner's decision, through a tool that hands a page over as text; its legal code was not. No text of CC BY or ODbL, no page of the OpenStreetMap Foundation (contributor terms, the waiver for CC BY, the guidelines on layers and tracing) and no data protection rule was read.
- Piece 1 was run against the in-memory store only: the store's suite (`crates/store/tests/suite/pilot_sites.rs`), the API's tests and the operator's commands. No app was built with it. The migration `0013` and the queries of `crates/store/src/postgres/pilot_sites.rs` were compiled and not run against a database on 2026-11-21: no statement of either has been executed anywhere.
- The code was read, not run. Bounds named "per address" for reads were found for the forecast only (`crates/server/src/api/forecast.rs`); that every other public read has none was concluded from a search for the refusal, not from every handler.
- `crates/server/src/api/live.rs`, `groups.rs`, `thermals.rs`, `radar.rs` and `maps.rs` were taken from [api.md](api.md) and their routes, not read line by line.
- `deploy/` was not read: what Caddy adds or hides in front of the server (headers, paths, limits).
- Which crate would make an OpenAPI document, and whether the registry mirror has it.
- The sizes are estimates from the size of comparable pieces in the repository (groups and plans for "large"), not from a plan of each.
