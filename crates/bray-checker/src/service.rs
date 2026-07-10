use crate::{CheckerOutcome, UnitCheckConclusions, UnitCheckRequest};

/// Whole-unit semantic checking invoked by binder orchestration.
///
/// Implementations must observe request cancellation while doing substantial
/// work and return [`CheckerOutcome::Cancelled`] without partial conclusions or
/// diagnostics. Implementations own semantic rules but never mutate or publish
/// the borrowed bound unit view.
pub trait UnitChecker: Sync {
    /// Checks one committed bound unit view and returns typed completion data.
    fn check_unit(&self, request: UnitCheckRequest<'_>) -> CheckerOutcome<UnitCheckConclusions>;
}
