# Working rules for AI sessions

Read `docs/plan.md`, `docs/architecture.md` and `docs/data-sources.md` before changing anything, and `docs/design.md` before changing a screen. They are the project's memory. If a change contradicts them, update the document in the same commit or do not make the change.

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

The iOS app cannot be built in a Linux cloud session. The `iOS` workflow builds it on GitHub's Mac runners on every push that touches the app or the core, and runs the unit tests and the UI tests. The UI tests tap through every screen and their screenshots are uploaded as the `ios-screenshots` artifact: download them with `gh run download` and look at them after every change to a screen. Read the logs with `gh run view`. Swift cannot be compiled here, so check every Apple API name against its documentation before pushing. On a Mac: `apps/ios/scripts/open-in-xcode.sh`. After changing the sample flight generator, regenerate `apps/ios/Venti/Resources/sample.igc` with `cargo run -p venti-flight --example sample_flight`.

The Android app builds in a cloud session once the Android SDK is there (`apps/android/README.md` lists what to install; keep it out of the repository). Gradle builds the Rust core and generates its Kotlin interface by itself:

```bash
cd apps/android
./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebug   # unit tests against the real core, lint, the app
./gradlew :app:assembleDebugAndroidTest                              # compiles the UI tests; running them needs an emulator
```

The UI tests need an emulator, which a cloud session without hardware virtualisation cannot run. The `Android` workflow runs them on every push that touches the app or the core, and uploads their screenshots as the `android-screenshots` artifact and the app as `venti-debug-apk`: download the screenshots with `gh run download` and look at them after every change to a screen. A new kind of error in the core needs a line in `apps/android/uniffi.toml`, or the Kotlin interface does not compile.

Cloud sessions for this repository use the `venti` environment, which has unrestricted network access. `.claude/settings.json` lists the commands that run without a permission prompt; add to it when a new tool joins the toolchain.

## Working practice

- Commit directly to `main` and push. No feature branches and no pull requests: the owner works on this alone.
- Decisions go in the decision log in `docs/architecture.md`.
- Mark anything not checked against a primary source as "to verify". Do not present it as fact.
- Prefer deleting code to adding a second way of doing something.
