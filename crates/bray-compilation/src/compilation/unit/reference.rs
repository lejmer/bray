use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{BoundExpression, BoundUnit, SemanticSelection, SemanticSelectionEntry};
use bray_checker::check_generic_arguments;
use bray_diagnostics::DiagnosticBag;
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableInstanceData, GenericDeclarationTemplateQuery,
    GenericOwnerId, GenericSubstitutionData, GenericSubstitutionId, StaticSymbolId,
    SymbolQueryRequest,
};
use bray_syntax::GenericArgumentListSyntax;

use super::support::{checker_unit_view, unit_contract_failure};
use crate::compilation::binder::{binding_query_error, type_binder};
use crate::compilation::checker::checker_result;
use crate::compilation::state::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

enum NamedReference {
    Callable(CallableDefinitionId),
    Static(StaticSymbolId),
}

impl Compilation {
    pub(super) fn named_reference_selections(
        &self,
        key: &bray_bound_tree::BoundUnitKey,
        cancellation: &CancellationToken,
        bound: &BoundUnit,
        types: &bray_bound_tree::CheckedExpressionTypes,
        semantic_context: &bray_checker::SemanticUnitContext,
        checker_context: &impl bray_checker::CheckerRequestContext<UpstreamError = FactQueryError>,
    ) -> Result<(Vec<SemanticSelectionEntry>, DiagnosticBag), FactQueryError> {
        let binding_context = self.binding_context_for(key, cancellation)?;

        let owner = binding_context
            .symbols()
            .symbol_for_key(key.declared_owner())
            .ok_or_else(|| {
                unit_contract_failure(
                    key,
                    crate::compilation::SemanticQueryViolation::Missing(
                        crate::compilation::SemanticDataKind::Symbol,
                    ),
                )
            })?;

        let unit = checker_unit_view(bound, semantic_context, checker_context)?;

        let mut entries = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

        let callees = bound
            .tree()
            .expressions()
            .filter_map(|(_, node)| match node {
                BoundExpression::Call(call) => Some(call.callee()),
                _ => None,
            })
            .collect::<std::collections::BTreeSet<_>>();

        for (expression, node) in bound.tree().expressions() {
            let BoundExpression::Name(name) = node else {
                continue;
            };

            let bray_bound_tree::BoundReferenceTarget::Surface(symbol) = name.target() else {
                continue;
            };

            let reference = match symbol {
                AnySymbolId::Static(declaration) => NamedReference::Static(declaration),
                symbol => {
                    let Some(definition) = CallableDefinitionId::try_new(symbol) else {
                        continue;
                    };

                    NamedReference::Callable(definition)
                }
            };

            let Some(substitution) = self.named_reference_substitution(
                &binding_context,
                owner,
                symbol,
                *name,
                unit,
                &mut diagnostics,
            )?
            else {
                continue;
            };

            let selection = match reference {
                NamedReference::Callable(definition) => SemanticSelection::CallableReference(
                    CallableInstanceData::new(definition, substitution),
                ),
                NamedReference::Static(declaration) => {
                    let (selection, selection_diagnostics) = self.static_reference_selection(
                        declaration,
                        substitution,
                        cancellation,
                        &binding_context,
                        name.origin().source_anchor().syntax(),
                    )?;

                    diagnostics = diagnostics.merged(&selection_diagnostics);

                    SemanticSelection::StaticReference(selection)
                }
            };

            if let SemanticSelection::CallableReference(instance) = &selection
                && !callees.contains(&expression)
                && let Some(expected) = types.expression(expression)
                && !expected.is_recovered()
                && let Some(actual) = self.resolve_callable_instance_signature(
                    &binding_context,
                    *instance,
                    &mut diagnostics,
                )?
                && !bray_checker::callable_contract_conversion_is_valid(
                    binding_context.semantic_values(),
                    actual.callable_type(),
                    expected.ty(),
                )
            {
                let anchor = name.origin().source_anchor().syntax();
                let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

                diagnostics.add(
                    bray_diagnostics::Diagnostic::new(
                        bray_diagnostics::DiagnosticId::new(span.start().bytes()),
                        bray_diagnostics::DiagnosticKind::CheckingIncompatibleExpressionType,
                        bray_diagnostics::SeverityKind::Error,
                    )
                    .with_primary_span(span)
                    .with_label(bray_diagnostics::DiagnosticLabel::primary(
                        bray_diagnostics::DiagnosticLabelKind::IncompatibleExpressionType,
                        span,
                    ))
                    .with_arg(bray_diagnostics::DiagnosticArg::expected_type(
                        bray_checker::diagnostic_type(checker_context, expected.ty())?,
                    ))
                    .with_arg(bray_diagnostics::DiagnosticArg::actual_type(
                        bray_checker::diagnostic_type(checker_context, actual.callable_type())?,
                    )),
                );
            }

            entries.push(SemanticSelectionEntry::new(expression, selection));
        }

        Ok((entries, diagnostics))
    }

    fn generic_reference_arity_diagnostic(
        &self,
        name: bray_bound_tree::BoundNameExpression,
        symbol: AnySymbolId,
        expected: usize,
        actual: usize,
    ) -> Result<bray_diagnostics::Diagnostic, FactQueryError> {
        let source = name.origin().source_anchor().syntax();

        let syntax = self
            .syntax_tree()
            .find_node(
                source.source_id(),
                source.syntax_kind(),
                source.full_range(),
                source.is_recovered(),
            )
            .ok_or_else(|| {
                crate::compilation::SemanticQueryFailure::located_contract(
                    crate::compilation::SemanticQueryContext::Symbol(symbol),
                    crate::compilation::SemanticQueryViolation::Missing(
                        crate::compilation::SemanticDataKind::Syntax,
                    ),
                    SourceSpan::new(source.source_id(), source.full_range()),
                )
            })?;

        bray_binder::generic_argument_count_diagnostic(&syntax, expected, actual).map_err(|cause| {
            crate::compilation::SemanticQueryFailure::GenericSubstitution {
                owner: GenericOwnerId::try_new(symbol),
                cause,
            }
            .into()
        })
    }

    fn named_reference_substitution<C>(
        &self,
        binding_context: &crate::compilation::binder::CompilationBindingContext<'_>,
        owner: AnySymbolId,
        symbol: AnySymbolId,
        name: bray_bound_tree::BoundNameExpression,
        unit: bray_checker::CheckerUnitView<'_, C>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<GenericSubstitutionId>, FactQueryError>
    where
        C: bray_checker::CheckerRequestContext + ?Sized,
        C::UpstreamError: Into<FactQueryError>,
    {
        let generic_owner = GenericOwnerId::try_new(symbol).ok_or_else(|| {
            crate::compilation::SemanticQueryFailure::contract(
                crate::compilation::SemanticQueryContext::Symbol(symbol),
                crate::compilation::SemanticQueryViolation::UnexpectedSymbolKind {
                    expected: crate::compilation::SemanticSymbolCategory::GenericOwner,
                    actual: symbol.kind(),
                },
            )
        })?;

        let declaration = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<GenericDeclarationTemplateQuery>::new(
                generic_owner,
            ))
            .map_err(binding_query_error)?;

        *diagnostics = diagnostics.merged(declaration.diagnostics());
        let parameters = declaration.value().parameters();

        let resolved = match name.generic_argument_list() {
            Some(anchor) => {
                let arguments = anchor
                    .find_descendant::<GenericArgumentListSyntax>(self.syntax_tree())
                    .ok_or_else(|| {
                        crate::compilation::SemanticQueryFailure::located_contract(
                            crate::compilation::SemanticQueryContext::Symbol(symbol),
                            crate::compilation::SemanticQueryViolation::Missing(
                                crate::compilation::SemanticDataKind::Syntax,
                            ),
                            SourceSpan::new(anchor.source_id(), anchor.full_range()),
                        )
                    })?;

                let arguments = arguments.generic_arguments().collect::<Vec<_>>();

                if arguments.len() != parameters.len() {
                    diagnostics.add(self.generic_reference_arity_diagnostic(
                        name,
                        symbol,
                        parameters.len(),
                        arguments.len(),
                    )?);

                    return Ok(None);
                }

                let bound_arguments = type_binder(binding_context, owner)
                    .map_err(binding_query_error)?
                    .bind_call_generic_arguments(&arguments, parameters)
                    .map_err(binding_query_error)?;

                let (bound_arguments, argument_diagnostics) = bound_arguments.into_parts();

                *diagnostics = diagnostics.merged(&argument_diagnostics);

                let resolved = checker_result(check_generic_arguments(unit, &bound_arguments))?;

                let (resolved, resolution_diagnostics) = resolved.into_parts();

                *diagnostics = diagnostics.merged(&resolution_diagnostics);

                let Some(resolved) = resolved else {
                    return Ok(None);
                };

                resolved
            }
            None if parameters.is_empty() => Vec::new(),
            None => return Ok(None),
        };

        let substitution =
            GenericSubstitutionData::try_new(generic_owner, parameters.iter().copied(), resolved)
                .map_err(
                |cause| crate::compilation::SemanticQueryFailure::GenericSubstitution {
                    owner: Some(generic_owner),
                    cause,
                },
            )?;

        binding_context
            .semantic_values()
            .intern_generic_substitution(substitution)
            .map(Some)
            .map_err(FactQueryError::SemanticValueStore)
    }
}
