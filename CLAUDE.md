# Working rules for AI sessions

Read `docs/plan.md`, `docs/architecture.md` and `docs/data-sources.md` before changing anything. They are the project's memory. If a change contradicts them, update the document in the same commit or do not make the change.

## Fixed by the owner

- Native UI only: SwiftUI, Jetpack Compose, and a web app in the browser. No React Native, Flutter or web views in the mobile apps.
- Shared logic is Rust. UI shells contain no business rules.
- Self-hosted on one Linux server. Do not add a service that bills by usage.
- The project pays for no AI inference. Users bring their own assistant or credentials.
- The code is open source. Never commit secrets, API keys or personal data.

## Architecture rules

- Crate dependencies point inward: `domain` first, then `flight`, `sensors`, `sources`, then everything else.
- Provider-specific types exist only inside that provider's `sources-*` crate.
- `flight` and `sensors` do no I/O and read no clock.
- Per-frame and audio code never crosses the Rust bridge.
- Safety-relevant behaviour (airspace, terrain, landing options) is deterministic code that works offline and without an assistant.

## Data sources

- No adapter without a manifest whose licence was verified on the provider's own page, with date and URL.
- No adapter without recorded fixtures and golden files.
- Data from different licence families is never merged into one record or bundle.
- Sources marked "Ask" in the catalogue are not used until the owner has permission in writing.

## Commands

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check licenses bans sources
VENTI_UPDATE_GOLDEN=1 cargo test --workspace   # rewrite golden files, then review the diff
cargo run -p venti-ingest -- regions/alps-de-at/region.toml   # live run against the providers
```

Cloud sessions reach the data providers only through the `venti` environment's host allowlist. A new provider needs its hosts added there before its fixtures can be recorded.

## Working practice

- Decisions go in the decision log in `docs/architecture.md`.
- Mark anything not checked against a primary source as "to verify". Do not present it as fact.
- Prefer deleting code to adding a second way of doing something.
