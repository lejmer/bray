use crate::id::SourceId;
use crate::identity::SourceIdentity;
use crate::input::SourceInput;
use crate::loader::{SourceLoadError, SourceLoader};
use crate::origin::SourceOrigin;
use crate::snapshot::SourceSnapshot;
use crate::text::TextRange;
use crate::version::SourceVersion;

/// Owns loaded source snapshots and provides lookup by source ID.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct SourceStore {
    loader: SourceLoader,
    snapshots: Vec<SourceSnapshot>,
}

impl SourceStore {
    /// Creates an empty source store.
    pub const fn new() -> Self {
        Self {
            loader: SourceLoader::new(),
            snapshots: Vec::new(),
        }
    }

    /// Creates an empty source store with space for at least `capacity` items.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            loader: SourceLoader::new(),
            snapshots: Vec::with_capacity(capacity),
        }
    }

    /// Inserts source text and returns its assigned source ID.
    pub fn insert(
        &mut self,
        identity: SourceIdentity,
        origin: SourceOrigin,
        version: impl Into<SourceVersion>,
        text: impl Into<String>,
    ) -> Result<SourceId, SourceLoadError> {
        let snapshot = self.loader.load_snapshot(identity, origin, version, text)?;

        Ok(self.insert_loaded_snapshot(snapshot))
    }

    /// Inserts a source input and returns its assigned source ID.
    pub fn insert_input(&mut self, input: SourceInput) -> Result<SourceId, SourceLoadError> {
        let snapshot = self.loader.load_input(input)?;

        Ok(self.insert_loaded_snapshot(snapshot))
    }

    /// Returns the source snapshot for `source_id`.
    pub fn get(&self, source_id: SourceId) -> Option<&SourceSnapshot> {
        self.snapshots.get(source_id.to_index()?)
    }

    /// Returns the source text for `source_id`.
    pub fn text(&self, source_id: SourceId) -> Option<&str> {
        self.get(source_id).map(SourceSnapshot::text)
    }

    /// Returns the source text slice for `source_id` and `range`.
    pub fn text_slice(&self, source_id: SourceId, range: TextRange) -> Option<&str> {
        self.get(source_id)?.text_slice(range)
    }

    /// Returns the source byte slice for `source_id` and `range`.
    pub fn byte_slice(&self, source_id: SourceId, range: TextRange) -> Option<&[u8]> {
        self.get(source_id)?.byte_slice(range)
    }

    /// Returns snapshots for the same logical source in insertion order.
    pub fn snapshots_for_identity(
        &self,
        identity: SourceIdentity,
    ) -> impl Iterator<Item = &SourceSnapshot> + '_ {
        self.snapshots
            .iter()
            .filter(move |snapshot| snapshot.identity() == identity)
    }

    /// Returns the highest-version snapshot for a logical source.
    pub fn latest_for_identity(&self, identity: SourceIdentity) -> Option<&SourceSnapshot> {
        self.snapshots_for_identity(identity)
            .max_by_key(|snapshot| snapshot.version())
    }

    /// Returns the number of source snapshots in the store.
    pub const fn len(&self) -> usize {
        self.snapshots.len()
    }

    /// Returns whether the store contains no source snapshots.
    pub const fn is_empty(&self) -> bool {
        self.snapshots.is_empty()
    }

    /// Returns source snapshots in source ID order.
    pub fn iter(&self) -> std::slice::Iter<'_, SourceSnapshot> {
        self.snapshots.iter()
    }

    fn insert_loaded_snapshot(&mut self, snapshot: SourceSnapshot) -> SourceId {
        let source_id = snapshot.source_id();

        self.snapshots.push(snapshot);

        source_id
    }
}

impl IntoIterator for SourceStore {
    type Item = SourceSnapshot;
    type IntoIter = std::vec::IntoIter<SourceSnapshot>;

    fn into_iter(self) -> Self::IntoIter {
        self.snapshots.into_iter()
    }
}

impl<'a> IntoIterator for &'a SourceStore {
    type Item = &'a SourceSnapshot;
    type IntoIter = std::slice::Iter<'a, SourceSnapshot>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::SourceStore;
    use crate::{
        SourceId, SourceIdentity, SourceInput, SourceOrigin, SourceOriginKind, SourceVersion,
        TextRange, TextSize,
    };

    #[test]
    fn source_store_assigns_ids_and_fetches_snapshots() {
        let mut store = SourceStore::new();

        let first = insert(
            &mut store,
            SourceIdentity::new(20),
            SourceOrigin::file("main.bray"),
            SourceVersion::new(0),
            "one",
        );

        let second = insert(
            &mut store,
            SourceIdentity::new(21),
            SourceOrigin::stdin(),
            SourceVersion::new(1),
            "two",
        );

        assert_eq!(first, SourceId::new(0));
        assert_eq!(second, SourceId::new(1));
        assert_eq!(store.text(first), Some("one"));

        assert_eq!(
            store.get(second).map(|snapshot| snapshot.version()),
            Some(SourceVersion::new(1))
        );

        assert_eq!(store.len(), 2);
    }

    #[test]
    fn source_store_iterates_in_source_id_order() {
        let mut store = SourceStore::new();

        insert(
            &mut store,
            SourceIdentity::new(30),
            SourceOrigin::stdin(),
            SourceVersion::new(0),
            "a",
        );

        insert(
            &mut store,
            SourceIdentity::new(31),
            SourceOrigin::stdin(),
            SourceVersion::new(0),
            "b",
        );

        let texts = store
            .iter()
            .map(|snapshot| snapshot.text())
            .collect::<Vec<_>>();

        assert_eq!(texts, vec!["a", "b"]);
    }

    #[test]
    fn source_store_slices_snapshots_by_source_id_and_range() {
        let mut store = SourceStore::new();

        let source_id = insert(
            &mut store,
            SourceIdentity::new(32),
            SourceOrigin::stdin(),
            SourceVersion::new(0),
            "aébc",
        );

        assert_eq!(
            store.text_slice(
                source_id,
                TextRange::new(TextSize::new(1), TextSize::new(3))
            ),
            Some("é")
        );

        assert_eq!(
            store.byte_slice(
                source_id,
                TextRange::new(TextSize::new(2), TextSize::new(4))
            ),
            Some(&"aébc".as_bytes()[2..4])
        );
    }

    #[test]
    fn source_store_inserts_source_inputs() {
        let mut store = SourceStore::new();

        let input = SourceInput::lsp_open_document(
            SourceIdentity::new(40),
            "file:///main.bray",
            SourceVersion::new(7),
            "module main\n",
        );

        let source_id = match store.insert_input(input) {
            Ok(source_id) => source_id,
            Err(error) => panic!("test source input should insert successfully: {error:?}"),
        };

        let snapshot = match store.get(source_id) {
            Some(snapshot) => snapshot,
            None => panic!("inserted source ID should resolve to a snapshot"),
        };

        assert_eq!(snapshot.source_id(), SourceId::new(0));
        assert_eq!(snapshot.identity(), SourceIdentity::new(40));
        assert_eq!(snapshot.origin().kind(), SourceOriginKind::LspDocument);
        assert_eq!(snapshot.origin().lsp_uri(), Some("file:///main.bray"));
        assert_eq!(snapshot.version(), SourceVersion::new(7));
        assert_eq!(snapshot.text(), "module main\n");
    }

    #[test]
    fn source_store_finds_snapshots_for_the_same_logical_source() {
        let mut store = SourceStore::new();

        let identity = SourceIdentity::new(50);

        let old = insert(
            &mut store,
            identity,
            SourceOrigin::file("main.bray"),
            SourceVersion::new(1),
            "old",
        );

        let new = insert(
            &mut store,
            identity,
            SourceOrigin::file("main.bray"),
            SourceVersion::new(3),
            "new",
        );

        insert(
            &mut store,
            SourceIdentity::new(51),
            SourceOrigin::file("other.bray"),
            SourceVersion::new(4),
            "other",
        );

        let matching_sources = store
            .snapshots_for_identity(identity)
            .map(SourceSnapshotView::from)
            .collect::<Vec<_>>();

        assert_eq!(
            matching_sources,
            vec![
                SourceSnapshotView::new(old, SourceVersion::new(1), "old"),
                SourceSnapshotView::new(new, SourceVersion::new(3), "new")
            ]
        );

        assert_eq!(
            store
                .latest_for_identity(identity)
                .map(|snapshot| snapshot.source_id()),
            Some(new)
        );
    }

    #[test]
    fn source_store_is_send_and_sync() {
        assert_send_sync::<SourceStore>();
    }

    fn assert_send_sync<T: Send + Sync>() {}

    #[derive(Debug, Eq, PartialEq)]
    struct SourceSnapshotView<'source> {
        source_id: SourceId,
        version: SourceVersion,
        text: &'source str,
    }

    impl<'source> SourceSnapshotView<'source> {
        fn new(source_id: SourceId, version: SourceVersion, text: &'source str) -> Self {
            Self {
                source_id,
                version,
                text,
            }
        }
    }

    impl<'source> From<&'source crate::SourceSnapshot> for SourceSnapshotView<'source> {
        fn from(snapshot: &'source crate::SourceSnapshot) -> Self {
            Self::new(snapshot.source_id(), snapshot.version(), snapshot.text())
        }
    }

    fn insert(
        store: &mut SourceStore,
        identity: SourceIdentity,
        origin: SourceOrigin,
        version: SourceVersion,
        text: &str,
    ) -> SourceId {
        match store.insert(identity, origin, version, text) {
            Ok(source_id) => source_id,
            Err(error) => panic!("test source should insert successfully: {error:?}"),
        }
    }
}
