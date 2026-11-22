# Venti

One free, open-source app for free flight: check conditions, plan, fly, review and share. It starts with paragliding in the German and Austrian Alps. *Venti* is Italian for "winds".

It is early work: an iPhone app, an Android app and a web app over one Rust core, and a server for live wind, airspace, forecasts, accounts and shared flights. None of it is published or running anywhere yet, and the cockpit has not flown. [docs/plan.md](docs/plan.md) has the state of each part.

## Try it

Needs Rust from [rustup.rs](https://rustup.rs); `rust-toolchain.toml` picks the version.

```bash
cargo test --workspace
cargo run -p venti-server -- dev
```

The second command serves the server's API on `http://127.0.0.1:8080` over recorded data, with a demo account, no network and no database. Try `curl 'http://127.0.0.1:8080/v1/stations?bbox=9.4,46.3,17.2,48.5'`. The apps can use it as their server; each app's README says how.

```bash
cargo run -p venti-server -- ingest regions/alps-de-at/region.toml
```

fetches live station observations and airspace for the first region from the providers and prints a health line for each source.

## Where things are

| Folder | What it is |
|---|---|
| [apps/ios](apps/ios/README.md) | The iPhone app (SwiftUI) |
| [apps/android](apps/android/README.md) | The Android app (Jetpack Compose) |
| [apps/web](apps/web/README.md) | The web app (SvelteKit) |
| [deploy](deploy/README.md) | Running the server on a machine: install, update, backup, restore |
| `crates/` | The Rust workspace: the shared core, flight analysis, data sources, the server |

| Document | What it covers |
|---|---|
| [docs/plan.md](docs/plan.md) | Product scope, owner decisions, roadmap and the state of each phase |
| [docs/architecture.md](docs/architecture.md) | Rust core, native apps, server, hosting, bring-your-own AI, decision log |
| [docs/design.md](docs/design.md) | How the apps look and behave, and why |
| [docs/data-sources.md](docs/data-sources.md) | The adapter framework for external data and the Germany/Austria source catalogue |
| [docs/adding-a-source.md](docs/adding-a-source.md) | Bringing a data source of your country into Venti, step by step, and taking one out |
| [docs/api.md](docs/api.md) | The server's HTTP API, endpoint by endpoint |
| [docs/mcp.md](docs/mcp.md) | The connector: the server's tools for a pilot's own Claude or ChatGPT |
| [docs/airspace.md](docs/airspace.md) | Airspace: the record, the OpenAir reader and the warnings |
| [docs/xctrack-parity.md](docs/xctrack-parity.md) | The cockpit's features measured against XCTrack |

The project is built by AI coding sessions under the direction of its owner. Their rules are in [CLAUDE.md](CLAUDE.md), their working notes in [AGENTS.md](AGENTS.md).

## Licence

GNU Affero General Public License, version 3 or later. See [LICENSE](LICENSE).
