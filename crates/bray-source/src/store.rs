use crate::id::SourceId;
use crate::origin::SourceOrigin;
use crate::snapshot::{SourceRevision, SourceSnapshot};
use crate::text::TextSizeOverflow;

/// Error returned when a source snapshot cannot be inserted.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum SourceStoreError {
    /// The store cannot assign another compact source ID.
    TooManySources { count: usize },
    /// The source text is too large for compact byte offsets.
    TextTooLarge(TextSizeOverflow),
}

impl From<TextSizeOverflow> for SourceStoreError {
    fn from(error: TextSizeOverflow) -> Self {
        Self::TextTooLarge(error)
    }
}

/// Owns source input snapshots and provides lookup by source ID.
#[derive(Debug, Default, Eq, PartialEq)]
pub struct SourceStore {
    snapshots: Vec<SourceSnapshot>,
}

impl SourceStore {
    /// Creates an empty source store.
    pub const fn new() -> Self {
        Self {
            snapshots: Vec::new(),
        }
    }

    /// Creates an empty source store with space for at least `capacity` items.
    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            snapshots: Vec::with_capacity(capacity),
        }
    }

    /// Inserts source text and returns its assigned source ID.
    pub fn insert(
        &mut self,
        origin: SourceOrigin,
        revision: impl Into<SourceRevision>,
        text: impl Into<String>,
    ) -> Result<SourceId, SourceStoreError> {
        let source_id = self.next_source_id()?;
        let snapshot = SourceSnapshot::new(source_id, origin, revision, text)?;

        self.snapshots.push(snapshot);

        Ok(source_id)
    }

    /// Returns the source snapshot for `source_id`.
    pub fn get(&self, source_id: SourceId) -> Option<&SourceSnapshot> {
        let index = usize::try_from(source_id.raw()).ok()?;

        self.snapshots.get(index)
    }

    /// Returns the source text for `source_id`.
    pub fn text(&self, source_id: SourceId) -> Option<&str> {
        self.get(source_id).map(SourceSnapshot::text)
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

    fn next_source_id(&self) -> Result<SourceId, SourceStoreError> {
        let raw = match u32::try_from(self.snapshots.len()) {
            Ok(raw) => raw,
            Err(_) => {
                return Err(SourceStoreError::TooManySources {
                    count: self.snapshots.len(),
                });
            }
        };

        Ok(SourceId::new(raw))
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
    use crate::{SourceId, SourceOrigin, SourceRevision};

    #[test]
    fn source_store_assigns_ids_and_fetches_snapshots() {
        let mut store = SourceStore::new();

        let first = insert(
            &mut store,
            SourceOrigin::file("main.bray"),
            SourceRevision::new(0),
            "one",
        );

        let second = insert(
            &mut store,
            SourceOrigin::stdin(),
            SourceRevision::new(1),
            "two",
        );

        assert_eq!(first, SourceId::new(0));
        assert_eq!(second, SourceId::new(1));
        assert_eq!(store.text(first), Some("one"));

        assert_eq!(
            store.get(second).map(|snapshot| snapshot.revision()),
            Some(SourceRevision::new(1))
        );

        assert_eq!(store.len(), 2);
    }

    #[test]
    fn source_store_iterates_in_source_id_order() {
        let mut store = SourceStore::new();

        insert(
            &mut store,
            SourceOrigin::stdin(),
            SourceRevision::new(0),
            "a",
        );

        insert(
            &mut store,
            SourceOrigin::stdin(),
            SourceRevision::new(0),
            "b",
        );

        let texts = store
            .iter()
            .map(|snapshot| snapshot.text())
            .collect::<Vec<_>>();

        assert_eq!(texts, vec!["a", "b"]);
    }

    #[test]
    fn source_store_is_send_and_sync() {
        assert_send_sync::<SourceStore>();
    }

    fn assert_send_sync<T: Send + Sync>() {}

    fn insert(
        store: &mut SourceStore,
        origin: SourceOrigin,
        revision: SourceRevision,
        text: &str,
    ) -> SourceId {
        match store.insert(origin, revision, text) {
            Ok(source_id) => source_id,
            Err(error) => panic!("test source should insert successfully: {error:?}"),
        }
    }
}
