use crate::analysis::analyze_storage_liveness;
use crate::analysis::check_control_flow;
use crate::analysis::check_refinements;
use crate::analysis::check_storage_flow;
use crate::constant::{check_constant_term, evaluate_constant};
use crate::dependency::check_dependency_contracts;
use crate::expression::check_expression_semantics;
use crate::pattern::check_patterns;
use crate::selection::{select_callable, select_iteration_source, select_operation};
use crate::storage::plan_storage;
use crate::target::check_target_validity;
use crate::type_check::check_expression_types;
use crate::{
    CallableSelectionRequest, CandidateSelection, CheckerOutcome, CheckerRequestContext,
    CheckerUnitView, ConstantEvaluationInput, ControlFlowCheckResult, ExpressionCandidateSet,
    ExpressionTypeInput, IterationSourceSelectionRequest, NestedCallableEvidence,
    OperationSelectionRequest, PatternCheckInput, TargetValidity, TargetValidityRequest,
};
use bray_bound_tree::{
    CheckedDependencyContracts, CheckedExpressionTypes, CheckedPatternFacts,
    CheckedRefinementFacts, DeclaredValueTypeTemplates, LivenessFacts, SelectedCall,
    SelectedIterationSource, SelectedOperation, StorageFlowFacts, StoragePlan,
};
use bray_symbols::{CallableSignatureFact, ConstantTermId, ConstantValueId};
use bray_symbols::{StructFieldTypeFact, UnionPayloadFieldTypeFact};

/// The standard Bray control-flow checker implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultControlFlowChecker;

/// The standard Bray expression-type checker implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultExpressionTypeChecker;

/// The standard cooperating expression type and semantic-selection checker.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultExpressionSemanticChecker;

/// The standard Bray pattern and match-coverage checker.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultPatternChecker;

/// The standard Bray constant evaluator implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultConstantEvaluator;

/// The standard Bray constant-expression checker implementation.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultConstantChecker;

/// The standard Bray semantic candidate and operation selector.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultSemanticSelector;

/// The standard Bray post-selection target-validity checker.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultTargetValidityChecker;

/// The standard Bray per-unit storage planner.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultStoragePlanner;

/// The standard Bray storage and obligation liveness analyzer.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultLivenessAnalyzer;

/// The standard Bray flow-sensitive fact analyzer.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultRefinementAnalyzer;

/// The standard Bray storage, ownership, and borrow checker.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultStorageFlowChecker;

/// The standard Bray dependency-contract checker.
#[derive(Clone, Copy, Debug, Default)]
pub struct DefaultDependencyContractChecker;

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

/// Pattern compatibility, binding typing, refutability, and match coverage checking.
pub trait PatternChecker<C>: Sync
where
    C: CheckerRequestContext
        + crate::CheckerSemanticFactProvider<StructFieldTypeFact>
        + crate::CheckerSemanticFactProvider<UnionPayloadFieldTypeFact>
        + ?Sized,
{
    /// Checks patterns in one committed bound unit.
    fn check_patterns(
        &self,
        request: CheckerUnitView<'_, C>,
        expression_types: &CheckedExpressionTypes,
        input: &PatternCheckInput,
    ) -> CheckerOutcome<CheckedPatternFacts> {
        check_patterns(request, expression_types, input)
    }
}

impl<C> PatternChecker<C> for DefaultPatternChecker where
    C: CheckerRequestContext
        + crate::CheckerSemanticFactProvider<StructFieldTypeFact>
        + crate::CheckerSemanticFactProvider<UnionPayloadFieldTypeFact>
        + ?Sized
{
}

/// Storage identity and occurrence-specific access planning over one checked bound unit.
pub trait StoragePlanner<C>: Sync
where
    C: CheckerRequestContext + crate::CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    /// Constructs persistent storage identities and evaluated access plans.
    fn plan_storage(
        &self,
        request: CheckerUnitView<'_, C>,
        declared_types: &DeclaredValueTypeTemplates,
        types: &CheckedExpressionTypes,
        patterns: &CheckedPatternFacts,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        iterations: &[SelectedIterationSource],
    ) -> CheckerOutcome<StoragePlan> {
        plan_storage(
            request,
            declared_types,
            types,
            patterns,
            selections,
            iterations,
        )
    }
}

impl<C> StoragePlanner<C> for DefaultStoragePlanner where
    C: CheckerRequestContext + crate::CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized
{
}

/// Storage, access, capability, and obligation liveness over one checked bound unit.
pub trait LivenessAnalyzer<C>: Sync
where
    C: CheckerRequestContext + ?Sized,
{
    /// Computes durable last-use and lexical scope-boundary decisions.
    fn analyze_liveness(
        &self,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
    ) -> CheckerOutcome<LivenessFacts> {
        analyze_storage_liveness(request, storage)
    }
}

impl<C> LivenessAnalyzer<C> for DefaultLivenessAnalyzer where C: CheckerRequestContext + ?Sized {}

/// Flow-sensitive semantic facts over one checked bound unit.
pub trait RefinementAnalyzer<C>: Sync
where
    C: CheckerRequestContext + ?Sized,
{
    /// Computes durable facts available before checked operation occurrences.
    fn analyze_refinements(
        &self,
        request: CheckerUnitView<'_, C>,
        patterns: &CheckedPatternFacts,
        storage: &StoragePlan,
    ) -> CheckerOutcome<CheckedRefinementFacts> {
        check_refinements(request, patterns, storage)
    }
}

impl<C> RefinementAnalyzer<C> for DefaultRefinementAnalyzer where C: CheckerRequestContext + ?Sized {}

/// Composite storage, ownership, movement, and borrow checking over one unit.
pub trait StorageFlowChecker<C>: Sync
where
    C: CheckerRequestContext + crate::CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    /// Computes durable per-operation storage decisions.
    fn check_storage_flow(
        &self,
        request: CheckerUnitView<'_, C>,
        storage: &StoragePlan,
        liveness: &LivenessFacts,
        refinements: &CheckedRefinementFacts,
    ) -> CheckerOutcome<StorageFlowFacts> {
        check_storage_flow(request, storage, liveness, refinements)
    }
}

impl<C> StorageFlowChecker<C> for DefaultStorageFlowChecker where
    C: CheckerRequestContext + crate::CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized
{
}

/// Value, access, borrow, witness, and selected-call dependency propagation over one unit.
pub trait DependencyContractChecker<C>: Sync
where
    C: CheckerRequestContext + crate::CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized,
{
    /// Computes durable normalized contracts without repeating storage analysis.
    fn check_dependency_contracts(
        &self,
        request: CheckerUnitView<'_, C>,
        selections: &bray_bound_tree::CheckedSemanticSelections,
        storage: &StoragePlan,
        flow: &StorageFlowFacts,
    ) -> CheckerOutcome<CheckedDependencyContracts> {
        check_dependency_contracts(request, selections, storage, flow)
    }
}

impl<C> DependencyContractChecker<C> for DefaultDependencyContractChecker where
    C: CheckerRequestContext + crate::CheckerSemanticFactProvider<CallableSignatureFact> + ?Sized
{
}

/// Cooperating expression typing and semantic selection over one bound semantic unit.
pub trait ExpressionSemanticChecker<C>: Sync
where
    C: CheckerRequestContext
        + crate::CheckerSemanticFactProvider<StructFieldTypeFact>
        + crate::CheckerSemanticFactProvider<UnionPayloadFieldTypeFact>
        + ?Sized,
{
    /// Computes final expression types and exact semantic selections together.
    fn check_expression_semantics(
        &self,
        request: CheckerUnitView<'_, C>,
        declared_types: &DeclaredValueTypeTemplates,
        nested_callables: &[NestedCallableEvidence],
        candidate_sets: &[ExpressionCandidateSet],
        pattern_input: &PatternCheckInput,
        iteration_sources: &[SelectedIterationSource],
    ) -> CheckerOutcome<(
        CheckedExpressionTypes,
        bray_bound_tree::CheckedSemanticSelections,
    )> {
        check_expression_semantics(
            request,
            declared_types,
            nested_callables,
            candidate_sets,
            pattern_input,
            iteration_sources,
        )
    }
}

impl<C> ExpressionSemanticChecker<C> for DefaultExpressionSemanticChecker where
    C: CheckerRequestContext
        + crate::CheckerSemanticFactProvider<StructFieldTypeFact>
        + crate::CheckerSemanticFactProvider<UnionPayloadFieldTypeFact>
        + ?Sized
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

    /// Selects one exact iterable and iterator protocol pair.
    fn select_iteration_source(
        &self,
        request: CheckerUnitView<'_, C>,
        input: &IterationSourceSelectionRequest,
    ) -> CheckerOutcome<CandidateSelection<bray_bound_tree::SelectedIterationSource>> {
        select_iteration_source(request, input)
    }
}

impl<C> SemanticSelector<C> for DefaultSemanticSelector where C: CheckerRequestContext + ?Sized {}

/// Closed constant-expression evaluation over one checked bound semantic unit.
pub trait ConstantEvaluator<C>: Sync
where
    C: CheckerRequestContext + ?Sized,
{
    /// Evaluates the request's constant-expression root to a closed value.
    fn evaluate_constant(
        &self,
        request: CheckerUnitView<'_, C>,
        input: &ConstantEvaluationInput<'_>,
    ) -> CheckerOutcome<ConstantValueId> {
        evaluate_constant(request, input)
    }

    /// Evaluates the request's constant-expression root and reports references reached.
    fn evaluate_constant_with_references(
        &self,
        request: CheckerUnitView<'_, C>,
        input: &ConstantEvaluationInput<'_>,
    ) -> CheckerOutcome<crate::EvaluatedConstant> {
        crate::constant::evaluate_constant_with_references(request, input)
    }
}

impl<C> ConstantEvaluator<C> for DefaultConstantEvaluator where C: CheckerRequestContext + ?Sized {}

/// Constant-expression validation and open-term construction over one checked bound unit.
pub trait ConstantChecker<C>: Sync
where
    C: CheckerRequestContext + ?Sized,
{
    /// Checks the request's constant-expression root and returns its checked term.
    fn check_constant_term(
        &self,
        request: CheckerUnitView<'_, C>,
        input: &ConstantEvaluationInput<'_>,
    ) -> CheckerOutcome<ConstantTermId> {
        check_constant_term(request, input)
    }
}

impl<C> ConstantChecker<C> for DefaultConstantChecker where C: CheckerRequestContext + ?Sized {}

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
