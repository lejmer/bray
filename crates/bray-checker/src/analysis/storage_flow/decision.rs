use bray_bound_tree::StorageOperationStatus;
use bray_diagnostics::DiagnosticKind;

pub(super) const fn diagnostic_kind(status: StorageOperationStatus) -> Option<DiagnosticKind> {
    match status {
        StorageOperationStatus::Uninitialized => {
            Some(DiagnosticKind::CheckingUseOfUninitializedStorage)
        }
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
        StorageOperationStatus::InactiveProjection => {
            Some(DiagnosticKind::CheckingInactiveStorageProjection)
        }
        StorageOperationStatus::NotCopyable => Some(DiagnosticKind::CheckingTypeIsNotCopyable),
        StorageOperationStatus::Valid | StorageOperationStatus::Recovered => None,
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
        StorageOperationStatus::Valid => 0,
        StorageOperationStatus::Recovered => 1,
        StorageOperationStatus::Uninitialized => 2,
        StorageOperationStatus::Moved => 3,
        StorageOperationStatus::MissingMutationAuthority => 4,
        StorageOperationStatus::MissingOwnership => 5,
        StorageOperationStatus::InactiveProjection => 6,
        StorageOperationStatus::NotCopyable => 7,
        StorageOperationStatus::ConflictingBorrow => 8,
    }
}
