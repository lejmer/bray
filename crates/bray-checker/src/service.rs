use bray_bound_tree::BoundUnit;

use crate::analysis::check_control_flow;
use crate::expression::check_expression_facts;
use crate::{CheckerOutcome, ControlFlowCheckResult, ExpressionFactCheckResult, UnitCheckRequest};

/// The standard Bray control-flow checker implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultControlFlowChecker;

/// Control-flow checking over one committed bound semantic unit.
///
/// Implementations must observe request cancellation while doing substantial
/// work and return [`CheckerOutcome::Cancelled`] without partial results or
/// diagnostics.
pub trait ControlFlowChecker: Sync {
    /// Checks one committed bound unit's control flow.
    fn check_control_flow(
        &self,
        request: UnitCheckRequest<'_>,
    ) -> CheckerOutcome<ControlFlowCheckResult> {
        check_control_flow(request)
    }
}

impl ControlFlowChecker for DefaultControlFlowChecker {}

/// Expression-fact checking over one committed bound semantic unit.
///
/// Implementations must observe request cancellation while doing substantial
/// work and return [`CheckerOutcome::Cancelled`] without partial results or
/// diagnostics.
pub trait ExpressionFactChecker: Sync {
    /// Publishes complete durable expression facts for one committed bound unit.
    fn check_expression_facts(
        &self,
        unit: &BoundUnit,
        request: UnitCheckRequest<'_>,
    ) -> CheckerOutcome<ExpressionFactCheckResult> {
        check_expression_facts(unit, request)
    }
}

impl ExpressionFactChecker for DefaultControlFlowChecker {}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundUnitId, ControlCompletionKind};

    use super::{ControlFlowChecker, DefaultControlFlowChecker};
    use crate::test_support::{
        available_compiler_known_symbols, callable_key, normally_completing_recovered_tree,
        recovered_tree, semantic_values,
    };
    use crate::{CheckerOutcome, UnitCheckRequest, UnitCheckRoot};

    #[test]
    fn default_checking_recovers_from_an_error_body_without_panicking() {
        let key = callable_key();
        let unit = BoundUnitId::new(4);
        let (tree, root) = recovered_tree(unit, &key);
        let view = tree.view(&key);

        let Ok(request) = UnitCheckRequest::new(
            view,
            UnitCheckRoot::CallableBody(root),
            semantic_values(),
            available_compiler_known_symbols(),
            &|| false,
        ) else {
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

        let Ok(request) = UnitCheckRequest::new(
            view,
            UnitCheckRoot::CallableBody(root),
            semantic_values(),
            available_compiler_known_symbols(),
            &|| true,
        ) else {
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

        let Ok(request) = UnitCheckRequest::new(
            view,
            UnitCheckRoot::CallableBody(root),
            semantic_values(),
            available_compiler_known_symbols(),
            &|| false,
        ) else {
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

        let request = UnitCheckRequest::new(
            view,
            UnitCheckRoot::CallableBody(foreign_root),
            semantic_values(),
            available_compiler_known_symbols(),
            &|| false,
        );

        assert!(matches!(
            request,
            Err(crate::UnitCheckRequestError::ForeignRoot)
        ));
    }
}
