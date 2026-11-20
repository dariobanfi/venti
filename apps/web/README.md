# Venti for the web

Venti in a browser: the map with live wind, the logbook and the flying plans of a pilot's groups. It is a SvelteKit site over the Rust core, which runs in the browser as WebAssembly, and it builds to static files that any web server can serve.

## Build and run

Needs Rust from [rustup.rs](https://rustup.rs) (`rust-toolchain.toml` picks the version) and Node 22.17 or later.

```bash
cd apps/web
npm ci
scripts/build-core.sh   # the Rust core as WebAssembly; run again after every change to the core
npm run dev             # http://localhost:5173
```

The wind, the flying sites and the account come from a Venti server. For one that answers from recorded data, with a demo account, run this in a second terminal anywhere in the repository; `npm run dev` passes the app's requests on to it:

```bash
cargo run -p venti-server -- dev
```

## Test and build

```bash
npm run check && npm run format:check && npm run licences && npm test
npx playwright install chromium   # once, for the browser tests
npm run build && npm run test:e2e
```

`npm run build` writes the site to `build/`, and `npm run preview` serves it. To serve it from a folder of a site, build with `VENTI_BASE_PATH=/folder`. The web server in front must pass `/v1`, `/f` and `/live` on to a Venti server, as [deploy/Caddyfile](../../deploy/Caddyfile) does.

## More

- [docs/design.md](../../docs/design.md#web): how the web app looks and where it differs from the phones.
- [docs/architecture.md](../../docs/architecture.md#web): how it works: the core in a worker, the browser's storage, the content security policy.
- [docs/api.md](../../docs/api.md#try-it): the development server's demo account.
- [AGENTS.md](AGENTS.md): notes for AI sessions: the tests, the code's layout, the security policy entry by entry.
