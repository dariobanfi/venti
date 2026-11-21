# Running Venti on a server

This folder runs Venti on one Linux machine with Docker Compose. It is the operator's manual: install, update, accounts, reports, backup and restore. Background, what was tried and what is still to verify are in [AGENTS.md](AGENTS.md); the server itself is described in [docs/architecture.md](../docs/architecture.md#server).

| File | What it is |
|---|---|
| `docker-compose.yml` | The services: Caddy, the web app's files, the Venti server, PostgreSQL with PostGIS, and an optional forecast service |
| `Dockerfile` | Builds the `venti-server` image from this repository |
| `web.Dockerfile` | Builds the web app from this repository into an image that puts the site's files where Caddy serves them |
| `setup.sh` | First install in one command: settings, build, start |
| `Caddyfile` | TLS, the web app's files, map tiles, and the way in to the API, the map regions, the pages of shared flights and live tracks, and the connector |
| `env.example` | Every setting, with comments. Copy it to `.env` |
| `db-init.sh` | Run once by the database when it is first created: makes the role the server signs in as |
| `backup.sh` | Dumps the database and removes old dumps |

No image is published: the server builds its own from a checkout. Nothing here has run on a real server yet; steps that could not be tried are marked *(to verify)*.

## What you need

- A machine with Debian 12 or 13, or Ubuntu 24.04 or 26.04 LTS, and root access. For wind stations alone, 2 cores, 4 GB of memory and 20 GB of disk are plenty; the recommended size is in [architecture.md](../docs/architecture.md#sizing).
- Ports 80 and 443 open to the internet.
- For HTTPS, a domain name whose DNS `A` record points at the machine (no `AAAA` record: see [The visitor's address](#the-visitors-address)). Without one the stack serves plain HTTP on port 80, which is fine for a first look and for nothing else. A stack on one's own computer, only for oneself, is kept to that computer with `VENTI_HTTP_BIND=127.0.0.1:8088` and `VENTI_HTTPS_BIND=127.0.0.1:8443` in `.env`: the site is then at `http://127.0.0.1:8088` and nothing of it is reachable from the network.

## First install

All commands run on the server.

1. **Install Docker Engine** with the Compose plugin (`docker-ce`, `docker-compose-plugin`) from Docker's own repository, following <https://docs.docker.com/engine/install/debian/> or <https://docs.docker.com/engine/install/ubuntu/>, after removing the distribution's `docker.io`. Check:

   ```bash
   docker --version && docker compose version
   ```

2. **Get the code.** `/opt/venti` is used throughout; any folder works.

   ```bash
   sudo mkdir -p /opt/venti && sudo chown "$USER" /opt/venti
   git clone https://github.com/dariobanfi/venti.git /opt/venti
   cd /opt/venti/deploy
   ```

3. **Run the setup.** With the site's domain name, whose DNS already points at this machine:

   ```bash
   ./setup.sh venti.example
   ```

   It writes the settings to `.env` with a database password it makes up, builds the two images from this checkout (the server and the web app; the first build compiles both and takes several minutes) and starts everything. With a domain the site gets HTTPS by itself and takes accounts by invite. Without one, `./setup.sh` alone serves plain HTTP on port 80 for a first look: **nothing is encrypted then**, and registration stays closed. `./setup.sh --forecasts …` also switches on the forecast service ([Forecasts](#forecasts); it wants 8 GB of memory or more).

   Run again, the script changes no setting that is there: it builds what has changed and starts it. Every setting is explained in `env.example`; to change one, edit `.env` and run `docker compose up -d`.

4. **By hand instead**, if you would rather see each step:

   ```bash
   cp env.example .env
   chmod 600 .env
   openssl rand -hex 24      # paste the output as POSTGRES_PASSWORD
   nano .env                 # VENTI_SITE_ADDRESS, VENTI_PUBLIC_URL, VENTI_REGISTRATION
   docker compose build
   docker compose up -d
   ```

5. **Check.**

   ```bash
   docker compose ps
   docker compose logs --tail 20 server
   curl -s http://localhost/v1/health        # with a domain: https://<domain>/v1/health
   ```

   Within seconds the log has one line per data source with what it delivered, and the health answer lists each source with `"stale": false`. The server creates the database schema itself. `web` is listed as exited: it has put the web app's files where Caddy serves them and is done. The site's front page is the app.

6. **Schedule the backup**: see [Backup](#backup). An install without a backup is not finished.

7. **Check the visitor's address** once: see [The visitor's address](#the-visitors-address).

8. **Invite the first pilot**, once the site has its domain and HTTPS: see [Accounts](#accounts). `docker compose logs server | grep 'PLAIN HTTP'` must find nothing.

### What ends up where

| Data | Place | Lose it and |
|---|---|---|
| Database | Docker volume `venti_db-data` | Every account and every pilot's flights are gone for good, unless there is a dump. The last hours of observations and the airspace the server fetches again |
| Raw provider payloads, about 240 MB a day, kept 14 days | Docker volume `venti_archive` | Nothing breaks; old parsing bugs can no longer be replayed |
| Tiles of map imagery, up to 20 GB (`VENTI_IMAGERY_CACHE_MB`) | Docker volume `venti_imagery` | Nothing breaks; the server asks the provider for each tile again |
| TLS certificates | Docker volume `venti_caddy-data` | Caddy asks for new ones, which the issuers rate-limit |
| Web app | Docker volume `venti_web`, filled from the image `venti-web` at every start | Nothing: start again |
| Map tiles | `deploy/data/tiles` (or the folder named in `.env`) | Copy them there again |
| Map regions for download | `deploy/data/maps` (or `VENTI_MAPS_DIR`) | Copy or build them again: [Map regions](#map-regions) |
| Settings | `deploy/.env` | Write them again; the database password must be the one the database was created with |
| Database dumps | `deploy/backups` (or `VENTI_BACKUP_DIR`) | There is nothing to restore from |

Flight files are in the database: one dump holds everything. Until map tiles are put in their folder the apps draw their maps from the public tile services, as they do in development. Files in `data/tiles` are served under `/tiles/` with range requests, as PMTiles archives need.

## Update

```bash
cd /opt/venti
git pull
cd deploy
./backup.sh
docker compose build
docker compose up -d
docker compose logs --tail 20 server
curl -s http://localhost/v1/health
```

The server upgrades the database schema when it starts, and an older server cannot read a newer schema: to go back after a schema change, restore the dump taken before the update. The API is away for a few seconds while the server restarts.

To update PostgreSQL, Caddy or PostGIS, change the image tag in `docker-compose.yml` in the repository, not on the server. A new major version of PostgreSQL cannot read the old data folder: dump with the old version, start the new one on an empty volume, restore *(to verify)*.

## Accounts

Who may create an account is `VENTI_REGISTRATION`:

| Value | Meaning |
|---|---|
| `invite` | Whoever has an invite code. The default when the setting is empty |
| `open` | Anyone |
| `closed` | Nobody; existing accounts keep working. What `env.example` starts with |

Make invite codes:

```bash
cd /opt/venti/deploy
docker compose exec server venti-server invite                              # one code, good until used
docker compose exec server venti-server invite --count 5 --expires-in-days 14
```

Each code makes one account and is printed once; the database keeps only a hash. If one is lost, make another.

- **A forgotten password cannot be recovered**, by the pilot or by you. The only way on is a new account and a fresh upload from the pilot's phone.
- **A pilot deletes their own account** in the app. To delete one for a pilot who cannot: `DELETE FROM accounts WHERE handle = '...'` in the database; everything of the account goes with it. It stays in older dumps until they rotate out.
- **Ten wrong passwords in an hour close signing in to an account for an hour**, also when someone else sends them. A pilot already signed in on a device notices nothing. To lift it at once, restart the server, which forgets the counts: `docker compose restart server`. Who is guessing: `docker compose logs --since 2h server | grep 'wrong password'`.
- **What pilots share and should not be there**: see [Reports and taking things down](#reports-and-taking-things-down). Do not hide a flight with an `UPDATE` by hand; its pilot would simply share it again.

### The visitor's address

The server counts sign-in attempts and connector requests by the visitor's address, which it takes from Caddy's `X-Forwarded-For` header and from nowhere else. Caddy therefore has a fixed address in the services' network, `172.30.250.2`. If `172.30.250.0/24` is taken on the machine, set `VENTI_NETWORK` in `.env` to the first three numbers of a free range.

**IPv4 only.** The ports are published on the machine's IPv4 addresses only. Give the domain an `A` record and no `AAAA` record; otherwise every IPv6 visitor would count as one. Serving IPv6 is described in [AGENTS.md](AGENTS.md#the-visitors-address) *(to verify)*.

**Check once on a new server** *(to verify)*:

1. The log says whose word it takes. It should read `X-Forwarded-For is believed from 172.30.250.2 and from nowhere else`:

   ```bash
   docker compose logs server | grep 'client addresses'
   ```

2. Sign in with a wrong password from a phone on mobile data, Wi-Fi off. `client=` should show the phone's public address; one that starts with `172.` means Docker hides the visitors' addresses:

   ```bash
   docker compose logs --since 5m server | grep 'wrong password\|nobody has'
   ```

3. After a day of use, `"seen"` is right. `"hidden"` means the server sees one private address for most requests, and the log names it (`does not see its visitors' addresses`); on a server one person uses at home that is fine. No answer means fewer than twenty requests so far.

   ```bash
   curl -s https://<domain>/v1/health | grep -o '"client_addresses":"[a-z_]*"'
   ```

Where the addresses are hidden, everyone counts as one: thirty wrong passwords by anyone close signing in for everyone for an hour, and the connector answers 120 requests a minute in total. Fix that before letting people in.

## Reports and taking things down

A signed-in pilot can report a flight, a pilot's page or a live track to you, with one of six reasons and some words ([api.md](../docs/api.md#reports)). The server stores the report and does nothing else. You read the reports and decide, with these commands; there is no web page for it.

```bash
cd /opt/venti/deploy
docker compose exec server venti-server reports                      # the open reports, oldest first
docker compose exec server venti-server reports --all                # the closed ones too
docker compose exec server venti-server close-report <ID> --note "Compared the two files: hers."
docker compose exec server venti-server take-down <FLIGHT>           # a flight's address, or the link to its page
docker compose exec server venti-server release <FLIGHT>
docker compose exec server venti-server remove-live-track <TRACK>    # a track's identifier, or the link to its page
docker compose exec server venti-server suspend-publishing <HANDLE>
docker compose exec server venti-server resume-publishing <HANDLE>
```

Each command does one thing, takes effect at once, and says what it did and what it did not. One pointed at something that is not there says so and fails. None deletes a flight or an account, and none closes a report: taking a flight down and closing its report are two commands.

This is what `reports` prints, here for three made-up pilots:

```
3 reports are open, oldest first.

Report 0000000000000000000000rep1, open, made 2026-11-17 17:05 UTC
  About:        flight fbyw03atd9qdt5bfe177ra1k94 of mara (Mara Muster); now public
                https://venti.example/f/fbyw03atd9qdt5bfe177ra1k94
  Reason:       not_a_flight
  Text:         I flew there that day.
                This is my track, not hers.
  Reported by:  demo

Report 0000000000000000000000rep2, open, made 2026-11-18 16:05 UTC
  About:        the page of mara (Mara Muster)
  Reason:       impersonation
  Reported by:  jonas

Report 0000000000000000000000rep3, open, made 2026-11-18 18:55 UTC
  About:        live track 11111111111111111111111111 of demo (Demo Pilot); now on everyone's map
                https://venti.example/live/11111111111111111111111111
  Reason:       other
  Text:         Nobody is flying there. It is night.
  Reported by:  mara

A report is one pilot's word about another. Nothing was done about any of them: see `venti-server --help` for take-down, remove-live-track, suspend-publishing and close-report.
```

| Command | What it does | What it does not do |
|---|---|---|
| `reports` | Lists the open reports, oldest first: what is reported and whose, what it is now, the link to look at it (when `VENTI_PUBLIC_URL` is set), the reason, the reporter's words and handle. `--all` adds the closed ones with their notes | Changes nothing, writes nothing to the log |
| `close-report <ID> --note TEXT` | Closes a report with your note of 1 to 1,000 characters. The same pilot can then report the same thing again | Tells nobody, and does nothing about what was reported |
| `take-down <FLIGHT>` | Makes one flight private and keeps it so: its link shows no flight, it is in no list, and its pilot cannot share it again, also not by uploading the same file. The pilot's apps are told that it was taken down | Does not delete it, does not say why, and does not stop the pilot from uploading a changed file: for that, suspend publishing |
| `release <FLIGHT>` | Lets the pilot share the flight again | Shares nothing |
| `remove-live-track <TRACK>` | Deletes one live track with its positions and retires its identifier; its link shows no track | Does not stop the pilot's app, which goes on under a new identifier. Cannot be undone |
| `suspend-publishing <HANDLE>` | The account can make nothing public: its public flights and live tracks become shared by link, and so does everything it sends from then on. Its apps are told | Does not delete, sign out or stop the account, and takes nothing from those who have a link |
| `resume-publishing <HANDLE>` | Gives the ability back | Makes nothing public again |

- **Look at what was reported before you act.** The server checked nothing of it, and anyone with an account can send one.
- **What `reports` prints is for your eyes.** It names who reported, and a live track's identifier is the key to where a person is. Do not paste it where others read.
- **The pilot is told the fact and nothing else.** The server knows no email address. Why, the pilot learns only if you tell them some other way.
- **Nothing tells you that reports wait** except a daily line in the log, ``3 reports are open: `venti-server reports` lists them``. Look at the log, or run `reports`.
- **Reports are kept** until closed and 90 days after (`VENTI_REPORT_DAYS`), and go with either account. They are in the dumps.
- **What is allowed on your server is yours to say.** The project has written no rules of conduct yet ([plan.md](../docs/plan.md#needed-from-the-owner)).

No command lists what you took down or suspended; these queries do:

```sql
SELECT f.id, a.handle, f.taken_down_at FROM flights f JOIN accounts a ON a.id = f.account_id
WHERE f.taken_down_at IS NOT NULL ORDER BY f.taken_down_at;
SELECT handle, publishing_suspended_at FROM accounts WHERE publishing_suspended_at IS NOT NULL;
```

## Groups

Pilots make groups, which only their members see, and plan flying days in them ([api.md](../docs/api.md#groups)). There is nothing to set up.

- **You see no group** through any command or the API. The database holds their names, members and plans, which say where people mean to be on a day: treat it as that. The log holds nothing of a group.
- **Invite codes are not in the database**, only their hashes. A keeper who lost one makes a new one in the app.
- **Plans are removed two days after their day** (`VENTI_PLAN_DAYS`) and are not in the dumps; groups and members are.
- **Nobody can report a group or a plan yet**, and no command removes one. Its keeper clears up in a group. If something must go all the same, do it in the database:

```sql
-- The groups, with who keeps each and how many members and plans it has.
SELECT g.id, g.name, k.handle AS keeper, g.created_at,
       (SELECT count(*) FROM group_members m WHERE m.group_id = g.id) AS members,
       (SELECT count(*) FROM group_plans p WHERE p.group_id = g.id) AS plans
FROM groups g JOIN accounts k ON k.id = g.keeper_id
ORDER BY g.created_at, g.id;

-- The plans of one group, with who is in on each.
SELECT p.id, p.day, p.meet_at, p.place, a.handle AS posted_by,
       (SELECT string_agg(i.handle, ', ' ORDER BY x.since, x.account_id)
        FROM group_plan_pilots x JOIN accounts i ON i.id = x.account_id
        WHERE x.plan_id = p.id) AS pilots_in
FROM group_plans p JOIN accounts a ON a.id = p.author_id
WHERE p.group_id = '<group>'
ORDER BY p.day, p.meet_at NULLS LAST, p.created_at, p.id;

-- A new name for one group.
UPDATE groups SET name = '<name>' WHERE id = '<group>';
-- One plan removed, with who was in on it.
DELETE FROM group_plans WHERE id = '<plan>';
-- One group removed, with its members' places in it and all its plans. This cannot be undone.
DELETE FROM groups WHERE id = '<group>';
```

Do not delete rows of `group_members` by hand: the database refuses to remove a group's keeper, and a member's plans go with the row. Nobody is told; the apps find out when they next ask. These statements have not been run through `docker compose exec` on a real server *(to verify)*.

## The connector

The server answers a pilot's own Claude or ChatGPT at `https://<domain>/mcp`, with wind and airspace, without sign-in ([docs/mcp.md](../docs/mcp.md)). It is on as soon as the server runs, and needs the domain and HTTPS: Claude reaches a connector over port 443 only.

- **Check after the install** *(to verify)*: this lists three tools. Then add the address in Claude as [docs/mcp.md](../docs/mcp.md#adding-it-to-an-assistant) describes and ask it for the wind somewhere.

  ```bash
  curl -s https://<domain>/mcp --header 'Content-Type: application/json' \
    --data '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
  ```

- **Too many requests.** An address that sends more than 120 requests in a minute is refused for the rest of that minute, with one line in the log. If the address in such a line is an assistant provider's, the bound is too low for the number of pilots; it is one number in `crates/server/src/accounts/mod.rs`.

  ```bash
  docker compose logs --since 1d server | grep 'connector: too many requests'
  ```

- **To take it off the internet**, remove the `handle /mcp` block from the `Caddyfile` and restart Caddy. The apps do not use it.

## Forecasts

The server has forecasts only from an [Open-Meteo](https://open-meteo.com/) instance you run yourself. Without one everything else works, `/v1/health` says `"forecasts": "not_configured"` and `/v1/forecast` answers `501`. The Compose file holds such an instance, switched off. To switch it on:

1. In `.env`, set

   ```
   VENTI_OPEN_METEO_URL=http://open-meteo:8080
   ```

2. Start it with the rest. From then on every `docker compose` command needs `--profile forecasts`, or put `COMPOSE_PROFILES=forecasts` in `.env`.

   ```bash
   docker compose --profile forecasts up -d
   ```

3. After a minute `/v1/health` says `"forecasts": "available"`, and the source `open-meteo` lists its models. Then ask for a place:

   ```bash
   curl 'https://your-site.example/v1/forecast?lat=47.7&lon=11.8'
   ```

The instance reads Open-Meteo's open data on AWS when a place is asked for, so the machine needs outbound HTTPS to `https://openmeteo.s3.amazonaws.com/data/`, and the first forecast for a place takes longer than the next. Open-Meteo's documentation asks for 8 to 16 GB of memory and fast disk; its cache is 8 GB unless `OPEN_METEO_CACHE_SIZE` says otherwise *(to verify on the first installation)*. An instance elsewhere works too: give its address as `VENTI_OPEN_METEO_URL` and leave the profile off. What is still to verify is in [AGENTS.md](AGENTS.md#forecasts).

With forecasts the server also makes its thermal map (`/v1/thermals`) by itself, from each run of ICON-D2 and of ICON-EU: 46 requests to the instance for each model every three hours for the first region, 92 in all, never more than half of what the server may ask in an hour. There is nothing to set. What it judged is written to the database (a few rows of some hundreds of kilobytes, replaced with every run) and read back at a start, so the map is there at once after a restart. Only the very first start answers `503` at `/v1/thermals` for a minute or two, until the first map is made. On 2026-11-21 an instance in remote-data mode read about 200 MB from the bucket for ICON-D2's part of the first whole map (ICON-EU's was not measured), and much less for one place's forecast; a run later reads the new run's files again *(to verify on the first installation how much)*.

## Map regions

The server hands out the map regions it finds in one folder, for the apps to download and draw without a network ([docs/api.md](../docs/api.md#maps)). While the folder is empty it hands out none. Background, estimates and what is still to verify are in [AGENTS.md](AGENTS.md#map-regions).

**Where they go.** Into `deploy/data/maps`, or the folder that `VENTI_MAPS_DIR` in `.env` names. Each region is a folder in it, with its files and a `map.toml` that describes them:

```
data/maps/
  alps-de-at/
    map.toml
    basemap.pmtiles
    terrain.pmtiles
```

The server must be able to read them: `chmod -R a+rX data/maps`.

**Build a region** with `tools/region-map/build.sh`, outside `data/maps`. The script's header says what it needs. It has not built a real region yet *(to verify)*.

```bash
cd /opt/venti
tools/region-map/build.sh regions/alps-de-at/region.toml /opt/venti-maps/alps-de-at
```

**Put it in place, or renew it.** A region is replaced whole, under a new `version`, which the script writes. Never change a region's files in place.

```bash
cd /opt/venti/deploy
mkdir -p data/maps
mv data/maps/alps-de-at /opt/venti-maps/alps-de-at.old    # when renewing: two folders with one id are both left out
mv /opt/venti-maps/alps-de-at data/maps/alps-de-at
docker compose restart server
docker compose logs --since 5m server | grep 'maps:'
curl -s http://localhost/v1/maps
```

After its start the server reads every file once, and lists a region when each file has the hash its `map.toml` states. A region it leaves out has a line in the log that says why. The restart breaks off the downloads that are under way; the apps go on with them afterwards, or start again when the version is new. Remove the old folder once the list shows the new version.

## What the web app's pages may do

Caddy sends the headers that keep the web app's pages out of frames and hold its workers to a list of what they may load; [apps/web/AGENTS.md](../apps/web/AGENTS.md#the-content-security-policy) has each entry and its reason. They were tried in Chromium only. If the web app's core does not start in Safari or Firefox (the logbook stays empty and the settings name no airspace), take the line that starts `header @scripts` out of the `Caddyfile`, run `docker compose restart caddy`, and report it.

## What each service may do

The services get less than Docker gives a container by default:

| | Caddy | Server | Database |
|---|---|---|---|
| Powers of root (`cap_drop`, `cap_add`) | listening on ports 80 and 443 | none | five, to hand its data folder to the user `postgres` |
| Gaining powers (`no-new-privileges`) | no | no | no |
| Writing outside its volumes (`read_only`) | no | no | no |
| Memory (`mem_limit`) | 256 MB | 1 GB | 2 GB |
| Processes and threads (`pids_limit`) | 256 | 1024 | 512 |

The memory limits fit the smallest machine above. On a larger one give the database more in `.env`, such as `VENTI_DB_MEMORY=16g` (and `VENTI_SERVER_MEMORY`, `VENTI_CADDY_MEMORY` likewise). A service that reaches its limit is restarted by Docker, as `docker compose ps` and `dmesg` show. The optional forecast service is not confined this way.

The database has two roles. `postgres` is its superuser, for you, with no password: `docker compose exec db psql --username postgres venti`. `venti` is the server's, owns the tables and is no superuser; `POSTGRES_PASSWORD` is its password. `db-init.sh` sets both up when the database is first created on an empty volume.

## Backup

`backup.sh` writes one dump of the whole database per run, checks it, and then deletes dumps older than 14 days (`VENTI_BACKUP_KEEP_DAYS`).

```bash
/opt/venti/deploy/backup.sh
ls -lh /opt/venti/deploy/backups
```

Run it every night with a systemd timer *(to verify)*. As root:

```ini
# /etc/systemd/system/venti-backup.service
[Unit]
Description=Venti database backup

[Service]
Type=oneshot
ExecStart=/opt/venti/deploy/backup.sh
```

```ini
# /etc/systemd/system/venti-backup.timer
[Unit]
Description=Nightly Venti database backup

[Timer]
OnCalendar=*-*-* 03:10:00
Persistent=true

[Install]
WantedBy=timers.target
```

```bash
systemctl daemon-reload
systemctl enable --now venti-backup.timer
systemctl list-timers venti-backup.timer
journalctl -u venti-backup.service --since yesterday
```

- **A dump is personal data**: every account with its password hash, every flight, every report with its reporter's words, every group. The script keeps the backup folder readable by its owner only; wherever you copy dumps to must be as careful.
- **Two things are left out on purpose**: live tracks, which the server removes 48 hours after a flight (`VENTI_LIVE_HOURS`), and the plans of groups. After a restore neither is there.
- **The dumps live on the same machine as the database.** They do not protect against losing the machine. Copy `deploy/backups` somewhere else, encrypted, by whatever means are at hand; where that copy goes is the owner's decision.
- **A deleted account lives on in older dumps**, fourteen days by default. If you restore a dump, delete again the accounts and flights that were deleted after it was taken.
- **Not in the dump**: the raw payload archive and the certificates, which come back by themselves, and `.env`: keep the database password in a password manager.

## Restore

Restoring replaces the whole database with one dump.

```bash
cd /opt/venti/deploy
ls -lh backups                                   # choose a dump
dump=backups/venti-20261119T031000Z.dump

docker compose stop server
docker compose exec -T db dropdb --username postgres --force venti
docker compose exec -T db createdb --username postgres --owner venti venti
docker compose exec -T db pg_restore --username postgres --dbname venti --exit-on-error < "$dump"
docker compose start server
curl -s http://localhost/v1/health
```

Everything after the dump is gone. The server refills the last three hours of stations on its next run, and the airspace is the dump's until the next daily run (`/v1/airspace` says how old it is). Accounts and flights uploaded after the dump are gone from the server, but a flight is still on the phone it came from. Pilots who signed in after the dump sign in again, and passwords changed after it are the old ones again.

On a new machine: do the first install up to step 3 with the old database password in `.env`, copy the dump over, run `docker compose up -d db` (which creates the two roles), then the restore commands above from `dropdb` on, then `docker compose up -d`.

Try a restore before it is needed, into a scratch database beside the live one:

```bash
docker compose exec -T db createdb --username postgres --owner venti restore_test
docker compose exec -T db pg_restore --username postgres --dbname restore_test --exit-on-error < "$dump"
docker compose exec -T db psql --username postgres --dbname restore_test --command \
  'SELECT (SELECT count(*) FROM observations) AS observations, (SELECT count(*) FROM accounts) AS accounts, (SELECT count(*) FROM flights) AS flights, (SELECT pg_size_pretty(sum(octet_length(file))) FROM flights) AS flight_files'
docker compose exec -T db dropdb --username postgres restore_test
```

## Looking inside

```bash
docker compose ps                                   # is everything up and healthy
docker compose logs --since 1h server               # one line per collection run; warnings name the failing requests
docker compose logs --since 1h caddy                # TLS and proxy problems
curl -s http://localhost/v1/health                  # per source: last success, last failure, stale or not
docker compose exec db psql --username venti venti  # the database itself
```

Useful queries:

```sql
-- The last runs of every source.
SELECT source, started_at, finished_at - started_at AS took, healthy,
       requests, failed_requests, stations, observations
FROM source_runs ORDER BY started_at DESC LIMIT 20;

-- How much is stored, per month.
SELECT date_trunc('month', time) AS month, count(*) FROM observations GROUP BY 1 ORDER BY 1;

-- The airspace the server holds and serves: per source, when it was fetched and how old its files are.
SELECT source, fetched_at, airspaces, file_date, newest_change FROM airspace_sets;

-- The last airspace runs. An unhealthy one left the stored set as it was.
SELECT source, started_at, healthy, requests, failed_requests, skipped, rejected, airspaces
FROM source_runs WHERE source = 'openaip' ORDER BY started_at DESC LIMIT 10;

-- Accounts, with how many flights each holds and how much room their files take.
SELECT a.handle, a.created_at, count(f.id) AS flights,
       pg_size_pretty(coalesce(sum(octet_length(f.file)), 0)) AS files
FROM accounts a LEFT JOIN flights f ON f.account_id = a.id
GROUP BY a.id ORDER BY a.created_at;

-- Invite codes: how many are waiting, used, or expired. The codes themselves are not in the database.
SELECT count(*) FILTER (WHERE used_at IS NULL AND (expires_at IS NULL OR expires_at > now())) AS waiting,
       count(*) FILTER (WHERE used_at IS NOT NULL) AS used,
       count(*) FILTER (WHERE used_at IS NULL AND expires_at <= now()) AS expired
FROM invites;
```

The log has a line when an account is created or deleted, when a password changes, for every wrong password, when an uploaded file had to be given up on, and once a day while reports are open. No line holds a password, a session token, anything of a report or anything of a group. Docker keeps at most 150 MB of logs for the three services.

When a source stays stale, run it by hand, outside the schedule and without touching the database, and read what it reports:

```bash
docker compose exec server venti-server ingest /app/regions/alps-de-at/region.toml
docker compose exec server venti-server ingest /app/regions/alps-de-at/region.toml --source openaip
```

Airspace (`openaip`) and flying sites (`osm-free-flying`) are fetched once a day. A failed run is tried again a day later, not sooner, also after a restart; meanwhile the server serves the last complete set, and after two failed days `/v1/health` reports the source as stale. An airspace the server could not read is named in the log (`left out`, `rejected`). If the Overpass instance for flying sites keeps refusing, name another in `VENTI_OVERPASS_URL` from the OpenStreetMap wiki's list (<https://wiki.openstreetmap.org/wiki/Overpass_API#Public_Overpass_API_instances>).

If the log says `the table of addresses that asked the connector is full: 100000 are being counted` (or the same of sign-in or new accounts), someone is sending requests from a very large number of addresses. The server keeps working under a shared limit; the machine then needs protection from outside it: the hosting company's, or a firewall rule for the networks the requests come from.

## Settings

All settings are described in `env.example`. The ones that deserve a second thought:

- **`VENTI_USER_AGENT`** is sent to the data providers with every request. Some ask operators to name a contact there; what to put in it is the owner's decision.
- **`VENTI_OVERPASS_URL`** is the Overpass API instance asked for flying sites. The main public one, the default, asks for few requests (the server sends one a day), a User-Agent that names the application, and an operator's e-mail address that is easy to find on the application's website.
- **`VENTI_OPEN_METEO_URL`**: see [Forecasts](#forecasts). **`VENTI_MAPS_DIR`**: see [Map regions](#map-regions). **`VENTI_REGISTRATION`**: see [Accounts](#accounts). **`VENTI_NETWORK`**: see [The visitor's address](#the-visitors-address).
- **`VENTI_PUBLIC_URL`** is the address people reach the site at. Shared flights' pages and preview pictures are named with it, and chat apps fetch previews only from a complete, correct address. It is also the one origin whose pages may call the connector from a browser.
- **`VENTI_CORS_ORIGINS`** is only needed when a web app on another origin calls the API, or the connector, from a browser.
- **`VENTI_OBSERVATION_MONTHS`** is how many whole calendar months of observations are kept. Lowering it deletes data at the next daily cleanup.
- **`VENTI_ACCOUNT_FLIGHTS`** and **`VENTI_ACCOUNT_STORAGE_MB`** bound what one account keeps: 25,000 flights and 4,096 MB of flight files unless set, about 1.4 GB in the database when full. Lowering them deletes nothing; an account above them cannot add more.
- **`VENTI_REPORT_DAYS`** (90) and **`VENTI_PLAN_DAYS`** (2, at least 1) are how long a closed report, and a plan after its day, are kept. Lowering them removes what is older at the next cleanup; raising them brings nothing back.

The server can also read a TOML file instead of variables (`crates/server/config.example.toml`); the Compose setup does not use it.
