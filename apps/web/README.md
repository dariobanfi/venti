# Venti for the web

A SvelteKit shell over the Rust core. The shell draws and talks to the browser; everything about flying lives in `crates/core` and below, and reaches the browser as WebAssembly. The app is built to static files: any web server can serve it, and nothing runs on the server.

## Building and running

Needs Rust from [rustup.rs](https://rustup.rs) (the toolchain in `rust-toolchain.toml` installs itself) and Node 22.17 or later.

```bash
cd apps/web
npm ci
scripts/build-core.sh   # the Rust core as WebAssembly, with its TypeScript declarations
npm run dev             # the app at http://localhost:5173, rebuilt as you edit
```

Run `scripts/build-core.sh` again whenever `crates/core`, `crates/core-wasm` or anything below them changes. The first run installs the `wasm-bindgen` tool in the version `Cargo.lock` names, which takes a few minutes.

The wind on the map comes from a Venti server. For one on this machine that answers from recorded data, run this in a second terminal, from anywhere in the repository:

```bash
cargo run -p venti-server -- dev   # six stations, on 127.0.0.1:8080
```

`npm run dev` and `npm run preview` pass the app's questions under `/v1` on to it (to another address with `VENTI_DEV_SERVER=http://…`). Without it the map says that the server could not answer.

```bash
npm run build           # the static site, in build/
npm run preview         # serves build/ at http://localhost:4173
```

To serve the app from a folder of a site instead of its root, build with `VENTI_BASE_PATH=/folder`.

## Testing

```bash
npm run check           # types
npm run format:check    # formatting (npm run format rewrites)
npm run licences        # every npm package's licence against an allowlist
npm test                # unit tests, in Node, against the real core
npm run build && npm run test:e2e   # the built site in a real browser
```

The end-to-end tests need a Chromium that Playwright can drive: `npx playwright install chromium` once. They start the Venti server in its development mode themselves, which compiles it the first time, and use one that is already running; with `VENTI_DEV_SERVER=http://127.0.0.1:18080` they start and use it there, when port 8080 belongs to something else. They use the app as a pilot would and keep a screenshot of every screen in `screenshots/<project>/`, for four projects: a phone's width and a desktop's, each in light and in dark appearance. **Look at the screenshots after every change to a screen.** One project is quicker: `npm run test:e2e -- --project=phone-light`.

The tests draw a real map without the network. The app asks OpenFreeMap and Mapterhorn for tiles as it always does; the test browser answers those requests from the small region the iPhone app bundles (`apps/ios/Venti/Resources/MapDemo`, the valley of the sample flight). Outside that valley the test maps are blank, which is the region's edge and not a fault.

The app uses the system's typeface, so the screenshots show the test machine's. A bare Linux machine's is wide and cuts text short that fits on a phone; `e2e/fonts.conf` says how the `Web` workflow gives its screenshots Inter instead.

Every push that touches the app or the core runs all of this in the `Web` workflow. Its `web-screenshots` artifact holds the screenshots and `web-site` the built site: `gh run download <run> -n web-screenshots`.

## Layout

| Path                         | What it is                                                                                                                                                                                                                                         |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/routes/`                | The screens and the frame around them: `/` is the map, `/logbook/` the logbook, `/logbook/statistics/` its statistics, `/logbook/flight/?id=…` one flight.                                                                                         |
| `src/lib/app/`               | What the app holds while it is open, the navigation, and dropping files on the window.                                                                                                                                                             |
| `src/lib/core/`              | The way into the Rust core. The core runs in a worker (`worker.ts`) and the page calls it through `client.ts`; `session.ts` and `idb-store.ts` keep the core's files in the browser. `pkg/` is built by `scripts/build-core.sh` and not committed. |
| `src/lib/design/`            | Number formatting, the climb colours, and the few components the app draws itself: metric, icon, dialogs, menu. The rules behind them are in `docs/design.md`.                                                                                     |
| `src/lib/logbook/`           | The logbook's model over the core, import, the list, the track thumbnail and the chart of the statistics.                                                                                                                                          |
| `src/lib/flight/`            | One flight: map, panel, altitude chart, details.                                                                                                                                                                                                   |
| `src/lib/map/`               | The shared map setup, the style preparation, label fonts, the map of all flights, the pilot's position.                                                                                                                                            |
| `src/lib/wind/`              | Live wind: when to ask the server and what to keep (`watch.ts`), how a station is drawn (`mark.ts`), the list of stations, one station with its charts.                                                                                            |
| `src/app.css`                | Colours, type sizes and spacing, as custom properties.                                                                                                                                                                                             |
| `e2e/`                       | Tests that use the built site in a browser and keep the screenshots.                                                                                                                                                                               |
| `scripts/build-core.sh`      | Builds the core for the browser. Its last step, `scripts/strip-core.mjs`, removes from the module what only the phones use.                                                                                                                        |
| `scripts/check-licences.mjs` | The licence allowlist for npm packages.                                                                                                                                                                                                            |
| `static/favicon.svg`         | The app icon, the same drawing as the iPhone's.                                                                                                                                                                                                    |
| `../../map/style.json`       | The map style shared by all platforms.                                                                                                                                                                                                             |
| `../ios/Venti/Resources/`    | The sample flight and the label fonts. The repository holds one copy of each; the build takes them from there.                                                                                                                                     |

Tests of a module sit beside it as `*.test.ts`.

## What the app does today

- **Logbook.** Import IGC flight files with the button or by dropping them on the page, several at once. Flights are listed by year with totals. A flight that is already there is recognised; a file that is not a flight is named with the reason. A new logbook offers a sample flight. It is made up, generated by `venti-flight`, not a real pilot's. The totals lead to the statistics: the last twelve months as a bar chart, the gliders flown, and the pilot's bests.
- **Flight.** The track on a terrain map, coloured by climb and sink, with the scored route under it and the altitude profile in a panel. Drag along the profile, or use the arrow keys on it, to move a marker along the track and read time, altitude and climb at that moment, or let the flight play. The details list how the flight scores, its thermals and its glides; choosing one marks it. The map can be tilted into 3D terrain. The flight's file can be downloaded again.
- **Map.** A terrain map with every logbook flight on it and, on request, your position. With a Venti server behind the site, every wind station in view is an arrow pointing where the wind blows, with mean wind and gusts under it; choosing one shows its reading, its last 24 hours and whose data it is. The same stations are there as a list.

On a narrow screen the app is laid out like the iPhone app: navigation at the bottom, the flight panel over the lower part of the map. On a wide one the navigation is a rail at the side, the flight panel stands beside the map with the details under it, and the logbook has columns for two more numbers.

## Where the flights are kept

In the browser, in IndexedDB, under the names the core uses on a phone's disk. They are on this device only and never sent anywhere. `docs/architecture.md` describes how the core and the browser's storage work together and what happens with two tabs, a full disk and a browser that offers no storage.

A browser may remove a site's data: when the device runs short of space, when the pilot clears site data, and in Safari after seven days without a visit _(to verify: this is Apple's stated policy for script-writable storage; it was not tested)_. After the first import the app asks the browser to keep its data (`navigator.storage.persist()`), which browsers grant or not by their own rules. Until flights are synchronised with a server, the files the pilot imported remain the copy to keep.

## Where the wind comes from

From the Venti server the site is served from: the app asks its own origin for `/v1/…`. Nothing has to be set. When the site is published, the web server that hands out these files also has to pass `/v1` on to the Venti server. A site that only hands out the files answers those questions with "404 Not Found"; the map then has no wind and does not mention it. `docs/api.md` describes the server's API.

## Map tiles

Fetched from OpenFreeMap (base map) and Mapterhorn (terrain, asked for only up to zoom 12) until the project has its own tile server; `docs/architecture.md` has the terms. Both see the IP address and the map areas of whoever uses the app. Label fonts are served by the app itself. The map shows its sources' credits: written out on a wide map, behind the "i" button on a narrow one.

## Not deployed

Nothing publishes this app. Publishing is the owner's decision; the `web-site` artifact of the `Web` workflow is the folder to serve when that day comes.
