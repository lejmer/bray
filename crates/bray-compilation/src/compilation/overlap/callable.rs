use std::collections::BTreeMap;

use bray_symbols::{
    CallableParameterName, CallablePosition, CallableSignatureTemplate, GenericParameterSymbolId,
    SemanticValueStore, TypeData, TypeExpressionTemplate,
};

use super::semantic::SemanticUnifier;

pub(in crate::compilation) fn callable_selection_surfaces_overlap(
    left: &CallableSignatureTemplate,
    left_generic_parameters: &[GenericParameterSymbolId],
    right: &CallableSignatureTemplate,
    right_generic_parameters: &[GenericParameterSymbolId],
    values: &SemanticValueStore,
) -> bool {
    let Some(left_parameters) = selection_parameters(left, values) else {
        return false;
    };

    let Some(right_parameters) = selection_parameters(right, values) else {
        return false;
    };

    match (left.receiver(), right.receiver()) {
        (None, None) => {}
        (Some(left_receiver), Some(right_receiver)) => {
            let mut unifier =
                SemanticUnifier::new(left_generic_parameters, right_generic_parameters, values);

            if !unifier.types_may_overlap(left_receiver.ty(), right_receiver.ty()) {
                return false;
            }
        }
        (None, Some(_)) | (Some(_), None) => return false,
    }

    let left_positional = positional_prefix_len(&left_parameters);
    let right_positional = positional_prefix_len(&right_parameters);
    let max_positional = left_positional.min(right_positional);

    for positional_count in 0..=max_positional {
        let mut unifier =
            SemanticUnifier::new(left_generic_parameters, right_generic_parameters, values);

        if call_shape_may_overlap(
            &left_parameters,
            &right_parameters,
            positional_count,
            &mut unifier,
        ) {
            return true;
        }
    }

    false
}

#[derive(Clone)]
struct SelectionParameter {
    name: CallableParameterName,
    position: CallablePosition,
    ty: TypeExpressionTemplate,
}

fn selection_parameters(
    signature: &CallableSignatureTemplate,
    values: &SemanticValueStore,
) -> Option<Vec<SelectionParameter>> {
    // Selection parameters own shallow Arc-backed templates beyond the borrowed signature match.
    let parameters = match signature.callable_type() {
        TypeExpressionTemplate::Callable(callable) => callable
            .parameters()
            .iter()
            .map(|parameter| SelectionParameter {
                name: parameter.name().clone(),
                position: parameter.position(),
                ty: parameter.ty().clone(),
            })
            .collect(),
        TypeExpressionTemplate::Resolved(ty) => {
            let ty = values.type_data(*ty);

            let TypeData::Callable(callable) = ty.as_ref() else {
                return None;
            };

            callable
                .parameters()
                .iter()
                .map(|parameter| SelectionParameter {
                    name: parameter.name().clone(),
                    position: parameter.position(),
                    ty: TypeExpressionTemplate::Resolved(parameter.ty()),
                })
                .collect()
        }
        _ => return None,
    };

    Some(parameters)
}

fn positional_prefix_len(parameters: &[SelectionParameter]) -> usize {
    parameters
        .iter()
        .take_while(|parameter| parameter.position == CallablePosition::PositionalOrNamed)
        .count()
}

fn call_shape_may_overlap(
    left: &[SelectionParameter],
    right: &[SelectionParameter],
    positional_count: usize,
    unifier: &mut SemanticUnifier<'_>,
) -> bool {
    if left.len() != right.len() {
        return false;
    }

    for (left, right) in left
        .iter()
        .take(positional_count)
        .zip(right.iter().take(positional_count))
    {
        if !unifier.type_templates_may_overlap(&left.ty, &right.ty) {
            return false;
        }
    }

    let left_named = left[positional_count..]
        .iter()
        .map(|parameter| (&parameter.name, &parameter.ty))
        .collect::<BTreeMap<_, _>>();

    let right_named = right[positional_count..]
        .iter()
        .map(|parameter| (&parameter.name, &parameter.ty))
        .collect::<BTreeMap<_, _>>();

    if left_named.len() != left.len() - positional_count
        || right_named.len() != right.len() - positional_count
        || left_named.keys().ne(right_named.keys())
    {
        return false;
    }

    for ((_, left), (_, right)) in left_named.iter().zip(&right_named) {
        if !unifier.type_templates_may_overlap(left, right) {
            return false;
        }
    }

    true
}

#[cfg(test)]
mod tests {
    use bray_symbols::{
        CallableSignatureTemplate, SemanticValueStore, TypeData, TypeExpressionTemplate,
    };

    use super::callable_selection_surfaces_overlap;

    #[test]
    fn callable_overlap_preserves_foreign_semantic_value_identity() {
        let source = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("source semantic store should build: {error:?}"));

        let target = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("target semantic store should build: {error:?}"));

        let foreign = source
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("source type should intern: {error:?}"));

        let signature = CallableSignatureTemplate::new(
            TypeExpressionTemplate::Resolved(foreign),
            None,
            [],
            TypeExpressionTemplate::Resolved(foreign),
        );

        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                callable_selection_surfaces_overlap(&signature, &[], &signature, &[], &target)
            }))
            .is_err()
        );
    }
}
