# Venti for the web

Venti in a browser: a SvelteKit site over the Rust core, which runs in the browser as WebAssembly. It builds to static files that any web server can serve.

Needs Rust from [rustup.rs](https://rustup.rs) and Node 22.17 or later.

```bash
cd apps/web
npm ci
scripts/build-core.sh              # the Rust core as WebAssembly; again after every change to the core
cargo run -p venti-server -- dev   # in a second terminal: a Venti server over recorded data
npm run dev                        # http://localhost:5173
```

- [docs/design.md](../../docs/design.md#web): how the web app looks and where it differs from the phones.
- [docs/architecture.md](../../docs/architecture.md#web): how it works; [docs/api.md](../../docs/api.md#try-it) has the development server's demo account.
- [AGENTS.md](AGENTS.md): working notes for AI sessions: the checks, the tests, the build for a server.
