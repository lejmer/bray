use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{
    BoundBlockItem, BoundExpression, BoundReferenceTarget, BoundStructuredExpressionKind,
    CheckedExpressionSemantics, SemanticSelection,
};
use bray_compiler_known::{ImplementationHook, RepresentationRole};
use bray_symbols::TypeData;

use crate::{CheckerRequestContext, CheckerUnitView};

pub(super) fn collect_generator_observations<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    semantics: &CheckedExpressionSemantics,
) -> bray_bound_tree::GeneratorObservations {
    let producers = request
        .unit()
        .tree()
        .expressions()
        .filter_map(|(id, expression)| {
            let BoundExpression::Structured(expression) = expression else {
                return None;
            };

            if expression.kind() != BoundStructuredExpressionKind::GeneralGenerator {
                return None;
            }

            let ty = semantics.types().expression(id)?;

            if ty.is_recovered() {
                return None;
            }

            let TypeData::Generator(element) = *request.semantic_values().type_data(ty.ty()) else {
                return None;
            };

            (request
                .available_compiler_known_symbols()
                .type_representation(request.semantic_values(), element)
                == Some(RepresentationRole::ScalarBool))
            .then_some(id)
        })
        .collect::<BTreeSet<_>>();

    if producers.is_empty() {
        return bray_bound_tree::GeneratorObservations::default();
    }

    let mut initializers = BTreeMap::new();
    let mut invalid_bindings = BTreeSet::new();

    for (_, block) in request.unit().tree().blocks() {
        for item in block.items() {
            if let BoundBlockItem::LocalBinding(binding) = item {
                for symbol in binding.bindings() {
                    initializers.insert(*symbol, binding.initializer());

                    if binding.bindings().len() != 1
                        || request
                            .view()
                            .pattern(binding.pattern())
                            .expect("checked binding retains its pattern")
                            .is_mutable()
                    {
                        invalid_bindings.insert(*symbol);
                    }
                }
            }
        }
    }

    let mut aliases = BTreeMap::new();

    for (id, _) in request.unit().tree().expressions() {
        let mut current = id;
        let mut seen = BTreeSet::new();

        while seen.insert(current) {
            if producers.contains(&current) {
                aliases.insert(id, current);
                break;
            }

            let Some(BoundExpression::Name(name)) = request.view().expression(current) else {
                break;
            };

            let BoundReferenceTarget::Local(bray_symbols::AnyLocalSymbolId::Binding(binding)) =
                name.target()
            else {
                break;
            };

            let Some(initializer) = initializers.get(&binding) else {
                break;
            };

            current = *initializer;
        }
    }

    let mut invalid = invalid_bindings
        .iter()
        .filter_map(|binding| {
            initializers
                .get(binding)
                .and_then(|id| aliases.get(id))
                .copied()
        })
        .collect::<BTreeSet<_>>();

    let mut folds = BTreeMap::new();

    for (id, expression) in request.unit().tree().expressions() {
        for child in expression.child_expressions() {
            let Some(producer) = aliases.get(&child) else {
                continue;
            };

            let valid = matches!(expression, BoundExpression::Structured(fold) if matches!(fold.kind(), BoundStructuredExpressionKind::BooleanAllFold | BoundStructuredExpressionKind::BooleanAnyFold))
                && matches!(semantics.selections().expression(id), Some(SemanticSelection::Iteration(selected)) if selected.source() == child
                        && request.available_compiler_known_symbols().symbol_implementation(selected.iterate().definition().symbol()) == Some(ImplementationHook::GeneratorSharedIterate)
                        && request.available_compiler_known_symbols().symbol_implementation(selected.next().definition().symbol()) == Some(ImplementationHook::GeneratorSharedNext));

            if valid {
                folds.insert(id, *producer);
            } else {
                invalid.insert(*producer);
            }
        }
    }

    aliases.retain(|_, producer| !invalid.contains(producer));
    folds.retain(|_, producer| !invalid.contains(producer));

    let bindings = initializers
        .into_iter()
        .filter_map(|(binding, initializer)| {
            aliases
                .get(&initializer)
                .map(|producer| (binding, *producer))
        })
        .collect();

    let ranges = producers
        .difference(&invalid)
        .map(|producer| {
            request
                .view()
                .expression(*producer)
                .expect("generator producer exists")
                .origin()
                .source_anchor()
                .syntax()
        })
        .collect::<Vec<_>>();

    let members = request
        .unit()
        .tree()
        .expressions()
        .filter_map(|(id, expression)| {
            let anchor = expression.origin().source_anchor().syntax();

            ranges
                .iter()
                .any(|range| {
                    range.source_id() == anchor.source_id()
                        && range.full_range().contains_range(anchor.full_range())
                })
                .then_some(id)
        })
        .collect();

    bray_bound_tree::GeneratorObservations::from_checked_parts(aliases, bindings, folds, members)
}
