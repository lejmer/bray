use crate::analysis::check_control_flow;
use crate::constant::evaluate_constant;
use crate::expression::check_expression_semantics;
use crate::selection::{select_callable, select_operation};
use crate::target::check_target_validity;
use crate::type_check::check_expression_types;
use crate::{
    CallableSelectionRequest, CandidateSelection, CheckerOutcome, CheckerRequestContext,
    CheckerUnitView, ConstantEvaluationInput, ControlFlowCheckResult, ExpressionCandidateSet,
    ExpressionTypeInput, OperationSelectionRequest, TargetValidity, TargetValidityRequest,
};
use bray_bound_tree::{
    CheckedExpressionTypes, DeclaredValueTypeTemplates, SelectedCall, SelectedOperation,
};
use bray_symbols::ConstantValueId;

/// The standard Bray control-flow checker implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultControlFlowChecker;

/// The standard Bray expression-type checker implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultExpressionTypeChecker;

/// The standard cooperating expression type and semantic-selection checker.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultExpressionSemanticChecker;

/// The standard Bray constant evaluator implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultConstantEvaluator;

/// The standard Bray semantic candidate and operation selector.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultSemanticSelector;

/// The standard Bray post-selection target-validity checker.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultTargetValidityChecker;

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
        request: CheckerUnitView<'_, C>,
    ) -> CheckerOutcome<ControlFlowCheckResult> {
        check_control_flow(request)
    }
}

impl<C> ControlFlowChecker<C> for DefaultControlFlowChecker where C: CheckerRequestContext + ?Sized {}

/// Expression type inference and compatibility checking over one bound semantic unit.
///
/// Type evidence can come from cooperating checking rules. Expected types constrain
/// compatibility but never select an overload or operation.
pub trait ExpressionTypeChecker<C>: Sync
where
    C: CheckerRequestContext + ?Sized,
{
    /// Resolves a canonical type or recovery type for every expression occurrence.
    fn check_expression_types(
        &self,
        request: CheckerUnitView<'_, C>,
        input: &ExpressionTypeInput,
    ) -> CheckerOutcome<CheckedExpressionTypes> {
        check_expression_types(request, input)
    }
}

impl<C> ExpressionTypeChecker<C> for DefaultExpressionTypeChecker where
    C: CheckerRequestContext + ?Sized
{
}

/// Cooperating expression typing and semantic selection over one bound semantic unit.
pub trait ExpressionSemanticChecker<C>: Sync
where
    C: CheckerRequestContext + ?Sized,
{
    /// Computes final expression types and exact semantic selections together.
    fn check_expression_semantics(
        &self,
        request: CheckerUnitView<'_, C>,
        declared_types: &DeclaredValueTypeTemplates,
        candidate_sets: &[ExpressionCandidateSet],
    ) -> CheckerOutcome<(
        CheckedExpressionTypes,
        bray_bound_tree::CheckedSemanticSelections,
    )> {
        check_expression_semantics(request, declared_types, candidate_sets)
    }
}

impl<C> ExpressionSemanticChecker<C> for DefaultExpressionSemanticChecker where
    C: CheckerRequestContext + ?Sized
{
}

/// Exact semantic candidate selection over checked expression types.
///
/// Candidate enumeration and source association remain binder responsibilities. Selection uses
/// only language-defined applicability inputs and never ranks candidates by result context.
pub trait SemanticSelector<C>: Sync
where
    C: CheckerRequestContext + ?Sized,
{
    /// Selects one callable and publishes its ABI and normalized argument/default mapping.
    fn select_callable(
        &self,
        request: CheckerUnitView<'_, C>,
        types: &CheckedExpressionTypes,
        input: CallableSelectionRequest,
    ) -> CheckerOutcome<CandidateSelection<SelectedCall>> {
        select_callable(request, types, input)
    }

    /// Selects one member, operator, index, construction, conversion, or witness operation.
    fn select_operation(
        &self,
        request: CheckerUnitView<'_, C>,
        types: &CheckedExpressionTypes,
        input: OperationSelectionRequest,
    ) -> CheckerOutcome<CandidateSelection<SelectedOperation>> {
        select_operation(request, types, input)
    }
}

impl<C> SemanticSelector<C> for DefaultSemanticSelector where C: CheckerRequestContext + ?Sized {}

/// Closed constant-expression evaluation over one checked bound semantic unit.
pub trait ConstantEvaluator<C>: Sync
where
    C: CheckerRequestContext + ?Sized,
{
    /// Evaluates and interns the request's constant-expression root.
    fn evaluate_constant(
        &self,
        request: CheckerUnitView<'_, C>,
        input: &ConstantEvaluationInput<'_>,
    ) -> CheckerOutcome<ConstantValueId> {
        evaluate_constant(request, input)
    }
}

impl<C> ConstantEvaluator<C> for DefaultConstantEvaluator where C: CheckerRequestContext + ?Sized {}

/// Target-dependent validation for an already selected representation, ABI, or alignment.
///
/// Validation reports target incompatibility without changing semantic selection.
pub trait TargetValidityChecker<C>: Sync
where
    C: crate::TargetValidityContext + ?Sized,
{
    /// Checks one selected target requirement at its source anchor.
    fn check_target_validity(
        &self,
        context: &C,
        request: &TargetValidityRequest,
    ) -> CheckerOutcome<TargetValidity> {
        check_target_validity(context, request)
    }
}

impl<C> TargetValidityChecker<C> for DefaultTargetValidityChecker where
    C: crate::TargetValidityContext + ?Sized
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
        CheckerOutcome, CheckerUnitRoot, CheckerUnitView, DeclaredUnitContext, SemanticUnitContext,
    };

    #[test]
    fn default_checking_recovers_from_an_error_body_without_panicking() {
        let key = callable_key();
        let unit = BoundUnitId::new(4);

        let (tree, root) = recovered_tree(unit, &key);

        let unit = callable_unit(&key, tree, root);
        let entry = callable_entry(&key);

        let context = TestCheckerContext::new(false);

        let Ok(request) = CheckerUnitView::new(&unit, &entry, &context) else {
            panic!("matching test roots must produce checker unit views");
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

        let Ok(request) = CheckerUnitView::new(&unit, &entry, &context) else {
            panic!("matching test roots must produce checker unit views");
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

        let Ok(request) = CheckerUnitView::new(&unit, &entry, &context) else {
            panic!("matching test roots must produce checker unit views");
        };

        let outcome = DefaultControlFlowChecker.check_control_flow(request);

        let CheckerOutcome::Complete(result) = outcome else {
            panic!("recovered control-flow checking must complete");
        };

        assert!(result.value().is_recovered());
        assert!(!result.value().completion().can_complete_normally());
    }

    #[test]
    fn unit_views_use_only_the_canonical_bound_unit_root() {
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

        let request = match CheckerUnitView::new(&unit, &entry, &context) {
            Ok(request) => request,
            Err(error) => panic!("canonical checker unit view must validate: {error:?}"),
        };

        assert_eq!(request.root(), CheckerUnitRoot::CallableBody(root));
        assert_ne!(request.root(), CheckerUnitRoot::CallableBody(non_root));
    }

    #[test]
    fn unit_views_reject_semantic_contexts_for_another_unit_category() {
        let key = callable_key();

        let (tree, root) = recovered_tree(BoundUnitId::new(9), &key);

        let unit = callable_unit(&key, tree, root);

        let SemanticUnitContext::CallableBody(declaration) = callable_entry(&key) else {
            panic!("callable test entries must retain their category");
        };

        let entry = SemanticUnitContext::RuntimeDefault(declaration);
        let context = TestCheckerContext::new(false);
        let request = CheckerUnitView::new(&unit, &entry, &context);

        assert!(matches!(
            request,
            Err(crate::CheckerUnitViewError::SemanticContextMismatch)
        ));
    }

    #[test]
    fn unit_views_reject_forged_semantic_contexts() {
        let key = callable_key();

        let (tree, root) = recovered_tree(BoundUnitId::new(10), &key);

        let unit = callable_unit(&key, tree, root);

        let context = TestCheckerContext::new(false);
        let forged = AnySymbolId::from(FunctionSymbolId::from_symbol_id(SymbolId::new(1)));

        let entry = SemanticUnitContext::CallableBody(DeclaredUnitContext::new(
            key.clone(),
            forged,
            forged,
        ));

        assert!(matches!(
            CheckerUnitView::new(&unit, &entry, &context),
            Err(crate::CheckerUnitViewError::SemanticContextMismatch)
        ));
    }
}
