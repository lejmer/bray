use std::collections::BTreeMap;

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TypedSymbolCollection<I, R> {
    records: Box<[R]>,
    indexes: BTreeMap<I, usize>,
}

impl<I, R> TypedSymbolCollection<I, R>
where
    I: Copy + Ord,
{
    pub(crate) fn new(records: Vec<R>, id: impl Fn(&R) -> I) -> Self {
        let indexes = records
            .iter()
            .enumerate()
            .map(|(index, record)| (id(record), index))
            .collect();

        Self {
            records: records.into_boxed_slice(),
            indexes,
        }
    }

    pub(crate) fn get(&self, id: I) -> Option<&R> {
        self.records.get(*self.indexes.get(&id)?)
    }

    pub(crate) fn records(&self) -> &[R] {
        &self.records
    }
}

#[cfg(test)]
mod tests {
    use super::TypedSymbolCollection;

    #[derive(Clone, Debug, Eq, PartialEq)]
    struct Record(u32);

    #[test]
    fn typed_collections_reject_unassigned_ids() {
        let collection = TypedSymbolCollection::new(vec![Record(2), Record(5)], |record| record.0);

        assert_eq!(collection.get(2), Some(&Record(2)));
        assert_eq!(collection.get(3), None);
        assert_eq!(collection.records(), [Record(2), Record(5)]);
    }
}
