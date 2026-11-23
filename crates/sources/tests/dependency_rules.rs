//! Enforces the crate dependency rules of `docs/architecture.md`.
//!
//! - `venti-domain` depends on no other workspace crate.
//! - `venti-sources` depends only on `venti-domain`.
//! - A provider crate (`venti-sources-*`) depends only on those two, and
//!   on what two providers share and no provider owns: `venti-hdf5`, a
//!   file format's reader, and `venti-outline`, the cut of a map tile
//!   along a country's outline.
//! - Only the server, which assembles the registry, depends on provider
//!   crates, so no other code can name a provider-specific type.
//! - The server may depend on `venti-flight`: it works out from a flight
//!   file what the apps work out, with the same code. It may depend on
//!   `venti-tools`: its connector offers the tools the app's assistant has,
//!   from the one definition. It never depends on the apps' own crates.
//! - `venti-mcp`, the protocol of the server's connector, depends on no
//!   other workspace crate: it knows the protocol and nothing about wind,
//!   airspace or this server. Only the server uses it.
//! - `venti-flight`, `venti-sensors`, `venti-airspace` and `venti-leagues`
//!   depend only on `venti-domain`.
//! - `venti-api-types`, the wire format shared by server and clients, and
//!   `venti-store`, the server's persistence, depend only on `venti-domain`.
//! - `venti-core`, the client core, never depends on the source framework:
//!   clients talk to our server, never to a data provider.
//! - `venti-core-wasm`, the browser's binding, depends only on `venti-core`:
//!   like every shell it calls the core and nothing below it.
//! - Nothing a client is built from reaches a server-side crate: a crate
//!   that depends on `venti-core`, and `venti-core` itself, never depends on
//!   the store, the server, the source framework or the region packs, not
//!   even through other crates.
//! - What the connector is made of cannot open a connection: nothing
//!   `venti-mcp` or `venti-tools` is built from, in or outside the
//!   workspace, is an HTTP client, a socket library or the runtime such
//!   code needs. A tool call can therefore not be made to fetch anything.

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

/// Data built into a crate that the apps may carry: it fetches nothing and
/// depends on no crate at all, so it is no way to a provider. The world's
/// time zones (2026-11-23) give a flight's times by the clock where it was
/// flown, offline on a phone. Checked below: such a crate has no
/// dependencies.
const BUILT_IN_DATA: [&str; 1] = ["venti-sources-timezones"];

/// Crates that assemble the registry and may therefore depend on provider crates.
const ASSEMBLERS: [&str; 1] = ["venti-server"];

/// Crates made for the apps alone: the client core, its browser binding,
/// and what the in-app assistant's provider, the cockpit and the handing
/// of a flight to a league are built from. A flight goes to a league from
/// the pilot's own device, never through the server. What the server
/// shares with the apps is `venti-domain`, `venti-api-types`,
/// `venti-flight`, `venti-airspace` and `venti-tools`.
const CLIENT_ONLY: [&str; 5] = [
    "venti-core",
    "venti-core-wasm",
    "venti-leagues",
    "venti-llm",
    "venti-sensors",
];

/// Crates through which a program opens a connection: HTTP clients, the
/// runtime their sockets live in, and the socket and TLS libraries below.
const NETWORK: [&str; 12] = [
    "reqwest",
    "ureq",
    "hyper",
    "hyper-util",
    "h2",
    "tokio",
    "mio",
    "socket2",
    "rustls",
    "native-tls",
    "openssl",
    "curl",
];

/// Every package of the lock file with the packages it depends on, of
/// every kind and for every platform: more than any one build uses.
fn locked_dependencies() -> BTreeMap<String, Vec<String>> {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../../Cargo.lock");
    let text = std::fs::read_to_string(path).expect("the workspace has a lock file");
    let lock: toml::Table = text.parse().expect("the lock file is TOML");
    let mut graph: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for package in lock["package"].as_array().expect("a list of packages") {
        let name = package["name"].as_str().expect("a name").to_owned();
        let dependencies = package
            .get("dependencies")
            .and_then(toml::Value::as_array)
            .into_iter()
            .flatten()
            // "name", or "name version" where two versions are locked.
            .filter_map(|dependency| dependency.as_str()?.split(' ').next())
            .map(str::to_owned);
        graph.entry(name).or_default().extend(dependencies);
    }
    graph
}

fn workspace_dependencies() -> BTreeMap<String, Vec<String>> {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata runs");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();

    let packages = metadata["packages"].as_array().unwrap();
    let names: Vec<&str> = packages
        .iter()
        .map(|p| p["name"].as_str().unwrap())
        .collect();
    packages
        .iter()
        .map(|package| {
            let internal = package["dependencies"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|dep| dep["kind"].is_null()) // normal dependencies only
                .map(|dep| dep["name"].as_str().unwrap())
                .filter(|name| names.contains(name))
                .map(str::to_owned)
                .collect();
            (package["name"].as_str().unwrap().to_owned(), internal)
        })
        .collect()
}

fn is_provider(name: &str) -> bool {
    name.starts_with("venti-sources-") && !BUILT_IN_DATA.contains(&name)
}

#[test]
fn built_in_data_depends_on_nothing() {
    let output = Command::new(env!("CARGO"))
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo metadata runs");
    let metadata: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    for package in metadata["packages"].as_array().unwrap() {
        let name = package["name"].as_str().unwrap();
        if !BUILT_IN_DATA.contains(&name) {
            continue;
        }
        let normal: Vec<&str> = package["dependencies"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|d| d["kind"].is_null())
            .map(|d| d["name"].as_str().unwrap())
            .collect();
        assert!(
            normal.is_empty(),
            "{name} is data the apps carry and must depend on nothing, but depends on {normal:?}"
        );
    }
}

#[test]
fn crate_dependencies_point_inward() {
    let graph = workspace_dependencies();
    let mut problems = Vec::new();

    for (name, dependencies) in &graph {
        let allowed: Option<&[&str]> = match name.as_str() {
            "venti-domain" => Some(&[]),
            "venti-sources" | "venti-flight" | "venti-sensors" | "venti-airspace"
            | "venti-api-types" | "venti-store" | "venti-leagues" => Some(&["venti-domain"]),
            "venti-regions" => Some(&["venti-domain", "venti-sources"]),
            // The model provider's crate stands alone: it knows JSON and
            // HTTP shapes, nothing about flying or about this app's data.
            "venti-llm" => Some(&[]),
            // So does the connector's protocol: it reads an HTTP request
            // into what is asked and builds the answer, for any tools.
            "venti-mcp" => Some(&[]),
            // The tools read the server's answers, in the server's own
            // wire format, say what an airspace means for a paraglider
            // from the table the cockpit warns by, and work out a
            // forecast's thermals by the flight engine's code, which is
            // what the apps show.
            "venti-tools" => Some(&[
                "venti-domain",
                "venti-api-types",
                "venti-airspace",
                "venti-flight",
            ]),
            "venti-core" => Some(&[
                "venti-domain",
                "venti-flight",
                "venti-sensors",
                "venti-airspace",
                "venti-api-types",
                "venti-leagues",
                "venti-llm",
                "venti-tools",
                // The world's time zones, data built in (`BUILT_IN_DATA`).
                "venti-sources-timezones",
            ]),
            "venti-core-wasm" => Some(&["venti-core"]),
            // Data built in depends on nothing (`built_in_data_depends_on_nothing`).
            data if BUILT_IN_DATA.contains(&data) => Some(&[]),
            // The reader of the HDF5 files that radar products come in
            // stands alone: it knows bytes, no provider and no radar.
            "venti-hdf5" => Some(&[]),
            // The outline of a country and the cut of a map tile along
            // it, which two providers of imagery share: it knows tiles and
            // no provider.
            "venti-outline" => Some(&["venti-domain", "venti-sources"]),
            // A provider's crate, and what two of them share: the reader
            // of one file format, and the cut along an outline.
            provider if is_provider(provider) => Some(&[
                "venti-domain",
                "venti-sources",
                "venti-hdf5",
                "venti-outline",
            ]),
            _ => None,
        };
        if let Some(allowed) = allowed {
            for dependency in dependencies {
                if !allowed.contains(&dependency.as_str()) {
                    problems.push(format!("{name} must not depend on {dependency}"));
                }
            }
        }
        if is_server_side(name) {
            for dependency in dependencies
                .iter()
                .filter(|d| CLIENT_ONLY.contains(&d.as_str()))
            {
                problems.push(format!(
                    "{name} runs on the server and must not depend on {dependency}, \
                     which is made for the apps"
                ));
            }
        }
        if !ASSEMBLERS.contains(&name.as_str()) {
            for dependency in dependencies.iter().filter(|d| is_provider(d)) {
                problems.push(format!(
                    "{name} must not depend on the provider crate {dependency}; \
                     only {ASSEMBLERS:?} may"
                ));
            }
        }
    }

    assert!(
        graph.contains_key("venti-domain"),
        "workspace metadata looks wrong: {graph:?}"
    );
    assert!(
        problems.is_empty(),
        "dependency rules broken:\n{}",
        problems.join("\n")
    );
}

/// Whether a crate exists only for the server.
fn is_server_side(name: &str) -> bool {
    is_provider(name)
        || [
            "venti-server",
            "venti-store",
            "venti-sources",
            "venti-regions",
            "venti-mcp",
        ]
        .contains(&name)
}

/// Every workspace crate `name` depends on, directly or through others.
fn reachable(graph: &BTreeMap<String, Vec<String>>, name: &str) -> BTreeSet<String> {
    let mut seen = BTreeSet::new();
    let mut queue = vec![name.to_owned()];
    while let Some(next) = queue.pop() {
        for dependency in graph.get(&next).into_iter().flatten() {
            if seen.insert(dependency.clone()) {
                queue.push(dependency.clone());
            }
        }
    }
    seen
}

#[test]
fn clients_never_reach_server_side_crates() {
    let graph = workspace_dependencies();
    let mut problems = Vec::new();

    for name in graph.keys() {
        let dependencies = reachable(&graph, name);
        let is_client = name == "venti-core" || dependencies.contains("venti-core");
        if !is_client {
            continue;
        }
        for dependency in dependencies.iter().filter(|d| is_server_side(d)) {
            problems.push(format!(
                "{name} is part of a client and must not depend on {dependency}, \
                 directly or through another crate"
            ));
        }
    }

    assert!(
        graph.contains_key("venti-core"),
        "workspace metadata looks wrong: {graph:?}"
    );
    assert!(
        problems.is_empty(),
        "dependency rules broken:\n{}",
        problems.join("\n")
    );
}

#[test]
fn what_the_connector_is_made_of_cannot_open_a_connection() {
    let graph = locked_dependencies();
    let mut problems = Vec::new();

    for name in ["venti-mcp", "venti-tools"] {
        assert!(graph.contains_key(name), "{name} is not in the lock file");
        for dependency in reachable(&graph, name) {
            if NETWORK.contains(&dependency.as_str()) {
                problems.push(format!(
                    "{name} is built from {dependency}, which can open a connection"
                ));
            }
        }
    }

    // The check sees such a crate where there is one: the server fetches
    // from providers.
    assert!(
        reachable(&graph, "venti-server").contains("reqwest"),
        "the lock file looks wrong: the server has no HTTP client"
    );
    assert!(
        problems.is_empty(),
        "dependency rules broken:\n{}",
        problems.join("\n")
    );
}
