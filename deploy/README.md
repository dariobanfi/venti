# Running Venti on a server

Last revised: 2026-11-20

This folder holds everything needed to run Venti on one Linux machine with Docker Compose:

| File | What it is |
|---|---|
| `docker-compose.yml` | The three services: Caddy, the Venti server, PostgreSQL with PostGIS |
| `Dockerfile` | Builds the `venti-server` image from this repository |
| `Caddyfile` | TLS, the web app's static files, map tiles, and the way in to the API, to the map regions the server hands out, to the pages of shared flights and to the connector for assistants |
| `env.example` | Every setting, with comments. Copy it to `.env` |
| `db-init.sh` | Run once by the database when it is first created: makes the role the server signs in as |
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

   Set `POSTGRES_PASSWORD`. Set `VENTI_SITE_ADDRESS` to the domain name, or leave `:80` to try the stack without one. **With `:80` nothing is encrypted**, so `env.example` keeps `VENTI_REGISTRATION=closed`: no accounts over plain HTTP. With a domain, set `VENTI_REGISTRATION=invite` as well. With a domain, set `VENTI_PUBLIC_URL` to the address people will use, such as `https://venti.example`: links to shared flights are built from it. Everything else has a working default. `.env` holds the database password: it is ignored by git and must stay out of any repository, chat or ticket.

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

7. **Invite the first pilot**, once the site has its domain name and HTTPS and `.env` says `VENTI_REGISTRATION=invite`. Nobody can create an account until you hand out an invite code; see [Accounts](#accounts). A server that takes accounts at a plain `http://` address says so in its log at start, in capitals: `docker compose logs server | grep 'PLAIN HTTP'` should find nothing.

### What ends up where

| Data | Place | Lose it and |
|---|---|---|
| Database | Docker volume `venti_db-data` | Every account and every pilot's flights are gone for good, unless there is a dump. Observations and run history are gone too; those the server refills for the last hours by itself, and the airspace with its first run after the start |
| Raw provider payloads, about 240 MB a day for the first region and kept for 14 days | Docker volume `venti_archive` | Nothing breaks; old parsing bugs can no longer be replayed |
| TLS certificates | Docker volume `venti_caddy-data` | Caddy asks for new ones, which the issuers rate-limit |
| Web app, map tiles | `deploy/data/web`, `deploy/data/tiles` (or the folders named in `.env`) | Copy them there again |
| Map regions for download | `deploy/data/maps` (or `VENTI_MAPS_DIR`) | The server hands out none until they are copied there again, or built again; see [Map regions](#map-regions) |
| Settings | `deploy/.env` | Write them again; the database password must match the one the database was created with |
| Database dumps | `deploy/backups` (or `VENTI_BACKUP_DIR`) | There is nothing to restore from |

Flight files are in the database, not in a folder: one dump holds everything. Map tiles do not exist yet, and the web app is not published. Until they are, the site's front page is empty and only `/v1`, the pages of shared flights under `/f/` and the pages of live tracks under `/live/` answer. Files put into `data/tiles` are served under `/tiles/` with HTTP range requests, which is what PMTiles archives need. That folder is for a map that reads its tiles piece by piece over the network. What an app downloads whole, to draw without a network, is in `data/maps` and goes out through the server: [Map regions](#map-regions).

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

`docker compose up -d` restarts only what changed. The API is unreachable for a few seconds while the server restarts; a collection run that was in flight is dropped and repeated. A download of a map region that was under way breaks off, and the app goes on with it afterwards.

To update PostgreSQL, Caddy or PostGIS, change the image tag in `docker-compose.yml` in the repository, not on the server. A new major version of PostgreSQL cannot read the old data folder: dump with the old version, start the new one on an empty volume, restore *(to verify when it first comes up)*.

## Accounts

Who may create an account is the setting `VENTI_REGISTRATION`:

| Value | Meaning |
|---|---|
| `invite` | Whoever has an invite code. This is what the server does when the setting is empty |
| `open` | Anyone |
| `closed` | Nobody. Existing accounts keep working. This is what `env.example` starts with, because it also starts without HTTPS |

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
- **Pilots can report what others share, and you can take it down**: see [Reports and taking things down](#reports-and-taking-things-down). Nothing there acts by itself: a report waits until you read it. Do not hide a flight with an `UPDATE` by hand any more; its pilot would simply share it again.
- **Attempts to sign in are counted** per account and per visitor's address, in the server's memory: restarting the server forgets the counts. The log has a line for every wrong password, with the address it came from and never the password.
- **Anyone can close signing in to an account for an hour** by sending ten wrong passwords for its handle, and can keep it closed by doing so every hour. That is the price of stopping guesses from many addresses, and it is the owner's decision ([architecture.md](../docs/architecture.md#decision-log)). A pilot who is already signed in on a device notices nothing and can still change the password or delete the account there. A pilot who is locked out of a new device this way has to wait, or you restart the server, which forgets the counts: `docker compose restart server`. The log shows who is guessing: `docker compose logs --since 2h server | grep 'wrong password'`.

### The visitor's address

The server counts by the address a request came from. Every request reaches it through Caddy, so it asks Caddy: Caddy writes the visitor's address into the `X-Forwarded-For` header, and the server believes that header from Caddy's address in the private network and from nowhere else (`VENTI_TRUSTED_PROXIES`, which `docker-compose.yml` sets). Caddy therefore has a fixed address there, `172.30.250.2`. If the range `172.30.250.0/24` is taken on the machine, set `VENTI_NETWORK` in `.env` to the first three numbers of a free one.

**IPv4 only, for now.** `docker-compose.yml` publishes ports 80 and 443 on the machine's IPv4 addresses and not on its IPv6 ones. The services' network has no IPv6, and Docker would otherwise take IPv6 connections itself and pass them on over IPv4: its documentation says a port published without a host address is mapped from the host's IPv6 addresses to the container's IPv4 address by its "userland proxy" (docs.docker.com, "Port publishing and mapping", read 2026-11-19). Caddy would then see the same private address for every IPv6 visitor, the server would count them as one, and thirty wrong passwords from any of them would close sign-in for all of them. So:

- **Give the domain an `A` record and no `AAAA` record.** Visitors on a network that has only IPv6, as some mobile networks do, are expected to reach the site all the same, through their provider's translation to IPv4 *(to verify from such a phone)*. With an `AAAA` record and the ports as they are, IPv6 visitors would get no answer on that address and depend on their browser trying IPv4 next.
- **Serving IPv6 properly** *(to verify; nobody has done it with this stack)* needs four things together: an IPv6 network for the services (`enable_ipv6: true` and an IPv6 subnet under `networks.default.ipam` in `docker-compose.yml`, per Docker's page "Use IPv6 networking", read 2026-11-19); a fixed IPv6 address for Caddy there, beside its IPv4 one; that address added to `VENTI_TRUSTED_PROXIES`; and the ports published without the `0.0.0.0`. Then the check below, from a phone on a mobile network that has IPv6, before the `AAAA` record goes in.

**Check once on a new server that the address is really the visitor's** *(to verify on a real server)*, in three places.

1. The server's log says at start whose word it takes:

   ```bash
   docker compose logs server | grep 'client addresses'
   ```

   It should read `X-Forwarded-For is believed from 172.30.250.2 and from nowhere else`. If it says that no proxy is trusted, `VENTI_TRUSTED_PROXIES` did not arrive.

2. Sign in with a wrong password from a phone on mobile data, with Wi-Fi off, and read the log:

   ```bash
   docker compose logs --since 5m server | grep 'wrong password\|nobody has'
   ```

   `client=` should show the phone's public address (the phone's browser shows it at any "what is my IP" page). If it shows an address that starts with `172.`, Docker is hiding the visitors' addresses from Caddy. On the development machine it did so for requests from the machine itself, which is harmless.

3. After a day of use, ask the server what it has seen:

   ```bash
   curl -s https://<domain>/v1/health | grep -o '"client_addresses":"[a-z_]*"'
   ```

   `"seen"` is right. `"hidden"` means that in the last hour twenty or more requests, and at least a tenth of all, came from one private address and from no other private one; the log has a line then that names the address (`does not see its visitors' addresses`). Nothing at all means fewer than twenty requests have arrived. A server that one person uses at home, on a private network, says `"hidden"` and is fine.

Where the addresses are hidden, every visitor counts as one: thirty wrong passwords by anyone lock everyone out of signing in for an hour, and the connector answers 120 requests a minute in total. Fix the setup before letting people in.

## Reports and taking things down

A pilot who is signed in can report to you a flight, a pilot's page or a live track, with one of six reasons and some words of their own ([api.md](../docs/api.md#reports)). The server stores the report and does nothing else. You read the reports and decide, with the commands below. There is no web page for any of this, and the API offers none of it: whoever can run `venti-server` on the server's machine, with the server's database, is the operator.

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

Each command does one thing, says in words what it did and what it did not, and takes effect at once, while the server runs. A command that was pointed at something that is not there says so and ends with a failure status, so that a script notices. None deletes a flight or an account. None closes a report: taking a flight down and closing the report about it are two commands, in whichever order.

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
| `reports` | Prints the open reports, oldest first. For each: what is reported and whose it is; what that is now (a flight: public, shared by link, private, taken down or deleted; a live track: on everyone's map, for those with its link, or gone); the link to look at it, when `VENTI_PUBLIC_URL` is set; the reason; the reporter's words; the reporter's handle. `--all` adds the closed reports with their notes | Changes nothing, and writes nothing to the log |
| `close-report <ID> --note TEXT` | Closes a report with your note of 1 to 1,000 characters: what you did, or why nothing. The report leaves the list of open ones. The same pilot can then report the same thing again | Nothing about what was reported. It tells neither the reporter nor the reported pilot |
| `take-down <FLIGHT>` | Makes one flight private, whatever its pilot had set, and keeps it so. Nobody but its pilot sees it, its link shows the page for no flight, and it is in no list. Its pilot cannot share it again, also not by deleting it and uploading the same file. The server tells the pilot's apps that the flight was taken down, and since when | It does not delete the flight or its file: the pilot keeps both. It does not say why: the server does not know. It does not touch the pilot's other flights. It does not stop the pilot from uploading the track as a changed file, which is another flight to the server: for that, suspend the account's publishing |
| `release <FLIGHT>` | Undoes `take-down`: the pilot may share the flight again | It shares nothing. The flight stays private until its pilot changes that |
| `remove-live-track <TRACK>` | Deletes one live track with its positions and takes its identifier out of use, as if its pilot had deleted it. Its link shows no track from then on | It does not stop the pilot's app, which is told that the identifier is used up and goes on under a new one, with a new link. To keep an account's gliders off everyone's map, suspend its publishing. A removed track cannot be brought back |
| `suspend-publishing <HANDLE>` | Takes from an account the ability to publish. Its public flights become shared by link, and so do its public live tracks. From then on it can make no flight public, and every live track it sends is shared by link, whatever its app asks for. The server tells the pilot's apps that publishing is suspended, and since when | It does not delete the account or sign it out, and the account goes on uploading, sharing by link and sending live tracks. It takes nothing away from those who have a link: for a flight use `take-down`, for a live track `remove-live-track`. It does not keep the person from making another account where registration allows it |
| `resume-publishing <HANDLE>` | Gives the ability back | It makes nothing public again. What the suspension made shared by link stays so until its pilot publishes it |

What to know as the person who runs this:

- **A report is one pilot's word about another.** The server checked nothing of it, and anyone with an account can send one. Look at what was reported before you act.
- **What `reports` prints is for your eyes.** It names who reported, which the server tells nobody else, and for a live track its identifier, which is the key to where a person is. It goes to your terminal and to no log. Do not paste it where others read.
- **The pilot is told the fact, and nothing else.** The server knows no email address and sends no message. A flight that was taken down says so to its pilot through the API, with the moment, and so does an account whose publishing is suspended; no app shows either yet. Why, and what to do about it, the pilot learns only if you tell them some other way.
- **The server says once a day that reports wait.** The line is ``3 reports are open: `venti-server reports` lists them``, written with the daily cleanup and left out while no report is open. It names nothing and nobody. Nothing is sent anywhere: if reports matter to you, look at the log or run `reports` yourself.
- **How long a report is kept.** An open report stays until you close it. A closed one is removed 90 days after it was closed (`VENTI_REPORT_DAYS`). A report goes with either of its two accounts: when the reporter or the reported pilot deletes their account, the report is deleted too. Reports are in the dumps, with the reporters' words, for as long as dumps are kept.
- **What bounds the reports.** An account sends at most 20 a day, a report's text has at most 1,000 characters, and an account has one open report about one thing. An account that sends reports to annoy you is one more thing registration by invite keeps small; there is no command that stops one account from reporting.
- **What is allowed on your server is yours to say.** The software has no rules of conduct, and the project has written none for pilots yet ([plan.md](../docs/plan.md#needed-from-the-owner)).

What you took down or suspended is in the database, and no command lists it:

```sql
SELECT f.id, a.handle, f.taken_down_at FROM flights f JOIN accounts a ON a.id = f.account_id
WHERE f.taken_down_at IS NOT NULL ORDER BY f.taken_down_at;
SELECT handle, publishing_suspended_at FROM accounts WHERE publishing_suspended_at IS NOT NULL;
```

None of this has run on a real server. It was tried on 2026-11-20 on a development machine, with the server's debug build and PostgreSQL 16 with PostGIS in a container: two accounts, a public flight, a live track and three reports, then every command above, the two queries, and what the API answered after each. Not tried: the commands through `docker compose exec`, in the image a server runs *(to verify on the first real install)*.

## Groups

Pilots can make groups, which only their members see, and plan flying days in them ([docs/api.md](../docs/api.md#groups)). There is nothing to set up and nothing to switch on. What to know as the person who runs this:

- **You are no member of anyone's group.** No command shows you one, and the API shows a group to its members only. The database is another matter: whoever can read it reads the names of the groups, who is in which, and the plans, which say where people mean to be on which day. Treat it as that.
- **Nothing of a group is in the log**: no name, no identifier, no invite code, no member, no word of a plan. Where a log line names the path of a request, a path under a group reads `/v1/groups/…`, whatever stood there. The one line the server writes about groups by itself is `removed 3 plans of groups for days more than 2 days gone by`, at most once an hour.
- **The invite codes are not in the database.** It holds a hash of each. A code is shown once, to the keeper of its group, when it is made; nobody can read it back, you neither. A keeper who lost it makes a new one in the app.
- **How long things are kept.** A plan is removed two days after its day (`VENTI_PLAN_DAYS`), by the server's date in UTC; the cleanup looks once an hour. A group and who is in it are kept until its members delete it or the last of them leaves. Everything a pilot put into groups goes when the pilot deletes the account.
- **What the dumps hold.** The groups, their names and who is in them: yes. The plans: no, see [Backup](#backup).
- **What bounds them.** An account is in at most 20 groups, a group has at most 50 members and 100 plans, and an account makes 10 groups, tries 20 invite codes and posts or changes 50 plans a day at most. The numbers are in `crates/server/src/groups.rs` with their reasons.
- **Nobody can report a group or a plan to you yet**, and no command removes one or renames one ([plan.md](../docs/plan.md#needed-from-the-owner)). In a group, its keeper is who clears up: the keeper deletes any plan and removes any member. If something has to go all the same, because a complaint reached you some other way, the database is where it is done:

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

Do not delete rows of `group_members` by hand to remove a member: the database refuses to take a group's keeper out of it, and a member's plans go with the member's row. Nobody is told about any of these statements, and the apps find out when they next ask.

None of this has run on a real server. The statements were tried on 2026-11-20 on a development machine against PostgreSQL 16 with PostGIS in a container, on a database the store's tests had filled; not through `docker compose exec`, in the image a server runs *(to verify on the first real install)*.

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

## Forecasts

The server has forecasts only if it is told where to get them: from an [Open-Meteo](https://open-meteo.com/) instance that you run yourself. Without one everything else works, `/v1/health` says `"forecasts": "not_configured"` and `/v1/forecast` answers `501`. The server never asks Open-Meteo's public service, whose terms are for non-commercial use within daily limits per address.

The Compose file holds such an instance, switched off. To switch it on:

1. In `.env`, set

   ```
   VENTI_OPEN_METEO_URL=http://open-meteo:8080
   ```

2. Start it with the rest:

   ```bash
   docker compose --profile forecasts up -d
   ```

   From then on every `docker compose` command for the stack needs `--profile forecasts`, or put `COMPOSE_PROFILES=forecasts` in `.env`.

3. Look at `/v1/health` after a minute: `"forecasts": "available"`, and the source `open-meteo` lists its `models` with the run each is at. Then ask for a place:

   ```bash
   curl 'https://your-site.example/v1/forecast?lat=47.7&lon=11.8'
   ```

**What the instance is.** Open-Meteo's own container image, unchanged, under the AGPL-3.0-or-later. It keeps no forecasts of its own to begin with: when a place is asked for it reads the parts it needs of Open-Meteo's open data on AWS (`https://openmeteo.s3.amazonaws.com/data/`, no key, Deutscher Wetterdienst's models under CC BY 4.0) and keeps what it read in a cache on disk. So the machine needs outbound HTTPS to that address, and the first forecast for a place takes longer than the following ones. Nothing but the server can reach the instance: it publishes no port.

**What it needs.** Open-Meteo's documentation asks for 8 to 16 GB of memory and fast disk for an instance. The cache is 8 GB unless `OPEN_METEO_CACHE_SIZE` says otherwise. Tried on 2026-11-19 with a cache of 256 MB and five places, which says nothing about a busy server: *to verify* on the first installation, with the size of the machine.

**The one setting that is easy to miss** is already made in the Compose file: `API_SYNC_APIKEYS`. Without it an instance does not tell which run of a model it holds, the server cannot date a forecast, and it serves none. The value guards nothing here, since the instance is reachable only from inside the stack.

**How much the server asks.** A forecast for a place is four requests to the instance and is then kept for half an hour for everyone who asks within about a kilometre. The server asks the instance at most 400 times an hour, a tenth of which is held back for its own look every ten minutes; when the hour is used up, places that are not at hand get `503` with `Retry-After`. What a forecast holds, and what it does not, is in [data-sources.md](../docs/data-sources.md#forecasts).

**Still to verify on a real machine:**

- The instance as the Compose file starts it. What was tried is the same image with the same settings, fed through a local relay to the same data, because the machine that tried it had no direct way out.
- How long a first forecast takes with a direct connection. Through that relay it was 25 to 60 seconds, which is longer than a request waits (8 seconds): the app is told to ask again in a few seconds, and the forecast is there when it does. If it stays slow, Open-Meteo's other arrangement copies the data to disk ahead of time ([sync](https://github.com/open-meteo/open-meteo/blob/main/docs/sync-command.md), with `--bounding-box 46.3,9.4,48.5,17.2` for the first region and the models `dwd_icon_d2_native,dwd_icon_eu_native`); it was not tried.
- Whether the model names the server asks for (`dwd_icon_d2_native`, `dwd_icon_eu_native`) stay as they are after Deutscher Wetterdienst's change of 30 November 2026.

An instance somewhere else works the same way: give its address as `VENTI_OPEN_METEO_URL`, and leave the profile off.

## Map regions

An app can download the map of a region and draw it where there is no network: a vector base map and the terrain, two PMTiles archives. The server hands out the regions it finds in one folder, at `/v1/maps` ([docs/api.md](../docs/api.md#maps)). While the folder is empty it hands out none, and everything else works as before.

None of this has run on a real server, and no region but the small one in the repository has been built. What was tried is said at the end. Every size here is an estimate *(to verify)*.

**Where the regions go.** Into `deploy/data/maps`, or the folder that `VENTI_MAPS_DIR` in `.env` names. The server sees it at `/srv/maps` and only reads it. Each region is a folder in it, called whatever you like, with the region's files and a `map.toml` that describes them:

```
data/maps/
  alps-de-at/
    map.toml
    basemap.pmtiles
    terrain.pmtiles
```

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

The rules, each of which the server holds a region to:

- `id`, `version` and every file's `name` are lowercase letters and digits with `-`, `_` and `.` between them, 64 characters at most. They become the file's address, `/v1/maps/<id>/<version>/<name>`.
- `role` is `basemap`, `terrain` or `other`.
- `sha256` is the SHA-256 of the file as it lies there, in lowercase (`sha256sum` prints it so).
- Every file names at least one credit: whose data it holds, worded as that provider asks. The apps show it wherever the file is drawn. A `url` starts with `https://` or `http://`.
- A file lies in the region's own folder. A link that leads out of the folder is refused.
- No two folders have the same `id`. If two do, neither is handed out.

The server runs as the user with the number 10001 and must be able to read the files: `chmod -R a+rX data/maps` makes them readable to everyone on the machine, which they are to everyone on the internet anyway.

**Building a region.** `tools/region-map/build.sh` is written to build the folder of the region the server collects for, `map.toml` and hashes included, from OpenStreetMap and from Mapterhorn's elevation tiles:

```bash
cd /opt/venti
tools/region-map/build.sh regions/alps-de-at/region.toml /opt/venti-maps/alps-de-at
```

**It has not been run for a real region.** Its header says what it needs and where each number comes from: Java, Go and Python; from the extract of Europe it starts with, which was 35.5 GB on 2026-11-19, about 18 GB of memory for Java and 180 to 360 GB of free disk by the rule of thumb in Planetiler's README; and less of all three from a smaller extract that covers the region. A server of the smallest size this runbook names cannot build a region. Build it on another machine and copy the folder over.

The credits the script writes are the ones above. They are those of the region in the repository, which is built from the same two sources; what OpenStreetMap asks of whoever passes its data on is in [data-sources.md](../docs/data-sources.md#flying-sites). Mapterhorn's terms for a server that hands an extract of its tiles on to many pilots are *to verify* with that project before a public release, as [architecture.md](../docs/architecture.md#maps-and-terrain) says of its tile service.

**What the server does with the folder.** When it starts, it reads the `map.toml` of every folder, then reads every file from its first byte to its last and works out its SHA-256. A region is handed out only if each hash is the one its `map.toml` states. An app checks its download against the hash the server lists; a wrong one would make every download fail at its end, after gigabytes. The reading happens beside everything else: the API answers at once, and a region appears at `/v1/maps` when its files are read. The log says what became of each, in lines like these:

```bash
docker compose logs server | grep 'maps:'
```

```
maps: reading the regions in /srv/maps; each is handed out once its files are checked
maps: handing out alps-de-at, version 2026-11-20: 2 files, 3712.4 MB
```

A region that is not handed out has a line that says why, with its folder: a file whose hash is another (the line has both hashes), a name that breaks the rule above, a file that is missing, two folders with one `id`. The server runs on without that region. How long the reading takes on a server's disk is *to verify*; expect about as long as the disk needs to read the files once.

**Renewing a region.** An app keeps a download going by the file's address, and the address holds the version: the bytes behind an address must never change. So a region is renewed whole, under a new `version`, and never edited in place.

1. Build the new folder somewhere outside `data/maps`, on the same disk. The script writes the day as the version.
2. Swap the folders and start the server again:

   ```bash
   cd /opt/venti/deploy
   mv data/maps/alps-de-at /opt/venti-maps/alps-de-at.old
   mv /opt/venti-maps/alps-de-at data/maps/alps-de-at
   docker compose restart server
   docker compose logs --since 5m server | grep 'maps:'
   curl -s http://localhost/v1/maps
   ```

3. Remove the old folder once the list shows the new version.

The old folder goes out of `data/maps` first: two folders with one `id` are both left out. Starting the server again ends the downloads that are under way. An app whose download was of the old version is told `404` when it asks for the rest, and starts again with the new one; an app that has the old version keeps drawing it and finds the new one in the list.

If a file is changed while the server runs, the server notices at the next request for it: the file's size or the time it was written is no longer what it read. It then stops handing that region out and says so in the log, once, until it is started again. A copy of the new files over the old ones therefore does not go out with the old hashes.

**Disk and traffic** *(estimates, to verify with the first region)*.

- **Disk.** Scaled up by area from the region in the repository (7 MB for one valley), the first region comes to about 1.6 GB of base map and 2.1 GB of terrain. That is arithmetic and not a measurement. [architecture.md](../docs/architecture.md#sizing) reckons with 3 to 5 GB a region. While a region is renewed, the disk holds it twice.
- **Traffic.** Every app that keeps the region downloads all of it once for each version. A hundred pilots and a region of 4 GB are 400 GB for each renewal; how often to renew is a question of how much the map changed, and of this number.
- **No limit.** The server sends a file as fast as a client takes it. It counts nothing and refuses nobody, however many download at once, and a download has no time limit. If downloads crowd out the rest, the place to bound them is in front of the server.
- **Through Caddy.** A download reaches the server through Caddy like every request to `/v1`. The server sends a file uncompressed, with its length, and answers requests for a part of it, so that a download that broke off goes on where it stopped; the `Caddyfile` adds nothing to requests under `/v1`. A download of gigabytes through Caddy has not been tried.

**Check once on a new server** *(to verify)*, from another machine: fetch a whole file, break the download off, let it go on, and compare the result with the list.

```bash
curl -s https://<domain>/v1/maps
curl -C - -O https://<domain>/v1/maps/alps-de-at/<version>/basemap.pmtiles    # press Ctrl-C, then run it again
sha256sum basemap.pmtiles                                                      # the sha256 of the list
```

**What was tried**, on 2026-11-20 on a development machine with the server's debug build, not in a container and not through Caddy: the region in the repository (two files, 7 MB) listed and fetched whole and in parts with `curl`, with and without `--compressed`; a download of a file of 200 MB broken off by stopping the server, which was gone half a second after it was told to stop, and gone on with after the next start, with the listed hash in the end; and the building script from its first line to its last with stand-ins for its three heavy steps, and the folder it wrote listed by the server. Not tried: a region of gigabytes, the script's real steps, the Compose setup with the folder mounted, and anything on a phone.

## What the web app's pages may do

Three lists bound what the web app can load, run and talk to. The browser enforces them. They are there because of the session token a signed-in browser keeps: a stranger's script that got into a page by a mistake could otherwise read it and send it away.

| List | Who sets it | What it says |
|---|---|---|
| The pages' policy | The web app's build writes it into every page (`apps/web/README.md` has each entry and its reason) | No script but the app's own files, no connection but to this site and the map's two tile services |
| `Content-Security-Policy: frame-ancestors 'none'; base-uri 'none'` and `X-Frame-Options: DENY` | Caddy, with everything of the web app | The pages are shown in no frame, so no other site can lay its own picture over the app's buttons. A policy inside a page cannot say this |
| `Content-Security-Policy: default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self' https://tiles.openfreemap.org https://tiles.mapterhorn.com` (and the two rules above) | Caddy, with the app's script files (`*.js`, `*.mjs`) | What a worker may do. A worker is held to the policy that arrives with its own script and to nothing a page says. The app has two: the core, which reads the session token and talks to the API, and the map engine |

The pages of shared flights have a policy of their own from the server, which allows no script at all.

Caddy sends the third list with every script file and not only with the two a worker starts from, because the build decides where those are and what they are called (today the core's under `/_app/immutable/workers/`, the map engine's under `/_app/immutable/assets/`). A browser reads the list only where a script is started as a worker.

Tried on 2026-11-19 with the web app as built that day, served by this Caddyfile in Caddy 2.10 and opened in Chromium: the map, the logbook and the settings drew, both workers started, the core answered and the map fetched its tiles, and the browser refused nothing. With the two tile hosts taken out of the third list the map engine's worker fetched no tile, so that worker is held to the list.

*To verify:* Safari and Firefox were not tried, here or by the web app's own tests. If the core does not start in one of them (the logbook stays empty and the settings name no airspace), the third list is the first thing to look at: take the line that starts `header @scripts` out of the `Caddyfile`, run `docker compose restart caddy`, and report it.

**When the map's tile hosts change** (for instance when the project serves its own tiles from `/tiles`), the third list changes with them. The hosts are in `apps/web/src/lib/map/online.ts` and in the `Caddyfile`, and CI fails when the two disagree.

Each page of the app is a file of its own (`/logbook/` is `logbook/index.html`), and Caddy answers an address with its page. An address that is no page gets the map's page. Until 2026-11-19 every address but `/` got the map's page: whoever opened or reloaded the logbook saw the map.

## What each service may do

Since 2026-11-19 the three services are held to less than Docker gives a container by default, so that whoever gets into one of them through a mistake in its software finds little there. Tried on a development machine that day with the server's debug build in a plain container: start, an invite, an account, a flight and its analysis, the share page and its picture through Caddy, `backup.sh`, and a restore over the live database. The image the Dockerfile builds was not run under these settings there; CI starts it with the same ones. *(To verify on the first real install.)*

| | Caddy | Server | Database |
|---|---|---|---|
| Powers of the machine's root user (`cap_drop`, `cap_add`) | one: listening on ports 80 and 443 | none | five, for the image's start: it hands the data folder to the user `postgres` and becomes that user |
| Gaining powers from a program that carries them (`no-new-privileges`) | no | no | no |
| Writing outside its volumes (`read_only`) | no | no | no |
| Memory (`mem_limit`) | 256 MB | 1 GB | 2 GB |
| Processes and threads (`pids_limit`) | 256 | 1024 | 512 |

The memory limits fit the smallest machine this runbook names. On a larger one give the database more, in `.env`: `VENTI_DB_MEMORY=16g` (and `VENTI_SERVER_MEMORY`, `VENTI_CADDY_MEMORY` likewise). A service that reaches its limit is restarted by Docker and says so in `docker compose ps` and in `dmesg`.

**The server is not the database's superuser.** The database has two roles. `postgres` is its superuser, for you: it has no password, so nothing signs in as it over the network, and from inside the container it needs none (`docker compose exec db psql --username postgres venti`). `venti` is the server's: it owns the database and the tables and is no superuser, so someone who got into the server can read and change what the server keeps, and cannot read files on the database's machine or run programs there, which a superuser can. `db-init.sh` makes both so, once, when the database is first created on an empty volume. `POSTGRES_PASSWORD` in `.env` is the password of `venti`.

A database that was created before 2026-11-19 has `venti` as its superuser, and that cannot be taken from the role a database was created with. Move it: take a dump with the old setup, start the new setup on an empty volume (`docker compose down`, then `docker volume rm venti_db-data`, then `docker compose up -d db`), and restore the dump with the commands under [Restore](#restore). A dump made by the old `backup.sh` does not say who owns its tables; after restoring one, hand them to the server's role before starting the server:

```bash
docker compose exec -T db psql --username postgres --dbname venti --command \
  "DO \$\$ DECLARE t record; BEGIN FOR t IN SELECT tablename FROM pg_tables WHERE schemaname = 'public' AND tablename <> 'spatial_ref_sys' LOOP EXECUTE format('ALTER TABLE public.%I OWNER TO venti', t.tablename); END LOOP; END \$\$"
```

*(To verify: nobody has such a database but developers; the command was not run against one.)*

The optional forecast service (`open-meteo`, profile `forecasts`) is not confined in these ways: it is someone else's image and was not tried under them.

## Backup

`backup.sh` writes one dump of the whole database per run and deletes dumps older than 14 days (`VENTI_BACKUP_KEEP_DAYS`). It checks the new dump before it deletes anything. It takes the dump as the database's superuser, from inside the database's container, and the dump says which role owns each table.

The dump holds everything the server knows but two things named below: the weather it collected, and every account with its password hash, its signed-in devices and its flights with their files, the reports pilots sent, with who sent them and what they wrote, and the groups pilots made, with their names and who is in which. There is nothing else to back up for these; flight files are not kept in a folder. That also means a dump is personal data. The script makes the backup folder readable by its owner only; wherever you copy dumps to has to be as careful.

One thing is left out of the dump on purpose: **live tracks**, the positions pilots share while they fly. The server removes each one 48 hours after its flight (`VENTI_LIVE_HOURS`), and a dump that held them would keep them for as long as dumps are kept. The dump has the two tables and none of their rows; it does keep the list of identifiers that live tracks have used, which holds no position. After a restore, nobody's live track is there; a flight that is under way begins its track anew with the next positions its app sends. Tried on 2026-11-19 on a development database with a track of 43,200 positions: the dump shrank from 698 kB to 74 kB, restored without complaint into an empty database, and the restored tables took a new track. *(To verify on the first real install, with `backup.sh` itself: that test ran `pg_dump` by hand with the script's two `--exclude-table-data` options.)*

The other thing left out, for the same reason: **the plans of groups**, which say where people mean to be on a day. The server removes each one two days after its day (`VENTI_PLAN_DAYS`). The dump has the two tables (`group_plans`, `group_plan_pilots`) and none of their rows; the groups and their members are in it. After a restore the groups are there and their plans are not: plans for the days ahead have to be posted again by whoever posted them. Tried on 2026-11-20 on a development database with two groups, four members, two plans and four pilots who were in: the restored database had the groups and the members, no plan, and took a new one. *(To verify on the first real install, with `backup.sh` itself: that test too ran `pg_dump` by hand, with the script's four `--exclude-table-data` options.)*

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
docker compose exec -T db dropdb --username postgres --force venti
docker compose exec -T db createdb --username postgres --owner venti venti
docker compose exec -T db pg_restore --username postgres --dbname venti --exit-on-error < "$dump"
docker compose start server
curl -s http://localhost/v1/health
```

The restored database lacks everything after the dump. The server refills the last three hours of the German stations on its next run and continues from there; the gap in between stays. The airspace is the dump's until the next daily run replaces it; `/v1/airspace` says how old it is.

For accounts and flights: accounts created after the dump are gone, and so are flights uploaded after it. A flight is still on the phone it was uploaded from. The apps are meant to notice that the server has gone back in time (the server tells a device whose bookmark lies in the future to compare its whole logbook) and to upload what is missing; that is built on the server and in no app yet. Sessions made after the dump stop working, and those pilots sign in again. Passwords changed after the dump are the old ones again.

On a new machine: do the first install up to step 3 with the old database password in `.env`, copy the dump over, run `docker compose up -d db` (which creates the two roles), then the restore commands above from `createdb` on (the fresh database is empty, so drop it first all the same), then `docker compose up -d`.

Try a restore before it is needed. A dump can be restored into a scratch database on the same server without touching the live one:

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

The log also has a line when an account is created or deleted, when a password is changed, for every wrong password, when an uploaded file had to be given up on, and once a day while reports are open, with their number. No line holds a password or a session token, and none holds anything of a report: not who sent it, not what or whom it is about, not a word of it. None holds anything of a group either; see [Groups](#groups).

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
- **`VENTI_OPEN_METEO_URL`** is the Open-Meteo instance asked for forecasts. It has no default: without it the server has no forecasts and says so. See [Forecasts](#forecasts).
- **`VENTI_MAPS_DIR`** is the folder on the machine with the map regions the server hands out. It has a default, `deploy/data/maps`, and an empty folder hands out nothing. See [Map regions](#map-regions).
- **`VENTI_OBSERVATION_MONTHS`** is how many whole calendar months of observations are kept. Older months are dropped once a day. Lowering it deletes data at the next cleanup.
- **`VENTI_CORS_ORIGINS`** is only needed when a web app on a *different* origin calls the API from a browser. The same origins may call the connector from a browser.
- **`VENTI_PUBLIC_URL`** is the address people reach the site at. The page of a shared flight names itself and its preview picture with it, and a chat app can only fetch a preview from a complete, correct address. Without it the server falls back to the address it listens on and says so in its log at start. It is also the one address whose pages may call the connector from a browser without being named in `VENTI_CORS_ORIGINS`.
- **`VENTI_REGISTRATION`** is who may create an account; see [Accounts](#accounts).
- **`VENTI_ACCOUNT_FLIGHTS`** and **`VENTI_ACCOUNT_STORAGE_MB`** are how much one account may keep: 25,000 flights and 4,096 MB of flight files unless set. A pilot does not reach either by flying; they are what keeps one account from filling the disk. The database stores the files compressed, in about a third of their size, so an account that is full takes about 1.4 GB there. With the query under [Looking inside](#looking-inside) you see what each account keeps. Lowering a number deletes nothing: an account above it keeps what it has and cannot add to it.
- **`VENTI_REPORT_DAYS`** is for how many days a report is kept after you closed it: 90 unless set. An open report is kept until it is closed, whatever this says. Lowering the number removes the closed reports that are older, at the next daily cleanup.
- **`VENTI_PLAN_DAYS`** is for how many days a plan that the members of a group made for a flying day is kept after that day: 2 unless set, and 1 at least. The day ends by the server's clock in UTC. Lowering the number removes the plans that are older within the hour; raising it brings none back.
- **`VENTI_NETWORK`** moves the services' private network; see [The visitor's address](#the-visitors-address).

Not settings, because nobody should have to tune them: how many wrong passwords are tolerated (10 an hour for one account, 30 an hour from one address), how many accounts one address may try to create (10 an hour), how large a flight file may be (4 MB) and how many one account may upload (1000 a day), how long an unused session lasts (400 days), how many forecasts one address may make the server fetch (12 places an hour; forecasts that are at hand are not counted), how many reports one account may send (20 a day), what bounds groups (see [Groups](#groups)), and how much the connector takes (64 kB a request, 10 seconds for an answer, 120 requests a minute from one address). An IPv6 visitor counts as its /64 network, and one /48 network as a whole gets four times what one address gets. They are in `crates/server/src/accounts/mod.rs`, `crates/server/src/groups.rs` and `crates/server/src/api/mcp.rs` with their reasons.

**The counting has a size.** Each of these counts is a table in the server's memory with room for 100,000 addresses (or handles, accounts, devices), 11 MB when full; there are thirteen, three of them for what accounts do with groups. A server with a few hundred pilots uses a few hundred places. If the log says

```
the table of addresses that asked the connector is full: 100000 are being counted
```

someone is sending requests from a very large number of addresses. The server keeps counting the ones it knows, counts all further ones together under one generous shared limit, and refuses nobody merely for being new; when even the shared limit is used up, newcomers get `429` until the window (a minute for the connector, an hour for sign-in and new accounts) is over. That is the point at which the machine needs protection from outside the server: the hosting company's, or a firewall rule for the networks the requests come from.

The server can also read a TOML file instead of variables; see `crates/server/config.example.toml`. The Compose setup does not use it.

## On a development machine

No server is needed to work on the server. The development mode needs nothing at all:

```bash
cargo run -p venti-server -- dev          # the API over recorded data on http://127.0.0.1:8080
```

It starts with a demo account and three flights, and with two further pilots whose public flights fill the list of everyone's flights; the handles, the password and a token that always works are in [docs/api.md](../docs/api.md#try-it).

It hands out map regions when it is told a folder of them. The one region the repository holds is the valley the apps bundle:

```bash
cargo run -p venti-server -- dev --maps apps/ios/Venti/Resources
curl http://127.0.0.1:8080/v1/maps
```

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
