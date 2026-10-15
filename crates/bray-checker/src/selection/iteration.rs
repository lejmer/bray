use bray_bound_tree::SelectedIterationSource;

use super::{
    CandidateSelection, IterationSourceSelectionRequest, SelectionCandidateSignature,
    SelectionFailure, SelectionFailureCandidate,
};

pub(super) fn select(
    input: &IterationSourceSelectionRequest,
) -> CandidateSelection<SelectedIterationSource> {
    if input.candidates().iter().any(|candidate| {
        let selection = candidate.selection();

        selection.expression() != input.expression()
            || selection.source() != input.source()
            || selection.mode() != input.mode()
    }) {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in select"
        );
    }

    match input.candidates() {
        [] => CandidateSelection::Failed(SelectionFailure::Unavailable),
        [candidate] => CandidateSelection::Selected(candidate.selection().clone()),
        candidates => CandidateSelection::Failed(SelectionFailure::Ambiguous(
            candidates
                .iter()
                .map(|candidate| {
                    let selection = candidate.selection();

                    SelectionFailureCandidate::new(
                        candidate.key().clone(),
                        SelectionCandidateSignature::Iteration {
                            source_type: selection.source_type(),
                            cursor_type: selection.cursor_type(),
                            element_type: selection.element_type(),
                        },
                    )
                })
                .collect(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{
        BoundExpressionId, BoundIterationSource, BoundUnitId, IterationSourceMode,
        SelectedIterationProtocolOperation, SelectedIterationSource, SelectedIterationTypes,
    };
    use bray_declarations::DeclarationId;
    use bray_symbols::testing::{
        implementation_instance, implementation_requirement, source_function_key,
    };
    use bray_symbols::{
        SemanticValueStore, SymbolId, SymbolKey, SymbolKind, TraitCallableMemberSymbolId,
        TraitSymbolId, TypeData,
    };

    use super::select;
    use crate::test_support::{
        callable_instance, expression_unit, literal_expression, push_expression,
    };
    use crate::{
        CandidateSelection, IterationSourceCandidate, IterationSourceSelectionRequest,
        SelectionFailure,
    };

    #[test]
    fn iteration_selection_is_unavailable_unique_or_stably_ambiguous() {
        let (expression, source) = expression_ids();

        let values = semantic_values();

        let unavailable = IterationSourceSelectionRequest::new(
            expression,
            source,
            IterationSourceMode::Shared,
            [],
        );

        assert_eq!(
            select(&unavailable),
            CandidateSelection::Failed(SelectionFailure::Unavailable)
        );

        let first = candidate(&values, expression, source, 1);
        let second = candidate(&values, expression, source, 2);

        let unique = IterationSourceSelectionRequest::new(
            expression,
            source,
            IterationSourceMode::Shared,
            [first.clone()],
        );

        assert_eq!(
            select(&unique),
            CandidateSelection::Selected(first.selection().clone())
        );

        let ambiguous = IterationSourceSelectionRequest::new(
            expression,
            source,
            IterationSourceMode::Shared,
            [second, first],
        );

        let CandidateSelection::Failed(SelectionFailure::Ambiguous(keys)) = select(&ambiguous)
        else {
            panic!("two distinct protocol pairs must remain ambiguous");
        };

        assert_eq!(keys.len(), 2);
        assert!(keys[0].key() < keys[1].key());
    }

    #[test]
    #[should_panic(expected = "Semantic-selection inputs")]
    fn iteration_selection_rejects_candidates_for_another_source_occurrence() {
        let (expression, source) = expression_ids();

        let values = semantic_values();
        let candidate = candidate(&values, expression, expression, 1);

        let request = IterationSourceSelectionRequest::new(
            expression,
            source,
            IterationSourceMode::Shared,
            [candidate],
        );

        select(&request);
    }

    fn expression_ids() -> (BoundExpressionId, BoundExpressionId) {
        let (_, expressions) = expression_unit(BoundUnitId::new(107), |tree, origin| {
            vec![
                push_expression(
                    tree,
                    literal_expression(origin, bray_bound_tree::BoundLiteralKind::Integer, None),
                ),
                push_expression(
                    tree,
                    literal_expression(origin, bray_bound_tree::BoundLiteralKind::Integer, None),
                ),
            ]
        });

        (expressions[0], expressions[1])
    }

    fn semantic_values() -> SemanticValueStore {
        match SemanticValueStore::try_new() {
            Ok(values) => values,
            Err(error) => panic!("test semantic values must be available: {error:?}"),
        }
    }

    fn candidate(
        values: &SemanticValueStore,
        expression: BoundExpressionId,
        source: BoundExpressionId,
        identity: u32,
    ) -> IterationSourceCandidate {
        let ty = match values.intern_type(TypeData::Error) {
            Ok(ty) => ty,
            Err(error) => panic!("test type must be internable: {error:?}"),
        };

        let trait_definition = TraitSymbolId::from_symbol_id(SymbolId::new(20));
        let requirement = implementation_requirement(values, trait_definition, ty, ty);
        let witness = implementation_instance(values, identity);
        let member = TraitCallableMemberSymbolId::from_symbol_id(SymbolId::new(30));

        let fulfillment =
            bray_symbols::TraitCallableFulfillmentSymbolId::from_symbol_id(SymbolId::new(31));

        let selection = SelectedIterationSource::new(
            expression,
            BoundIterationSource::new(source, IterationSourceMode::Shared),
            SelectedIterationTypes::new(ty, ty, ty),
            SelectedIterationProtocolOperation::new(
                requirement,
                witness,
                callable_instance(member.into()),
                callable_instance(fulfillment.into()),
            ),
            SelectedIterationProtocolOperation::new(
                requirement,
                witness,
                callable_instance(member.into()),
                callable_instance(fulfillment.into()),
            ),
        );

        let Some(key) = SymbolKey::source_declaration(
            source_function_key(),
            SymbolKind::NamedTraitImplementation,
            DeclarationId::new(identity),
        ) else {
            panic!("implementation keys must be source-declared");
        };

        IterationSourceCandidate::new(key.clone(), key, selection)
    }
}
