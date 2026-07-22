use std::collections::BTreeMap;

use crate::{InterfaceCoherenceRecord, InterfaceSymbolReference};

pub(super) fn coherence_record_indexes(
    records: &[InterfaceCoherenceRecord],
) -> Option<BTreeMap<&InterfaceSymbolReference, Vec<u32>>> {
    let mut indexes = BTreeMap::<_, Vec<_>>::new();

    for (index, record) in records.iter().enumerate() {
        let index = u32::try_from(index).ok()?;

        for implementation in &*record.implementations {
            indexes.entry(implementation).or_default().push(index);
        }
    }

    Some(indexes)
}
