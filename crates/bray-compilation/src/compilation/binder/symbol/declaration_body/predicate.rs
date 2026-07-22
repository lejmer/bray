use std::collections::BTreeMap;

use bray_binder::{BinderFactContext, BinderFactError, BinderFactResult, SymbolFactProvider};
use bray_bound_tree::{BoundUnitKey, CheckedTemplateKind};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::{
    Diagnostic, DiagnosticBag, DiagnosticId, DiagnosticKind, DiagnosticResult, SeverityKind,
};
use bray_package_interface::InterfacePredicateDefinitionState;
use bray_source::SourceSpan;
use bray_symbols::{
    ErrorPredicateDefinition, PredicateDefinition, PredicateDefinitionFact,
    PredicateDefinitionState, PredicateDefinitionSymbolId, PredicateSemanticSummary,
    PredicateSignatureTemplateFact, PredicateSymbolId, SymbolFactRequest, SymbolFactResult,
    SymbolOrigin, TraitPredicateFulfillmentDefinitionFact, TraitPredicateFulfillmentSymbolId,
    TraitPredicateMemberDefinitionFact, TraitPredicateMemberSymbolId,
};

use super::super::binding::CompilationSymbolFactBinding;
use super::super::cache::CompilationSymbolFacts;
use super::super::imported::{imported_declaration_template, imported_predicate_definition_state};
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
        return imported_predicate_definition(context, address);
    }

    if predicate_origin(context, owner)? != SymbolOrigin::Source {
        return Ok(DiagnosticResult::without_diagnostics(
            PredicateDefinitionState::OpaqueTrusted,
        ));
    }

    let signature = context.symbol_fact(
        SymbolFactRequest::<PredicateSignatureTemplateFact>::new(owner),
    )?;

    let key = predicate_definition_key(context, owner)?;

    if signature.value().is_trusted()
        && let Some(key) = key.as_ref()
    {
        let diagnostic = predicate_diagnostic(
            key.source().syntax(),
            DiagnosticKind::BindingTrustedPredicateBodyNotAllowed,
        );

        let diagnostics = signature
            .diagnostics()
            .merged(&DiagnosticBag::single(diagnostic));

        return Ok(DiagnosticResult::new(
            PredicateDefinitionState::Error(ErrorPredicateDefinition),
            diagnostics,
        ));
    }

    let Some(key) = key else {
        let state = missing_predicate_state(owner, signature.value().is_trusted());
        let diagnostics =
            match missing_predicate_diagnostic_kind(owner, signature.value().is_trusted()) {
                Some(kind) => {
                    let anchor = predicate_declaration_anchor(context, owner)?;
                    let diagnostic = predicate_diagnostic(anchor, kind);

                    signature
                        .diagnostics()
                        .merged(&DiagnosticBag::single(diagnostic))
                }
                None => signature.diagnostics().clone(),
            };

        return Ok(DiagnosticResult::new(state, diagnostics));
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

fn imported_predicate_definition(
    context: &CompilationBinderFacts<'_>,
    address: bray_symbols::ImportedSymbolFactAddress,
) -> BinderFactResult<DiagnosticResult<PredicateDefinitionState<PredicateDefinition>>> {
    let state = imported_predicate_definition_state(context, address)?;

    // Each typed result retains the imported fact's shallow immutable diagnostics.
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
                return Err(BinderFactError::DependencyUnavailable);
            };

            let diagnostics = state.diagnostics().merged(template_result.diagnostics());
            let definition = PredicateDefinition::new(PredicateSemanticSummary::new(
                template.template().behavior().dependency_contract(),
            ));

            Ok(DiagnosticResult::new(
                PredicateDefinitionState::Defined(definition),
                diagnostics,
            ))
        }
    }
}

fn missing_predicate_diagnostic_kind(
    owner: PredicateDefinitionSymbolId,
    is_trusted: bool,
) -> Option<DiagnosticKind> {
    match owner {
        PredicateDefinitionSymbolId::Predicate(_) if is_trusted => None,
        PredicateDefinitionSymbolId::TraitMember(_) => None,
        PredicateDefinitionSymbolId::Predicate(_)
        | PredicateDefinitionSymbolId::TraitFulfillment(_) => {
            Some(DiagnosticKind::BindingPredicateBodyRequired)
        }
    }
}

fn predicate_declaration_anchor(
    context: &CompilationBinderFacts<'_>,
    owner: PredicateDefinitionSymbolId,
) -> BinderFactResult<SyntaxAnchor> {
    context
        .symbols()
        .symbol_key(owner.into_any())
        .and_then(bray_symbols::SymbolKey::source_declaration_id)
        .and_then(|declaration| context.declarations().declaration(declaration))
        .map(bray_declarations::DeclarationRecord::syntax_anchor)
        .ok_or(BinderFactError::DependencyUnavailable)
}

fn predicate_diagnostic(anchor: SyntaxAnchor, kind: DiagnosticKind) -> Diagnostic {
    let span = SourceSpan::new(anchor.source_id(), anchor.full_range());

    Diagnostic::new(
        DiagnosticId::new(span.range().start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(span)
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

    use bray_diagnostics::DiagnosticKind;
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

    #[test]
    fn invalid_predicate_tails_publish_structured_diagnostics() {
        let compilation = compilation(concat!(
            "module app;\n",
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
        assert_eq!(
            missing
                .diagnostics()
                .iter()
                .map(bray_diagnostics::Diagnostic::kind)
                .collect::<Vec<_>>(),
            [DiagnosticKind::BindingPredicateBodyRequired]
        );
        assert_eq!(
            defined
                .diagnostics()
                .iter()
                .map(bray_diagnostics::Diagnostic::kind)
                .collect::<Vec<_>>(),
            [DiagnosticKind::BindingTrustedPredicateBodyNotAllowed]
        );
    }
}
