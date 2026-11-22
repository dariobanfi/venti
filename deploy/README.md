# Running Venti on a server

This folder runs Venti on one Linux machine with Docker Compose: Caddy, the web app, the Venti server, PostgreSQL with PostGIS and an optional forecast service. No image is published; the server builds its own from this checkout.

## First start

On a machine with Docker Engine and its Compose plugin, with the site's domain pointing at it:

```bash
git clone https://github.com/dariobanfi/venti.git /opt/venti
cd /opt/venti/deploy
./setup.sh venti.example   # writes .env, builds the images, starts everything
```

## Update

```bash
cd /opt/venti && git pull && cd deploy
./backup.sh
docker compose build && docker compose up -d
```

## More

- [docs/running-a-server.md](../docs/running-a-server.md): the manual: install, accounts, reports, forecasts, map regions, backup, restore, every setting.
- `env.example`: every setting, with comments.
