pub(super) fn selection_ordinal_u32(value: usize) -> u32 {
    match u32::try_from(value) {
        Ok(value) => value,
        Err(_) => panic!(
            "A semantic-selection input ordinal cannot be represented by the public protocol. in selection_ordinal_u32, count: {:?}",
            value
        ),
    }
}

pub(super) fn selection_ordinal_u64(value: usize) -> u64 {
    match u64::try_from(value) {
        Ok(value) => value,
        Err(_) => panic!(
            "A semantic-selection input ordinal cannot be represented by the public protocol. in selection_ordinal_u64, count: {:?}",
            value
        ),
    }
}
