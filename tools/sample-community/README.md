# Sample community

Puts seven made-up sample pilots (names, countries, drawn pictures, generated flights, the six sample public groups and a few messages) into a running Venti through its API. Runnable again: it skips what is there.

```bash
docker compose exec server venti-server invite --count 7     # in deploy/, on a server that takes accounts by invite
python3 tools/sample-community/seed.py http://127.0.0.1:8088 --invite CODE1 --invite CODE2 …
```

It runs `cargo run -p venti-server --example sample_community` for the files (`--data FOLDER` to use ones written before). The pilots' password is the development mode's unless `--password` says otherwise: keep that to a stack on your own computer. Who the pilots are: `crates/server/src/dev/sample_pilots.rs`.
