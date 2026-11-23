//! `docs/sources.json`: every source the repository has an adapter of,
//! from its manifest, with its licence and its credit, for the developer
//! pages and for anyone who reads the repository without Rust.

use std::path::{Path, PathBuf};

use serde_json::{Value, json};
use venti_sources::testing::assert_golden;
use venti_sources::{Licence, Manifest};

fn repository() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// A licence as the list says it.
fn licence(licence: &Licence) -> Value {
    let mut said = json!({
        "name": licence.name,
        "credit": licence.attribution,
        "link": licence.link,
        "commercial": licence.commercial,
        "redistribution": licence.redistribution,
        "share_alike": licence.share_alike,
        "verified_on": licence.verified_on,
    });
    if let Some(notice) = &licence.notice {
        said["notice"] = json!(notice);
    }
    said
}

/// Rewritten with VENTI_UPDATE_GOLDEN=1, as the golden answers are.
#[test]
fn the_list_of_sources_in_the_repository_is_every_manifest() {
    let mut files: Vec<PathBuf> = std::fs::read_dir(repository().join("crates"))
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|crate_dir| {
            crate_dir
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("sources-"))
        })
        .flat_map(|crate_dir| {
            std::fs::read_dir(&crate_dir)
                .unwrap()
                .map(|entry| entry.unwrap().path())
                .filter(|file| {
                    file.file_name()
                        .and_then(|name| name.to_str())
                        .is_some_and(|name| name.starts_with("manifest") && name.ends_with(".toml"))
                })
                .collect::<Vec<_>>()
        })
        .collect();
    files.sort();
    assert!(!files.is_empty());

    let mut sources: Vec<(String, Value)> = files
        .iter()
        .map(|file| {
            let text = std::fs::read_to_string(file).unwrap();
            let manifest = Manifest::from_toml(&text)
                .unwrap_or_else(|error| panic!("{}: {error}", file.display()));
            // The crate's folder and the file's name: where the manifest
            // is in the repository.
            let name = |path: &Path| path.file_name().unwrap().to_string_lossy().into_owned();
            let within = format!("crates/{}/{}", name(file.parent().unwrap()), name(file));
            let mut said = json!({
                "id": manifest.id.as_str(),
                "provider": manifest.provider,
                "kinds": manifest.kinds,
                "countries": manifest.countries,
                "licence": licence(&manifest.licence),
                "manifest": within,
            });
            if !manifest.assets.is_empty() {
                said["assets"] = manifest
                    .assets
                    .iter()
                    .map(|asset| {
                        json!({
                            "name": asset.name,
                            "provider": asset.provider,
                            "licence": licence(&asset.licence),
                        })
                    })
                    .collect();
            }
            (manifest.id.as_str().to_owned(), said)
        })
        .collect();
    sources.sort_by(|a, b| a.0.cmp(&b.0));
    let ids: Vec<&str> = sources.iter().map(|(id, _)| id.as_str()).collect();
    let mut unique = ids.clone();
    unique.dedup();
    assert_eq!(unique, ids, "two manifests name one source");

    let list = json!({
        "description": "Every data source Venti's server has an adapter of, from its manifest \
                        (manifest.toml in each crates/sources-*), with its licence and the \
                        credit it asks for; docs/data-sources.md says more of each. Written by \
                        crates/server/tests/sources.rs.",
        "sources": sources.into_iter().map(|(_, said)| said).collect::<Vec<_>>(),
    });
    assert_golden(&repository().join("docs/sources.json"), &list);
}
