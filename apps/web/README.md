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

The wind on the map and the pilot's account come from a Venti server. For one on this machine that answers from recorded data, run this in a second terminal, from anywhere in the repository:

```bash
cargo run -p venti-server -- dev   # six stations and a demo account, on 127.0.0.1:8080
```

`npm run dev` and `npm run preview` pass the app's questions under `/v1`, and the pages of shared flights under `/f`, on to it (to another address with `VENTI_DEV_SERVER=http://…`). Without it the map says that the server could not answer. The demo account is `demo` with the password `demo-pilot-password`; the server forgets everything when it stops.

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

| Path                         | What it is                                                                                                                                                                                                                                                                                                                                                                                         |
| ---------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `src/routes/`                | The screens and the frame around them: `/` is the map, `/logbook/` the logbook, `/logbook/statistics/` its statistics, `/logbook/flight/?id=…` one flight, `/settings/` the settings, `/settings/account/` the pilot's account and `/settings/airspace/` the pilot's airspace.                                                                                                                     |
| `src/lib/app/`               | What the app holds while it is open, the navigation, dropping files on the window, and the site's content security policy (`security-policy.ts`).                                                                                                                                                                                                                                                  |
| `src/lib/core/`              | The way into the Rust core. The core runs in a worker (`worker.ts`) and the page calls it through `client.ts`; `session.ts` and `idb-store.ts` keep the core's files in the browser; `account-source.ts`, `airspace-source.ts`, `forecast-source.ts`, `sites-source.ts` and `wind-source.ts` send the core's requests to the server. `pkg/` is built by `scripts/build-core.sh` and not committed. |
| `src/lib/account/`           | The pilot's account: when a sync runs (`model.svelte.ts`), the form to sign in, the signed-in screen, and the sheet that shares a flight.                                                                                                                                                                                                                                                          |
| `src/lib/airspace/`          | The pilot's airspace: when the server is asked for it (`model.svelte.ts`), when a map asks the core for what it shows (`view.ts`), how it is drawn (`layers.ts`), and the sentences about files and sources (`words.ts`).                                                                                                                                                                          |
| `src/lib/design/`            | Number formatting, the climb colours, and the few components the app draws itself: metric, icon, dialogs, menu. The rules behind them are in `docs/design.md`.                                                                                                                                                                                                                                     |
| `src/lib/forecast/`          | The forecast for a place: the table of one day (`ForecastTable.svelte`), which days, hours and rows it has (`table.ts`), and the sheet around it with the day, the model and what the numbers are.                                                                                                                                                                                                 |
| `src/lib/logbook/`           | The logbook's model over the core, import, the list, the track thumbnail and the chart of the statistics.                                                                                                                                                                                                                                                                                          |
| `src/lib/flight/`            | One flight: map, panel, altitude chart, details.                                                                                                                                                                                                                                                                                                                                                   |
| `src/lib/map/`               | The shared map setup, the style preparation, label fonts, the map of all flights, the pilot's position, and the panel a chosen station or site opens in.                                                                                                                                                                                                                                           |
| `src/lib/sites/`             | Flying sites: when to ask the server and how long to use an answer (`watch.ts`), the signs (`symbols.ts`) and the layer (`mark.ts`), the sentences around them (`words.ts`), the list, one site with its compass.                                                                                                                                                                                  |
| `src/lib/wind/`              | Live wind: when to ask the server and what to keep (`watch.ts`), how a station is drawn (`mark.ts`), the list of stations, one station with its charts.                                                                                                                                                                                                                                            |
| `src/app.css`                | Colours, type sizes and spacing, as custom properties.                                                                                                                                                                                                                                                                                                                                             |
| `e2e/`                       | Tests that use the built site in a browser and keep the screenshots.                                                                                                                                                                                                                                                                                                                               |
| `scripts/build-core.sh`      | Builds the core for the browser. Its last step, `scripts/strip-core.mjs`, removes from the module what only the phones use.                                                                                                                                                                                                                                                                        |
| `scripts/check-licences.mjs` | The licence allowlist for npm packages.                                                                                                                                                                                                                                                                                                                                                            |
| `static/favicon.svg`         | The app icon, the same drawing as the iPhone's.                                                                                                                                                                                                                                                                                                                                                    |
| `../../map/style.json`       | The map style shared by all platforms.                                                                                                                                                                                                                                                                                                                                                             |
| `../ios/Venti/Resources/`    | The sample flight and the label fonts. The repository holds one copy of each; the build takes them from there.                                                                                                                                                                                                                                                                                     |

Tests of a module sit beside it as `*.test.ts`.

## What the app does today

- **Logbook.** Import IGC flight files with the button or by dropping them on the page, several at once. Flights are listed by year with totals. A flight that is already there is recognised; a file that is not a flight is named with the reason. A new logbook offers a sample flight. It is made up, generated by `venti-flight`, not a real pilot's. The totals lead to the statistics: the last twelve months as a bar chart, the gliders flown, and the pilot's bests.
- **Flight.** The track on a terrain map, coloured by climb and sink, with the scored route under it and the altitude profile in a panel. Drag along the profile, or use the arrow keys on it, to move a marker along the track and read time, altitude and climb at that moment, or let the flight play. The details list how the flight scores, the airspace it entered, its thermals and its glides; choosing one marks it. The map can be tilted into 3D terrain. The flight's file can be downloaded again.
- **Map.** A terrain map with every logbook flight on it and, on request, your position. With a Venti server behind the site, every wind station in view is an arrow pointing where the wind blows, with mean wind and gusts under it; choosing one shows its reading, its last 24 hours and whose data it is. The same stations are there as a list. Once the map shows a valley and not a country, the take-offs, landings, winch sites and training slopes the server has are round signs on it, dark for a take-off and light for a landing; choosing one shows what its source says about it, the wind it works in as words and on a small compass, and where to correct it. They are there as a list too. A site's page and a station's page lead to the forecast for their place: a table of one day, hour by hour, with the wind at every height the weather model gives, cloud, rain and temperature, one model at a time, and under it what the numbers are and that none of them says whether the day can be flown. The first source is OpenStreetMap, which has only part of the sites: a sign is not permission to fly there, and a place without one may still be a site.
- **Airspace.** Both maps draw airspace: magenta where a paraglider stays out, amber where something deserves attention, a thin grey line for the rest, named with floor and ceiling when zoomed in. It comes from the Venti server, which the app asks for the airspace around what a map shows and whose answer it keeps in the browser, and from OpenAir files the pilot brings in under Settings, Airspace, with a button or by dropping them there. That screen lists both, with the provider's credit, licence and notice, says what a file left out, and has the switch that turns the server's airspace off. A flight's details say which of that airspace the flight entered: when, how long, between which heights, and "possibly" where the app cannot place a limit above ground or a flight level. That is a check against the airspace the app has today, not that of the day of the flight, and it knows nothing about what was active. Nothing warns: the cockpit is the phone's.
- **Account.** Under Settings: sign in to the Venti server the site is served from, or create an account there, with an invite code where the server takes accounts by invite. The server says who may register, and the form follows: no field for a code where anyone may, and no form where the server takes no new accounts. Signed in, the logbook is kept in step with the server: when the site is opened or comes back to the front, after an import or a deletion, and on request. The screen says how the two stand and what the last sync did. Sign out, or delete the account with its flights on the server; the flights in the browser stay either way.
- **Sharing.** From a flight's menu: who can see the flight (only you, anyone with the link, everyone), and the link to its page on the server, which can be copied. Nothing is shared until the pilot says so.

On a narrow screen the app is laid out like the iPhone app: navigation at the bottom, the flight panel over the lower part of the map. On a wide one the navigation is a rail at the side, the flight panel stands beside the map with the details under it, and the logbook has columns for two more numbers.

## Where the flights are kept

In the browser, in IndexedDB, under the names the core uses on a phone's disk. Without an account they are on this device only and never sent anywhere. With one they are also copied to the server, private until shared, and a flight deleted here is deleted there and on the pilot's other devices. `docs/architecture.md` describes how the core and the browser's storage work together and what happens with two tabs, a full disk and a browser that offers no storage.

A browser may remove a site's data: when the device runs short of space, when the pilot clears site data, and in Safari after seven days without a visit _(to verify: this is Apple's stated policy for script-writable storage; it was not tested)_. After the first import the app asks the browser to keep its data (`navigator.storage.persist()`), which browsers grant or not by their own rules. Without an account, the files the pilot imported remain the copy to keep.

## Where the session is kept

Signing in gives the browser a session token, which goes with every request about the account. A phone keeps it in its Keychain or Keystore. A browser offers a page nothing of the kind: whatever a page's code can read, every script that runs on the same origin can read. With the server's API as it is (the token travels in a header that the page's own code sets), the token has to be somewhere scripts can reach.

It is kept in IndexedDB, in the database that holds the logbook but not among the core's files, and it is read by the worker that sends the requests. The page itself never holds it.

- **What that is good for.** The browser sends it nowhere on its own account, as it would a cookie, so no other site can make the browser act for the pilot. It lasts across visits and is shared by the site's tabs, like the logbook, which a sync that runs by itself needs. It goes when the pilot signs out, when the server no longer takes it, and with the site's data.
- **What it does not protect against.** A script running on this origin could read the token and act for the account until that session is signed out: read, upload, share and delete flights. It could not delete the account or change the password; those need the password. The app loads no script from anywhere else: no analytics, no fonts, no content network, and the map engine is part of the build. What remains is a flaw in the app or in a package it is built from that lets a stranger's script run, and whoever can change the files on the web server. Against the first there is the content security policy below: a script that is not one of the app's files does not run, and a page can send nothing to a host the app does not use.
- **Why not elsewhere.** `localStorage` is as readable and cannot be reached from the worker. `sessionStorage` or memory alone would sign the pilot out with every closed tab, with a password of fifteen characters and a sync that is meant to run by itself.

The better place is a cookie that scripts cannot read (`HttpOnly`). The server sets no cookies today, by a decision recorded in `docs/architecture.md`; what it would have to offer is listed there.

## Content security policy

Every page carries a content security policy: a list of what the page may load, run and talk to, which the browser enforces. It is there because of the session token (above): a stranger's script on a page could read the token, and the policy keeps such a script from running and from sending anything away.

The build writes the policy into each page as a `meta` element, ahead of everything the page loads. That is SvelteKit's own `csp` setting (`vite.config.ts`); the list is `src/lib/app/security-policy.ts`. It is in the page and not in a header because the app is static files that any web server may hand out. It is SvelteKit's setting and not a line in `src/app.html` because every page starts the app with a small script of its own, written into the page, whose text changes with every build: the setting works out its hash page by page.

```
default-src 'none';
script-src 'self' 'sha256-…';
style-src 'self';
style-src-attr 'unsafe-hashes' 'sha256-…';
img-src 'self' data:;
connect-src 'self' data: https://tiles.openfreemap.org https://tiles.mapterhorn.com;
worker-src 'self';
base-uri 'none';
form-action 'none'
```

| Entry                                                                    | Why it is there                                                                                                                                                                                                                                                                                                                                                                                                                                        |
| ------------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| `default-src 'none'`                                                     | Whatever no other line allows is refused: frames, plug-ins, media, web fonts, a manifest.                                                                                                                                                                                                                                                                                                                                                              |
| `script-src 'self'`                                                      | The app's own script files.                                                                                                                                                                                                                                                                                                                                                                                                                            |
| `script-src 'sha256-…'`                                                  | The one script written into each page, with which SvelteKit starts the app, by the hash of its text. No other script in a page runs, no handler in markup, and no text is made into code.                                                                                                                                                                                                                                                              |
| `style-src 'self'`                                                       | The app's own style sheets, the map engine's among them. No `style` element.                                                                                                                                                                                                                                                                                                                                                                           |
| `style-src-attr 'unsafe-hashes' 'sha256-…'`                              | One `style` attribute, which is SvelteKit's: it hides the element that tells a screen reader the title of the page the pilot has moved to. The build reads that attribute from the SvelteKit that is installed and names it by its hash, so an update cannot leave the two apart, and no other inline style is let through. What the app's scripts set (where a chart's labels stand, the colours of a legend) is not an attribute and needs no leave. |
| `img-src 'self'`                                                         | The app icon.                                                                                                                                                                                                                                                                                                                                                                                                                                          |
| `img-src data:`                                                          | The map engine's style sheet draws the icons of its own controls, such as the "i" of the credits, from `data:` addresses.                                                                                                                                                                                                                                                                                                                              |
| `connect-src 'self'`                                                     | What a page fetches of the app's own files: the fonts of the map's labels and the sample flight.                                                                                                                                                                                                                                                                                                                                                       |
| `connect-src data:`                                                      | For every script the app has no letters for, the label fonts hold a file that says "no glyphs here", and the build keeps the small files inside a script as `data:` addresses. The map asks for one when a name is in such a script (Arabic, Thai, Georgian). A `data:` address reaches no host.                                                                                                                                                       |
| `connect-src https://tiles.openfreemap.org https://tiles.mapterhorn.com` | A page asks the first for the description of the base map and the second for elevation tiles. They are the hosts in `src/lib/map/online.ts`; a unit test holds the map style and the policy together.                                                                                                                                                                                                                                                  |
| `worker-src 'self'`                                                      | The core and the map engine each run in a worker, both from files of the app. `script-src` would say the same; it is said outright so that a change there cannot change it unseen.                                                                                                                                                                                                                                                                     |
| `base-uri 'none'`, `form-action 'none'`                                  | No `base` element can send the app's relative addresses elsewhere, and no form posts anywhere: the app's forms are handled by its scripts.                                                                                                                                                                                                                                                                                                             |

Not in it, because the browser refused nothing without them: `'unsafe-inline'`, `'unsafe-eval'`, `'wasm-unsafe-eval'` and `blob:`. The core is WebAssembly, but it is compiled in its worker, which a policy in a page does not reach (below). The map engine is given its worker as a file of the app (`setWorkerUrl`) and makes none from a blob.

**How the list was found.** By starting from `default-src 'none'` with `'self'` alone for scripts, styles, images, connections and workers, and reading what Chromium refused while the screens were used; then by the browser tests in all four projects (2026-11-19). Those tests now fail on anything the browser refuses under the policy, in a page or in a worker (`refused` in `e2e/helpers.ts`), so a change that needs a new entry shows itself. `e2e/policy.spec.ts` checks that every page has the policy ahead of everything it loads, and that a script written into a page, a handler in markup, text made into code and a request to another host are all refused. A test in `e2e/sites.spec.ts` names a site in Arabic, Thai and Georgian, which is what `data:` is there for.

_To verify:_ everything here was seen in Chromium only. In Safari and Firefox two things may differ. A browser that holds a worker to its page's policy would need `'wasm-unsafe-eval'` in `script-src`, or the core does not start. And the map engine decodes its elevation tiles through an image with a `blob:` address where a browser lacks `createImageBitmap`, which would need `blob:` in `img-src`.

### What the web server has to send

Two things a policy in a page cannot do.

1. **Say who may show the site inside a frame.** Browsers ignore `frame-ancestors` in a `meta` element. The web server sends it as a header with the app's pages:

   ```
   Content-Security-Policy: frame-ancestors 'none'
   ```

   A page then has two policies, this one and its own, and the browser holds it to both. `deploy/Caddyfile` sends this header for the app's files, with `base-uri 'none'` beside it and `X-Frame-Options` for browsers that predate it.

2. **Reach the workers.** A worker is held to the policy that arrives with its own script, not to its page's ([the specification](https://www.w3.org/TR/CSP3/) lets only workers made from `blob:` and `data:` addresses inherit, and MDN says the same under "CSP in workers"). Seen in Chromium: the map engine's worker fetched tiles from an address the page's policy did not allow, and the core compiled WebAssembly under a page policy without `'wasm-unsafe-eval'`. The core's worker is the one that reads the session token and talks to the server. A stranger's script cannot become a worker (`worker-src 'self'`), so this is no way in; but the workers themselves are held to no list unless the web server sends one with their scripts. The build puts the core's worker under `/_app/immutable/workers/` and the map engine's under `/_app/immutable/assets/`, so the header goes with every script file of the app; a browser reads it only where a script is started as a worker:

   ```
   Content-Security-Policy: default-src 'none'; script-src 'self' 'wasm-unsafe-eval'; connect-src 'self' https://tiles.openfreemap.org https://tiles.mapterhorn.com
   ```

   `deploy/Caddyfile` sends it, and CI compares its tile hosts with those in `src/lib/map/online.ts`. Tried in Chromium twice: here, with a proxy in the browser tests that added the header to the core's worker, where every screen worked and without `'wasm-unsafe-eval'` the core did not start; and through the real Caddy by the session that set the header, where without the tile hosts the map engine's worker fetched no tile. Both workers are held to it.

## Where the wind and the sites come from

From the Venti server the site is served from: the app asks its own origin for `/v1/…`. Nothing has to be set. When the site is published, the web server that hands out these files also has to pass `/v1` on to the Venti server, and `/f`, where the pages of shared flights are. A site that only hands out the files answers those questions with "404 Not Found"; the map then has no wind and no flying sites and does not mention it, and the settings say that there is no account to sign in to. `docs/api.md` describes the server's API.

## Map tiles

Fetched from OpenFreeMap (base map) and Mapterhorn (terrain, asked for only up to zoom 12) until the project has its own tile server; `docs/architecture.md` has the terms. Both see the IP address and the map areas of whoever uses the app. Label fonts are served by the app itself. The map shows its sources' credits: written out on a wide map, behind the "i" button on a narrow one.

## Not deployed

Nothing publishes this app. Publishing is the owner's decision; the `web-site` artifact of the `Web` workflow is the folder to serve when that day comes.
