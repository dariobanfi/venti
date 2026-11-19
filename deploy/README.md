# Running Venti on a server

Last revised: 2026-11-19

This folder holds everything needed to run Venti on one Linux machine with Docker Compose:

| File | What it is |
|---|---|
| `docker-compose.yml` | The three services: Caddy, the Venti server, PostgreSQL with PostGIS |
| `Dockerfile` | Builds the `venti-server` image from this repository |
| `Caddyfile` | TLS, the web app's static files, map tiles, and the way in to the API, to the pages of shared flights and to the connector for assistants |
| `env.example` | Every setting, with comments. Copy it to `.env` |
| `backup.sh` | Dumps the database and removes old dumps |

Since 2026-11-19 the server keeps pilots' accounts and flights, beside the weather it collects. That changes what the machine is: the database now holds what cannot be made again, and what belongs to other people. The backup is no longer a convenience.

Nothing here has run on a real server yet: the project has none. The stack was built and exercised on a development machine on 2026-11-19 (image build, start, API through Caddy, backup, restore into an empty database), and again the same day with accounts: an invite, an account, a flight and its share page through Caddy, a backup, and a restore that brought the account and the flight file back byte for byte. That second run used the server's debug build inside a plain container, not the image; the image itself is built and started by CI. Steps that could not be tried are marked *(to verify)*.

No image is published anywhere. The server builds its own image from a checkout of the repository. Publishing images to a registry is the owner's decision; see [architecture.md](../docs/architecture.md#deployment).

## What you need

- A machine with Debian 12 or 13, or Ubuntu 24.04 or 26.04 LTS, and root access. These are the releases Docker's install pages listed on 2026-11-19. The recommended size is in [architecture.md](../docs/architecture.md#sizing); for wind stations alone 2 cores, 4 GB of memory and 20 GB of disk are plenty.
- Ports 80 and 443 open to the internet.
- For HTTPS: a domain name whose DNS record points at the machine. Without one the stack serves plain HTTP on port 80, which is fine for a first look and for nothing else.

## First install

All commands run on the server.

1. **Install Docker Engine** with the Compose plugin (`docker-ce`, `docker-compose-plugin`) from Docker's own package repository, following <https://docs.docker.com/engine/install/debian/> or <https://docs.docker.com/engine/install/ubuntu/>. Those pages say to remove the distribution's own `docker.io` package first. Check with:

   ```bash
   docker --version && docker compose version
   ```

2. **Get the code.** `/opt/venti` is used throughout; any folder works.

   ```bash
   sudo mkdir -p /opt/venti && sudo chown "$USER" /opt/venti
   git clone https://github.com/dariobanfi/venti.git /opt/venti
   cd /opt/venti/deploy
   ```

3. **Write the settings.**

   ```bash
   cp env.example .env
   chmod 600 .env
   openssl rand -hex 24      # paste the output as POSTGRES_PASSWORD
   nano .env
   ```

   Set `POSTGRES_PASSWORD`. Set `VENTI_SITE_ADDRESS` to the domain name, or leave `:80` to try the stack without one. With a domain, set `VENTI_PUBLIC_URL` to the address people will use, such as `https://venti.example`: links to shared flights are built from it. Everything else has a working default. `.env` holds the database password: it is ignored by git and must stay out of any repository, chat or ticket.

4. **Build and start.**

   ```bash
   docker compose build
   docker compose up -d
   ```

   The first build compiles the server and takes several minutes. Later builds reuse the compiled dependencies.

5. **Check.**

   ```bash
   docker compose ps
   docker compose logs --tail 20 server
   curl -s http://localhost/v1/health        # with a domain: https://<domain>/v1/health
   ```

   Within a few seconds of the first start the log shows one line per data source with what it delivered: stations and observations for the weather sources, airspaces for openAIP. The health answer lists each source with `"stale": false`. The database schema is created by the server on its first start; there is no separate migration step.

6. **Schedule the backup**; see [Backup](#backup). An install without a backup is not finished.

7. **Invite the first pilot.** Nobody can create an account until you hand out an invite code; see [Accounts](#accounts).

### What ends up where

| Data | Place | Lose it and |
|---|---|---|
| Database | Docker volume `venti_db-data` | Every account and every pilot's flights are gone for good, unless there is a dump. Observations and run history are gone too; those the server refills for the last hours by itself, and the airspace with its first run after the start |
| Raw provider payloads, about 240 MB a day for the first region and kept for 14 days | Docker volume `venti_archive` | Nothing breaks; old parsing bugs can no longer be replayed |
| TLS certificates | Docker volume `venti_caddy-data` | Caddy asks for new ones, which the issuers rate-limit |
| Web app, map tiles | `deploy/data/web`, `deploy/data/tiles` (or the folders named in `.env`) | Copy them there again |
| Settings | `deploy/.env` | Write them again; the database password must match the one the database was created with |
| Database dumps | `deploy/backups` (or `VENTI_BACKUP_DIR`) | There is nothing to restore from |

Flight files are in the database, not in a folder: one dump holds everything. Map tiles do not exist yet, and the web app is not published. Until they are, the site's front page is empty and only `/v1` and the pages of shared flights under `/f/` answer. Files put into `data/tiles` are served under `/tiles/` with HTTP range requests, which is what PMTiles archives need.

## Update

```bash
cd /opt/venti
git pull
cd deploy
./backup.sh
docker compose build server
docker compose up -d
docker compose logs --tail 20 server
curl -s http://localhost/v1/health
```

The server upgrades the database schema when it starts. A newer schema is not readable by an older server: to go back to an older version after a schema change, restore the dump taken before the update.

`docker compose up -d` restarts only what changed. The API is unreachable for a few seconds while the server restarts; a collection run that was in flight is dropped and repeated.

To update PostgreSQL, Caddy or PostGIS, change the image tag in `docker-compose.yml` in the repository, not on the server. A new major version of PostgreSQL cannot read the old data folder: dump with the old version, start the new one on an empty volume, restore *(to verify when it first comes up)*.

## Accounts

Who may create an account is the setting `VENTI_REGISTRATION`:

| Value | Meaning |
|---|---|
| `invite` | Whoever has an invite code. This is the default |
| `open` | Anyone |
| `closed` | Nobody. Existing accounts keep working |

Make invite codes on the server:

```bash
cd /opt/venti/deploy
docker compose exec server venti-server invite                              # one code, good until used
docker compose exec server venti-server invite --count 5 --expires-in-days 14
```

Each code creates one account. A code is printed once and then known only to whoever you give it to: the database keeps a hash of it. If one gets lost, make another.

What to know as the person who runs this:

- **A forgotten password cannot be recovered**, by the pilot or by you. The database holds a hash from which the password cannot be read back, and there is no email address to send a link to. If a pilot is locked out for good, the only way on is a new account and a fresh upload of their flights from their phone.
- **A pilot deletes their own account** in the app, with their password. It is gone from the database at once, with its flights. It is still in the dumps taken before, until they rotate out (see [Backup](#backup)). If someone asks you to delete an account for them and cannot do it themselves, that is a manual `DELETE FROM accounts WHERE handle = '...'` in the database; everything that belongs to the account goes with it.
- **There is no way yet to report or take down a shared flight** other than by hand: `UPDATE flights SET visibility = 'private' WHERE id = '...'` hides one. Keep registration by invite until that exists.
- **Attempts to sign in are counted** per account and per visitor's address, in the server's memory: restarting the server forgets the counts. The log has a line for every wrong password, with the address it came from and never the password.

### The visitor's address

The server counts by the address a request came from. Every request reaches it through Caddy, so it asks Caddy: Caddy writes the visitor's address into the `X-Forwarded-For` header, and the server believes that header from Caddy's address in the private network and from nowhere else (`VENTI_TRUSTED_PROXIES`, which `docker-compose.yml` sets). Caddy therefore has a fixed address there, `172.30.250.2`. If the range `172.30.250.0/24` is taken on the machine, set `VENTI_NETWORK` in `.env` to the first three numbers of a free one.

Check once on a new server that the address is really the visitor's *(to verify on a real server)*: sign in with a wrong password from a phone on mobile data and read the log.

```bash
docker compose logs --since 5m server | grep 'wrong password\|nobody has'
```

`client=` should show the phone's public address. If it shows an address that starts with `172.`, Docker is hiding the visitors' addresses from Caddy. On the development machine it did so for requests from the machine itself; whether it does for some real visitors, for example those who arrive over IPv6, has to be seen on the real server. Where it happens, every visitor counts as one, and thirty wrong passwords by anyone lock everyone out of signing in for an hour. Fix the Docker setup before letting people in.

## The connector

The server answers a pilot's own Claude or ChatGPT at `https://<domain>/mcp`: wind stations and airspace, for reading, without sign-in. [docs/mcp.md](../docs/mcp.md) says what it offers, how a pilot adds it and what the server learns from it. It is on as soon as the server runs. There is nothing to set.

- **It needs the domain and HTTPS.** Claude reaches a connector from Anthropic's servers, over HTTPS on port 443 and no other port (Anthropic's help page, read 2026-11-19). On `:80` without a domain only programs on your own network can use it.
- **Check after the install** *(to verify on a real server)*. This lists three tools:

  ```bash
  curl -s https://<domain>/mcp --header 'Content-Type: application/json' \
    --data '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
  ```

  Then add the address in Claude as [docs/mcp.md](../docs/mcp.md#adding-it-to-an-assistant) describes and ask it for the wind somewhere. Nobody has done that yet.
- **What it writes to the log.** Nothing for an ordinary request. One line when an address has sent more than 120 requests in a minute; it is then refused until the minute is over, and however often it asks again in that minute, there is no second line:

  ```bash
  docker compose logs --since 1d server | grep 'connector: too many requests'
  ```

  An assistant's provider sends many pilots' questions from few addresses. If the address in such a line is a provider's, the bound is too low for the number of pilots, and it is one number in `crates/server/src/accounts/mod.rs`.
- **The visitor's address matters here as it does for sign-in.** If Docker hides the visitors' addresses (see above), everyone counts as one address, and 120 requests a minute is all the connector answers.
- **Web pages.** A page in a browser may call the connector only from `VENTI_PUBLIC_URL` and the origins in `VENTI_CORS_ORIGINS`. Assistants are not affected: they do not call from a browser.
- **No sign-in.** An assistant that looks for a sign-in service at `/.well-known/oauth-...` gets `404` from the server, which is the answer that there is none.
- **Taking it off the internet:** remove the `handle /mcp` block from the `Caddyfile` and restart Caddy. The API and the apps do not use it.

## Backup

`backup.sh` writes one dump of the whole database per run and deletes dumps older than 14 days (`VENTI_BACKUP_KEEP_DAYS`). It checks the new dump before it deletes anything.

The dump holds everything the server knows: the weather it collected, and every account with its password hash, its signed-in devices and its flights with their files. There is nothing else to back up for these; flight files are not kept in a folder. That also means a dump is personal data. The script makes the backup folder readable by its owner only; wherever you copy dumps to has to be as careful.

```bash
/opt/venti/deploy/backup.sh
ls -lh /opt/venti/deploy/backups
```

Run it every night with a systemd timer. As root:

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

The timer units were not run on a real server *(to verify)*; the script itself was.

**These dumps live on the same machine as the database.** They protect against a bad migration or a mistaken delete. They do not protect against losing the machine, and with it every pilot's flights. The architecture calls for an encrypted copy at a different site; where that copy goes, and what it may cost, is the owner's decision and is not set up here. Until it is, copy `deploy/backups` somewhere else by whatever means are at hand, encrypted.

**A deleted account lives on in the dumps** taken before it was deleted, for as long as dumps are kept: fourteen days by default. The API document tells pilots so. If you restore a dump, accounts and flights that were deleted after it was taken come back; delete them again.

What is not in the dump: the raw payload archive (replaceable), the certificates (replaceable), `.env` (keep a copy of the database password in a password manager).

## Restore

Restoring replaces the whole database with the content of one dump.

```bash
cd /opt/venti/deploy
ls -lh backups                                   # choose a dump
dump=backups/venti-20261119T031000Z.dump

docker compose stop server
docker compose exec -T db dropdb --username venti --force venti
docker compose exec -T db createdb --username venti venti
docker compose exec -T db pg_restore --username venti --dbname venti --no-owner --exit-on-error < "$dump"
docker compose start server
curl -s http://localhost/v1/health
```

The restored database lacks everything after the dump. The server refills the last three hours of the German stations on its next run and continues from there; the gap in between stays. The airspace is the dump's until the next daily run replaces it; `/v1/airspace` says how old it is.

For accounts and flights: accounts created after the dump are gone, and so are flights uploaded after it. A flight is still on the phone it was uploaded from. The apps are meant to notice that the server has gone back in time (the server tells a device whose bookmark lies in the future to compare its whole logbook) and to upload what is missing; that is built on the server and in no app yet. Sessions made after the dump stop working, and those pilots sign in again. Passwords changed after the dump are the old ones again.

On a new machine: do the first install up to step 3 with the old database password in `.env`, copy the dump over, run `docker compose up -d db`, then the restore commands above from `createdb` on (the fresh database is empty, so drop it first all the same), then `docker compose up -d`.

Try a restore before it is needed. A dump can be restored into a scratch database on the same server without touching the live one:

```bash
docker compose exec -T db createdb --username venti restore_test
docker compose exec -T db pg_restore --username venti --dbname restore_test --no-owner --exit-on-error < "$dump"
docker compose exec -T db psql --username venti --dbname restore_test --command \
  'SELECT (SELECT count(*) FROM observations) AS observations, (SELECT count(*) FROM accounts) AS accounts, (SELECT count(*) FROM flights) AS flights, (SELECT pg_size_pretty(sum(octet_length(file))) FROM flights) AS flight_files'
docker compose exec -T db dropdb --username venti restore_test
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

The log also has a line when an account is created or deleted, when a password is changed, for every wrong password, and when an uploaded file had to be given up on. No line holds a password or a session token.

**How much log there is.** Docker keeps five files of ten megabytes for each of the three services and drops the oldest (`logging:` in `docker-compose.yml`), so the logs take 150 MB at most and `docker compose logs` reaches back as far as that goes. The server itself writes nothing per request. A refusal for too many requests is one line per address and window. A failure that repeats with every request (the database is away, the server is busy, requests run out of time) is written once, and then as one line every ten seconds that says how many more there were. The server's tests hold it to that: 2,500 refused requests and 40 that time out may leave 4 kB of log, and leave a handful of lines. Caddy's own log holds no line per request either, because the `Caddyfile` has no `log` directive; if you add one, the rotation above still bounds it.

When a source stays stale: run it by hand, outside the schedule and without touching the database, and read what it reports.

```bash
docker compose exec server venti-server ingest /app/regions/alps-de-at/region.toml
docker compose exec server venti-server ingest /app/regions/alps-de-at/region.toml --source openaip
```

Airspace is fetched once a day, one file per country, and a run in which any file did not arrive fails as a whole; the hand run above names the country. A run that fails is tried again a day later, not sooner. While it fails the server keeps serving the last complete set, and `/v1/airspace` says when that was fetched. After two failed days `/v1/health` reports the source as stale. Restarting the server does not bring the next run forward: it waits out what is left of the day. An airspace the provider's file holds and the server could not read is named in the log (`left out`, `rejected`); the run stays healthy, and that airspace is missing from what pilots get.

Flying sites follow the same rules, with one request a day to an Overpass API instance (`--source osm-free-flying`). The public instances are shared and sometimes refuse a server or run out of time; the hand run says which. If the main one keeps refusing, name another in `VENTI_OVERPASS_URL`: the OpenStreetMap wiki lists them with what each asks of its users (<https://wiki.openstreetmap.org/wiki/Overpass_API#Public_Overpass_API_instances>).

## Settings

All settings are described in `env.example`. The ones that deserve a second thought:

- **`VENTI_USER_AGENT`** is sent to the data providers with every request. Some providers ask operators to name a contact there. What to put in it is the owner's decision; the default names only the program and its version.
- **`VENTI_OVERPASS_URL`** is the Overpass API instance asked for OpenStreetMap's flying sites. The default is the main public one, whose operator asks for few requests (the server sends one a day), a User-Agent that names the application, and an e-mail address of the operator that is easy to find on the application's website.
- **`VENTI_OBSERVATION_MONTHS`** is how many whole calendar months of observations are kept. Older months are dropped once a day. Lowering it deletes data at the next cleanup.
- **`VENTI_CORS_ORIGINS`** is only needed when a web app on a *different* origin calls the API from a browser. The same origins may call the connector from a browser.
- **`VENTI_PUBLIC_URL`** is the address people reach the site at. The page of a shared flight names itself and its preview picture with it, and a chat app can only fetch a preview from a complete, correct address. Without it the server falls back to the address it listens on and says so in its log at start. It is also the one address whose pages may call the connector from a browser without being named in `VENTI_CORS_ORIGINS`.
- **`VENTI_REGISTRATION`** is who may create an account; see [Accounts](#accounts).
- **`VENTI_NETWORK`** moves the services' private network; see [The visitor's address](#the-visitors-address).

Not settings, because nobody should have to tune them: how many wrong passwords are tolerated (10 an hour for one account, 30 an hour from one address), how many accounts one address may try to create (10 an hour), how large a flight file may be (4 MB) and how many one account may upload (1000 a day), how long an unused session lasts (400 days), and how much the connector takes (64 kB a request, 10 seconds for an answer, 120 requests a minute from one address). They are in `crates/server/src/accounts/mod.rs` and `crates/server/src/api/mcp.rs` with their reasons.

The server can also read a TOML file instead of variables; see `crates/server/config.example.toml`. The Compose setup does not use it.

## On a development machine

No server is needed to work on the server. The development mode needs nothing at all:

```bash
cargo run -p venti-server -- dev          # the API over recorded data on http://127.0.0.1:8080
```

It starts with a demo account and three flights; its handle, password and a token that always works are in [docs/api.md](../docs/api.md#try-it).

For the real thing, and for the store's tests against PostgreSQL, start a database in a container. The password below is for a throwaway database on the loopback address; it is not a secret.

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

Two things to know in a cloud session for AI coding: the Docker daemon may have to be started first (`dockerd &`), and Docker Hub may refuse pulls with "429 Too Many Requests", in which case `mirror.gcr.io/library/<image>` serves the same official images (seen on 2026-11-19).
