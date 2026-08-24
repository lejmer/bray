use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundUnit, BoundUnitRoot, BoundWalkControl, BoundWalkEvent,
    BoundWalkOutcome, SemanticSelection, SemanticSelectionEntry, walk_bound_unit_view,
};
use bray_checker::{NestedCallableEvidence, check_generic_arguments};
use bray_diagnostics::{
    Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{
    AnySymbolId, CheckedConstraintKind, GenericConstraintObligationKey, GenericConstraintsQuery,
    GenericOwnerId, GenericSubstitutionData, ImplementationRequirementKey, ImplementationSelection,
    ProofOutcome, StaticInstanceKey, StaticInstanceTemplateId, StaticReferenceSelection,
    SymbolQueryRequest,
};
use bray_syntax::GenericArgumentListSyntax;

use super::support::checker_unit_view;
use crate::compilation::binder::{
    binding_query_error, generic_parameter_ids, type_binder,
};
use crate::compilation::checker::checker_result;
use crate::compilation::state::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn static_instance_selections(
        &self,
        key: &bray_bound_tree::BoundUnitKey,
        cancellation: &CancellationToken,
        bound: &BoundUnit,
        semantic_context: &bray_checker::SemanticUnitContext,
        checker_context: &impl bray_checker::CheckerRequestContext,
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

            let bray_bound_tree::BoundReferenceTarget::Surface(AnySymbolId::Static(declaration)) =
                name.target()
            else {
                continue;
            };

            let parameters = generic_parameter_ids(binding_context.symbols(), declaration.into())
                .map_err(binding_query_error)?;

            let source = name.origin().source_anchor().syntax();

            let resolved = match name.generic_argument_list() {
                Some(anchor) => {
                    let arguments = anchor
                        .find_descendant::<GenericArgumentListSyntax>(self.syntax_tree())
                        .ok_or(FactQueryError::InfrastructureFailure)?;

                    let arguments = arguments.generic_arguments().collect::<Vec<_>>();

                    let bound_arguments = type_binder(&binding_context, owner)
                        .map_err(binding_query_error)?
                        .bind_call_generic_arguments(&arguments, &parameters)
                        .map_err(binding_query_error)?;

                    let (bound_arguments, argument_diagnostics) = bound_arguments.into_parts();

                    diagnostics = diagnostics.merged(&argument_diagnostics);

                    let resolved = checker_result(check_generic_arguments(unit, &bound_arguments))?;

                    let (resolved, resolution_diagnostics) = resolved.into_parts();

                    diagnostics = diagnostics.merged(&resolution_diagnostics);

                    let Some(resolved) = resolved else {
                        continue;
                    };

                    resolved
                }
                None if parameters.is_empty() => Vec::new(),
                None => continue,
            };

            let substitution = GenericSubstitutionData::try_new(
                GenericOwnerId::try_new(declaration.into())
                    .ok_or(FactQueryError::InfrastructureFailure)?,
                parameters,
                resolved,
            )
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let substitution = binding_context
                .semantic_values()
                .intern_generic_substitution(substitution)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let (selection, selection_diagnostics) = self.static_reference_selection(
                declaration,
                substitution,
                cancellation,
                &binding_context,
                source,
            )?;

            diagnostics = diagnostics.merged(&selection_diagnostics);

            entries.push(SemanticSelectionEntry::new(
                expression,
                SemanticSelection::StaticReference(selection),
            ));
        }

        Ok((entries, diagnostics))
    }

    pub(in crate::compilation) fn static_reference_selection(
        &self,
        declaration: bray_symbols::StaticSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
        cancellation: &CancellationToken,
        binding_context: &crate::compilation::binder::CompilationBindingContext<'_>,
        source: bray_declarations::SyntaxAnchor,
    ) -> Result<(StaticReferenceSelection, DiagnosticBag), FactQueryError> {
        let template = StaticInstanceTemplateId::new(declaration);
        let target = self.requested_target().profile().identity().clone();

        let Ok(concrete_substitution) = binding_context
            .semantic_values()
            .require_concrete_substitution(substitution)
        else {
            return Ok((
                StaticReferenceSelection::open(template, substitution, [], target),
                DiagnosticBag::new(),
            ));
        };

        let (witnesses, diagnostics) = self.static_instance_witnesses(
            declaration,
            substitution,
            cancellation,
            binding_context,
            source,
        )?;

        Ok((
            StaticReferenceSelection::Closed(StaticInstanceKey::new(
                template,
                concrete_substitution,
                witnesses,
                target,
            )),
            diagnostics,
        ))
    }

    pub(in crate::compilation) fn static_instance_witnesses(
        &self,
        declaration: bray_symbols::StaticSymbolId,
        substitution: bray_symbols::GenericSubstitutionId,
        cancellation: &CancellationToken,
        binding_context: &crate::compilation::binder::CompilationBindingContext<'_>,
        source: bray_declarations::SyntaxAnchor,
    ) -> Result<(Vec<bray_symbols::ImplementationInstanceId>, DiagnosticBag), FactQueryError> {
        let owner = GenericOwnerId::try_new(declaration.into())
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let satisfaction = self.generic_constraint_satisfaction_with_cancellation(
            GenericConstraintObligationKey::new(owner, substitution),
            cancellation,
        )?;

        let constraints = binding_context
            .resolve_symbol_query(SymbolQueryRequest::<GenericConstraintsQuery>::new(owner))
            .map_err(binding_query_error)?;

        let mut diagnostics = satisfaction.diagnostics().merged(constraints.diagnostics());

        if *satisfaction.value() != ProofOutcome::Proven {
            let span = SourceSpan::new(source.source_id(), source.full_range());

            diagnostics.add(
                Diagnostic::new(
                    DiagnosticId::new(span.start().bytes()),
                    DiagnosticKind::CheckingStaticConstraintUnsatisfied,
                    SeverityKind::Error,
                )
                .with_primary_span(span),
            );
        }

        let values = binding_context.semantic_values();
        let mut witnesses = Vec::new();

        for constraint in constraints.value().constraints() {
            let CheckedConstraintKind::TraitSatisfaction {
                subject,
                application,
            } = constraint.kind()
            else {
                continue;
            };

            let subject = values
                .substitute_type(subject, substitution)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let application = values
                .substitute_trait_application(application, substitution)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let selection = self.implementation_selection_result_with_cancellation(
                ImplementationRequirementKey::new(subject, application),
                cancellation,
            )?;

            diagnostics = diagnostics.merged(selection.diagnostics());

            if let ImplementationSelection::Selected(witness) = selection.value() {
                witnesses.push(*witness);
            }
        }

        Ok((witnesses, diagnostics))
    }

    pub(super) fn nested_callable_evidence(
        &self,
        bound: &BoundUnit,
        cancellation: &CancellationToken,
    ) -> Result<Vec<NestedCallableEvidence>, FactQueryError> {
        let mut evidence = Vec::new();
        let mut failure = None;

        let outcome = walk_bound_unit_view(bound.view(), bound.root(), |event| {
            if cancellation.is_cancelled() {
                failure = Some(FactQueryError::Cancelled);

                return BoundWalkControl::Stop;
            }

            let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
                return BoundWalkControl::Continue;
            };

            let Some(BoundExpression::AnonymousCallable(callable)) =
                bound.view().expression(expression)
            else {
                return BoundWalkControl::Continue;
            };

            // Each independently demandable nested query owns its cache key.
            let nested_bound =
                match self.bound_unit_with_cancellation(callable.unit().clone(), cancellation) {
                    Ok(nested) => nested,
                    Err(error) => {
                        failure = Some(error);

                        return BoundWalkControl::Stop;
                    }
                };

            let BoundUnitRoot::AnonymousCallable {
                callable: callable_symbol,
                ..
            } = nested_bound.result().value().root()
            else {
                failure = Some(FactQueryError::InfrastructureFailure);

                return BoundWalkControl::Stop;
            };

            let nested = match self.declared_value_type_templates_with_cancellation(
                callable.unit().clone(),
                cancellation,
            ) {
                Ok(nested) => nested,
                Err(error) => {
                    failure = Some(error);

                    return BoundWalkControl::Stop;
                }
            };

            let Some(callable_type) = nested.result().value().callable_type() else {
                failure = Some(FactQueryError::InfrastructureFailure);

                return BoundWalkControl::Stop;
            };

            // The outer expression query owns this template after the nested query handle drops.
            evidence.push(NestedCallableEvidence::new(
                expression,
                callable_symbol,
                callable_type.clone(),
            ));

            BoundWalkControl::Continue
        });

        if let Some(error) = failure {
            return Err(error);
        }

        match outcome {
            BoundWalkOutcome::Completed => Ok(evidence),
            BoundWalkOutcome::Stopped | BoundWalkOutcome::MissingNode(_) => {
                Err(FactQueryError::InfrastructureFailure)
            }
        }
    }
}
