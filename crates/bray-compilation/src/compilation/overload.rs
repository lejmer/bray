use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_binder::{NameAccess, SymbolQueryProvider};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticCallableOverloadArm,
    DiagnosticCallableOverloadContext, DiagnosticCallableOverloadProblem, DiagnosticKind,
    DiagnosticLabelKind, DiagnosticRelatedLocation, DiagnosticRelatedLocationKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, CallableOverloadSymbolId, CallableSignatureQuery, CallableSignatureTemplate,
    CallableSymbolId, GenericDeclarationTemplate, GenericDeclarationTemplateQuery, GenericOwnerId,
    ImplementationSymbolId, ImportedSymbolSkeleton, MemberLookupResult, NamedTypeSymbolId,
    SymbolGraph, SymbolOrigin, SymbolQueryRequest, TraitSymbolId, diagnostic_symbol_kind,
};
use bray_syntax::PathSyntax;

use super::Compilation;
use super::binder::{CompilationBindingContext, binding_query_error};
use super::diagnostics::symbol_diagnostic_identity;
use super::foreign::diagnostic::template_diagnostic_type;
use super::limits::try_count_comparison;
use super::overlap::callable_selection_surfaces_overlap;
use crate::fact::{CancellationToken, CompilationFactKey, FactQueryError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum CallableFamilyContext {
    Module(bray_symbols::ModuleSymbolId),
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
    diagnostic: DiagnosticCallableOverloadArm,
}

impl Compilation {
    pub(in crate::compilation) fn callable_overload_diagnostics(
        &self,
        cancellation: &CancellationToken,
    ) -> Result<&DiagnosticBag, FactQueryError> {
        self.query_with_cancellation(
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
        let binding_context = self.binding_context(cancellation)?;

        let imported = binding_context
            .imported_symbols()
            .map_err(binding_query_error)?;

        let values = self.semantic_value_store()?;

        let mut diagnostics = DiagnosticBag::new();
        let mut memberships = BTreeMap::new();
        let mut reported_membership_conflicts = BTreeSet::new();

        let maximum_comparisons = self.semantic_pairwise_limit();

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

            let owner = symbols
                .containing_symbol(family.id().into())
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let mut seen = BTreeMap::<CallableSymbolId, Vec<SyntaxAnchor>>::new();
            let mut arms: Vec<CallableArm> = Vec::new();

            for anchor in family.arm_syntax() {
                let path = anchor
                    .find_descendant::<PathSyntax>(self.syntax_tree())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let result = match owner {
                    AnySymbolId::Module(module) => {
                        binding_context.bind_surface_path(module, &path, NameAccess::Internal)
                    }
                    owner => {
                        binding_context.bind_owner_surface_path(owner, &path, NameAccess::Internal)
                    }
                }
                .map_err(binding_query_error)?;

                diagnostics.add_range(result.diagnostics().iter().cloned());

                let MemberLookupResult::Found(symbol) = result.value() else {
                    continue;
                };

                let Some(callable) = CallableSymbolId::try_from_any(*symbol) else {
                    diagnostics.add(problem_diagnostic(
                        *anchor,
                        DiagnosticKind::CheckingInvalidCallableOverloadArm,
                        DiagnosticCallableOverloadProblem::ArmSymbolKind {
                            symbol: symbol_diagnostic_identity(symbols, imported, *symbol)?,
                            actual: diagnostic_symbol_kind(symbol.kind()),
                        },
                        &[],
                    ));

                    continue;
                };

                let Some(arm) = self.callable_overload_arm(
                    &binding_context,
                    imported,
                    callable,
                    *anchor,
                    cancellation,
                    &mut diagnostics,
                )?
                else {
                    continue;
                };

                if let Some(previous) = seen.get_mut(&callable) {
                    diagnostics.add(problem_diagnostic(
                        *anchor,
                        DiagnosticKind::CheckingDuplicateCallableOverloadArm,
                        DiagnosticCallableOverloadProblem::DuplicateArm {
                            arm: arm.diagnostic,
                        },
                        previous,
                    ));

                    previous.push(*anchor);

                    continue;
                }

                seen.insert(callable, vec![*anchor]);

                record_family_membership(
                    &mut diagnostics,
                    &mut memberships,
                    &mut reported_membership_conflicts,
                    symbols,
                    imported,
                    family.id(),
                    callable,
                    &arm.diagnostic,
                    *anchor,
                )?;

                if let (Some(expected_context), Some(provided_context)) =
                    (expected_context, arm.context)
                {
                    if provided_context != expected_context {
                        diagnostics.add(problem_diagnostic(
                            *anchor,
                            DiagnosticKind::CheckingInvalidCallableOverloadArm,
                            DiagnosticCallableOverloadProblem::ContextMismatch {
                                arm: arm.diagnostic,
                                required: diagnostic_callable_context(
                                    symbols,
                                    imported,
                                    expected_context,
                                )?,
                                provided: diagnostic_callable_context(
                                    symbols,
                                    imported,
                                    provided_context,
                                )?,
                            },
                            &[],
                        ));

                        continue;
                    }
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
                        let attempted = comparisons
                            .checked_add(1)
                            .ok_or(FactQueryError::InfrastructureFailure)?;

                        diagnostics.add(
                            source_diagnostic(
                                left.anchor,
                                DiagnosticKind::CheckingCallableOverloadLimitExceeded,
                            )
                            .with_arg(DiagnosticArg::actual_count(attempted))
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
                        diagnostics.add(problem_diagnostic(
                            left.anchor,
                            DiagnosticKind::CheckingConflictingCallableOverloadSignature,
                            DiagnosticCallableOverloadProblem::ConflictingSignatures {
                                arm: left.diagnostic.clone(),
                                conflicting: right.diagnostic.clone(),
                            },
                            &[right.anchor],
                        ));

                        diagnostics.add(problem_diagnostic(
                            right.anchor,
                            DiagnosticKind::CheckingConflictingCallableOverloadSignature,
                            DiagnosticCallableOverloadProblem::ConflictingSignatures {
                                arm: right.diagnostic.clone(),
                                conflicting: left.diagnostic.clone(),
                            },
                            &[left.anchor],
                        ));
                    }
                }
            }
        }

        Ok(diagnostics)
    }

    fn callable_overload_arm(
        &self,
        binding_context: &CompilationBindingContext<'_>,
        imported: Option<&ImportedSymbolSkeleton>,
        callable: CallableSymbolId,
        anchor: SyntaxAnchor,
        cancellation: &CancellationToken,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<CallableArm>, FactQueryError> {
        let signature = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(callable))
            .map_err(binding_query_error)?;

        diagnostics.add_range(signature.diagnostics().iter().cloned());

        let Some(owner) = GenericOwnerId::try_new(callable.into_any()) else {
            return Err(FactQueryError::InfrastructureFailure);
        };

        let generic = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<GenericDeclarationTemplateQuery>::new(
                owner,
            ))
            .map_err(binding_query_error)?;

        diagnostics.add_range(generic.diagnostics().iter().cloned());

        let may_be_satisfied =
            self.generic_declaration_may_be_satisfied(generic.value(), cancellation)?;

        let context = self
            .symbol_graph()?
            .containing_symbol(callable.into_any())
            .and_then(callable_family_context);

        let parameter_types = signature
            .value()
            .parameter_type_templates(self.semantic_value_store()?)
            .map_err(|_| FactQueryError::InfrastructureFailure)?
            .iter()
            .map(|ty| template_diagnostic_type(self, ty, cancellation))
            .collect::<Result<Vec<_>, _>>()?;

        let diagnostic = DiagnosticCallableOverloadArm::new(
            symbol_diagnostic_identity(self.symbol_graph()?, imported, callable.into_any())?,
            signature.value().receiver().is_some(),
            parameter_types,
            template_diagnostic_type(self, signature.value().result(), cancellation)?,
        );

        Ok(Some(CallableArm {
            anchor,
            signature,
            generic,
            context,
            may_be_satisfied,
            diagnostic,
        }))
    }
}

fn callable_family_context(symbol: AnySymbolId) -> Option<CallableFamilyContext> {
    match symbol {
        AnySymbolId::Module(id) => Some(CallableFamilyContext::Module(id)),
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

fn diagnostic_callable_context(
    symbols: &SymbolGraph,
    imported: Option<&ImportedSymbolSkeleton>,
    context: CallableFamilyContext,
) -> Result<DiagnosticCallableOverloadContext, FactQueryError> {
    let context = match context {
        CallableFamilyContext::Module(id) => DiagnosticCallableOverloadContext::Module(
            symbol_diagnostic_identity(symbols, imported, id.into())?,
        ),
        CallableFamilyContext::NamedType(id) => {
            let symbol = match id {
                NamedTypeSymbolId::Struct(id) => AnySymbolId::Struct(id),
                NamedTypeSymbolId::Union(id) => AnySymbolId::Union(id),
            };

            DiagnosticCallableOverloadContext::NamedType(symbol_diagnostic_identity(
                symbols, imported, symbol,
            )?)
        }
        CallableFamilyContext::Trait(id) => DiagnosticCallableOverloadContext::Trait(
            symbol_diagnostic_identity(symbols, imported, id.into())?,
        ),
        CallableFamilyContext::Implementation(id) => {
            DiagnosticCallableOverloadContext::Implementation(symbol_diagnostic_identity(
                symbols,
                imported,
                id.into_any(),
            )?)
        }
    };

    Ok(context)
}

fn record_family_membership(
    diagnostics: &mut DiagnosticBag,
    memberships: &mut BTreeMap<CallableSymbolId, (CallableOverloadSymbolId, SyntaxAnchor)>,
    reported: &mut BTreeSet<(
        CallableOverloadSymbolId,
        CallableOverloadSymbolId,
        CallableSymbolId,
    )>,
    symbols: &SymbolGraph,
    imported: Option<&ImportedSymbolSkeleton>,
    family: CallableOverloadSymbolId,
    callable: CallableSymbolId,
    arm: &DiagnosticCallableOverloadArm,
    anchor: SyntaxAnchor,
) -> Result<(), FactQueryError> {
    let Some((previous_family, previous_anchor)) = memberships.get(&callable).copied() else {
        memberships.insert(callable, (family, anchor));

        return Ok(());
    };

    if previous_family == family {
        return Ok(());
    }

    let key = if previous_family < family {
        (previous_family, family, callable)
    } else {
        (family, previous_family, callable)
    };

    if !reported.insert(key) {
        return Ok(());
    }

    let first = symbol_diagnostic_identity(symbols, imported, previous_family.into())?;
    let second = symbol_diagnostic_identity(symbols, imported, family.into())?;

    diagnostics.add(problem_diagnostic(
        previous_anchor,
        DiagnosticKind::CheckingConflictingCallableOverloadFamily,
        DiagnosticCallableOverloadProblem::ConflictingFamilies {
            arm: arm.clone(),
            first: first.clone(),
            second: second.clone(),
        },
        &[anchor],
    ));

    diagnostics.add(problem_diagnostic(
        anchor,
        DiagnosticKind::CheckingConflictingCallableOverloadFamily,
        DiagnosticCallableOverloadProblem::ConflictingFamilies {
            arm: arm.clone(),
            first,
            second,
        },
        &[previous_anchor],
    ));

    Ok(())
}

fn source_diagnostic(anchor: SyntaxAnchor, kind: DiagnosticKind) -> Diagnostic {
    let label = match kind {
        DiagnosticKind::CheckingDuplicateCallableOverloadArm => {
            DiagnosticLabelKind::DuplicateOverloadArm
        }
        DiagnosticKind::CheckingConflictingCallableOverloadFamily
        | DiagnosticKind::CheckingConflictingCallableOverloadSignature => {
            DiagnosticLabelKind::ConflictingOverload
        }
        DiagnosticKind::CheckingInvalidCallableOverloadArm
        | DiagnosticKind::CheckingCallableOverloadLimitExceeded => {
            DiagnosticLabelKind::InvalidOverload
        }
        _ => unreachable!("callable overload diagnostics must use an overload category"),
    };

    crate::compilation::diagnostics::labeled_source_diagnostic(anchor, kind, label)
}

fn problem_diagnostic(
    anchor: SyntaxAnchor,
    kind: DiagnosticKind,
    problem: DiagnosticCallableOverloadProblem,
    related: &[SyntaxAnchor],
) -> Diagnostic {
    let relationship = if kind == DiagnosticKind::CheckingDuplicateCallableOverloadArm {
        DiagnosticRelatedLocationKind::FirstDeclaration
    } else {
        DiagnosticRelatedLocationKind::ConflictingDeclaration
    };

    related.iter().copied().fold(
        source_diagnostic(anchor, kind).with_arg(DiagnosticArg::callable_overload_problem(problem)),
        |diagnostic, related| {
            diagnostic.with_related_location(DiagnosticRelatedLocation::new(
                relationship,
                SourceSpan::new(related.source_id(), related.full_range()),
            ))
        },
    )
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticArgName, DiagnosticArgValue, DiagnosticCallableOverloadProblem, DiagnosticKind,
    };

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
    fn callable_overload_families_report_indistinguishable_arms() {
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
    second,
}
"#,
        );

        let diagnostics = diagnostic_kinds(compilation.semantic_diagnostics());

        assert_eq!(
            diagnostics
                .iter()
                .filter(|kind| {
                    **kind == DiagnosticKind::CheckingConflictingCallableOverloadSignature
                })
                .count(),
            2
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.semantic_diagnostics(),
            DiagnosticKind::CheckingConflictingCallableOverloadSignature,
        );
    }

    #[test]
    fn callable_overloads_do_not_select_by_result_type() {
        let compilation = compilation(
            r#"module app;

func integer(pos value: bool) -> i32
{
    return 1;
}

func boolean(pos value: bool) -> bool
{
    return value;
}

overload choose =
{
    integer,
    boolean,
}
"#,
        );

        assert!(
            diagnostic_kinds(compilation.semantic_diagnostics())
                .contains(&DiagnosticKind::CheckingConflictingCallableOverloadSignature)
        );
    }

    #[test]
    fn callable_overloads_do_not_select_by_execution_mode() {
        let compilation = compilation(
            r#"module app;

func synchronous(pos value: bool)
{
}

async func asynchronous(pos value: bool)
{
}

overload choose =
{
    synchronous,
    asynchronous,
}
"#,
        );

        assert!(
            diagnostic_kinds(compilation.semantic_diagnostics())
                .contains(&DiagnosticKind::CheckingConflictingCallableOverloadSignature)
        );
    }

    #[test]
    fn type_owned_overloads_do_not_select_by_receiver_capability() {
        let compilation = compilation(
            r#"module app;

struct Value
{
    internal func shared() -> i32
    {
        return 1;
    }

    internal mut func mutable() -> i32
    {
        return 2;
    }

    overload read =
    {
        shared,
        mutable,
    }
}
"#,
        );

        assert!(
            diagnostic_kinds(compilation.semantic_diagnostics())
                .contains(&DiagnosticKind::CheckingConflictingCallableOverloadSignature)
        );
    }

    #[test]
    fn callable_overload_repeated_arm_retains_every_prior_origin() {
        let compilation = compilation(
            r#"module app;

func first(pos value: bool)
{
}

overload choose =
{
    first,
    first,
    first,
}
"#,
        );

        let diagnostics = compilation
            .semantic_diagnostics()
            .iter()
            .filter(|diagnostic| {
                diagnostic.kind() == DiagnosticKind::CheckingDuplicateCallableOverloadArm
            })
            .collect::<Vec<_>>();

        assert_eq!(diagnostics.len(), 2);
        assert_eq!(diagnostics[0].related_locations().len(), 1);
        assert_eq!(diagnostics[1].related_locations().len(), 2);

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.semantic_diagnostics(),
            DiagnosticKind::CheckingDuplicateCallableOverloadArm,
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

        bray_testing::assert_goal_state_diagnostic_kind(
            diagnostics,
            DiagnosticKind::CheckingCallableOverloadLimitExceeded,
        );
    }

    #[test]
    fn callable_overload_families_report_cross_family_arms() {
        let compilation = compilation(
            r#"module app;

func convert(pos value: bool)
{
}

overload first =
{
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
                .filter(|kind| {
                    **kind == DiagnosticKind::CheckingConflictingCallableOverloadFamily
                })
                .count(),
            2
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.semantic_diagnostics(),
            DiagnosticKind::CheckingConflictingCallableOverloadFamily,
        );
    }

    #[test]
    fn callable_overload_arm_reports_the_exact_non_callable_symbol() {
        let compilation = compilation(
            r#"module app;

struct Value
{
}

overload first =
{
    Value,
}
"#,
        );

        assert_eq!(
            diagnostic_kinds(compilation.semantic_diagnostics()),
            [DiagnosticKind::CheckingInvalidCallableOverloadArm]
        );

        bray_testing::assert_goal_state_diagnostic_kind(
            compilation.semantic_diagnostics(),
            DiagnosticKind::CheckingInvalidCallableOverloadArm,
        );
    }

    #[test]
    fn callable_overload_arm_reports_its_different_owning_context() {
        let compilation = compilation(
            r#"module app;

func outside()
{
}

struct Value
{
    func member()
    {
    }

    overload choose =
    {
        member,
        outside,
    }
}
"#,
        );

        let problem = callable_overload_problem(compilation.semantic_diagnostics());

        assert!(
            matches!(
                problem,
                DiagnosticCallableOverloadProblem::ContextMismatch { .. }
            ),
            "unexpected overload problem: {problem:?}"
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

    fn callable_overload_problem(
        diagnostics: &bray_diagnostics::DiagnosticBag,
    ) -> &DiagnosticCallableOverloadProblem {
        let diagnostic = diagnostics
            .iter()
            .find(|diagnostic| {
                diagnostic.kind() == DiagnosticKind::CheckingInvalidCallableOverloadArm
            })
            .unwrap_or_else(|| {
                panic!("expected one invalid callable overload diagnostic, got: {diagnostics:?}")
            });

        let argument = diagnostic
            .args()
            .iter()
            .find(|argument| argument.name() == DiagnosticArgName::CallableOverloadProblem)
            .unwrap_or_else(|| panic!("expected callable overload problem argument"));

        let DiagnosticArgValue::CallableOverloadProblem(problem) = argument.value() else {
            panic!("callable overload problem argument has wrong value: {argument:?}");
        };

        problem
    }
}
