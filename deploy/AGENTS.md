# Notes for AI sessions working on the deployment

Working notes: what to know before changing how Venti is run on a server, with the background, the estimates, what was tried and what is still to verify. The `README.md` beside this file is for whoever runs a server.

## What has run where

Nothing here has run on a real server: the project has none. The stack was built and used on a development machine on 2026-11-19: the image built and started, the API through Caddy, a backup, and a restore into an empty database. A second run the same day, with accounts, used the server's debug build in a plain container rather than the image: an invite, an account, a flight and its share page through Caddy, a backup, and a restore that brought the account and the flight file back byte for byte. CI builds the image and starts it on every push. No image is published anywhere; publishing to a registry is the owner's decision ([architecture.md](../docs/architecture.md#deployment)).

## Accounts and the visitor's address

- **Sign-in counts live in the server's memory**, per account and per visitor's address: 10 wrong passwords an hour for one account, 30 from one address. That anyone can close an account's sign-in for an hour, and keep it closed, is the price of stopping guesses from many addresses, and the owner's decision ([architecture.md](../docs/architecture.md#decision-log)).
- **Why IPv4 only.** The services' network has no IPv6. A port published without a host address would be mapped by Docker's "userland proxy" from the host's IPv6 addresses to the container's IPv4 address (docs.docker.com, "Port publishing and mapping", read 2026-11-19), so Caddy would see the same private address for every IPv6 visitor, and thirty wrong passwords from any of them would close sign-in for all of them. `docker-compose.yml` therefore publishes 80 and 443 on `0.0.0.0` only. Visitors on networks with only IPv6, as some mobile networks are, are expected to come through their provider's translation to IPv4 *(to verify from such a phone)*. With an `AAAA` record, IPv6 visitors would get no answer on that address and depend on their browser trying IPv4 next.
- **Serving IPv6 properly** *(to verify; nobody has done it with this stack)* needs four things together: an IPv6 network for the services (`enable_ipv6: true` and an IPv6 subnet under `networks.default.ipam` in `docker-compose.yml`, per Docker's page "Use IPv6 networking", read 2026-11-19); a fixed IPv6 address for Caddy beside its IPv4 one; that address in `VENTI_TRUSTED_PROXIES`; and the ports published without `0.0.0.0`. Then the runbook's check, from a phone on a mobile network with IPv6, before the `AAAA` record goes in.
- On the development machine Docker hid the visitor's address for requests from the machine itself, which is harmless. When `/v1/health` says `"hidden"` is in [docs/api.md](../docs/api.md).

## Reports

- **What bounds them.** An account sends at most 20 reports a day, a report's text has at most 1,000 characters, and an account has one open report about one thing. There is no command that stops one account from reporting; registration by invite keeps that risk small.
- **The daily line** (``3 reports are open: `venti-server reports` lists them``) is written with the daily cleanup, left out while no report is open, and names nothing and nobody. Nothing is sent anywhere.
- **No app shows yet** that a flight was taken down or an account's publishing suspended, although the API says so with the moment.
- **What was tried**, on 2026-11-20 on a development machine, with the server's debug build and PostgreSQL 16 with PostGIS in a container: two accounts, a public flight, a live track and three reports, then every command, the two queries, and what the API answered after each. Not tried: the commands through `docker compose exec`, in the image a server runs *(to verify on the first real install)*. The runbook's sample of `reports` and its commands are held to the program by the test `the_operators_guide_shows_the_commands_as_they_are` in `crates/server/tests/moderation.rs`: after a change to what `reports` prints, copy `crates/server/tests/golden/operator_reports.txt` into `README.md`, and every line there that starts `docker compose exec server venti-server` must name a command the program has.

## Groups

- **The log.** Where a log line names a request's path, a path under a group reads `/v1/groups/…`, whatever stood there. The one line the server writes about groups by itself is `removed 3 plans of groups for days more than 2 days gone by`, at most once an hour.
- **How long things are kept.** A plan goes two days after its day by the server's date in UTC; the cleanup looks once an hour. A group and its members stay until its members delete it or the last of them leaves. Everything a pilot put into groups goes when the pilot deletes the account.
- **The bounds** are in [plan.md](../docs/plan.md#needed-from-the-owner) and, with their reasons, in `crates/server/src/groups.rs`.
- **What was tried**: the runbook's statements, on 2026-11-20 on a development machine against PostgreSQL 16 with PostGIS in a container, on a database the store's tests had filled; not through `docker compose exec` in the image.

## The connector

- Claude reaches a connector from Anthropic's servers over HTTPS on port 443 and no other (Anthropic's help page, read 2026-11-19). On `:80` without a domain only programs on the operator's own network can use it. Nobody has yet added a real server to Claude.
- It writes nothing to the log for an ordinary request, and one line for an address that passes 120 requests in a minute, however often it asks again in that minute. An assistant's provider sends many pilots' questions from few addresses, which is why that line matters.
- A page in a browser may call it only from `VENTI_PUBLIC_URL` and the origins in `VENTI_CORS_ORIGINS`; assistants do not call from a browser.
- An assistant that looks for a sign-in service at `/.well-known/oauth-…` gets `404` from the server, which is the answer that there is none.

## Forecasts

- The server never asks Open-Meteo's public service, whose terms are for non-commercial use within daily limits per address.
- The instance in `docker-compose.yml` is Open-Meteo's own image, unchanged, under the AGPL-3.0-or-later. It starts with no forecasts: when a place is asked for it reads the parts it needs of Open-Meteo's open data on AWS (no key; Deutscher Wetterdienst's models under CC BY 4.0) and keeps them in a cache on disk. It publishes no port; only the server reaches it.
- **`API_SYNC_APIKEYS` is set in the Compose file** and is easy to miss: without it an instance does not say which run of a model it holds, the server cannot date a forecast, and it serves none. Its value guards nothing, since the instance is reachable only inside the stack.
- **How much the server asks.** A forecast for a place is four requests to the instance, then kept for half an hour for everyone who asks within about a kilometre. The server asks the instance at most 400 times an hour, a tenth of which is held back for its own look every ten minutes; when the hour is used up, places not at hand get `503` with `Retry-After`. What a forecast holds is in [data-sources.md](../docs/data-sources.md#forecasts).
- **Tried** on 2026-11-19 with a cache of 256 MB and five places, which says nothing about a busy server: the same image with the same settings, fed through a local relay to the same data, because that machine had no direct way out.
- **Still to verify on a real machine:** the instance as the Compose file starts it; how long a first forecast takes with a direct connection (through the relay it was 25 to 60 seconds, longer than a request waits, 8 seconds: the app is told to ask again, and the forecast is there when it does; if it stays slow, Open-Meteo's [sync](https://github.com/open-meteo/open-meteo/blob/main/docs/sync-command.md) copies the data to disk ahead of time, with `--bounding-box 46.3,9.4,48.5,17.2` and the models `dwd_icon_d2_native,dwd_icon_eu_native`, not tried); and whether those model names stay after Deutscher Wetterdienst's change of 30 November 2026.

## Map regions

Notes behind the short section of the same name in [README.md](README.md#map-regions). The code is `crates/server/src/maps.rs` (reading a folder, the rules, the hashes) and `crates/server/src/api/maps.rs` (the list, the files and their ranges); the tests are `crates/server/tests/maps.rs` and `maps_log.rs`; the API is described in [docs/api.md](../docs/api.md#maps) and the decisions are in the log of [docs/architecture.md](../docs/architecture.md#decision-log).

None of this has run on a real server, and no region but the small one in the repository has been built. Every size here is an estimate *(to verify)*.

**The setting has one name in two places.** In `.env`, `VENTI_MAPS_DIR` is the folder on the machine, `deploy/data/maps` unless set. `docker-compose.yml` mounts that folder into the server's container at `/srv/maps`, read-only, and gives the server its own `VENTI_MAPS_DIR=/srv/maps`. Run without Compose, the server takes the folder from `maps_dir` in its configuration file or from the variable, and hands out no maps when neither is set.

**What a `map.toml` holds.**

```toml
id = "alps-de-at"
name = "Germany and Austria"
version = "2026-11-20"
covers = { south = 46.3, west = 9.4, north = 48.5, east = 17.2 }

[[files]]
role = "basemap"
name = "basemap.pmtiles"
sha256 = "0f343b0931126a20f133d67c2b018a3b5f1a2b5e0f8f1d3f6f0f4d8a3b1c2d3e"
credits = [
  { text = "© OpenStreetMap contributors", url = "https://www.openstreetmap.org/copyright" },
  { text = "© OpenMapTiles", url = "https://openmaptiles.org" },
]

[[files]]
role = "terrain"
name = "terrain.pmtiles"
sha256 = "5f1a2b5e0f8f1d3f6f0f4d8a3b1c2d3e0f343b0931126a20f133d67c2b018a3b"
credits = [
  { text = "© Mapterhorn and the surveys it combines", url = "https://mapterhorn.com/attribution" },
]
```

The two hashes above are made up. The rules the server holds a region to, each with a test:

- `id`, `version` and every file's `name` are lowercase letters and digits with `-`, `_` and `.` between them, 64 characters at most (`venti_api_types::v1::is_map_name`). They become the file's address, `/v1/maps/<id>/<version>/<name>`.
- `role` is `basemap`, `terrain` or `other`. A word the server does not know is refused, not passed on.
- `sha256` is the SHA-256 of the file as it lies there, in lowercase.
- Every file names at least one credit: whose data it holds, worded as that provider asks. A `url` starts with `https://` or `http://`.
- A file lies in the region's own folder. A link that leads out of the folder is refused; a link to a file beside it is not.
- No two folders have the same `id`. If two do, neither is handed out.
- A key the server does not know makes the `map.toml` unusable, as one does in the server's configuration file: a mistyped `credits` must not pass for no credits.

A region that breaks a rule is left out, the log says which rule, and the server runs on.

**What the server does with the folder.** When it starts, it reads the `map.toml` of every folder, then reads every file from its first byte to its last and works out its SHA-256, on a thread beside everything else. The API answers at once, and a region appears at `/v1/maps` when its files are read. The log has lines like these:

```
maps: reading the regions in /srv/maps; each is handed out once its files are checked
maps: handing out alps-de-at, version 2026-11-20: 2 files, 3712.4 MB
```

The second line's size is made up. The server's debug build read the 7 MB of the region in the repository in about a tenth of a second on a development machine (2026-11-20). How long gigabytes take on a server's disk is *to verify*; expect about as long as the disk needs to read the files once. The hashes are worked out again at every start.

If a file is changed or taken away while the server runs, the server notices at the next request for it: the file's size or the time it was written is no longer what it read. It then stops handing that region out and says so in the log, once, until it is started again. So a copy of new files over old ones does not go out with the old hashes, and the runbook tells operators to swap folders and restart.

**Stopping.** A download ends when the server is told to stop, and the app goes on with it by its range of bytes afterwards. Measured on 2026-11-20 with a file of 200 MB and a slow client: a server whose downloads did not end at a stop was still there after 15 seconds, and one whose downloads do was gone in half a second. `stop_grace_period: 10s` in `docker-compose.yml` therefore still fits.

**Building a region.** `tools/region-map/build.sh` has not built one. Its header says what it needs and where each number comes from: Java, Go and Python; from the extract of Europe it starts with, which was 35.5 GB on 2026-11-19, about 18 GB of memory for Java and 180 to 360 GB of free disk by the rule of thumb in Planetiler's README; and less of all three from a smaller extract that covers the region. A server of the smallest size the runbook names cannot build a region.

The credits the script writes are those of the region in the repository, which is built from the same two sources; what OpenStreetMap asks of whoever passes its data on is in [data-sources.md](../docs/data-sources.md#flying-sites). Mapterhorn's terms for a server that hands an extract of its tiles on to many pilots are *to verify* with that project before a public release, as [architecture.md](../docs/architecture.md#maps-and-terrain) says of its tile service.

**Disk and traffic** *(estimates, to verify with the first region)*.

- **Disk.** Scaled up by area from the region in the repository (7 MB for one valley), the first region comes to about 1.6 GB of base map and 2.1 GB of terrain. That is arithmetic and not a measurement. [architecture.md](../docs/architecture.md#sizing) reckons with 3 to 5 GB a region. While a region is renewed, the disk holds it twice.
- **Traffic.** Every app that keeps the region downloads all of it once for each version. A hundred pilots and a region of 4 GB are 400 GB for each renewal.
- **No limit.** The server sends a file as fast as a client takes it. It counts nothing and refuses nobody, however many download at once, and a download has no time limit. A client that stops reading holds its connection and one open file until the connection ends. If downloads crowd out the rest, the place to bound them is in front of the server.
- **Through Caddy.** A download reaches the server through Caddy like every request to `/v1`. The server sends a file uncompressed, with its length, and answers requests for a part of it; the `Caddyfile` adds nothing to requests under `/v1`. A download of gigabytes through Caddy has not been tried.

**To verify on the first real install**, from another machine: fetch a whole file, break the download off, let it go on, and compare the result with the list.

```bash
curl -s https://<domain>/v1/maps
curl -C - -O https://<domain>/v1/maps/alps-de-at/<version>/basemap.pmtiles    # press Ctrl-C, then run it again
sha256sum basemap.pmtiles                                                      # the sha256 of the list
```

**What was tried**, on 2026-11-20 on a development machine with the server's debug build, not in a container and not through Caddy: the region in the repository (two files, 7 MB) listed and fetched whole and in parts with `curl`, with and without `--compressed`; a download of a file of 200 MB broken off by stopping the server and gone on with after the next start, with the listed hash in the end; the building script from its first line to its last with stand-ins for its three heavy steps, and the folder it wrote listed by the server; and `docker compose config` on the Compose file with the new folder. CI builds the image and starts it without a maps folder. Not tried: a region of gigabytes, the script's real steps, the Compose setup running with the folder mounted, and anything on a phone.

**The region in the repository** is `apps/ios/Venti/Resources/MapDemo`, with a `map.toml` of its own, and `venti-server dev --maps apps/ios/Venti/Resources` hands it out. The iOS and the Android workflow start the development server that way. `tools/demo-map/build.sh` rebuilds its two archives and then writes the day and the new hashes into that `map.toml`; the list of the region is a golden file of the server's tests and the example in `docs/api.md`, so after a rebuild run `VENTI_UPDATE_GOLDEN=1 cargo test -p venti-server --test maps` and copy `crates/server/tests/golden/maps.json` into the document.

## The web app behind Caddy

- The headers Caddy sends, and why, are in [apps/web/AGENTS.md](../apps/web/AGENTS.md#the-content-security-policy). The pages of shared flights have a policy of their own from the server, which allows no script at all.
- Each page of the web app is a file of its own (`/logbook/` is `logbook/index.html`), and Caddy answers an address with its page. An address that is no page gets the map's page.
- Tried on 2026-11-19 with the web app as built that day, served by this `Caddyfile` in Caddy 2.10 and opened in Chromium: the map, the logbook and the settings drew, both workers started, the core answered and the map fetched its tiles, and the browser refused nothing. Safari and Firefox were not tried.
- On an update the API is away for a few seconds; a collection run that was under way is dropped and repeated, and a map region's download breaks off and is taken up again by the app.

## The services' confinement

- Tried on 2026-11-19 on a development machine with the server's debug build in a plain container: start, an invite, an account, a flight and its analysis, the share page and its picture through Caddy, `backup.sh`, and a restore over the live database. The image the Dockerfile builds was not run under these settings there; CI starts it with the same ones *(to verify on the first real install)*. The optional `open-meteo` service is someone else's image and was not tried under them.
- The server's role is no superuser, so someone who got into the server can read and change what the server keeps, and cannot read files on the database's machine or run programs there.
- **A database created before 2026-11-19** has `venti` as its superuser, which cannot be taken from the role a database was created with. Move it: take a dump with the old setup, start the new setup on an empty volume (`docker compose down`, `docker volume rm venti_db-data`, `docker compose up -d db`), and restore the dump. A dump made by the old `backup.sh` does not say who owns its tables; after restoring one, hand them to the server's role before starting the server *(to verify: nobody but developers has such a database, and the command was not run against one)*:

  ```bash
  docker compose exec -T db psql --username postgres --dbname venti --command \
    "DO \$\$ DECLARE t record; BEGIN FOR t IN SELECT tablename FROM pg_tables WHERE schemaname = 'public' AND tablename <> 'spatial_ref_sys' LOOP EXECUTE format('ALTER TABLE public.%I OWNER TO venti', t.tablename); END LOOP; END \$\$"
  ```

## Backup and restore

- `backup.sh` takes the dump as the database's superuser, from inside the database's container, and the dump says which role owns each table.
- **Live tracks are left out** by `--exclude-table-data`: the dump has their two tables and none of their rows, and keeps the list of identifiers live tracks have used, which holds no position. After a restore a flight under way begins its track anew with the next positions its app sends. Tried on 2026-11-19 with a track of 43,200 positions: the dump shrank from 698 kB to 74 kB, restored into an empty database, and the restored tables took a new track.
- **The plans of groups are left out** the same way (`group_plans`, `group_plan_pilots`); groups and members are in the dump. Tried on 2026-11-20 with two groups, four members, two plans and four pilots in on them: the restored database had the groups and members, no plan, and took a new one.
- Both tests ran `pg_dump` by hand with the script's `--exclude-table-data` options, not `backup.sh` itself; the systemd timer was not run on a real server *(to verify on the first real install)*.
- The architecture calls for an encrypted copy of the dumps at a different site; where it goes, and what it may cost, is the owner's decision and is not set up.
- After a restore the apps are meant to notice that the server has gone back in time (the server tells a device whose bookmark lies in the future to compare its whole logbook) and to upload what is missing. That is built on the server and in no app yet.

## Logs and limits

- Docker keeps five files of ten megabytes for each of the three services (`logging:` in `docker-compose.yml`). The server writes nothing per request. A refusal for too many requests is one line per address and window. A failure that repeats with every request (database away, server busy, requests timing out) is written once and then as one line every ten seconds with how many more there were; the server's tests hold it to that: 2,500 refused requests and 40 that time out leave about 4 kB of log. Caddy writes no line per request either, because the `Caddyfile` has no `log` directive.
- Without `VENTI_PUBLIC_URL` the server names shared flights' pages with the address it listens on, and says so in its log at start.
- **Bounds that are not settings**, because nobody should have to tune them: 10 wrong passwords an hour for one account and 30 from one address; 10 new accounts an hour from one address; a flight file of 4 MB at most and 1,000 uploads a day per account; an unused session lasts 400 days; one address may make the server fetch forecasts for 12 places an hour (places at hand are not counted); 20 reports a day per account; the bounds of groups; and the connector's 64 kB a request, 10 seconds for an answer and 120 requests a minute from one address. An IPv6 visitor counts as its /64 network, and one /48 as a whole gets four times what one address gets. They are in `crates/server/src/accounts/mod.rs`, `crates/server/src/groups.rs` and `crates/server/src/api/mcp.rs` with their reasons.
- **Each count is a table in memory** with room for 100,000 addresses (or handles, accounts, devices), 11 MB when full; there are thirteen, three of them for what accounts do with groups. When one is full the server keeps counting those it knows and counts all others together under one generous shared limit, refusing nobody merely for being new; when even that is used up, newcomers get `429` until the window is over (a minute for the connector, an hour for sign-in and new accounts).

## Working on the server without a server

The development mode needs nothing: `cargo run -p venti-server -- dev` serves the API over recorded data on `http://127.0.0.1:8080`, with a demo account and two further pilots ([docs/api.md](../docs/api.md#try-it) has the handles, the password and a token that always works). With `--maps apps/ios/Venti/Resources` it also hands out the region in the repository.

For the real thing, and for the store's tests against PostgreSQL, start a database in a container. The password is for a throwaway database on the loopback address; it is not a secret.

```bash
docker run -d --name venti-db -e POSTGRES_USER=venti -e POSTGRES_PASSWORD=venti -e POSTGRES_DB=venti \
  -p 127.0.0.1:5432:5432 postgis/postgis:16-3.4

# The store's behavioural suite against PostgreSQL. Without the variable these tests are skipped.
VENTI_TEST_DATABASE_URL=postgres://venti:venti@127.0.0.1:5432/venti cargo test -p venti-store

# The server against the live providers, taking accounts from anyone.
VENTI_DATABASE_URL=postgres://venti:venti@127.0.0.1:5432/venti \
VENTI_REGIONS=regions/alps-de-at/region.toml \
VENTI_REGISTRATION=open \
  cargo run -p venti-server -- serve

# The image a server runs.
docker build -f deploy/Dockerfile -t venti-server .
```

In a cloud session the Docker daemon may have to be started first (`dockerd &`), and Docker Hub may refuse pulls with "429 Too Many Requests"; `mirror.gcr.io/library/<image>` serves the same official images (seen on 2026-11-19).
