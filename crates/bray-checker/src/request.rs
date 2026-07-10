use bray_bound_tree::BoundUnitDraft;

use crate::CheckerCancellation;

/// Typed inputs for whole-unit semantic checking.
#[derive(Clone, Copy)]
pub struct UnitCheckRequest<'draft> {
    draft: BoundUnitDraft<'draft>,
    cancellation: &'draft dyn CheckerCancellation,
}

impl<'draft> UnitCheckRequest<'draft> {
    /// Creates a checker request over committed read-only bound structure.
    pub const fn new(
        draft: BoundUnitDraft<'draft>,
        cancellation: &'draft dyn CheckerCancellation,
    ) -> Self {
        Self {
            draft,
            cancellation,
        }
    }

    /// Returns the read-only bound draft to analyze.
    pub const fn draft(self) -> BoundUnitDraft<'draft> {
        self.draft
    }

    /// Returns whether compilation cancellation has been requested.
    pub fn is_cancelled(self) -> bool {
        self.cancellation.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use super::UnitCheckRequest;

    #[test]
    fn requests_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<UnitCheckRequest<'static>>();
    }
}
