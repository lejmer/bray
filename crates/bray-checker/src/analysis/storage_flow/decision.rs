use bray_bound_tree::StorageOperationStatus;
use bray_diagnostics::DiagnosticKind;

pub(super) const fn diagnostic_kind(status: StorageOperationStatus) -> Option<DiagnosticKind> {
    match status {
        StorageOperationStatus::Moved => Some(DiagnosticKind::CheckingUseOfMovedStorage),
        StorageOperationStatus::ConflictingBorrow => {
            Some(DiagnosticKind::CheckingConflictingBorrow)
        }
        StorageOperationStatus::MissingMutationAuthority => {
            Some(DiagnosticKind::CheckingMissingMutationAuthority)
        }
        StorageOperationStatus::MissingOwnership => {
            Some(DiagnosticKind::CheckingMissingStorageOwnership)
        }
        StorageOperationStatus::Unreachable
        | StorageOperationStatus::Valid
        | StorageOperationStatus::Recovered
        | StorageOperationStatus::Uninitialized
        | StorageOperationStatus::InactiveProjection
        | StorageOperationStatus::NotCopyable => None,
    }
}

pub(super) const fn more_conservative(
    current: StorageOperationStatus,
    incoming: StorageOperationStatus,
) -> StorageOperationStatus {
    if status_rank(incoming) > status_rank(current) {
        incoming
    } else {
        current
    }
}

const fn status_rank(status: StorageOperationStatus) -> u8 {
    match status {
        StorageOperationStatus::Unreachable => 0,
        StorageOperationStatus::Valid => 1,
        StorageOperationStatus::Recovered => 2,
        StorageOperationStatus::Uninitialized => 3,
        StorageOperationStatus::Moved => 4,
        StorageOperationStatus::MissingMutationAuthority => 5,
        StorageOperationStatus::MissingOwnership => 6,
        StorageOperationStatus::InactiveProjection => 7,
        StorageOperationStatus::NotCopyable => 8,
        StorageOperationStatus::ConflictingBorrow => 9,
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::StorageOperationStatus;
    use bray_diagnostics::DiagnosticKind;

    use super::diagnostic_kind;

    #[test]
    fn storage_failures_map_to_exact_structured_diagnostics() {
        let cases = [
            (
                StorageOperationStatus::Moved,
                DiagnosticKind::CheckingUseOfMovedStorage,
            ),
            (
                StorageOperationStatus::ConflictingBorrow,
                DiagnosticKind::CheckingConflictingBorrow,
            ),
            (
                StorageOperationStatus::MissingMutationAuthority,
                DiagnosticKind::CheckingMissingMutationAuthority,
            ),
            (
                StorageOperationStatus::MissingOwnership,
                DiagnosticKind::CheckingMissingStorageOwnership,
            ),
        ];

        for (status, expected) in cases {
            assert_eq!(diagnostic_kind(status), Some(expected));
        }
    }
}
