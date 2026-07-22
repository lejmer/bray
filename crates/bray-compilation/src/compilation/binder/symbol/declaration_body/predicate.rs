use std::collections::BTreeMap;

use bray_binder::{BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider};
use bray_bound_tree::{BoundUnitKey, CheckedTemplateKind};
use bray_diagnostics::DiagnosticResult;
use bray_symbols::{
    ErrorPredicateDefinition, PredicateDefinition, PredicateDefinitionFact,
    PredicateDefinitionState, PredicateDefinitionSymbolId, PredicateSemanticSummary,
    PredicateSignatureTemplateFact, PredicateSymbolId, SymbolFactRequest, SymbolFactResult,
    SymbolOrigin, TraitPredicateFulfillmentDefinitionFact, TraitPredicateFulfillmentSymbolId,
    TraitPredicateMemberDefinitionFact, TraitPredicateMemberSymbolId,
};

use super::super::binding::CompilationSymbolFactBinding;
use super::super::cache::CompilationSymbolFacts;
use super::super::imported::imported_declaration_template;
use super::shared::{checked_source_expression, empty_dependency_contract, syntax_diagnostics};
use crate::compilation::binder::CompilationBinderFacts;
use crate::fact::{CompilationFactKey, SymbolFactCache};

impl_declaration_body_fact!(
    PredicateDefinitionFact,
    predicate_definitions,
    bind_predicate_definition
);
impl_declaration_body_fact!(
    TraitPredicateMemberDefinitionFact,
    trait_predicate_member_definitions,
    bind_trait_predicate_member_definition
);
impl_declaration_body_fact!(
    TraitPredicateFulfillmentDefinitionFact,
    trait_predicate_fulfillment_definitions,
    bind_trait_predicate_fulfillment_definition
);

fn bind_predicate_definition(
    context: &CompilationBinderFacts<'_>,
    owner: PredicateSymbolId,
) -> BinderFactResult<DiagnosticResult<PredicateDefinitionState<PredicateDefinition>>> {
    predicate_definition(context, owner.into())
}

fn bind_trait_predicate_member_definition(
    context: &CompilationBinderFacts<'_>,
    owner: TraitPredicateMemberSymbolId,
) -> BinderFactResult<DiagnosticResult<PredicateDefinitionState<PredicateDefinition>>> {
    predicate_definition(context, owner.into())
}

fn bind_trait_predicate_fulfillment_definition(
    context: &CompilationBinderFacts<'_>,
    owner: TraitPredicateFulfillmentSymbolId,
) -> BinderFactResult<DiagnosticResult<PredicateDefinitionState<PredicateDefinition>>> {
    predicate_definition(context, owner.into())
}

fn predicate_definition(
    context: &CompilationBinderFacts<'_>,
    owner: PredicateDefinitionSymbolId,
) -> BinderFactResult<DiagnosticResult<PredicateDefinitionState<PredicateDefinition>>> {
    if let Some(address) = context.imported_fact_address(owner.into_any())? {
        let imported = imported_declaration_template(
            context,
            address,
            CheckedTemplateKind::PredicateDefinition,
        )?;

        let diagnostics = imported.diagnostics().clone();

        let state = match imported.value() {
            Some(template) => PredicateDefinitionState::Defined(PredicateDefinition::new(
                PredicateSemanticSummary::new(template.template().behavior().dependency_contract()),
            )),
            None => missing_predicate_state(owner, true),
        };

        return Ok(DiagnosticResult::new(state, diagnostics));
    }

    if predicate_origin(context, owner)? != SymbolOrigin::Source {
        return Ok(DiagnosticResult::without_diagnostics(
            PredicateDefinitionState::OpaqueTrusted,
        ));
    }

    let signature = context.symbol_fact(
        SymbolFactRequest::<PredicateSignatureTemplateFact>::new(owner),
    )?;

    let Some(key) = predicate_definition_key(context, owner)? else {
        let state = missing_predicate_state(owner, signature.value().is_trusted());

        return Ok(DiagnosticResult::new(
            state,
            signature.diagnostics().clone(),
        ));
    };

    let parser_diagnostics = syntax_diagnostics(context, key.source().syntax());
    let checked = checked_source_expression(context, key)?;
    let diagnostics = bray_diagnostics::DiagnosticBag::merged_all([
        signature.diagnostics(),
        &checked.diagnostics,
        &parser_diagnostics,
    ]);

    let state = if checked.is_recovered || diagnostics.has_errors() {
        PredicateDefinitionState::Error(ErrorPredicateDefinition)
    } else {
        // TODO(BRA-224): Publish the predicate body's propagated dependency contract.
        PredicateDefinitionState::Defined(PredicateDefinition::new(PredicateSemanticSummary::new(
            empty_dependency_contract(context)?,
        )))
    };

    Ok(DiagnosticResult::new(state, diagnostics))
}

fn missing_predicate_state(
    owner: PredicateDefinitionSymbolId,
    is_trusted: bool,
) -> PredicateDefinitionState<PredicateDefinition> {
    match owner {
        PredicateDefinitionSymbolId::TraitMember(_) => PredicateDefinitionState::Required,
        PredicateDefinitionSymbolId::Predicate(_) if is_trusted => {
            PredicateDefinitionState::OpaqueTrusted
        }
        PredicateDefinitionSymbolId::Predicate(_)
        | PredicateDefinitionSymbolId::TraitFulfillment(_) => {
            PredicateDefinitionState::Error(ErrorPredicateDefinition)
        }
    }
}

fn predicate_definition_key(
    context: &CompilationBinderFacts<'_>,
    owner: PredicateDefinitionSymbolId,
) -> BinderFactResult<Option<BoundUnitKey>> {
    let compilation = context.compilation();
    let result = compilation.fact(
        CompilationFactKey::PredicateDefinitionKeys,
        &compilation.state.predicate_definition_keys,
        || {
            let symbols = compilation.symbol_graph()?;
            let mut definitions = BTreeMap::new();

            for key in compilation.declared_unit_keys()? {
                if key.kind() != bray_bound_tree::BoundUnitKind::PredicateDefinition {
                    continue;
                }

                let symbol = symbols
                    .symbol_for_key(key.declared_owner())
                    .and_then(PredicateDefinitionSymbolId::try_from_any)
                    .ok_or(crate::fact::FactQueryError::InfrastructureFailure)?;

                if definitions.insert(symbol, key).is_some() {
                    return Err(crate::fact::FactQueryError::InfrastructureFailure);
                }
            }

            Ok(definitions)
        },
    );

    match result {
        Ok(definitions) => Ok(definitions.get(&owner).cloned()),
        Err(error) => Err(super::super::binding::binder_error(error.clone())),
    }
}

fn predicate_origin(
    context: &CompilationBinderFacts<'_>,
    owner: PredicateDefinitionSymbolId,
) -> BinderFactResult<SymbolOrigin> {
    match owner {
        PredicateDefinitionSymbolId::Predicate(owner) => context
            .symbols()
            .predicate(owner)
            .map(|record| record.origin()),
        PredicateDefinitionSymbolId::TraitMember(owner) => context
            .symbols()
            .trait_predicate_member(owner)
            .map(|record| record.origin()),
        PredicateDefinitionSymbolId::TraitFulfillment(owner) => context
            .symbols()
            .trait_predicate_fulfillment(owner)
            .map(|record| record.origin()),
    }
    .ok_or(BinderFactError::DependencyUnavailable)
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_symbols::{PredicateDefinitionState, PredicateDefinitionSymbolId, SymbolOrigin};

    use crate::test_support::compilation;

    #[test]
    fn predicate_definitions_preserve_defined_required_and_opaque_states() {
        let compilation = compilation(concat!(
            "module app;\n",
            "predicate valid(value: bool) = value;\n",
            "trusted predicate opaque();\n",
            "trait Contract\n",
            "{\n",
            "    predicate required();\n",
            "    predicate provided(value: bool) = value;\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must build: {error:?}"));

        let predicates = symbols
            .predicates()
            .iter()
            .filter(|predicate| predicate.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [defined, opaque] = predicates.as_slice() else {
            panic!("test source must declare two predicates: {predicates:?}");
        };

        let defined_id = PredicateDefinitionSymbolId::Predicate(defined.id());

        let defined = compilation
            .predicate_definition(defined_id)
            .unwrap_or_else(|error| panic!("predicate definition must publish: {error:?}"));

        let repeated = compilation
            .predicate_definition(defined_id)
            .unwrap_or_else(|error| panic!("predicate definition must be reusable: {error:?}"));

        assert!(Arc::ptr_eq(&defined, &repeated));
        assert!(defined.diagnostics().is_empty());
        assert!(matches!(
            defined.value(),
            PredicateDefinitionState::Defined(_)
        ));

        let opaque = compilation
            .predicate_definition(PredicateDefinitionSymbolId::Predicate(opaque.id()))
            .unwrap_or_else(|error| panic!("opaque predicate state must publish: {error:?}"));

        assert!(matches!(
            opaque.value(),
            PredicateDefinitionState::OpaqueTrusted
        ));

        let members = symbols
            .trait_predicate_members()
            .iter()
            .filter(|member| member.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [required, provided] = members.as_slice() else {
            panic!("test trait must declare two predicate members: {members:?}");
        };

        let required = compilation
            .predicate_definition(PredicateDefinitionSymbolId::TraitMember(required.id()))
            .unwrap_or_else(|error| panic!("required predicate state must publish: {error:?}"));

        let provided = compilation
            .predicate_definition(PredicateDefinitionSymbolId::TraitMember(provided.id()))
            .unwrap_or_else(|error| panic!("provided predicate state must publish: {error:?}"));

        assert!(matches!(
            required.value(),
            PredicateDefinitionState::Required
        ));
        assert!(matches!(
            provided.value(),
            PredicateDefinitionState::Defined(_)
        ));
    }
}
