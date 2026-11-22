# Venti

One free, open-source app for free flight: check conditions, plan, fly, review and share. It starts with paragliding in the German and Austrian Alps. *Venti* is Italian for "winds".

It is an iPhone app, an Android app and a web app over one Rust core, and a server for live wind, airspace, forecasts, accounts and shared flights. It is early work: nothing is published yet, and the cockpit has not flown.

## Try it

Needs Rust from [rustup.rs](https://rustup.rs); `rust-toolchain.toml` picks the version.

```bash
cargo test --workspace
cargo run -p venti-server -- dev   # the API on http://127.0.0.1:8080 over recorded data, with a demo account
```

## Where things are

- [apps/ios](apps/ios/README.md), [apps/android](apps/android/README.md), [apps/web](apps/web/README.md): the three apps.
- [deploy](deploy/README.md): running the server on a machine.
- `crates/`: the Rust workspace: the shared core, flight analysis, data sources, the server.
- [docs/plan.md](docs/plan.md): scope, roadmap and the state of each part.
- [docs/architecture.md](docs/architecture.md): how it is built, and the decision log.
- [docs/](docs): design, data sources, the API, the connector and the rest.

The project is built by AI coding sessions under the direction of its owner. Their rules are in [AGENTS.md](AGENTS.md).

Licence: GNU Affero General Public License, version 3 or later. See [LICENSE](LICENSE).
