# Notes for AI sessions

The binding rules for AI sessions are in [CLAUDE.md](CLAUDE.md): what the owner has fixed, the architecture and data-source rules, the commands, and how to work. As it says, read [docs/plan.md](docs/plan.md), [docs/architecture.md](docs/architecture.md) and [docs/data-sources.md](docs/data-sources.md) before changing anything, and [docs/design.md](docs/design.md) before changing a screen.

## Notes by folder

- [apps/ios/AGENTS.md](apps/ios/AGENTS.md): the iOS workflow's artifacts, the development server's demo data, the code's layout, what has never been tried on an iPhone, and what is open before the App Store.
- [apps/android/AGENTS.md](apps/android/AGENTS.md): checks before a push, reading a red workflow, the pitfalls of the UI tests and the emulator, conventions of the code, how Gradle builds the core, the layout, and what has never been tried on a phone.
- [apps/web/AGENTS.md](apps/web/AGENTS.md): checks before a push, binding the core for the browser, the hosts the app talks to, the tests and their pitfalls, the layout, where the session token is kept, and the content security policy entry by entry.
- [deploy/AGENTS.md](deploy/AGENTS.md): what of the server's stack has run where, the reasons behind the runbook's steps, what is still to verify on a real server, the bounds that are not settings, and working on the server with a local database.

## Across the repository

- **The owner's Mac takes its crates from a registry mirror that lags crates.io by about two weeks.** A version published more recently than that cannot be built there (`cargo` says "failed to select a version" and names the mirror). When adding or updating a dependency, take a version that is at least three weeks old, or check with the owner; on 2026-11-20 the lockfile was moved back to what the mirror had (flate2 1.1.10, toml 1.2.0, zip 9.4.1 and 25 others by a patch version or two).

- **A README is a page for people**: what the folder is in a few sentences, how to build and run it in the fewest commands that work, and links with a few words each. No feature lists, no history, no lists of what is not verified, no layout tables, nothing another document already says. Working knowledge goes into the `AGENTS.md` beside it; what a feature does goes into `docs/`. `deploy/README.md` is the operator's manual and holds the steps an operator carries out.
- **`apps/web` is formatted by Prettier, its Markdown included**: `npm run format:check` there covers `README.md` and `AGENTS.md`, and fails on a table whose columns are not lined up.
- **Other files point at sections of the READMEs and AGENTS.md files by their headings**: code comments, `deploy/env.example`, `deploy/docker-compose.yml`, `deploy/backup.sh`, `crates/server/config.example.toml` and the documents under `docs/`. Search for the file's name before renaming or removing a heading. `crates/server/tests/moderation.rs` reads `deploy/README.md` itself (see [deploy/AGENTS.md](deploy/AGENTS.md#reports)).
- **The two small READMEs under `crates/`** (`crates/flight/tests/fixtures/xctsk/README.md`, `crates/server/assets/README.md`) record where files came from and under which licence. They stay beside the files they describe.
