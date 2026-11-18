# Venti

One free, open-source app for free flight: check conditions, plan, fly, review and share. It starts with paragliding in the German and Austrian Alps. *Venti* is Italian for "winds".

**Status:** Phase 0, foundations. The data-source framework and the first two weather adapters exist; there is no app yet.

```bash
cargo test --workspace
cargo run -p venti-ingest -- regions/alps-de-at/region.toml
```

The second command fetches live station observations for the first region and prints a health line per source.

| Document | What it covers |
|---|---|
| [docs/plan.md](docs/plan.md) | Product scope, owner decisions, roadmap, what is needed from the owner |
| [docs/architecture.md](docs/architecture.md) | Rust core, native apps, server, hosting, bring-your-own AI, decision log |
| [docs/data-sources.md](docs/data-sources.md) | The adapter framework for external data and the Germany/Austria source catalogue |
| [docs/xctrack-parity.md](docs/xctrack-parity.md) | Cockpit feature checklist measured against XCTrack |

The project is built and maintained by AI coding sessions under the direction of its owner. The documents above are the project's memory between sessions; [CLAUDE.md](CLAUDE.md) holds the working rules.

## Licence

GNU Affero General Public License, version 3 or later. See [LICENSE](LICENSE).
