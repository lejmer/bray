use crate::analysis::check_control_flow;
use crate::{CheckerOutcome, UnitCheckConclusions, UnitCheckRequest};

/// Whole-unit semantic checking invoked by binder orchestration.
///
/// Implementations must observe request cancellation while doing substantial
/// work and return [`CheckerOutcome::Cancelled`] without partial conclusions or
/// diagnostics. Implementations own semantic rules but never mutate or publish
/// the borrowed bound unit view.
pub trait UnitChecker: Sync {
    /// Checks one committed bound unit view and returns typed completion data.
    fn check_unit(&self, request: UnitCheckRequest<'_>) -> CheckerOutcome<UnitCheckConclusions> {
        check_control_flow(request)
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::BoundUnitId;

    use super::UnitChecker;
    use crate::test_support::{callable_key, recovered_tree};
    use crate::{CheckerOutcome, UnitCheckRequest, UnitCheckRoot};

    #[test]
    fn default_checking_recovers_from_an_error_body_without_panicking() {
        let key = callable_key();
        let unit = BoundUnitId::new(4);
        let (tree, root) = recovered_tree(unit, &key);
        let view = tree.view(&key);

        let Ok(request) = UnitCheckRequest::new(view, UnitCheckRoot::CallableBody(root), &|| false)
        else {
            panic!("matching test roots must produce checker requests");
        };

        let outcome = StructuralChecker.check_unit(request);

        let CheckerOutcome::Complete(result) = outcome else {
            panic!("recovered graph construction must complete");
        };

        assert_eq!(result.value().unit(), unit);
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn default_checking_publishes_nothing_after_cancellation() {
        let key = callable_key();
        let unit = BoundUnitId::new(5);
        let (tree, root) = recovered_tree(unit, &key);
        let view = tree.view(&key);

        let Ok(request) = UnitCheckRequest::new(view, UnitCheckRoot::CallableBody(root), &|| true)
        else {
            panic!("matching test roots must produce checker requests");
        };

        let outcome = StructuralChecker.check_unit(request);

        assert_eq!(outcome, CheckerOutcome::Cancelled);
    }

    #[test]
    fn requests_reject_roots_from_another_bound_unit() {
        let key = callable_key();
        let (tree, _) = recovered_tree(BoundUnitId::new(6), &key);
        let (_, foreign_root) = recovered_tree(BoundUnitId::new(7), &key);
        let view = tree.view(&key);

        let request =
            UnitCheckRequest::new(view, UnitCheckRoot::CallableBody(foreign_root), &|| false);

        assert!(matches!(
            request,
            Err(crate::UnitCheckRequestError::ForeignRoot)
        ));
    }

    struct StructuralChecker;

    impl UnitChecker for StructuralChecker {}
}
