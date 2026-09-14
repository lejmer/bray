use crate::catalog_revision::CatalogGrammarRevision;

pub(crate) fn source_digest<T: AsRef<[u8]>, E>(
    grammar_revision: CatalogGrammarRevision,
    manifest: &str,
    mut read_source: impl FnMut(&str) -> Result<T, E>,
) -> Result<String, E> {
    let mut hasher = blake3::Hasher::new();

    hasher.update(b"bray compiler-known catalog\0");
    hasher.update(&grammar_revision.raw().to_le_bytes());
    hasher.update(manifest.as_bytes());

    for relative_path in manifest_paths(manifest) {
        let contents = read_source(relative_path)?;

        hasher.update(relative_path.as_bytes());
        hasher.update(&[0]);
        hasher.update(contents.as_ref());
        hasher.update(&[0]);
    }

    Ok(hasher.finalize().to_hex().to_string())
}

pub(crate) fn manifest_paths(manifest: &str) -> impl Iterator<Item = &str> {
    manifest.lines().filter(|line| !line.is_empty())
}
