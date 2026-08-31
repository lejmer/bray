use bray_binder::BindingQueryContext;
use bray_bound_tree::{BoundExpression, BoundUnit, SemanticSelection, SemanticSelectionEntry};
use bray_checker::check_generic_arguments;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    AnySymbolId, CallableDefinitionId, CallableInstanceData, GenericOwnerId,
    GenericSubstitutionData, GenericSubstitutionId, StaticSymbolId,
};
use bray_syntax::GenericArgumentListSyntax;

use super::support::checker_unit_view;
use crate::compilation::binder::{binding_query_error, generic_parameter_ids, type_binder};
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
        semantic_context: &bray_checker::SemanticUnitContext,
        checker_context: &impl bray_checker::CheckerRequestContext<UpstreamError = FactQueryError>,
    ) -> Result<(Vec<SemanticSelectionEntry>, DiagnosticBag), FactQueryError> {
        let binding_context = self.binding_context_for(key, cancellation)?;

        let owner = binding_context
            .symbols()
            .symbol_for_key(key.declared_owner())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let unit = checker_unit_view(bound, semantic_context, checker_context)?;

        let mut entries = Vec::new();
        let mut diagnostics = DiagnosticBag::new();

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
                name.generic_argument_list(),
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

            entries.push(SemanticSelectionEntry::new(expression, selection));
        }

        Ok((entries, diagnostics))
    }

    fn named_reference_substitution<C>(
        &self,
        binding_context: &crate::compilation::binder::CompilationBindingContext<'_>,
        owner: AnySymbolId,
        symbol: AnySymbolId,
        generic_arguments: Option<bray_declarations::SyntaxAnchor>,
        unit: bray_checker::CheckerUnitView<'_, C>,
        diagnostics: &mut DiagnosticBag,
    ) -> Result<Option<GenericSubstitutionId>, FactQueryError>
    where
        C: bray_checker::CheckerRequestContext + ?Sized,
        C::UpstreamError: Into<FactQueryError>,
    {
        let parameters = generic_parameter_ids(binding_context.symbols(), symbol)
            .map_err(binding_query_error)?;

        let resolved = match generic_arguments {
            Some(anchor) => {
                let arguments = anchor
                    .find_descendant::<GenericArgumentListSyntax>(self.syntax_tree())
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                let arguments = arguments.generic_arguments().collect::<Vec<_>>();

                let bound_arguments = type_binder(binding_context, owner)
                    .map_err(binding_query_error)?
                    .bind_call_generic_arguments(&arguments, &parameters)
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

        let owner = GenericOwnerId::try_new(symbol).ok_or(FactQueryError::InfrastructureFailure)?;

        let substitution = GenericSubstitutionData::try_new(owner, parameters, resolved)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        binding_context
            .semantic_values()
            .intern_generic_substitution(substitution)
            .map(Some)
            .map_err(|_| FactQueryError::InfrastructureFailure)
    }
}
