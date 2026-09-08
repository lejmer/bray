use std::collections::BTreeMap;

use bray_binder::BindingQueryContext;
use bray_symbols::{
    CallableConditions, CallableContractSet, GenericOwnerId, GenericSubstitutionId,
    PredicateDefinitionSymbolId, PredicateInstanceData, SelfTypeContext, SemanticValueStore,
    TraitApplicationId, TraitMemberFulfillmentId, TraitMemberRequirementId, TypeId,
};

use super::mismatch::CallableContractMismatch;
use crate::compilation::binder::CompilationBindingContext;
use crate::fact::FactQueryError;

pub(super) fn condition_contract_mismatch(
    values: &SemanticValueStore,
    subject: TypeId,
    context: &CompilationBindingContext<'_>,
    application: TraitApplicationId,
    fulfillment_context: Option<SelfTypeContext>,
    substitution: Option<GenericSubstitutionId>,
    matched_fulfillments: &BTreeMap<TraitMemberRequirementId, TraitMemberFulfillmentId>,
    required: &CallableContractSet,
    provided: &CallableContractSet,
) -> Result<Option<CallableContractMismatch>, FactQueryError> {
    let application = values.trait_application_data(application)?;
    let trait_substitution = application.substitution();
    let requirement_context = SelfTypeContext::Trait(application.definition());
    let compilation = context.compilation();
    let cancellation = context.cancellation();

    let normalize_self = |mut condition| -> Result<_, FactQueryError> {
        for context in [Some(requirement_context), fulfillment_context]
            .into_iter()
            .flatten()
        {
            condition =
                values.substitute_contextual_self_in_constant_term(condition, context, subject)?;
        }

        Ok(condition)
    };

    let expand = |condition| -> Result<_, FactQueryError> {
        let condition = normalize_self(condition)?;

        let condition =
            compilation.expanded_predicate_condition(condition, cancellation, |predicate| {
                selected_predicate(context, matched_fulfillments, predicate)
            })?;

        condition.map(normalize_self).transpose()
    };

    let mismatch = bray_checker::check_callable_condition_implication(
        values,
        required.conditions(),
        provided.conditions(),
        |condition| {
            let condition = values.substitute_constant_term(condition, trait_substitution)?;
            let condition = normalize_self(condition)?;

            let condition = match substitution {
                Some(substitution) => values.substitute_constant_term(condition, substitution)?,
                None => condition,
            };

            expand(condition)
        },
        expand,
    )?;

    Ok(mismatch.map(CallableContractMismatch::ConditionImplication))
}

fn selected_predicate(
    context: &CompilationBindingContext<'_>,
    fulfillments: &BTreeMap<TraitMemberRequirementId, TraitMemberFulfillmentId>,
    predicate: PredicateInstanceData,
) -> Result<Option<PredicateInstanceData>, FactQueryError> {
    let PredicateDefinitionSymbolId::TraitMember(requirement) = predicate.definition() else {
        return Ok(Some(predicate));
    };

    let Some(TraitMemberFulfillmentId::Predicate(fulfillment)) =
        fulfillments.get(&TraitMemberRequirementId::Predicate(requirement))
    else {
        return Ok(Some(predicate));
    };

    let values = context.semantic_values();
    let source = values.generic_substitution_data(predicate.substitution())?;
    let definition = PredicateDefinitionSymbolId::TraitFulfillment(*fulfillment);

    let Some(owner) = GenericOwnerId::try_new(definition.into_any()) else {
        return Ok(None);
    };

    let substitution = values.intern_generic_substitution(source.with_owner(owner))?;

    Ok(Some(PredicateInstanceData::new(definition, substitution)))
}
