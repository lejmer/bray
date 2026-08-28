use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundUnit, BoundUnitRoot, BoundWalkControl, BoundWalkEvent,
    BoundWalkOutcome, walk_bound_unit_view,
};
use bray_checker::NestedCallableEvidence;
use bray_diagnostics::{Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, SeverityKind};
use bray_source::SourceSpan;
use bray_symbols::{
    CheckedConstraintKind, GenericConstraintObligationKey, GenericConstraintsQuery, GenericOwnerId,
    ImplementationRequirementKey, ImplementationSelection, ProofOutcome, StaticInstanceKey,
    StaticInstanceTemplateId, StaticReferenceSelection, SymbolQueryRequest,
};

use crate::compilation::binder::binding_query_error;
use crate::compilation::state::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
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
