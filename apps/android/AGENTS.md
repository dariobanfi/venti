# Notes for AI sessions working on the Android app

Working notes: what to know before changing the app or its tests. What the app does and what is not built is in [docs/plan.md](../../docs/plan.md) and [docs/design.md](../../docs/design.md), how it is built in [docs/architecture.md](../../docs/architecture.md). The `README.md` beside this file is for people.

## Checks before a push

- From `apps/android`, with `ANDROID_HOME` set: `./gradlew :app:testDebugUnitTest :app:lintDebug :app:assembleDebugAndroidTest -Pventi.abis=x86_64`. The unit tests call the real core; the last task only compiles the UI tests.
- Run them again after every rebase that brings a change to `crates/core`. A field added to a record of the core breaks the tests that build that record by hand: `ReachTest` (`ReachView`), `AheadTest` (`AirspaceAheadSpan`), `WindByHeightTest` (`WindAtHeightView`), `GroupsModelTest` (`GroupMember`). Where a test needs a changed record, take a real one from the core and use `copy`.
- Lint fails on a drawable that nothing uses: add an icon in the commit that uses it.
- The UI tests cannot run in a cloud session. The `Android` workflow runs them, in about half an hour. A run that has started finishes; of the runs waiting behind it only the newest is kept, so a push is often tested together with later commits.
- When a run is red: the artifact `android-test-reports` holds `TEST-*.xml` with the failed test and the line, and `android-screenshots` holds `failed-<test>.png`. That picture is the last frame that was drawn, which can be older than what the failed assertion looked at.

## Pitfalls of the UI tests

- Compose draws a test's frames only when the test asks. Never wait with `Thread.sleep`; use `TestDevice.letTheAppRun`.
- A replay goes on while a test looks, by two seconds of the clock at most for each refresh. What belongs together during a thermal or a glide is looked for at one look (`waitForAll`); what comes late in a flight is waited for with `TestDevice.WHOLE_FLIGHT_SECONDS`.
- A screenshot waits until the map has drawn all it was given (`TestDevice.keepScreenshot`, `letTheMapFinish`). The glider and its track are one large piece of data and reach the map later than the line to the target does.
- Rows that are one element for a screen reader end their modifiers with `clearAndSetSemantics`: a test tag before it stays, tags on what is inside the row are gone. Find such a row by its tag, its description and what it says (`saying`).
- An instrument is read as "1,453 m, 238 m over ground": the value, then the line under it. A test that takes a number from it takes the first.
- Words on screen are mostly the core's. Read them in `crates/core` before asserting them: in a plan's list of who is in, the pilot of the phone is "You"; a place out of reach says "Out of reach." or "Out of reach: the ground is in the way."
- `venti-server dev` keeps what the tests send until it stops, and the light and the dark test run against the same one. A second report about the same thing is not a new one, so the two appearances report two pilots' flights; both post a plan for today. Its demo pilot signs in as `demo` with `demo-pilot-password`; neither is a secret.
- A bottom sheet stops at half height only if its content is taller than half the screen when it opens: wrap the content in `Box(Modifier.fillMaxHeight())`.
- With a sheet open over a sheet there are two "Done" buttons: give the upper one a test tag (`report-close`).
- Now and then the emulator's renderer cuts off part of the status bar. That is not the app.

## Conventions in the code

- Words that are not the core's stand in an object beside the screen (`PlanWords`, `GroupWords`, `ReportWords`, `AheadWords`, `ReachWords`), so that a unit test can hold them.
- Talking to the pilot's server is one way round: the core puts the request together, `AccountModel.answer` sends it with the session's token, the core reads the answer. Nothing of groups, plans or reports is kept on the phone.
- Icons are Material Symbols, outlined, 24 px, taken as Android vector drawables from `google/material-design-icons` (`symbols/android/<name>/materialsymbolsoutlined/<name>_24px.xml`), with the tint removed and the fill set to `#FF000000`, named `ic_<name>.xml`.
