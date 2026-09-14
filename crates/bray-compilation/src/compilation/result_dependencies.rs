use bray_binder::{BindingQueryContext, SymbolQueryProvider};
use bray_checker::CheckerInfrastructureError;
use bray_symbols::{
    ExactSymbolId, ImplementationRequirementKey, ImplementationSelection, SymbolQueryRequest,
};

use super::binder::CompilationBindingContext;
use super::checker::{checker_binder_error, checker_query_error};
use super::implementation::{implementation_callable_instance, implementation_fulfillments};
use crate::fact::FactQueryError;

type CheckerQueryResult<T> = bray_checker::CheckerQueryResult<T, FactQueryError>;

pub(super) fn result_dispatch_requirement(
    binding_context: &CompilationBindingContext<'_>,
    dispatch: bray_symbols::TraitConstraintDispatch,
) -> CheckerQueryResult<ImplementationRequirementKey> {
    match dispatch {
        bray_symbols::TraitConstraintDispatch::TraitDefault(requirement) => Ok(requirement),
        bray_symbols::TraitConstraintDispatch::Constraint { owner, ordinal } => {
            let constraints = binding_context
                .resolve_symbol_query(
                    SymbolQueryRequest::<bray_symbols::GenericConstraintsQuery>::new(owner),
                )
                .map_err(checker_binder_error)?;

            constraints
                .value()
                .constraints()
                .iter()
                .find_map(|constraint| match constraint.kind() {
                    bray_symbols::CheckedConstraintKind::TraitSatisfaction {
                        subject,
                        application,
                    } if constraint.ordinal() == ordinal => {
                        Some(ImplementationRequirementKey::new(subject, application))
                    }
                    _ => None,
                })
                .ok_or_else(|| CheckerInfrastructureError::InvalidSemanticSelectionInput.into())
        }
    }
}

pub(super) fn result_witness_callable(
    binding_context: &CompilationBindingContext<'_>,
    callable: bray_symbols::CallableInstanceId,
    requirement: ImplementationRequirementKey,
) -> CheckerQueryResult<
    Option<(
        bray_symbols::CallableInstanceData,
        bray_symbols::SelfTypeContext,
    )>,
> {
    let values = binding_context.semantic_values();

    let selected = binding_context
        .compilation()
        .implementation_selection_result_with_cancellation(
            requirement,
            binding_context.cancellation(),
        )
        .map_err(checker_query_error)?;

    let ImplementationSelection::Selected(witness) = selected.value() else {
        return Ok(None);
    };

    let instance = values
        .implementation_instance_data(*witness)
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    let abstract_callable = values
        .callable_instance_data(callable)
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    let member = bray_symbols::TraitCallableMemberSymbolId::try_from_any(
        abstract_callable.definition().symbol(),
    )
    .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

    let fulfillments = implementation_fulfillments(binding_context, instance.definition())
        .map_err(checker_query_error)?;

    let application = values
        .trait_application_data(requirement.trait_application())
        .map_err(CheckerInfrastructureError::SemanticValueStore)?;

    let selected = implementation_callable_instance(
        binding_context,
        fulfillments.callables,
        member,
        application.substitution(),
        instance.substitution(),
    )
    .map_err(checker_query_error)?;

    selected
        .map(|selected| {
            let context = if selected.uses_trait_default() {
                bray_symbols::SelfTypeContext::Trait(application.definition())
            } else {
                bray_symbols::SelfTypeContext::Implementation(instance.definition())
            };

            super::implementation::instantiate_implementation_member(
                binding_context,
                selected.instance(),
                *abstract_callable,
            )
            .map(|instance| (instance, context))
            .map_err(checker_query_error)
        })
        .transpose()
}
