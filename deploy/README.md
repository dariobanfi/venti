# Running Venti on a server

Last revised: 2026-11-19

This folder holds everything needed to run Venti on one Linux machine with Docker Compose:

| File | What it is |
|---|---|
| `docker-compose.yml` | The three services: Caddy, the Venti server, PostgreSQL with PostGIS |
| `Dockerfile` | Builds the `venti-server` image from this repository |
| `Caddyfile` | TLS, the web app's static files, map tiles, and the way in to the API |
| `env.example` | Every setting, with comments. Copy it to `.env` |
| `backup.sh` | Dumps the database and removes old dumps |

Nothing here has run on a real server yet: the project has none. The stack was built and exercised on a development machine on 2026-11-19 (image build, start, API through Caddy, backup, restore into an empty database). Steps that could not be tried there are marked *(to verify)*.

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

   Set `POSTGRES_PASSWORD`. Set `VENTI_SITE_ADDRESS` to the domain name, or leave `:80` to try the stack without one. Everything else has a working default. `.env` holds the database password: it is ignored by git and must stay out of any repository, chat or ticket.

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

   Within a few seconds of the first start the log shows one line per data source with the number of stations and observations it delivered, and the health answer lists each source with `"stale": false`. The database schema is created by the server on its first start; there is no separate migration step.

6. **Schedule the backup**; see [Backup](#backup). An install without a backup is not finished.

### What ends up where

| Data | Place | Lose it and |
|---|---|---|
| Database | Docker volume `venti_db-data` | Observations and run history are gone; the server refills the last hours by itself |
| Raw provider payloads, about 240 MB a day for the first region and kept for 14 days | Docker volume `venti_archive` | Nothing breaks; old parsing bugs can no longer be replayed |
| TLS certificates | Docker volume `venti_caddy-data` | Caddy asks for new ones, which the issuers rate-limit |
| Web app, map tiles | `deploy/data/web`, `deploy/data/tiles` (or the folders named in `.env`) | Copy them there again |
| Settings | `deploy/.env` | Write them again; the database password must match the one the database was created with |
| Database dumps | `deploy/backups` (or `VENTI_BACKUP_DIR`) | |

Neither the web app nor map tiles exist yet. Until they do, the site's front page is empty and only `/v1` answers. Files put into `data/tiles` are served under `/tiles/` with HTTP range requests, which is what PMTiles archives need.

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

## Backup

`backup.sh` writes one dump of the whole database per run and deletes dumps older than 14 days (`VENTI_BACKUP_KEEP_DAYS`). It checks the new dump before it deletes anything.

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

**These dumps live on the same machine as the database.** They protect against a bad migration or a mistaken delete. They do not protect against losing the machine. The architecture calls for an encrypted copy at a different site; where that copy goes, and what it may cost, is the owner's decision and is not set up here. Until it is, copy `deploy/backups` somewhere else by whatever means are at hand.

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

The restored database lacks everything after the dump. The server refills the last three hours of the German stations on its next run and continues from there; the gap in between stays.

On a new machine: do the first install up to step 3 with the old database password in `.env`, copy the dump over, run `docker compose up -d db`, then the restore commands above from `createdb` on (the fresh database is empty, so drop it first all the same), then `docker compose up -d`.

Try a restore before it is needed. A dump can be restored into a scratch database on the same server without touching the live one:

```bash
docker compose exec -T db createdb --username venti restore_test
docker compose exec -T db pg_restore --username venti --dbname restore_test --no-owner --exit-on-error < "$dump"
docker compose exec -T db psql --username venti --dbname restore_test --command 'SELECT count(*) FROM observations'
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
```

When a source stays stale: run it by hand, outside the schedule and without touching the database, and read what it reports.

```bash
docker compose exec server venti-server ingest /app/regions/alps-de-at/region.toml
```

## Settings

All settings are described in `env.example`. The ones that deserve a second thought:

- **`VENTI_USER_AGENT`** is sent to the data providers with every request. Some providers ask operators to name a contact there. What to put in it is the owner's decision; the default names only the program and its version.
- **`VENTI_OBSERVATION_MONTHS`** is how many whole calendar months of observations are kept. Older months are dropped once a day. Lowering it deletes data at the next cleanup.
- **`VENTI_CORS_ORIGINS`** is only needed when a web app on a *different* origin calls the API from a browser.

The server can also read a TOML file instead of variables; see `crates/server/config.example.toml`. The Compose setup does not use it.

## On a development machine

No server is needed to work on the server. The development mode needs nothing at all:

```bash
cargo run -p venti-server -- dev          # the API over recorded data on http://127.0.0.1:8080
```

For the real thing, and for the store's tests against PostgreSQL, start a database in a container. The password below is for a throwaway database on the loopback address; it is not a secret.

```bash
docker run -d --name venti-db -e POSTGRES_USER=venti -e POSTGRES_PASSWORD=venti -e POSTGRES_DB=venti \
  -p 127.0.0.1:5432:5432 postgis/postgis:16-3.4

# The store's behavioural suite against PostgreSQL. Without the variable these tests are skipped.
VENTI_TEST_DATABASE_URL=postgres://venti:venti@127.0.0.1:5432/venti cargo test -p venti-store

# The server against the live providers.
VENTI_DATABASE_URL=postgres://venti:venti@127.0.0.1:5432/venti \
VENTI_REGIONS=regions/alps-de-at/region.toml \
  cargo run -p venti-server -- serve

# The image a server runs.
docker build -f deploy/Dockerfile -t venti-server .
```

Two things to know in a cloud session for AI coding: the Docker daemon may have to be started first (`dockerd &`), and Docker Hub may refuse pulls with "429 Too Many Requests", in which case `mirror.gcr.io/library/<image>` serves the same official images (seen on 2026-11-19).
