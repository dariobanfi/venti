# Working rules for AI sessions

Read `docs/plan.md`, `docs/architecture.md` and `docs/data-sources.md` before changing anything. Before changing anything a user sees, read `design/AGENTS.md` and `design/DESIGN.md`, the product's design language, look at `design/reference/screens/`, and read `docs/design.md`, which says what each screen does. They are the project's memory. If a change contradicts them, update the document in the same commit or do not make the change.

## Fixed by the owner

- Native UI only: SwiftUI, Jetpack Compose, and a web app in the browser. No React Native, Flutter or web views in the mobile apps.
- The look is the owner's design language, "Valley", in `design/`: its tokens, type, icons, logo and art, in the themes `light`, `dark` and `flight`. A screen takes colours, sizes, radii and fonts from the tokens and never writes its own; it uses the files in `design/assets` and never redraws, recolours or copies them. `design/DESIGN.md` decides how something looks; `docs/design.md` says what a screen does and where a platform differs. Do not change the files in `design/` except on the owner's word.
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
cargo deny check licenses bans sources advisories
VENTI_UPDATE_GOLDEN=1 cargo test --workspace   # rewrite golden files, then review the diff
cargo run -p venti-server -- ingest regions/alps-de-at/region.toml   # live run against the providers
cargo run -p venti-server -- dev   # the API on 127.0.0.1:8080 over recorded data; no network, no database
```

The iOS app cannot be built in a Linux cloud session. The `iOS` workflow builds it on GitHub's Mac runners on every push that touches the app or the core, and runs the unit tests and the UI tests. The UI tests tap through every screen and their screenshots are uploaded as the `ios-screenshots` artifact: download them with `gh run download` and look at them after every change to a screen. Read the logs with `gh run view`. Swift cannot be compiled here, so check every Apple API name against its documentation before pushing. On a Mac: `apps/ios/scripts/open-in-xcode.sh`. After changing the sample flight generator, regenerate `apps/ios/Venti/Resources/sample.igc` with `cargo run -p venti-flight --example sample_flight`.

The Android app builds in a cloud session once the Android SDK is there (`apps/android/README.md` lists what to install; keep it out of the repository). Gradle builds the Rust core and generates its Kotlin interface by itself:

```bash
cd apps/android
./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebug   # unit tests against the real core, lint, the app
./gradlew :app:assembleDebugAndroidTest                              # compiles the UI tests; running them needs an emulator
```

The UI tests need an emulator, which a cloud session without hardware virtualisation cannot run. The `Android` workflow runs them on every push that touches the app or the core, and uploads their screenshots as the `android-screenshots` artifact and the app as `venti-debug-apk`: download the screenshots with `gh run download` and look at them after every change to a screen. For the wind map the workflow starts `venti-server dev` and names it to the tests. A UI test must not wait with `Thread.sleep`: Compose draws a test's frames only when the test asks, so the screenshot would show the moment the test fell asleep; use `TestDevice.letTheAppRun`. A new kind of error in the core needs a line in `apps/android/uniffi.toml`, or the Kotlin interface does not compile.

The web app (`apps/web`) builds and tests in a cloud session; `apps/web/README.md` has the commands and `apps/web/AGENTS.md` the details for sessions:

```bash
cd apps/web
npm ci
scripts/build-core.sh                                 # the Rust core as WebAssembly; again after every change to the core
npm run check && npm run format:check && npm run licences && npm test
npm run build && npm run test:e2e                     # the built site in a real browser, against `venti-server dev`
```

The browser tests keep a screenshot of every screen in `apps/web/screenshots/`, at a phone's and a desktop's width, light and dark; the `Web` workflow uploads them as the `web-screenshots` artifact. A field added to a record of the core is a type error in the web app's hand-built test records until they have it, so run `npm run check` after changing a record.

Every checkout and every git worktree needs its own `target` folder. Two that share one, by a link or by `CARGO_TARGET_DIR`, take each other's builds for fresh, and tests then pass or fail against code that is not there.

Cloud sessions for this repository use the `venti` environment, which has unrestricted network access. `.claude/settings.json` lists the commands that run without a permission prompt; add to it when a new tool joins the toolchain.

## Working practice

- Commit directly to `main` and push. No feature branches and no pull requests: the owner works on this alone.
- Commits are in the owner's name, with the session as co-author in the message's last lines. Before the first commit of a session: `git config user.name "Dario Banfi"` and `git config user.email "hello@dariobanfi.com"`.
- A README is a short page for people: what the folder is, how to build and run it, where to read more. Working notes for AI sessions go into the `AGENTS.md` beside it ([AGENTS.md](AGENTS.md) lists them); what a feature does goes into `docs/`. Do not add status, history or feature lists to a README.
- Decisions go in the decision log in `docs/architecture.md`.
- Mark anything not checked against a primary source as "to verify". Do not present it as fact.
- Prefer deleting code to adding a second way of doing something.
- Show the owner what changed. After a change to a screen on any platform, send them the new screenshots from the workflow's artifact without waiting to be asked, so that they can direct the work.
- Before a screen is called done, go through the list at the end of `design/AGENTS.md`: every theme against `design/reference/screens/`, the loading, empty, error and offline states, 360 px wide and text at 130%, and no colour or size in the diff that is not a token.
