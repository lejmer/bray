use crate::compilation::binder::BindingQueryResult;

use bray_binder::{BindingError, BindingQueryContext, BindingQueryError, SymbolQueryProvider};
use bray_bound_tree::{BoundUnitKey, CheckedTemplateKind};
use bray_diagnostics::DiagnosticResult;
use bray_package_interface::InterfacePredicateDefinitionState;
use bray_symbols::{
    ErrorPredicateDefinition, PredicateDefinition, PredicateDefinitionQuery,
    PredicateDefinitionState, PredicateDefinitionSymbolId, PredicateSemanticSummary,
    PredicateSignatureTemplateQuery, PredicateSymbolId, SymbolOrigin, SymbolQueryRequest,
    TraitPredicateFulfillmentDefinitionQuery, TraitPredicateFulfillmentSymbolId,
    TraitPredicateMemberDefinitionQuery, TraitPredicateMemberSymbolId,
};

use super::super::binding::CompilationSymbolQueryEvaluator;
use super::super::cache::CompilationSymbolSemantics;
use super::super::imported::{
    imported_declaration_template, imported_predicate_definition_state, missing_imported_template,
};
use super::shared::{checked_source_expression, syntax_diagnostics};
use crate::compilation::binder::CompilationBindingContext;
use crate::fact::SymbolQueryCache;

impl_declaration_body_query!(
    PredicateDefinitionQuery,
    predicate_definitions,
    bind_predicate_definition
);
impl_declaration_body_query!(
    TraitPredicateMemberDefinitionQuery,
    trait_predicate_member_definitions,
    bind_trait_predicate_member_definition
);
impl_declaration_body_query!(
    TraitPredicateFulfillmentDefinitionQuery,
    trait_predicate_fulfillment_definitions,
    bind_trait_predicate_fulfillment_definition
);

fn bind_predicate_definition(
    context: &CompilationBindingContext<'_>,
    owner: PredicateSymbolId,
) -> BindingQueryResult<DiagnosticResult<PredicateDefinitionState<PredicateDefinition>>> {
    predicate_definition(context, owner.into())
}

fn bind_trait_predicate_member_definition(
    context: &CompilationBindingContext<'_>,
    owner: TraitPredicateMemberSymbolId,
) -> BindingQueryResult<DiagnosticResult<PredicateDefinitionState<PredicateDefinition>>> {
    predicate_definition(context, owner.into())
}

fn bind_trait_predicate_fulfillment_definition(
    context: &CompilationBindingContext<'_>,
    owner: TraitPredicateFulfillmentSymbolId,
) -> BindingQueryResult<DiagnosticResult<PredicateDefinitionState<PredicateDefinition>>> {
    predicate_definition(context, owner.into())
}

fn predicate_definition(
    context: &CompilationBindingContext<'_>,
    owner: PredicateDefinitionSymbolId,
) -> BindingQueryResult<DiagnosticResult<PredicateDefinitionState<PredicateDefinition>>> {
    if let Some(address) = context.imported_semantic_address(owner.into_any())? {
        return imported_predicate_definition(context, address);
    }

    if predicate_origin(context, owner)? != SymbolOrigin::Source {
        return Ok(DiagnosticResult::without_diagnostics(
            PredicateDefinitionState::OpaqueTrusted,
        ));
    }

    let signature = context.resolve_symbol_query(SymbolQueryRequest::<
        PredicateSignatureTemplateQuery,
    >::new(owner))?;

    let key = predicate_definition_key(context, owner)?;

    if signature.value().is_trusted() && key.is_some() {
        return Ok(DiagnosticResult::new(
            PredicateDefinitionState::Error(ErrorPredicateDefinition),
            signature.diagnostics().clone(),
        ));
    }

    let Some(key) = key else {
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
        PredicateDefinitionState::Defined(PredicateDefinition::new(
            PredicateSemanticSummary::new(checked.dependency_contract)
                .with_condition(checked.condition),
        ))
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

fn imported_predicate_definition(
    context: &CompilationBindingContext<'_>,
    address: bray_symbols::ImportedSemanticAddress,
) -> BindingQueryResult<DiagnosticResult<PredicateDefinitionState<PredicateDefinition>>> {
    let state = imported_predicate_definition_state(context, address)?;

    match state.value() {
        InterfacePredicateDefinitionState::Required => Ok(DiagnosticResult::new(
            PredicateDefinitionState::Required,
            state.diagnostics().clone(),
        )),
        InterfacePredicateDefinitionState::OpaqueTrusted => Ok(DiagnosticResult::new(
            PredicateDefinitionState::OpaqueTrusted,
            state.diagnostics().clone(),
        )),
        InterfacePredicateDefinitionState::Defined => {
            let template_result = imported_declaration_template(
                context,
                address,
                CheckedTemplateKind::PredicateDefinition,
            )?;

            let Some(template) = template_result.value() else {
                return Err(missing_imported_template(
                    address,
                    bray_package_interface::InterfaceSemanticRecordKind::DeclarationTemplate,
                ));
            };

            let diagnostics = state.diagnostics().merged(template_result.diagnostics());

            let definition = PredicateDefinition::new(
                PredicateSemanticSummary::new(template.template().behavior().dependency_contract())
                    .with_condition(
                        template
                            .template()
                            .result()
                            .raw()
                            .try_into()
                            .ok()
                            .and_then(|index: usize| template.template().nodes().get(index))
                            .and_then(|node| match node.operation() {
                                bray_bound_tree::CheckedTemplateOperation::Constant {
                                    term,
                                    ..
                                } => Some(*term),
                                _ => None,
                            }),
                    ),
            );

            Ok(DiagnosticResult::new(
                PredicateDefinitionState::Defined(definition),
                diagnostics,
            ))
        }
    }
}

pub(in crate::compilation) fn predicate_definition_key(
    context: &CompilationBindingContext<'_>,
    owner: PredicateDefinitionSymbolId,
) -> BindingQueryResult<Option<BoundUnitKey>> {
    context
        .compilation()
        .declared_unit_key(
            owner.into_any(),
            bray_bound_tree::BoundUnitKind::PredicateDefinition,
        )
        .map_err(super::super::binding::binder_error)
}

fn predicate_origin(
    context: &CompilationBindingContext<'_>,
    owner: PredicateDefinitionSymbolId,
) -> BindingQueryResult<SymbolOrigin> {
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
    .ok_or(BindingQueryError::Binding(
        BindingError::SymbolRecordUnavailable(owner.into_any()),
    ))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_diagnostics::DiagnosticKind;
    use bray_symbols::{
        DependencyRequirementKind, PredicateDefinitionState, PredicateDefinitionSymbolId,
        SymbolOrigin,
    };

    use super::super::super::test_support::assert_parameter_dependency_contract;
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

        let PredicateDefinitionState::Defined(definition) = defined.value() else {
            panic!("source predicate must publish a defined state");
        };

        assert_parameter_dependency_contract(
            &compilation,
            definition.semantic().dependency_contract(),
            0,
            &[
                DependencyRequirementKind::StorageAlive,
                DependencyRequirementKind::StorageInitialized,
            ],
        );

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

    #[test]
    fn predicate_definition_states_do_not_duplicate_declaration_diagnostics() {
        let compilation = compilation(concat!(
            "trusted module app;\n",
            "predicate missing();\n",
            "trusted predicate defined() = true;\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("symbol graph must build: {error:?}"));

        let predicates = symbols
            .predicates()
            .iter()
            .filter(|predicate| predicate.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [missing, defined] = predicates.as_slice() else {
            panic!("test source must declare two predicates: {predicates:?}");
        };

        let missing = compilation
            .predicate_definition(PredicateDefinitionSymbolId::Predicate(missing.id()))
            .unwrap_or_else(|error| panic!("missing predicate body must publish: {error:?}"));

        let defined = compilation
            .predicate_definition(PredicateDefinitionSymbolId::Predicate(defined.id()))
            .unwrap_or_else(|error| panic!("trusted predicate body must publish: {error:?}"));

        assert!(matches!(
            missing.value(),
            PredicateDefinitionState::Error(_)
        ));

        assert!(matches!(
            defined.value(),
            PredicateDefinitionState::Error(_)
        ));

        assert!(missing.diagnostics().is_empty());
        assert!(defined.diagnostics().is_empty());

        assert_eq!(
            compilation
                .declaration_diagnostics()
                .iter()
                .map(bray_diagnostics::Diagnostic::kind)
                .collect::<Vec<_>>(),
            [
                DiagnosticKind::DeclarationBodyRequired,
                DiagnosticKind::DeclarationBodyNotAllowed,
            ]
        );
    }
}
