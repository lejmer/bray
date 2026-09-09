use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_bound_tree::{CallableProofTarget, CheckedSemanticSelections, SemanticSelection};
use bray_symbols::{
    CallableInstanceData, ExactSymbolId, GenericConstraintsQuery, GenericSubstitutionId,
    ImplementationRequirementKey, ResolvedCallableEvidenceTarget, SelfTypeContext,
    SymbolQueryRequest, TraitConstraintDispatch, TypeId,
};

use crate::compilation::binder::binding_query_error;
use crate::compilation::substitution::{
    substitute_callable_context, substitute_contextual_self,
    substitute_contextual_self_in_application,
};
use crate::compilation::{
    Compilation, SemanticDataKind, SemanticQueryContext, SemanticQueryFailure,
    SemanticQueryViolation,
};
use crate::fact::{CancellationToken, FactQueryError};

/// The substituted environment in which one declaration supplies its proof.
#[derive(Clone, Copy, Debug, Default, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct ProofContext {
    pub(super) substitution: Option<GenericSubstitutionId>,
    pub(super) contextual_self: Option<(SelfTypeContext, TypeId)>,
    pub(super) allow_open_requirements: bool,
}

pub(super) enum EvidenceSelection {
    Direct(CallableInstanceData, ProofContext),
    Fulfillment(CallableInstanceData, ProofContext),
    Open,
    Unavailable,
}

impl Compilation {
    pub(super) fn source_proof_context(
        &self,
        key: &bray_bound_tree::BoundUnitKey,
        cancellation: &CancellationToken,
    ) -> Result<ProofContext, FactQueryError> {
        let binding = self.binding_context(cancellation)?;

        let symbol = binding
            .symbols()
            .symbol_for_key(key.declared_owner())
            .ok_or_else(|| {
                SemanticQueryFailure::contract(
                    SemanticQueryContext::Unit(key.clone()),
                    SemanticQueryViolation::Missing(SemanticDataKind::Symbol),
                )
            })?;

        let Some(owner) = bray_symbols::GenericOwnerId::try_new(symbol) else {
            return Ok(ProofContext::default());
        };

        let parameters =
            crate::compilation::binder::visible_generic_parameters(binding.symbols(), symbol);

        let substitution = crate::compilation::substitution::identity_substitution(
            binding.semantic_values(),
            owner,
            &parameters,
        )?;

        let allow_open_requirements = !parameters.is_empty()
            || matches!(
                binding.symbols().contextual_self_scope(symbol),
                Some(SelfTypeContext::Trait(_))
            );

        Ok(ProofContext {
            substitution: Some(substitution),
            contextual_self: None,
            allow_open_requirements,
        })
    }

    pub(in crate::compilation) fn source_callable_evidence_target(
        &self,
        target: CallableProofTarget,
        selections: &CheckedSemanticSelections,
        cancellation: &CancellationToken,
    ) -> Result<Option<ResolvedCallableEvidenceTarget>, FactQueryError> {
        let (callable, dispatch) = match target {
            CallableProofTarget::Implicit { callable, .. } => (callable, None),
            CallableProofTarget::Call(expression) => {
                let Some(SemanticSelection::Call(call)) = selections.expression(expression) else {
                    return Ok(None);
                };

                let bray_bound_tree::BoundCallableTarget::Declaration(callable) = call.target()
                else {
                    return Ok(None);
                };

                (callable, call.resolution().trait_dispatch())
            }
        };

        let dispatch = dispatch
            .map(|dispatch| self.proof_dispatch_requirement(dispatch, cancellation))
            .transpose()?;

        let callable = self
            .semantic_value_store()?
            .intern_callable_instance(callable)?;

        Ok(Some(ResolvedCallableEvidenceTarget::new(
            callable, dispatch,
        )))
    }

    fn proof_dispatch_requirement(
        &self,
        dispatch: TraitConstraintDispatch,
        cancellation: &CancellationToken,
    ) -> Result<ImplementationRequirementKey, FactQueryError> {
        let (owner, ordinal) = match dispatch {
            TraitConstraintDispatch::Constraint { owner, ordinal } => (owner, ordinal),
            TraitConstraintDispatch::TraitDefault(requirement) => return Ok(requirement),
        };

        let binding = self.binding_context(cancellation)?;

        let constraints = binding
            .resolve_symbol_query(SymbolQueryRequest::<GenericConstraintsQuery>::new(owner))
            .map_err(binding_query_error)?;

        let constraint = constraints
            .value()
            .constraints()
            .iter()
            .find(|constraint| constraint.ordinal() == ordinal);

        match constraint.map(|constraint| constraint.kind()) {
            Some(bray_symbols::CheckedConstraintKind::TraitSatisfaction {
                subject,
                application,
            }) => Ok(ImplementationRequirementKey::new(subject, application)),
            _ => Err(SemanticQueryFailure::contract(
                SemanticQueryContext::Symbol(owner.symbol()),
                SemanticQueryViolation::Missing(SemanticDataKind::GenericConstraint),
            )
            .into()),
        }
    }

    pub(super) fn select_callable_evidence(
        &self,
        target: ResolvedCallableEvidenceTarget,
        context: ProofContext,
        cancellation: &CancellationToken,
        proof_diagnostics: &mut bray_diagnostics::DiagnosticBag,
    ) -> Result<EvidenceSelection, FactQueryError> {
        let values = self.semantic_value_store()?;
        let callable = values.callable_instance_data(target.callable())?;

        let callable = substitute_callable_context(
            &values,
            *callable,
            context.substitution,
            context.contextual_self,
        )?;

        let substitution = callable.substitution();

        let Some(requirement) = target.dispatch() else {
            return Ok(EvidenceSelection::Direct(
                callable,
                ProofContext {
                    substitution: Some(substitution),
                    contextual_self: None,
                    allow_open_requirements: context.allow_open_requirements,
                },
            ));
        };

        let mut subject = requirement.subject();
        let mut application = requirement.trait_application();

        if let Some(parent) = context.substitution {
            subject = values.substitute_type(subject, parent)?;
            application = values.substitute_trait_application(application, parent)?;
        }

        subject = substitute_contextual_self(&values, subject, context.contextual_self)?;

        application = substitute_contextual_self_in_application(
            &values,
            application,
            context.contextual_self,
        )?;

        let requirement = ImplementationRequirementKey::new(subject, application);
        let binding = self.binding_context(cancellation)?;
        let checker = crate::compilation::checker::CompilationCheckerContext::new(binding);
        let mut diagnostics = bray_diagnostics::DiagnosticBag::new();

        let requirement = bray_checker::normalize_implementation_requirement(
            &checker,
            requirement,
            &mut diagnostics,
        )?;

        *proof_diagnostics = proof_diagnostics.merged(&diagnostics);

        if diagnostics.has_errors() {
            return Ok(EvidenceSelection::Unavailable);
        }

        let subject = requirement.subject();
        let application = requirement.trait_application();

        let Some(member) =
            bray_symbols::TraitCallableMemberSymbolId::try_from_any(callable.definition().symbol())
        else {
            return Ok(EvidenceSelection::Unavailable);
        };

        let application_data = values.trait_application_data(application)?;

        if binding
            .containing_symbol(callable.definition().symbol())
            .map_err(binding_query_error)?
            != Some(application_data.definition().into())
        {
            return Ok(EvidenceSelection::Unavailable);
        }

        let selection =
            self.implementation_selection_result_with_cancellation(requirement, cancellation)?;

        *proof_diagnostics = proof_diagnostics.merged(selection.diagnostics());

        if selection.diagnostics().has_errors()
            || matches!(
                selection.value(),
                bray_symbols::ImplementationSelection::Ambiguous(_)
            )
        {
            return Ok(EvidenceSelection::Unavailable);
        }

        let bray_symbols::ImplementationSelection::Selected(witness) = selection.value() else {
            return Ok(
                if !context.allow_open_requirements
                    || values.implementation_requirement_is_concrete(requirement)?
                {
                    EvidenceSelection::Unavailable
                } else {
                    EvidenceSelection::Open
                },
            );
        };

        let implementation = values.implementation_instance_data(*witness)?;

        let conformance = self.trait_implementation_conformance_with_cancellation(
            implementation.definition(),
            cancellation,
        )?;

        *proof_diagnostics = proof_diagnostics.merged(conformance.diagnostics());

        if conformance.diagnostics().has_errors() || !conformance.value().is_valid() {
            return Ok(EvidenceSelection::Unavailable);
        }

        let fulfillments = crate::compilation::implementation::implementation_fulfillments(
            &binding,
            implementation.definition(),
        )?;

        let application = application_data;

        let selected = crate::compilation::implementation::implementation_callable_instance(
            &binding,
            fulfillments.callables,
            member,
            application.substitution(),
            implementation.substitution(),
        )?;

        let Some(selected) = selected else {
            return Ok(EvidenceSelection::Unavailable);
        };

        let selected = selected.with_call_arguments(&binding, callable)?;

        let contextual_self = selected
            .uses_trait_default()
            .then_some((SelfTypeContext::Trait(application.definition()), subject));

        let instance = selected.instance();

        Ok(EvidenceSelection::Fulfillment(
            instance,
            ProofContext {
                substitution: Some(instance.substitution()),
                contextual_self,
                allow_open_requirements: context.allow_open_requirements,
            },
        ))
    }
}
