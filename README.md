# Venti

One free, open-source app for free flight: check conditions, plan, fly, review and share. It starts with paragliding in the German and Austrian Alps. *Venti* is Italian for "winds".

**Status:** early. The data-source framework and the first two weather adapters exist. The iPhone app keeps a logbook on the device: it imports IGC flight files and shows each flight on a terrain map, coloured by climb, with its altitude profile. The server exists in a first version: it collects the live wind and weather stations of Germany and Austria and serves them over an HTTP API. It runs on no machine yet. The iPhone app can draw a server's wind stations on its map once it is given a server's address; until the project's server is online that is a server you run yourself, so for most people there is still no live weather in the app, and there is no community. The web app has the logbook and the flight screen in a browser, with the flights kept in the browser; it is published nowhere yet. The Android app has the iPhone app's logbook, flight screen, map with a server's wind stations, cockpit, assistant and settings; it has run on an emulator only, and every push builds an installable copy. See [apps/ios](apps/ios/README.md), [apps/android](apps/android/README.md) and [apps/web](apps/web/README.md).

```bash
cargo test --workspace
cargo run -p venti-server -- dev
cargo run -p venti-server -- ingest regions/alps-de-at/region.toml
```

The second command serves the server's API over recorded data on `http://127.0.0.1:8080`, with no network and no database; try `curl 'http://127.0.0.1:8080/v1/stations?bbox=9.4,46.3,17.2,48.5'`. The third fetches live station observations for the first region and prints a health line per source. Running the real server is described in [deploy/README.md](deploy/README.md).

| Document | What it covers |
|---|---|
| [docs/plan.md](docs/plan.md) | Product scope, owner decisions, roadmap, what is needed from the owner |
| [docs/architecture.md](docs/architecture.md) | Rust core, native apps, server, hosting, bring-your-own AI, decision log |
| [docs/data-sources.md](docs/data-sources.md) | The adapter framework for external data and the Germany/Austria source catalogue |
| [docs/api.md](docs/api.md) | The server's HTTP API, endpoint by endpoint |
| [deploy/README.md](deploy/README.md) | Running the server on a machine: install, update, backup, restore |
| [docs/design.md](docs/design.md) | How the apps look and behave, and why |
| [docs/xctrack-parity.md](docs/xctrack-parity.md) | Cockpit feature checklist measured against XCTrack |

The project is built and maintained by AI coding sessions under the direction of its owner. The documents above are the project's memory between sessions; [CLAUDE.md](CLAUDE.md) holds the working rules.

## Licence

GNU Affero General Public License, version 3 or later. See [LICENSE](LICENSE).
