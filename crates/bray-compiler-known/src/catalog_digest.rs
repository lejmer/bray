use std::path::Path;

use crate::catalog_revision::CatalogGrammarRevision;

pub(crate) fn source_digest(
    grammar_revision: CatalogGrammarRevision,
    manifest: &str,
    catalog_directory: &Path,
) -> Result<String, std::io::Error> {
    let mut hasher = blake3::Hasher::new();

    hasher.update(b"bray compiler-known catalog\0");
    hasher.update(&grammar_revision.raw().to_le_bytes());
    hasher.update(manifest.as_bytes());

    for relative_path in manifest_paths(manifest) {
        let contents = std::fs::read(catalog_directory.join(relative_path))?;

        hasher.update(relative_path.as_bytes());
        hasher.update(&[0]);
        hasher.update(&contents);
        hasher.update(&[0]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

pub(crate) fn manifest_paths(manifest: &str) -> impl Iterator<Item = &str> {
    manifest.lines().filter(|line| !line.is_empty())
}
