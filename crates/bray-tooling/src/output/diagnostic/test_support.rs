use bray_source::{SourceIdentity, SourceOrigin, SourceStore, SourceVersion};

pub(crate) fn file_source_store(text: &str) -> SourceStore {
    let mut sources = SourceStore::new();

    match sources.insert(
        SourceIdentity::new(0),
        SourceOrigin::file("main.bray"),
        SourceVersion::new(0),
        text,
    ) {
        Ok(_) => {}
        Err(error) => panic!("test source should insert: {error:?}"),
    }

    sources
}
