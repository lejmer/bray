use crate::analysis::check_control_flow;
use crate::{CheckerOutcome, CheckerRequestContext, ControlFlowCheckResult, UnitCheckRequest};

/// The standard Bray control-flow checker implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultControlFlowChecker;

/// Control-flow checking over one committed bound semantic unit.
///
/// Implementations must observe request cancellation while doing substantial
/// work and return [`CheckerOutcome::Cancelled`] without partial results or
/// diagnostics.
pub trait ControlFlowChecker<C>: Sync
where
    C: CheckerRequestContext + ?Sized,
{
    /// Checks one committed bound unit's control flow.
    fn check_control_flow(
        &self,
        request: UnitCheckRequest<'_, C>,
    ) -> CheckerOutcome<ControlFlowCheckResult> {
        check_control_flow(request)
    }
}

impl<C> ControlFlowChecker<C> for DefaultControlFlowChecker where C: CheckerRequestContext + ?Sized {}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundUnitId, ControlCompletionKind};

    use super::{ControlFlowChecker, DefaultControlFlowChecker};
    use crate::test_support::{
        TestCheckerContext, available_compiler_known_symbols, callable_entry, callable_key,
        normally_completing_recovered_tree, recovered_tree,
    };
    use crate::{CheckerOutcome, UnitCheckEntryContext, UnitCheckRequest, UnitCheckRoot};

    #[test]
    fn default_checking_recovers_from_an_error_body_without_panicking() {
        let key = callable_key();
        let unit = BoundUnitId::new(4);
        let (tree, root) = recovered_tree(unit, &key);
        let view = tree.view(&key);
        let entry = callable_entry(&key);
        let context = TestCheckerContext::new(false);

        let Ok(request) =
            UnitCheckRequest::new(view, UnitCheckRoot::CallableBody(root), &entry, &context)
        else {
            panic!("matching test roots must produce checker requests");
        };

        assert!(std::ptr::eq(
            request.available_compiler_known_symbols(),
            available_compiler_known_symbols()
        ));

        let outcome = DefaultControlFlowChecker.check_control_flow(request);

        let CheckerOutcome::Complete(result) = outcome else {
            panic!("recovered graph construction must complete");
        };

        assert_eq!(result.value().unit(), unit);
        assert!(
            result
                .value()
                .completion()
                .contains(ControlCompletionKind::Recovered)
        );
        assert!(result.value().is_recovered());
        assert!(result.diagnostics().is_empty());
    }

    #[test]
    fn default_checking_publishes_nothing_after_cancellation() {
        let key = callable_key();
        let unit = BoundUnitId::new(5);
        let (tree, root) = recovered_tree(unit, &key);
        let view = tree.view(&key);
        let entry = callable_entry(&key);
        let context = TestCheckerContext::new(true);

        let Ok(request) =
            UnitCheckRequest::new(view, UnitCheckRoot::CallableBody(root), &entry, &context)
        else {
            panic!("matching test roots must produce checker requests");
        };

        let outcome = DefaultControlFlowChecker.check_control_flow(request);

        assert_eq!(outcome, CheckerOutcome::Cancelled);
    }

    #[test]
    fn recovery_only_control_does_not_prove_normal_completion() {
        let key = callable_key();
        let unit = BoundUnitId::new(8);
        let (tree, root) = normally_completing_recovered_tree(unit, &key);
        let view = tree.view(&key);
        let entry = callable_entry(&key);
        let context = TestCheckerContext::new(false);

        let Ok(request) =
            UnitCheckRequest::new(view, UnitCheckRoot::CallableBody(root), &entry, &context)
        else {
            panic!("matching test roots must produce checker requests");
        };

        let outcome = DefaultControlFlowChecker.check_control_flow(request);

        let CheckerOutcome::Complete(result) = outcome else {
            panic!("recovered control-flow checking must complete");
        };

        assert!(result.value().is_recovered());
        assert!(!result.value().completion().can_complete_normally());
    }

    #[test]
    fn requests_reject_roots_from_another_bound_unit() {
        let key = callable_key();
        let (tree, _) = recovered_tree(BoundUnitId::new(6), &key);
        let (_, foreign_root) = recovered_tree(BoundUnitId::new(7), &key);
        let view = tree.view(&key);
        let entry = callable_entry(&key);
        let context = TestCheckerContext::new(false);

        let request = UnitCheckRequest::new(
            view,
            UnitCheckRoot::CallableBody(foreign_root),
            &entry,
            &context,
        );

        assert!(matches!(
            request,
            Err(crate::UnitCheckRequestError::ForeignRoot)
        ));
    }

    #[test]
    fn requests_reject_entry_contexts_for_another_unit_category() {
        let key = callable_key();
        let (tree, root) = recovered_tree(BoundUnitId::new(9), &key);
        let view = tree.view(&key);

        let UnitCheckEntryContext::CallableBody(declaration) = callable_entry(&key) else {
            panic!("callable test entries must retain their category");
        };

        let entry = UnitCheckEntryContext::RuntimeDefault(declaration);

        let context = TestCheckerContext::new(false);

        let request =
            UnitCheckRequest::new(view, UnitCheckRoot::CallableBody(root), &entry, &context);

        assert!(matches!(
            request,
            Err(crate::UnitCheckRequestError::EntryContextMismatch)
        ));
    }
}
