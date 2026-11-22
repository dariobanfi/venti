# Working rules for AI sessions

This is the file a session reads first, and its rules bind. The notes of single folders are listed [at the end](#notes-by-folder).

## Read before changing anything

- Always: [docs/plan.md](docs/plan.md), [docs/architecture.md](docs/architecture.md) and [docs/data-sources.md](docs/data-sources.md). They are the project's memory. If a change contradicts them, update the document in the same commit or do not make the change.
- Before changing anything a user sees: [design/AGENTS.md](design/AGENTS.md) and [design/DESIGN.md](design/DESIGN.md), the product's design language, the pictures in `design/reference/screens/`, and [docs/design.md](docs/design.md), which says what each screen does.
- Before adding an endpoint or a table to the server: [docs/data-platform.md](docs/data-platform.md), the principles owner decision 14 gives it, what the server holds, and the pieces still missing in their build order.
- Before adding or removing a data source: [docs/adding-a-source.md](docs/adding-a-source.md), with the template (`tools/new-source/new.sh`) and the one check (`tools/new-source/check.sh`). A station network is one line in `station_networks()` in `crates/server/src/adapters.rs`; before one joins, read [Pioupiou's condition](docs/data-sources.md#pioupious-condition).

## Fixed by the owner

- Native UI only: SwiftUI, Jetpack Compose, and a web app in the browser. No React Native, Flutter or web views in the mobile apps.
- The look is the owner's design language, "Valley", in `design/`: its tokens, type, icons, logo and art, in the themes `light`, `dark` and `flight`. A screen takes colours, sizes, radii and fonts from the tokens and never writes its own; it uses the files in `design/assets` and never redraws, recolours or copies them. `design/DESIGN.md` decides how something looks; `docs/design.md` says what a screen does and where a platform differs. Do not change the files in `design/` except on the owner's word.
- Shared logic is Rust. UI shells contain no business rules.
- Venti is one central platform that the owner runs, on one Linux server and on a set of them when it grows. It is not made for clubs or pilots to host a Venti of their own: there is one Venti, and the apps belong to it. "Self-hosted" means the owner's own server in place of a cloud service. Do not add a service that bills by usage.
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
- Venti is not commercial (owner decision 17 in `docs/plan.md`): a source whose own pages state a licence that allows the use, also one for non-commercial use only, is used under it without asking the provider. Its conditions hold: credit, share-alike, no mixing.
- Sources marked "Ask" in the catalogue (no stated licence for the use, or terms that do not cover it) are not used until the owner has permission in writing. Sessions send no email or message to a provider: they draft, the owner sends.

## Commands

The Rust workspace:

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo deny check licenses bans sources advisories
VENTI_UPDATE_GOLDEN=1 cargo test --workspace   # rewrite golden files, then review the diff
cargo run -p venti-server -- ingest regions/alps-de-at/region.toml   # live run against the providers
cargo run -p venti-server -- dev   # the API on 127.0.0.1:8080 over recorded data; no network, no database
```

Every checkout and every git worktree needs its own `target` folder. Two that share one, by a link or by `CARGO_TARGET_DIR`, take each other's builds for fresh, and tests then pass or fail against code that is not there.

The iPhone app builds on a Mac only: `apps/ios/scripts/open-in-xcode.sh` opens it in Xcode, and `apps/ios/scripts/ui-tests.sh` runs its tests on simulators ([apps/ios/AGENTS.md](apps/ios/AGENTS.md)). A cloud session cannot compile Swift, so there check every Apple API name against its documentation before pushing; the `iOS` workflow builds the app on GitHub's Mac runners on every push that touches the app or the core, runs the unit tests and the UI tests, and uploads the screenshots as the `ios-screenshots` artifact (`gh run download`; the logs with `gh run view`). After changing the sample flight generator, regenerate `apps/ios/Venti/Resources/sample.igc` with `cargo run -p venti-flight --example sample_flight`.

The Android app needs the Android SDK ([apps/android/AGENTS.md](apps/android/AGENTS.md#setting-up-and-installing) lists what to install; keep it out of the repository). Gradle builds the Rust core and generates its Kotlin interface by itself:

```bash
cd apps/android
./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebug   # unit tests against the real core, lint, the app
./gradlew :app:assembleDebugAndroidTest                              # compiles the UI tests; running them needs an emulator
```

The UI tests need an emulator: on the owner's Mac `apps/android/scripts/ui-tests.sh` runs them ([apps/android/AGENTS.md](apps/android/AGENTS.md)); a cloud session has no hardware virtualisation and leaves them to the `Android` workflow, which uploads the screenshots as `android-screenshots` and the app as `venti-debug-apk`. A UI test must not wait with `Thread.sleep`: Compose draws a test's frames only when the test asks, so the screenshot would show the moment the test fell asleep; use `TestDevice.letTheAppRun`. A new kind of error in the core needs a line in `apps/android/uniffi.toml`, or the Kotlin interface does not compile.

The web app ([apps/web/AGENTS.md](apps/web/AGENTS.md) has the details):

```bash
cd apps/web
npm ci
scripts/build-core.sh                                 # the Rust core as WebAssembly; again after every change to the core
npm run check && npm run format:check && npm run licences && npm test
npm run build && npm run test:e2e                     # the built site in a real browser, against `venti-server dev`
```

The browser tests keep a screenshot of every screen in `apps/web/screenshots/`, at a phone's and a desktop's width, light and dark; the `Web` workflow uploads them as the `web-screenshots` artifact. A field added to a record of the core is a type error in the web app's hand-built test records until they have it, so run `npm run check` after changing a record.

Cloud sessions for this repository use the `venti` environment, which has unrestricted network access. `.claude/settings.json` lists the commands that run without a permission prompt; add to it when a new tool joins the toolchain.

## Working practice

- Commit directly to `main` and push. No feature branches and no pull requests: the owner works on this alone.
- Commits are in the owner's name, `Dario Banfi <hello@dariobanfi.com>` and no other address, with the session as co-author in the message's last lines. Before the first commit of a session: `git config user.name "Dario Banfi"` and `git config user.email "hello@dariobanfi.com"`. Where the owner's git signs commits, never pass `--no-gpg-sign`; if the signing is refused, ask the owner to approve it and try again.
- Instructions for sessions live in `AGENTS.md` files only: this one, and one in a folder that needs notes of its own. No other file holds rules for sessions. A README is a few lines for people: what the folder is, the fewest commands that build and run it, where to read more; no status, history or feature lists. A long manual, and what a feature does, go in `docs/`.
- Decisions go in the decision log in `docs/architecture.md`.
- What waits for the owner (something only they can do, a decision, something to look at) goes on [docs/owner-todo.md](docs/owner-todo.md) the day it comes up, in one line, and comes off when it is done.
- Mark anything not checked against a primary source as "to verify". Do not present it as fact.
- Prefer deleting code to adding a second way of doing something.
- Show the owner what changed. After a change to a screen on any platform, send them the new screenshots, from the test run on the Mac or the workflow's artifact, without waiting to be asked, so that they can direct the work.
- Before a screen is called done, go through the list at the end of `design/AGENTS.md`: every theme against `design/reference/screens/`, the loading, empty, error and offline states, 360 px wide and text at 130%, and no colour or size in the diff that is not a token.
- Other files point at sections of the documents, READMEs and `AGENTS.md` files by their headings: code comments, the files in `deploy/`, `crates/server/config.example.toml`, messages the server prints, and the documents under `docs/`. Search for the file's name before renaming or removing a heading. `crates/server/tests/moderation.rs` reads `docs/running-a-server.md` itself ([deploy/AGENTS.md](deploy/AGENTS.md#reports)).
- The three small READMEs under `crates/` (`crates/flight/tests/fixtures/xctsk/`, `crates/leagues/tests/fixtures/`, `crates/server/assets/`) record where files came from and under which licence. They stay beside the files they describe.

### On the owner's Mac

- **When the owner's Mac is attached, its checkout, `~/code/venti`, is the repository.** Edit, build, test, commit and push there, and do not push to wait for CI: the same tests take minutes on the Mac. If the Mac refuses a step or leaves the session, say so and wait for the owner; do not go on through CI.
- **Native code is built under `~/code` only**: the Mac kills binaries compiled elsewhere. Rustup's tools come before Homebrew's `cargo`, which has no iPhone or Android targets: `export PATH=/opt/homebrew/opt/rustup/bin:$PATH`.
- **The registries are mirrors that lag.** The Mac takes its crates and its npm packages from mirrors that are weeks behind crates.io and npm; `cargo` then says "failed to select a version" and names the mirror. When adding or updating a dependency, take a version that is at least three weeks old, or check with the owner. Never point cargo or npm at another registry.
- **What is the owner's there, and not a session's to use or stop**: the "iPhone 18 Pro" simulator, an emulator that is already running (the windowed `pixel6-api36` on port 5554), the stack on 127.0.0.1:8088, and the stashes in the checkout, which are dropped only on the owner's word.
- **The owner's own Venti on the Mac is kept at the newest main.** It is the Compose project in `deploy/`, on 127.0.0.1:8088, and a debug build of either phone app asks it by itself, so a stack that lags hides the day's server work. After every push that touches `crates/`, `deploy/`, `regions/`, `map/` or `apps/web`: `docker compose build` and `docker compose up -d` in `deploy/`, then read `/v1/health` and the server's log, in the background, and tell the owner in a line. No asking and no dump first: everything in it is development data, and where a change is quicker with an empty database or volume, empty it. A real installation is another matter: [docs/running-a-server.md](docs/running-a-server.md) is its runbook.
- **`deploy/.env` on the Mac is never committed and not changed without a reason.** Its lines `VENTI_HTTP_BIND=127.0.0.1:8088` and `VENTI_HTTPS_BIND=127.0.0.1:8443` keep the stack to the Mac: without them `docker compose up` publishes the site on the network. `docker compose config` shows the ports and the mounts a start would use, without starting anything: look at it first.
- **A checkout that moves leaves its old path behind** in three places outside the repository: Xcode's package cache (`target/apple/DerivedData/SourcePackages/workspace-state.json`; the build stops with "There is no XCFramework found at" the old path, and writing the new path into that file is enough), `VENTI_MAPS_DIR` in `deploy/.env`, and the mounts of the running containers, which hold until the stack is started again.

### Several sessions at once

- **One session coordinates and agents implement.** The coordinator makes a change the apps share (a record of the core, the map style, `tools/made-up-day/serve.py`) before it briefs them, and writes the documents under `docs/` itself: three agents editing one document is three versions of it.
- **One agent to an app folder** (`apps/ios`, `apps/android`, `apps/web`), all in the main checkout at the same time. Each is told to write nowhere else, to run no git command that changes anything, and which ports and devices are the others'.
- **Rust work that would disturb the apps' builds gets a worktree of its own**: `git worktree add --detach ~/code/venti-<name> HEAD`, with its own `target` folder. Android's Gradle builds the core from whatever the main checkout holds, so a half-edited core there fails an app agent's build. Keep one such worktree and use it again: a new one compiles the workspace from nothing. The coordinator takes the finished work over as a patch (`git diff` in the worktree, new files added with `git add -N` first, then `git apply --3way` in the main checkout), runs the workspace's checks there and commits. Two agents that each add a row to the decision log conflict in that one place; keep both rows.
- **Agents do not commit.** A commit on the Mac is signed with the owner's key and needs the owner at the laptop; the coordinator stages a finished piece and commits when it is whole.
- **One session at a time writes a file.** A session that shares a checkout looks at `git log -3 -- <file>` before editing a file and commits only its own files by path. A branch is never moved forward over a working tree that still holds the older files: whoever commits such a file next takes the newer change out again.
- **Devices and ports are handed out, not found**: the "iPhone 17" simulator and ports 8090 and 8091 for the iPhone's tests, the virtual device `pixel6-api36-b` on port 5556 and ports 8190 and 8191 for Android's, 8290 and 8291 for the web's browser tests. `apps/ios/scripts/ui-tests.sh` and `apps/android/scripts/ui-tests.sh` deal the UI tests out to several simulators or emulators and count their ports up from there.
- **An agent runs the tests its change touches.** Whole suites, clippy and the workspace's tests are the coordinator's, once, at integration, with nothing else running. Four heavy jobs at once is the most the Mac does without each slowing the others: one per app and one for Rust.
- **A brief names what to read, the exact commands that work here, what "done" is, and what to report.** It names a section or gives the facts, never "read `docs/architecture.md` and `docs/design.md`": the two are too large to read whole. A task that meets a question of design stops and reports it.
- **A report is all the coordinator sees**, so a brief asks for the tests run with their numbers, the screenshots' paths, and what was not checked.
- **A fresh agent for each piece**, named for the piece, briefed to report within about half an hour and to say at once when a step takes more than ten minutes or fails twice. The coordinator tells the owner of a stall when it happens, not at the end.

## Notes by folder

- [design/AGENTS.md](design/AGENTS.md): the owner's design pack. Where to start before building a screen, which token file each platform takes, the hard rules of the look, and the list to go through before a screen is done.
- [apps/ios/AGENTS.md](apps/ios/AGENTS.md): building and testing on the Mac and in the workflow, the development server's demo data, the conventions of the screens, the code's layout, what has never been tried on an iPhone.
- [apps/android/AGENTS.md](apps/android/AGENTS.md): checks before a push, the UI tests and the emulators, their pitfalls, conventions of the code, how Gradle builds the core, the layout, what has never been tried on a phone.
- [apps/web/AGENTS.md](apps/web/AGENTS.md): checks before a push, binding the core for the browser, the hosts the app talks to, the tests and their pitfalls, the layout, where the session token is kept, the content security policy entry by entry.
- [deploy/AGENTS.md](deploy/AGENTS.md): what of the server's stack has run where, the reasons behind the runbook's steps, what is still to verify on a real server, the bounds that are not settings, and working on the server with a local database.
