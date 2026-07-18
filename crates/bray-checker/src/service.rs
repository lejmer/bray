use crate::analysis::check_control_flow;
use crate::type_check::check_expression_types;
use crate::{
    CheckerOutcome, CheckerRequestContext, ControlFlowCheckResult, ExpressionTypeInput,
    UnitCheckRequest,
};
use bray_bound_tree::CheckedExpressionTypes;

/// The standard Bray control-flow checker implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultControlFlowChecker;

/// The standard Bray expression-type checker implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultExpressionTypeChecker;

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

/// Expression type inference and compatibility checking over one bound semantic unit.
///
/// Type evidence can come from focused literal or selection rules. Expected types constrain
/// compatibility but never select an overload or operation.
pub trait ExpressionTypeChecker<C>: Sync
where
    C: CheckerRequestContext + ?Sized,
{
    /// Resolves a canonical type or recovery type for every expression occurrence.
    fn check_expression_types(
        &self,
        request: UnitCheckRequest<'_, C>,
        input: &ExpressionTypeInput,
    ) -> CheckerOutcome<CheckedExpressionTypes> {
        check_expression_types(request, input)
    }
}

impl<C> ExpressionTypeChecker<C> for DefaultExpressionTypeChecker where
    C: CheckerRequestContext + ?Sized
{
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundCallableBody, BoundNodeOrigin, BoundTreeBuilder, BoundUnitId, ControlCompletionKind,
    };
    use bray_symbols::{AnySymbolId, FunctionSymbolId, SymbolId};

    use super::{ControlFlowChecker, DefaultControlFlowChecker};
    use crate::test_support::{
        TestCheckerContext, available_compiler_known_symbols, callable_entry, callable_key,
        callable_unit, normally_completing_recovered_tree, recovered_tree,
    };
    use crate::{
        CheckerOutcome, DeclaredUnitCheckEntry, UnitCheckEntryContext, UnitCheckRequest,
        UnitCheckRoot,
    };

    #[test]
    fn default_checking_recovers_from_an_error_body_without_panicking() {
        let key = callable_key();
        let unit = BoundUnitId::new(4);

        let (tree, root) = recovered_tree(unit, &key);

        let unit = callable_unit(&key, tree, root);
        let entry = callable_entry(&key);

        let context = TestCheckerContext::new(false);

        let Ok(request) = UnitCheckRequest::new(&unit, &entry, &context) else {
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

        assert_eq!(result.value().unit(), unit.unit());

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

        let unit = callable_unit(&key, tree, root);
        let entry = callable_entry(&key);

        let context = TestCheckerContext::new(true);

        let Ok(request) = UnitCheckRequest::new(&unit, &entry, &context) else {
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

        let unit = callable_unit(&key, tree, root);
        let entry = callable_entry(&key);

        let context = TestCheckerContext::new(false);

        let Ok(request) = UnitCheckRequest::new(&unit, &entry, &context) else {
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
    fn requests_use_only_the_canonical_bound_unit_root() {
        let key = callable_key();
        let mut tree = BoundTreeBuilder::new(BoundUnitId::new(6));
        let origin = BoundNodeOrigin::source(key.source());

        let Ok(root) = tree.push_callable_body(BoundCallableBody::error(origin, None)) else {
            panic!("canonical callable root must fit");
        };

        let Ok(non_root) = tree.push_callable_body(BoundCallableBody::error(origin, None)) else {
            panic!("same-unit non-root callable must fit");
        };

        let unit = callable_unit(&key, tree.finish(), root);
        let entry = callable_entry(&key);

        let context = TestCheckerContext::new(false);

        let request = match UnitCheckRequest::new(&unit, &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("canonical checker request must validate: {error:?}"),
        };

        assert_eq!(request.root(), UnitCheckRoot::CallableBody(root));
        assert_ne!(request.root(), UnitCheckRoot::CallableBody(non_root));
    }

    #[test]
    fn requests_reject_entry_contexts_for_another_unit_category() {
        let key = callable_key();

        let (tree, root) = recovered_tree(BoundUnitId::new(9), &key);

        let unit = callable_unit(&key, tree, root);

        let UnitCheckEntryContext::CallableBody(declaration) = callable_entry(&key) else {
            panic!("callable test entries must retain their category");
        };

        let entry = UnitCheckEntryContext::RuntimeDefault(declaration);
        let context = TestCheckerContext::new(false);
        let request = UnitCheckRequest::new(&unit, &entry, &context);

        assert!(matches!(
            request,
            Err(crate::UnitCheckRequestError::EntryContextMismatch)
        ));
    }

    #[test]
    fn requests_reject_forged_entry_payloads() {
        let key = callable_key();

        let (tree, root) = recovered_tree(BoundUnitId::new(10), &key);

        let unit = callable_unit(&key, tree, root);

        let context = TestCheckerContext::new(false);
        let forged = AnySymbolId::from(FunctionSymbolId::from_symbol_id(SymbolId::new(1)));

        let entry = UnitCheckEntryContext::CallableBody(DeclaredUnitCheckEntry::new(
            key.clone(),
            forged,
            forged,
        ));

        assert!(matches!(
            UnitCheckRequest::new(&unit, &entry, &context),
            Err(crate::UnitCheckRequestError::EntryContextMismatch)
        ));
    }
}
