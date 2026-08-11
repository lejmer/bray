#[path = "src/catalog_digest.rs"]
mod catalog_digest;
#[path = "src/catalog_revision.rs"]
mod catalog_revision;

use std::path::PathBuf;

const EXPECTED_DIGEST: &str = include_str!("src/catalog/generated/catalog.sha256");
const MANIFEST: &str = include_str!("catalog/catalog.braydef-manifest");

fn main() {
    println!("cargo::rerun-if-changed=catalog/catalog.braydef-manifest");
    println!("cargo::rerun-if-changed=src/catalog/generated/catalog.sha256");

    for relative_path in catalog_digest::manifest_paths(MANIFEST) {
        println!("cargo::rerun-if-changed=catalog/{relative_path}");
    }

    if std::env::var_os("CARGO_FEATURE_GENERATION").is_some() {
        return;
    }

    let catalog_directory = PathBuf::from("catalog");

    let digest = match catalog_digest::source_digest(
        catalog_revision::CatalogGrammarRevision::SUPPORTED,
        MANIFEST,
        &catalog_directory,
    ) {
        Ok(digest) => digest,
        Err(error) => panic!("failed to hash compiler-known catalog sources: {error}"),
    };

    assert_eq!(
        digest,
        EXPECTED_DIGEST.trim(),
        "compiler-known generated output is stale, run `cargo xtask compiler-known generate`"
    );
}
