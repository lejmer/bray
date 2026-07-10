use bray_bound_tree::BoundUnitView;

use crate::CheckerCancellation;

/// Typed inputs for whole-unit semantic checking.
#[derive(Clone, Copy)]
pub struct UnitCheckRequest<'view> {
    view: BoundUnitView<'view>,
    cancellation: &'view dyn CheckerCancellation,
}

impl<'view> UnitCheckRequest<'view> {
    /// Creates a checker request over committed read-only bound structure.
    pub const fn new(
        view: BoundUnitView<'view>,
        cancellation: &'view dyn CheckerCancellation,
    ) -> Self {
        Self { view, cancellation }
    }

    /// Returns the read-only bound unit view to analyze.
    pub const fn view(self) -> BoundUnitView<'view> {
        self.view
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
