# Notes for AI sessions working on the deployment

Working notes: what to know before changing how Venti is run on a server, with the background, the estimates, what was tried and what is still to verify. The `README.md` beside this file is for whoever runs a server.

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
