# Venti

One free, open-source app for free flight: check conditions, plan, fly, review and share. It starts with paragliding in the German and Austrian Alps. *Venti* is Italian for "winds".

It is an iPhone app, an Android app and a web app over one Rust core, and a server for live wind, airspace, forecasts, accounts and shared flights. It is early work: nothing is published yet, and the cockpit has not flown.

## Try it

Needs Rust from [rustup.rs](https://rustup.rs); `rust-toolchain.toml` picks the version.

```bash
cargo test --workspace
cargo run -p venti-server -- dev   # the API on http://127.0.0.1:8080 over recorded data, with a demo account
```

## Change the apps for yourself

Each app is a folder of its own; its README has the full list of what to install.

- **iPhone** (`apps/ios`, a Mac with Xcode): `apps/ios/scripts/open-in-xcode.sh`, then press Run. On your own iPhone: copy `apps/ios/Config/Local.example.xcconfig` to `Local.xcconfig` and put in your Apple ID's team; a free Apple ID works, its builds last seven days.
- **Android** (`apps/android`, Java 17 and the Android SDK): `cd apps/android && ./gradlew :app:installDebug` with your phone connected. `-Pventi.server=https://…` points it at another Venti.
- **Web** (`apps/web`, Node 22): `cd apps/web && npm ci && scripts/build-core.sh && npm run dev`, beside `cargo run -p venti-server -- dev`.

What a screen does and why is in [docs/design.md](docs/design.md); the look is in `design/`. Logic every app shares (the flight, the forecast's words, the rules) is Rust in `crates/core`: change it there and all three apps follow. A coding assistant reads [AGENTS.md](AGENTS.md) first.

## Where things are

- [apps/ios](apps/ios/README.md), [apps/android](apps/android/README.md), [apps/web](apps/web/README.md): the three apps.
- [deploy](deploy/README.md): running the server on a machine.
- `crates/`: the Rust workspace: the shared core, flight analysis, data sources, the server.
- [docs/plan.md](docs/plan.md): scope, roadmap and the state of each part.
- [docs/architecture.md](docs/architecture.md): how it is built, and the decision log.
- [docs/](docs): design, data sources, the API, the connector and the rest.

The project is built by AI coding sessions under the direction of its owner. Their rules are in [AGENTS.md](AGENTS.md).

Licence: GNU Affero General Public License, version 3 or later. See [LICENSE](LICENSE).
