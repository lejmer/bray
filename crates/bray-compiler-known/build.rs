#[path = "src/catalog_digest.rs"]
mod catalog_digest;

use std::path::PathBuf;

const EXPECTED_DIGEST: &str = include_str!("src/catalog/generated/digest.txt");
const MANIFEST: &str = include_str!("catalog/manifest.txt");

fn main() {
    println!("cargo::rerun-if-changed=catalog/manifest.txt");
    println!("cargo::rerun-if-changed=src/catalog/generated/digest.txt");

    for relative_path in catalog_digest::manifest_paths(MANIFEST) {
        println!("cargo::rerun-if-changed=catalog/{relative_path}");
    }

    if std::env::var_os("CARGO_FEATURE_GENERATION").is_some() {
        return;
    }

    let catalog_directory = PathBuf::from("catalog");
    let digest = match catalog_digest::source_digest(MANIFEST, &catalog_directory) {
        Ok(digest) => digest,
        Err(error) => panic!("failed to hash compiler-known catalog sources: {error}"),
    };

    assert_eq!(
        digest,
        EXPECTED_DIGEST.trim(),
        "compiler-known generated output is stale, run `cargo xtask compiler-known generate`"
    );
}
