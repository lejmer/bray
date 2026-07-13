use bray_bound_tree::{
    BoundCallableBodyId, BoundExpressionId, BoundUnitKey, BoundUnitKeyData,
    CheckedControlFlowFacts, CheckedUnitBuildError, ControlFlowCheckedAnonymousCallable,
    ControlFlowCheckedCallableBody, ControlFlowCheckedConstantTemplateUnit,
    ControlFlowCheckedConstraintUnit, ControlFlowCheckedContractClauseUnit,
    ControlFlowCheckedPredicateDefinitionUnit, ControlFlowCheckedRuntimeDefaultUnit,
};
use bray_checker::{
    CheckerCancellation, CheckerOutcome, ControlFlowChecker, UnitCheckRequest,
    UnitCheckRequestError, UnitCheckRoot,
};
use bray_diagnostics::DiagnosticResult;
use bray_symbols::AnonymousCallableSymbolId;

use crate::request::BinderRequestResult;
use crate::{BinderCancellation, BinderDependency, CheckedUnitComputation};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CheckedUnitAssemblyError {
    Cancelled,
    InvalidCheckerRequest(UnitCheckRequestError),
    InvalidCheckedUnit(CheckedUnitBuildError),
}

impl From<CheckedUnitBuildError> for CheckedUnitAssemblyError {
    fn from(error: CheckedUnitBuildError) -> Self {
        Self::InvalidCheckedUnit(error)
    }
}

pub(crate) fn assemble_callable_body<C, K>(
    request: BinderRequestResult,
    nested_units: Vec<BoundUnitKey>,
    checker: &C,
    cancellation: &K,
    root: BoundCallableBodyId,
) -> Result<CheckedUnitComputation<ControlFlowCheckedCallableBody>, CheckedUnitAssemblyError>
where
    C: ControlFlowChecker + ?Sized,
    K: BinderCancellation + ?Sized,
{
    assemble_checked_unit(
        request,
        nested_units,
        checker,
        cancellation,
        UnitCheckRoot::CallableBody(root),
        |parts| {
            ControlFlowCheckedCallableBody::try_new(
                &parts.key,
                parts.tree,
                parts.local_symbols,
                parts.nested_units,
                parts.control_flow_facts,
                root,
            )
        },
    )
}

pub(crate) fn assemble_anonymous_callable<C, K>(
    request: BinderRequestResult,
    nested_units: Vec<BoundUnitKey>,
    checker: &C,
    cancellation: &K,
    callable: AnonymousCallableSymbolId,
    root: BoundCallableBodyId,
) -> Result<CheckedUnitComputation<ControlFlowCheckedAnonymousCallable>, CheckedUnitAssemblyError>
where
    C: ControlFlowChecker + ?Sized,
    K: BinderCancellation + ?Sized,
{
    assemble_checked_unit(
        request,
        nested_units,
        checker,
        cancellation,
        UnitCheckRoot::CallableBody(root),
        |parts| {
            ControlFlowCheckedAnonymousCallable::try_new(
                &parts.key,
                parts.tree,
                parts.local_symbols,
                parts.nested_units,
                parts.control_flow_facts,
                callable,
                root,
            )
        },
    )
}

macro_rules! define_expression_assembler {
    ($function:ident, $result:ty) => {
        pub(crate) fn $function<C, K>(
            request: BinderRequestResult,
            nested_units: Vec<BoundUnitKey>,
            checker: &C,
            cancellation: &K,
            root: BoundExpressionId,
        ) -> Result<CheckedUnitComputation<$result>, CheckedUnitAssemblyError>
        where
            C: ControlFlowChecker + ?Sized,
            K: BinderCancellation + ?Sized,
        {
            assemble_checked_unit(
                request,
                nested_units,
                checker,
                cancellation,
                UnitCheckRoot::Expression(root),
                |parts| {
                    <$result>::try_new(
                        &parts.key,
                        parts.tree,
                        parts.local_symbols,
                        parts.nested_units,
                        parts.control_flow_facts,
                        root,
                    )
                },
            )
        }
    };
}

define_expression_assembler!(
    assemble_runtime_default,
    ControlFlowCheckedRuntimeDefaultUnit
);
define_expression_assembler!(
    assemble_constant_template,
    ControlFlowCheckedConstantTemplateUnit
);
define_expression_assembler!(
    assemble_predicate_definition,
    ControlFlowCheckedPredicateDefinitionUnit
);
define_expression_assembler!(assemble_constraint, ControlFlowCheckedConstraintUnit);
define_expression_assembler!(
    assemble_contract_clause,
    ControlFlowCheckedContractClauseUnit
);

fn assemble_checked_unit<T, C, K>(
    request: BinderRequestResult,
    nested_units: Vec<BoundUnitKey>,
    checker: &C,
    cancellation: &K,
    root: UnitCheckRoot,
    assemble: impl FnOnce(CheckedUnitParts) -> Result<T, CheckedUnitBuildError>,
) -> Result<CheckedUnitComputation<T>, CheckedUnitAssemblyError>
where
    C: ControlFlowChecker + ?Sized,
    K: BinderCancellation + ?Sized,
{
    let bridge = CheckerCancellationBridge(cancellation);
    let checker_request = UnitCheckRequest::new(request.unit().view(), root, &bridge)
        .map_err(CheckedUnitAssemblyError::InvalidCheckerRequest)?;

    let checker_result = match checker.check_control_flow(checker_request) {
        CheckerOutcome::Complete(result) => result,
        CheckerOutcome::Cancelled => return Err(CheckedUnitAssemblyError::Cancelled),
    };

    let (control_flow_facts, checker_diagnostics) = checker_result
        .map(|result| result.into_facts())
        .into_parts();

    let (unit, binder_diagnostics, dependencies) = request.into_parts();
    let diagnostics = binder_diagnostics.merged(&checker_diagnostics);
    let (key, tree, local_symbols, _) = unit.into_parts();

    let checked = assemble(CheckedUnitParts {
        key,
        tree,
        local_symbols,
        nested_units,
        control_flow_facts,
    })?;

    Ok(CheckedUnitComputation::new(
        DiagnosticResult::new(checked, diagnostics),
        dependencies,
    ))
}

pub(crate) fn direct_nested_units(
    enclosing: &BoundUnitKey,
    dependencies: &[BinderDependency],
) -> Vec<BoundUnitKey> {
    dependencies
        .iter()
        .filter_map(|dependency| {
            let BinderDependency::Unit(nested) = dependency else {
                return None;
            };

            let BoundUnitKeyData::AnonymousCallable(anonymous) = nested.data() else {
                return None;
            };

            if anonymous.enclosing() != enclosing {
                return None;
            }

            // The checked unit and query dependency set both retain this Arc-backed key.
            Some(nested.clone())
        })
        .collect()
}

struct CheckedUnitParts {
    key: BoundUnitKey,
    tree: bray_bound_tree::BoundTree,
    local_symbols: bray_symbols::LocalSymbolSnapshot,
    nested_units: Vec<BoundUnitKey>,
    control_flow_facts: CheckedControlFlowFacts,
}

struct CheckerCancellationBridge<'cancellation, K: ?Sized>(&'cancellation K);

impl<K> CheckerCancellation for CheckerCancellationBridge<'_, K>
where
    K: BinderCancellation + ?Sized,
{
    fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundCallableBody, BoundNodeOrigin, BoundUnitKey, BoundUnitKind, ControlCompletionKind,
    };
    use bray_checker::{
        CheckerOutcome, ControlFlowCheckResult, ControlFlowChecker, DefaultControlFlowChecker,
        UnitCheckRequest,
    };
    use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};

    use super::{CheckedUnitAssemblyError, assemble_callable_body, direct_nested_units};
    use crate::BinderDependency;
    use crate::fact::test_support::TestFixture;

    #[test]
    fn successful_assembly_attaches_checker_facts_before_publication() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "}\n",
        ));

        let facts = fixture.context();
        let (request, root) = frozen_error_body(&facts);

        let computation = match assemble_callable_body(
            request,
            Vec::new(),
            &DefaultControlFlowChecker,
            &|| false,
            root,
        ) {
            Ok(computation) => computation,
            Err(error) => panic!("matching checked unit must assemble: {error:?}"),
        };

        let checked = computation.result().value();

        assert_eq!(checked.control_flow_facts().unit(), checked.unit());
        assert_eq!(
            checked.control_flow_facts().kind(),
            BoundUnitKind::CallableBody
        );
        assert!(
            checked
                .control_flow_facts()
                .completion()
                .contains(ControlCompletionKind::Recovered)
        );
    }

    #[test]
    fn cancellation_returns_without_a_partial_checked_unit() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "}\n",
        ));

        let facts = fixture.context();
        let (request, root) = frozen_error_body(&facts);

        let result = assemble_callable_body(
            request,
            Vec::new(),
            &DefaultControlFlowChecker,
            &|| true,
            root,
        );

        assert!(matches!(result, Err(CheckedUnitAssemblyError::Cancelled)));
    }

    #[test]
    fn assembly_merges_binder_diagnostics_before_checker_diagnostics() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "}\n",
        ));

        let facts = fixture.context();
        let binder_diagnostic = diagnostic(1, DiagnosticKind::BindingUnresolvedName);
        let checker_diagnostic = diagnostic(2, DiagnosticKind::BindingWrongNameKind);
        let (request, root) = frozen_error_body_with_diagnostic(&facts, binder_diagnostic.clone());
        let checker = DiagnosticChecker(checker_diagnostic.clone());

        let computation =
            match assemble_callable_body(request, Vec::new(), &checker, &|| false, root) {
                Ok(computation) => computation,
                Err(error) => panic!("matching checked unit must assemble: {error:?}"),
            };

        assert_eq!(
            computation.result().diagnostics().diagnostics(),
            &[binder_diagnostic, checker_diagnostic]
        );
    }

    #[test]
    fn nested_unit_collection_keeps_only_direct_anonymous_dependencies() {
        let fixture = TestFixture::from_source(concat!(
            "module app;\n",
            "const size: i32 = 1;\n",
            "func main()\n",
            "{\n",
            "}\n",
        ));

        let facts = fixture.context();
        let (request, _) = frozen_error_body(&facts);
        let enclosing = request.unit().key();
        let direct = BoundUnitKey::anonymous_callable(enclosing.clone(), enclosing.source());
        let indirect = BoundUnitKey::anonymous_callable(direct.clone(), enclosing.source());

        let dependencies = [
            BinderDependency::Unit(indirect),
            BinderDependency::Unit(direct.clone()),
        ];

        assert_eq!(direct_nested_units(enclosing, &dependencies), vec![direct]);
    }

    struct DiagnosticChecker(Diagnostic);

    impl ControlFlowChecker for DiagnosticChecker {
        fn check_control_flow(
            &self,
            request: UnitCheckRequest<'_>,
        ) -> CheckerOutcome<ControlFlowCheckResult> {
            let CheckerOutcome::Complete(result) =
                DefaultControlFlowChecker.check_control_flow(request)
            else {
                return CheckerOutcome::Cancelled;
            };

            CheckerOutcome::complete(*result.value(), DiagnosticBag::single(self.0.clone()))
        }
    }

    fn frozen_error_body<C>(
        facts: &C,
    ) -> (
        crate::request::BinderRequestResult,
        bray_bound_tree::BoundCallableBodyId,
    )
    where
        C: crate::BinderFactContext + ?Sized,
    {
        frozen_error_body_inner(facts, None)
    }

    fn frozen_error_body_with_diagnostic<C>(
        facts: &C,
        diagnostic: Diagnostic,
    ) -> (
        crate::request::BinderRequestResult,
        bray_bound_tree::BoundCallableBodyId,
    )
    where
        C: crate::BinderFactContext + ?Sized,
    {
        frozen_error_body_inner(facts, Some(diagnostic))
    }

    fn frozen_error_body_inner<C>(
        facts: &C,
        diagnostic: Option<Diagnostic>,
    ) -> (
        crate::request::BinderRequestResult,
        bray_bound_tree::BoundCallableBodyId,
    )
    where
        C: crate::BinderFactContext + ?Sized,
    {
        let (mut request, _) = crate::binding::request_and_block(facts);
        let origin = BoundNodeOrigin::source(request.unit().key().source());
        let body = BoundCallableBody::error(origin, None);

        let root = match request.unit_mut().tree_mut().push_callable_body(body) {
            Ok(root) => root,
            Err(error) => panic!("test callable body must fit: {error:?}"),
        };

        if let Some(diagnostic) = diagnostic {
            request.add_diagnostic(diagnostic);
        }

        let request = match request.finish() {
            Ok(request) => request,
            Err(error) => panic!("test request must freeze: {error:?}"),
        };

        (request, root)
    }

    fn diagnostic(id: u32, kind: DiagnosticKind) -> Diagnostic {
        Diagnostic::new(DiagnosticId::new(id), kind, SeverityKind::Error)
    }
}
