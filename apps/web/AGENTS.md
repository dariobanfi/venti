# Notes for AI sessions working on the web app

Working notes: what to know before changing the app or its tests. What the app does and what is not built is in [docs/plan.md](../../docs/plan.md) and [docs/design.md](../../docs/design.md), how it is built in [docs/architecture.md](../../docs/architecture.md) under "Web". The `README.md` beside this file is for people.

## Checks before a push

- From `apps/web`: `npm run check && npm run format:check && npm run licences && npm test`, then `npm run build && npm run test:e2e`. After a change to `crates/core`, `crates/core-wasm` or anything below them, `scripts/build-core.sh` first: the tests run against the module that was built last.
- The browser tests take about a quarter of an hour on four cores, for 260 tests at four widths and appearances. One file at one width is quick: `npx playwright test e2e/plans.spec.ts --project=phone-light`.
- They start `venti-server dev` with `cargo run` and use one that is already running. Another session's server on port 8080 holds what that session's tests left in it: name a port of your own for the tests, `VENTI_DEV_SERVER=http://127.0.0.1:18431`. Build the server with the same `CARGO_PROFILE_…` settings in the environment as the tests run with, or `cargo run` builds it again.
- The preview the tests start listens on port 4173 and on no other (`playwright.config.ts`), so two sessions cannot run the browser tests on one machine at the same time. Not tried.
- The `Web` workflow takes 35 to 45 minutes, most of it the browser tests, which are slower on GitHub's machines than here. It lets a run that has started finish; of the runs waiting behind it only the newest is kept, so a push is often tested together with later commits. Look for the newest finished run that holds the commit, not for the commit's own.

## The core for the browser

- **After a change to the core or its binding:** `scripts/build-core.sh`, then `npm run check`. A record that gained a field is a type error in the hand-built test records until they have it.
- **A reader in which the pilot is "you" needs the account first.** The core tells whose a plan or a group is by the handle of the stored account, and takes any failure to read that file for "signed out". Over the browser's storage a file is fetched on request, so after a reload such a reader would call nothing the pilot's own. `crates/core-wasm` therefore reads answers about groups through `Session::call_as_the_pilot`, which looks at the account first: the call then fails for want of the file, and the session fetches it and calls again. Bind a new reader of that kind the same way, and test it over a store that was reloaded, as `src/lib/core/groups-source.test.ts` does.
- **A record the page hands back to the core** needs `serde::Deserialize` under the core's `wasm` feature, as `TrackPoint` and `PlanDraft` have. One that only comes out needs `Serialize` alone.
- **An object of the core that holds much is freed by hand.** `Terrain` holds half a megabyte of heights for each tile it was given. `src/lib/core/terrain-source.ts` makes one for a flight and frees it with the answer.
- **Only what a page calls is bound.** What `wasm-bindgen` exports stays in the module. `Terrain::add_archive` and the list of a pilot's groups alone are not bound, because nothing in the app calls them.

## Hosts the app talks to

- The core's worker fetches elevation tiles for the ground under a flight from the host in `src/lib/map/online.ts`, which the map uses too. That host is in the page's content security policy (`src/lib/app/security-policy.ts`) and in the web server's header for the workers (`deploy/Caddyfile`). Another host would need both. The browser tests fail on anything the policy refuses (`refused` in `e2e/helpers.ts`), which is how a missing entry shows.
- Flying plans are asked of the app's own origin by the worker, with the session token, through `sendToThisOrigin` in `src/lib/core/account-source.ts`. A new request from the pilot's account goes the same way.

## Testing

- **The ground under a flight** is tested with real elevation tiles, cut one at a time from `apps/ios/Venti/Resources/MapDemo/terrain.pmtiles`: in Node by `src/lib/core/terrain-source.test.ts`, in the browser tests by `e2e/bundled-map.ts`, which answers every request to the tile hosts, the core's worker's among them. Do not copy that file into the app. The sample flight lies on two tiles of zoom 12; a flight outside the Tegernsee valley has no ground in the tests.
- **`addSampleFlight` waits for the ground** (`groundDrawn` in `e2e/helpers.ts`). A test that keeps the elevation tiles away has to open the flight itself, as the last test of `e2e/ground.spec.ts` does.
- **With the ground, the marked moment of a flight reads "Over ground"; without it, "In the air".** A test that reads the flight panel at a moment says which of the two it expects, or reads the time.
- **The development server's clock stands still** at 19:05 UTC on 18 November 2026, and its made-up plans are for the 18th to the 20th. The board of plans is asked for from the browser's own today. A test that wants a plan on the board posts one, or sets the browser's clock to that day with `page.clock.setFixedTime`; `e2e/plans.spec.ts` does both.
- **Tests that change groups make accounts of their own**, and the long one a group of its own. The demo account is only read.
- **The same test runs at four widths, more than one at a time, on one server.** What one run posts into the development mode's "Mustergruppe", the other sees for a moment. Look for a made-up plan by its place in full, and for a member by name.
- **Members who came into a group at the same moment are listed in an order no test can know.** With the server's clock standing still, that is every two members a test adds.
- **A plan with a position cannot be made in the web app**, which has no waypoints to take one from. `postFromAPhone` in `e2e/plans.spec.ts` posts one through the API.
- **A day of a plan is a date, not a moment.** Unit tests of `src/lib/groups/days.ts` set the time zone of the test process (`process.env.TZ`) to show that today is the reader's date and not UTC's; they put it back afterwards.

## Pitfalls

- **A form that fills itself when it opens must not fill itself again** while the pilot types. The board is asked for anew when the tab comes back to the front, which hands the form new objects. `src/lib/groups/PlanForm.svelte` reads what it starts from inside `untrack`, so that only opening fills it.
- **A board is one account's.** `src/lib/groups/board.ts` puts the board away when another pilot has signed in, before the server is asked. State of that kind that outlives a page needs the same.
- **What a pilot typed is set as text.** No `{@html}` for a group's name, a place or a note. `e2e/plans.spec.ts` posts a plan whose place is markup and looks for no image.
- **A dialog stays in the page and is opened and closed**, as on the other screens, and keeps its words while it closes. _To verify:_ what a browser does with the focus when an open dialog is taken out of the page instead; it was not tried.
- **The switch in a row** is the class `toggle` in `src/app.css`. It is not a style of one screen.
- **The navigation's bar has four sections** and shares the width of a narrow screen between them. A fifth needs the bar looked at on a phone's width, in the screenshots.
