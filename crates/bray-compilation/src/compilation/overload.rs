use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_binder::{NameAccess, SymbolFactProvider};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{DiagnosticArg, DiagnosticBag, DiagnosticKind};
use bray_symbols::{
    AnySymbolId, CallableOverloadSymbolId, CallableSignatureFact, CallableSignatureTemplate,
    CallableSymbolId, GenericDeclarationTemplate, GenericDeclarationTemplateFact, GenericOwnerId,
    ImplementationSymbolId, MemberLookupResult, NamedTypeSymbolId, SymbolFactRequest, SymbolOrigin,
    TraitSymbolId,
};
use bray_syntax::PathSyntax;

use super::Compilation;
use super::binder::{CompilationBinderFacts, binder_fact_error};
use super::diagnostics::source_diagnostic;
use super::limits::try_count_comparison;
use super::overlap::callable_selection_surfaces_overlap;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CallableFamilyContext {
    Module,
    NamedType(NamedTypeSymbolId),
    Trait(TraitSymbolId),
    Implementation(ImplementationSymbolId),
}

#[derive(Clone)]
struct CallableArm {
    anchor: SyntaxAnchor,
    signature: Arc<bray_diagnostics::DiagnosticResult<CallableSignatureTemplate>>,
    generic: Arc<bray_diagnostics::DiagnosticResult<GenericDeclarationTemplate>>,
    context: Option<CallableFamilyContext>,
    may_be_satisfied: bool,
}

impl Compilation {
    pub(in crate::compilation) fn callable_overload_diagnostics(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&DiagnosticBag, FactQueryError> {
        self.query_fact_with_cancellation(
            CompilationFactKey::CallableOverloadValidation,
            &self.state.callable_overload_validation,
            cancellation,
            |cancellation| self.compute_callable_overload_diagnostics(cancellation),
        )
    }

    fn compute_callable_overload_diagnostics(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<DiagnosticBag, FactQueryError> {
        let symbols = self.symbol_graph()?;
        let facts = self.binder_facts(cancellation)?;
        let values = self.semantic_value_store()?;

        let mut diagnostics = DiagnosticBag::new();
        let mut memberships = BTreeMap::new();
        let mut reported_membership_conflicts = BTreeSet::new();

        let maximum_comparisons = self
            .options()
            .semantic_analysis_limits()
            .pairwise_comparisons();

        let mut comparisons = 0;

        for family in symbols
            .callable_overloads()
            .iter()
            .filter(|family| family.origin() == SymbolOrigin::Source)
        {
            cancellation.check()?;

            let expected_context = symbols
                .containing_symbol(family.id().into())
                .and_then(callable_family_context);

            let module = symbols
                .containing_module(family.id().into())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let mut seen = BTreeSet::new();
            let mut arms: Vec<CallableArm> = Vec::new();

            for anchor in family.arm_syntax() {
                let path = anchor
                    .find_descendant::<PathSyntax>(self.syntax_tree())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let result = facts
                    .bind_surface_path(module.id(), &path, NameAccess::Internal)
                    .map_err(binder_fact_error)?;

                diagnostics.add_range(result.diagnostics().iter().cloned());

                let MemberLookupResult::Found(symbol) = result.value() else {
                    continue;
                };

                let Some(callable) = CallableSymbolId::try_from_any(*symbol) else {
                    diagnostics.add(source_diagnostic(
                        *anchor,
                        DiagnosticKind::CheckingInvalidCallableOverloadArm,
                    ));

                    continue;
                };

                if !seen.insert(callable) {
                    diagnostics.add(source_diagnostic(
                        *anchor,
                        DiagnosticKind::CheckingDuplicateCallableOverloadArm,
                    ));

                    continue;
                }

                record_family_membership(
                    &mut diagnostics,
                    &mut memberships,
                    &mut reported_membership_conflicts,
                    family.id(),
                    callable,
                    *anchor,
                );

                let Some(arm) = self.callable_overload_arm(
                    &facts,
                    callable,
                    *anchor,
                    cancellation,
                    &mut diagnostics,
                )?
                else {
                    continue;
                };

                if expected_context.is_some()
                    && arm.context.is_some()
                    && arm.context != expected_context
                {
                    diagnostics.add(source_diagnostic(
                        *anchor,
                        DiagnosticKind::CheckingInvalidCallableOverloadArm,
                    ));

                    continue;
                }

                if let Some(first) = arms.first()
                    && first.signature.value().receiver().is_some()
                        != arm.signature.value().receiver().is_some()
                {
                    diagnostics.add(source_diagnostic(
                        *anchor,
                        DiagnosticKind::CheckingInvalidCallableOverloadArm,
                    ));

                    continue;
                }

                arms.push(arm);
            }

            for (index, left) in arms.iter().enumerate() {
                if !left.may_be_satisfied {
                    continue;
                }

                for right in &arms[index + 1..] {
                    cancellation.check()?;

                    if !right.may_be_satisfied {
                        continue;
                    }

                    if !try_count_comparison(&mut comparisons, maximum_comparisons) {
                        diagnostics.add(
                            source_diagnostic(
                                left.anchor,
                                DiagnosticKind::CheckingCallableOverloadLimitExceeded,
                            )
                            .with_arg(DiagnosticArg::maximum_count(maximum_comparisons)),
                        );

                        return Ok(diagnostics);
                    }

                    if callable_selection_surfaces_overlap(
                        left.signature.value(),
                        left.generic.value().parameters(),
                        right.signature.value(),
                        right.generic.value().parameters(),
                        values,
                    )
                    .map_err(|_| FactQueryError::InfrastructureFailure)?
                    {
                        diagnostics.add(source_diagnostic(
                            left.anchor,
                            DiagnosticKind::CheckingConflictingCallableOverloadSignature,
                        ));

                        diagnostics.add(source_diagnostic(
                            right.anchor,
                            DiagnosticKind::CheckingConflictingCallableOverloadSignature,
                        ));
                    }
                }
            }
        }

        Ok(diagnostics)
    }

    fn callable_overload_arm(
        &self,
        facts: &CompilationBinderFacts<'_>,
        callable: CallableSymbolId,
        anchor: SyntaxAnchor,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<CallableArm>, FactQueryError> {
        let signature = facts
            .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(callable))
            .map_err(binder_fact_error)?;

        diagnostics.add_range(signature.diagnostics().iter().cloned());

        let Some(owner) = GenericOwnerId::try_new(callable.into_any()) else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let generic = facts
            .symbol_fact(SymbolFactRequest::<GenericDeclarationTemplateFact>::new(
                owner,
            ))
            .map_err(binder_fact_error)?;

        diagnostics.add_range(generic.diagnostics().iter().cloned());

        let may_be_satisfied =
            self.generic_declaration_may_be_satisfied(generic.value(), cancellation)?;

        let context = self
            .symbol_graph()?
            .containing_symbol(callable.into_any())
            .and_then(callable_family_context);

        Ok(Some(CallableArm {
            anchor,
            signature,
            generic,
            context,
            may_be_satisfied,
        }))
    }
}

fn callable_family_context(symbol: AnySymbolId) -> Option<CallableFamilyContext> {
    match symbol {
        AnySymbolId::Module(_) => Some(CallableFamilyContext::Module),
        AnySymbolId::Struct(id) => Some(CallableFamilyContext::NamedType(id.into())),
        AnySymbolId::Union(id) => Some(CallableFamilyContext::NamedType(id.into())),
        AnySymbolId::Trait(id) => Some(CallableFamilyContext::Trait(id)),
        AnySymbolId::InherentImplementation(id) => Some(CallableFamilyContext::Implementation(
            ImplementationSymbolId::from(id),
        )),
        AnySymbolId::UnnamedTraitImplementation(id) => Some(CallableFamilyContext::Implementation(
            ImplementationSymbolId::from(id),
        )),
        AnySymbolId::NamedTraitImplementation(id) => Some(CallableFamilyContext::Implementation(
            ImplementationSymbolId::from(id),
        )),
        _ => None,
    }
}

fn record_family_membership(
    diagnostics: &mut DiagnosticBag,
    memberships: &mut BTreeMap<CallableSymbolId, (CallableOverloadSymbolId, SyntaxAnchor)>,
    reported: &mut BTreeSet<(
        CallableOverloadSymbolId,
        CallableOverloadSymbolId,
        CallableSymbolId,
    )>,
    family: CallableOverloadSymbolId,
    callable: CallableSymbolId,
    anchor: SyntaxAnchor,
) {
    let Some((previous_family, previous_anchor)) = memberships.get(&callable).copied() else {
        memberships.insert(callable, (family, anchor));

        return;
    };

    if previous_family == family {
        return;
    }

    let key = if previous_family < family {
        (previous_family, family, callable)
    } else {
        (family, previous_family, callable)
    };

    if !reported.insert(key) {
        return;
    }

    diagnostics.add(source_diagnostic(
        previous_anchor,
        DiagnosticKind::CheckingConflictingCallableOverloadFamily,
    ));

    diagnostics.add(source_diagnostic(
        anchor,
        DiagnosticKind::CheckingConflictingCallableOverloadFamily,
    ));
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::DiagnosticKind;

    use crate::test_support::{compilation, compilation_with_options, diagnostic_kinds};
    use crate::{CompilationOptions, SelectedTarget, SemanticAnalysisLimits, WorkerBudget};

    #[test]
    fn callable_overload_families_accept_distinguishable_explicit_call_surfaces() {
        let compilation = compilation(
            r#"module app;

func one(pos value: bool)
{
}

func two(pos value: bool, pos other: bool)
{
}

overload choose =
{
    one,
    two,
}
"#,
        );

        let diagnostics = diagnostic_kinds(compilation.semantic_diagnostics());

        assert!(
            !diagnostics
                .iter()
                .any(|kind| is_callable_overload_diagnostic(*kind))
        );
    }

    #[test]
    fn callable_overload_families_report_duplicate_and_indistinguishable_arms() {
        let compilation = compilation(
            r#"module app;

func first(pos value: bool)
{
}

func second(pos value: bool)
{
}

overload choose =
{
    first,
    first,
    second,
}
"#,
        );

        let diagnostics = diagnostic_kinds(compilation.semantic_diagnostics());

        assert_eq!(
            diagnostics
                .iter()
                .filter(|kind| **kind == DiagnosticKind::CheckingDuplicateCallableOverloadArm)
                .count(),
            1
        );

        assert_eq!(
            diagnostics
                .iter()
                .filter(|kind| {
                    **kind == DiagnosticKind::CheckingConflictingCallableOverloadSignature
                })
                .count(),
            2
        );
    }

    #[test]
    fn callable_overloads_stop_at_the_comparison_limit() {
        let options = CompilationOptions::new(
            WorkerBudget::serial(),
            bray_symbols::ProductKind::Library,
            SelectedTarget::baseline(),
        )
        .with_semantic_analysis_limits(SemanticAnalysisLimits::new(256, 0));

        let compilation = compilation_with_options(
            r#"module app;

func first(pos value: bool)
{
}

func second(pos value: bool)
{
}

overload choose =
{
    first,
    second,
}
"#,
            options,
        );

        let diagnostics = compilation
            .callable_overload_diagnostics(&compilation.state.cancellation)
            .unwrap_or_else(|error| panic!("limited overload validation must finish: {error:?}"));

        assert_eq!(
            diagnostic_kinds(diagnostics),
            [DiagnosticKind::CheckingCallableOverloadLimitExceeded]
        );
    }

    #[test]
    fn callable_overload_families_report_invalid_and_cross_family_arms() {
        let compilation = compilation(
            r#"module app;

struct Value
{
}

func convert(pos value: bool)
{
}

overload first =
{
    Value,
    convert,
}

overload second =
{
    convert,
}
"#,
        );

        let diagnostics = diagnostic_kinds(compilation.semantic_diagnostics());

        assert_eq!(
            diagnostics
                .iter()
                .filter(|kind| **kind == DiagnosticKind::CheckingInvalidCallableOverloadArm)
                .count(),
            1
        );

        assert_eq!(
            diagnostics
                .iter()
                .filter(|kind| {
                    **kind == DiagnosticKind::CheckingConflictingCallableOverloadFamily
                })
                .count(),
            2
        );
    }

    #[test]
    fn impossible_generic_arms_do_not_conflict_with_applicable_arms() {
        let compilation = compilation(
            r#"module app;

func impossible<T>(pos value: T)
    with(false)
{
}

func concrete(pos value: bool)
{
}

overload choose =
{
    impossible,
    concrete,
}
"#,
        );

        let diagnostics = diagnostic_kinds(compilation.semantic_diagnostics());

        assert!(
            !diagnostics.contains(&DiagnosticKind::CheckingConflictingCallableOverloadSignature)
        );
    }

    const fn is_callable_overload_diagnostic(kind: DiagnosticKind) -> bool {
        matches!(
            kind,
            DiagnosticKind::CheckingInvalidCallableOverloadArm
                | DiagnosticKind::CheckingDuplicateCallableOverloadArm
                | DiagnosticKind::CheckingConflictingCallableOverloadFamily
                | DiagnosticKind::CheckingConflictingCallableOverloadSignature
        )
    }
}
