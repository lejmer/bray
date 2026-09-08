use crate::catalog_revision::CatalogGrammarRevision;

pub(crate) fn source_digest_with<T: AsRef<[u8]>, E>(
    grammar_revision: CatalogGrammarRevision,
    manifest: &str,
    mut contents: impl FnMut(&str) -> Result<T, E>,
) -> Result<String, E> {
    let mut hasher = blake3::Hasher::new();

    hasher.update(b"bray compiler-known catalog\0");
    hasher.update(&grammar_revision.raw().to_le_bytes());
    hasher.update(manifest.as_bytes());

    for relative_path in manifest_paths(manifest) {
        let contents = contents(relative_path)?;

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

#[cfg(test)]
mod tests {
    use super::source_digest_with;
    use crate::catalog_revision::CatalogGrammarRevision;

    #[test]
    fn catalog_digest_tracks_the_supplied_generation_inputs() {
        let digest = |contents: &[u8]| {
            source_digest_with(
                CatalogGrammarRevision::SUPPORTED,
                "first\nsecond\n",
                |path| {
                    Ok::<_, ()>(if path == "first" {
                        contents
                    } else {
                        b"unchanged"
                    })
                },
            )
            .unwrap()
        };

        assert_eq!(digest(b"original"), digest(b"original"));
        assert_ne!(digest(b"original"), digest(b"changed"));

        assert_eq!(
            source_digest_with::<&[u8], _>(
                CatalogGrammarRevision::SUPPORTED,
                "missing\n",
                |_| Err(7)
            ),
            Err(7),
        );
    }
}
