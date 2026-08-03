use bray_source::{
    SourceId, SourceIdentity, SourceInput, SourceLoadError, SourceOrigin, SourceSnapshot,
    SourceStore, SourceVersion,
};

/// Creates a virtual source snapshot for tests.
pub fn test_source_snapshot(text: &str) -> SourceSnapshot {
    match SourceSnapshot::new(
        SourceId::new(0),
        SourceIdentity::new(0),
        SourceOrigin::virtual_source("test-source"),
        SourceVersion::new(0),
        text,
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => panic!("test source should fit in TextSize: {error:?}"),
    }
}

/// Creates a source store containing virtual source snapshots for tests.
pub fn test_source_store(texts: impl IntoIterator<Item = impl AsRef<str>>) -> SourceStore {
    match try_test_source_store(texts) {
        Ok(store) => store,
        Err(error) => panic!("test source should insert successfully: {error:?}"),
    }
}

/// Creates indexed virtual source inputs with readable test names.
pub fn test_source_inputs(
    name_prefix: &str,
    texts: impl IntoIterator<Item = impl AsRef<str>>,
) -> Vec<SourceInput> {
    texts
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let identity = SourceIdentity::new(raw_source_identity(index));

            SourceInput::virtual_text(
                identity,
                format!("{name_prefix}-{index}.bray"),
                SourceVersion::new(0),
                text.as_ref(),
            )
        })
        .collect()
}

/// Returns a test source snapshot by compact source index.
pub fn test_source_at(sources: &SourceStore, index: u32) -> &SourceSnapshot {
    match sources.get(SourceId::new(index)) {
        Some(source) => source,
        None => panic!("test source {index} should exist"),
    }
}

/// Tries to create a source store containing virtual source snapshots for tests.
pub fn try_test_source_store(
    texts: impl IntoIterator<Item = impl AsRef<str>>,
) -> Result<SourceStore, SourceLoadError> {
    let mut store = SourceStore::new();

    for (index, text) in texts.into_iter().enumerate() {
        insert_source(&mut store, index, text.as_ref())?;
    }

    Ok(store)
}

fn insert_source(store: &mut SourceStore, index: usize, text: &str) -> Result<(), SourceLoadError> {
    let identity = SourceIdentity::new(raw_source_identity(index));
    let origin = SourceOrigin::virtual_source(format!("test-source-{index}"));

    store.insert(identity, origin, SourceVersion::new(0), text)?;

    Ok(())
}

fn raw_source_identity(index: usize) -> u32 {
    match u32::try_from(index) {
        Ok(raw) => raw,
        Err(error) => panic!("test source index should fit in u32: {error:?}"),
    }
}
