use crate::BoundUnitId;

pub(super) fn checked_unit_entry<T>(
    expected_unit: BoundUnitId,
    actual_unit: BoundUnitId,
    index: Option<usize>,
    entries: &[T],
) -> Option<&T> {
    if actual_unit != expected_unit {
        return None;
    }

    index.and_then(|index| entries.get(index))
}
