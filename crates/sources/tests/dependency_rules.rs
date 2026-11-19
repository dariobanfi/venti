//! Enforces the crate dependency rules of `docs/architecture.md`.
//!
//! - `venti-domain` depends on no other workspace crate.
//! - `venti-sources` depends only on `venti-domain`.
//! - A provider crate (`venti-sources-*`) depends only on those two.
//! - Only binaries that assemble the registry depend on provider crates, so
//!   no other code can name a provider-specific type.
//! - `venti-flight` and `venti-sensors` depend only on `venti-domain`.
//! - `venti-core`, the client core, never depends on the source framework:
//!   clients talk to our server, never to a data provider.
//! - `venti-core-wasm`, the browser's binding, depends only on `venti-core`:
//!   like every shell it calls the core and nothing below it.

use std::collections::BTreeMap;
use std::process::Command;

/// Crates that assemble the registry and may therefore depend on provider crates.
const ASSEMBLERS: [&str; 1] = ["venti-ingest"];

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
            "venti-sources" | "venti-flight" | "venti-sensors" => Some(&["venti-domain"]),
            "venti-core" => Some(&["venti-domain", "venti-flight", "venti-sensors"]),
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
