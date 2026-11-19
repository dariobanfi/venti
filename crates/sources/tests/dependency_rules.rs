//! Enforces the crate dependency rules of `docs/architecture.md`.
//!
//! - `venti-domain` depends on no other workspace crate.
//! - `venti-sources` depends only on `venti-domain`.
//! - A provider crate (`venti-sources-*`) depends only on those two.
//! - Only the server, which assembles the registry, depends on provider
//!   crates, so no other code can name a provider-specific type.
//! - `venti-flight`, `venti-sensors` and `venti-airspace` depend only on
//!   `venti-domain`.
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

use std::collections::{BTreeMap, BTreeSet};
use std::process::Command;

/// Crates that assemble the registry and may therefore depend on provider crates.
const ASSEMBLERS: [&str; 1] = ["venti-server"];

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
    name.starts_with("venti-sources-")
}

#[test]
fn crate_dependencies_point_inward() {
    let graph = workspace_dependencies();
    let mut problems = Vec::new();

    for (name, dependencies) in &graph {
        let allowed: Option<&[&str]> = match name.as_str() {
            "venti-domain" => Some(&[]),
            "venti-sources" | "venti-flight" | "venti-sensors" | "venti-airspace"
            | "venti-api-types" | "venti-store" => Some(&["venti-domain"]),
            "venti-regions" => Some(&["venti-domain", "venti-sources"]),
            // The model provider's crate stands alone: it knows JSON and
            // HTTP shapes, nothing about flying or about this app's data.
            "venti-llm" => Some(&[]),
            // The tools read the server's answers, in the server's own
            // wire format.
            "venti-tools" => Some(&["venti-domain", "venti-api-types"]),
            "venti-core" => Some(&[
                "venti-domain",
                "venti-flight",
                "venti-sensors",
                "venti-airspace",
                "venti-api-types",
                "venti-llm",
                "venti-tools",
            ]),
            "venti-core-wasm" => Some(&["venti-core"]),
            provider if is_provider(provider) => Some(&["venti-domain", "venti-sources"]),
            _ => None,
        };
        if let Some(allowed) = allowed {
            for dependency in dependencies {
                if !allowed.contains(&dependency.as_str()) {
                    problems.push(format!("{name} must not depend on {dependency}"));
                }
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
