use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, CheckedExpressionTypes, ConstructionTarget,
    SelectedConstructionInput,
};
use bray_symbols::CallablePosition;

use crate::type_check::numeric_literal_accepts_type;
use crate::{CheckerRequestContext, CheckerUnitView};

use super::super::{ConstructionInputSurface, SelectionConstructionInputRejection};

pub(super) struct MappedConstructionInputs {
    pub(super) values: Vec<SelectedConstructionInput>,
    pub(super) recovered: bool,
}

pub(super) enum ConstructionInputMapping {
    Mapped(MappedConstructionInputs),
    Rejected(SelectionConstructionInputRejection),
}

struct SourceConstructionInput<'name> {
    expression: BoundExpressionId,
    name: Option<&'name bray_symbols::SymbolName>,
    is_recovered: bool,
}

pub(super) fn map_construction_inputs<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    expression: BoundExpressionId,
    target: ConstructionTarget,
    surfaces: &[ConstructionInputSurface],
) -> ConstructionInputMapping
where
    C: CheckerRequestContext + ?Sized,
{
    let source = construction_inputs(request, expression);
    let mut surfaces = surfaces.iter().collect::<Vec<_>>();

    surfaces.sort_unstable_by_key(|surface| surface.ordinal());

    if !construction_surface_is_valid(target, &surfaces) {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in map_construction_inputs"
        );
    }

    let mut supplied = vec![false; surfaces.len()];
    let mut values = Vec::with_capacity(surfaces.len());
    let mut positional_index = 0;
    let mut saw_named = false;
    let mut recovered = false;

    for (source_ordinal, input) in source.into_iter().enumerate() {
        let source_ordinal = super::super::capacity::selection_ordinal_u64(source_ordinal);

        let surface_index = match input.name {
            Some(name) => {
                saw_named = true;

                let Some(index) = surfaces.iter().position(|surface| surface.name() == name) else {
                    return ConstructionInputMapping::Rejected(
                        SelectionConstructionInputRejection::UnknownName {
                            provided: name.as_str().to_owned(),
                            accepted: surfaces
                                .iter()
                                .map(|surface| surface.name().as_str().to_owned())
                                .collect(),
                        },
                    );
                };

                Some(index)
            }
            None if saw_named => {
                return ConstructionInputMapping::Rejected(
                    SelectionConstructionInputRejection::PositionalAfterNamed,
                );
            }
            None => {
                let index = positional_index;

                positional_index += 1;

                surfaces.get(index).and_then(|surface| {
                    (surface.position() == CallablePosition::PositionalOrNamed).then_some(index)
                })
            }
        };

        let Some(surface_index) = surface_index else {
            return ConstructionInputMapping::Rejected(
                SelectionConstructionInputRejection::PositionalUnavailable {
                    ordinal: source_ordinal,
                },
            );
        };

        if supplied[surface_index] {
            return ConstructionInputMapping::Rejected(
                SelectionConstructionInputRejection::Duplicate {
                    name: input.name.map(|name| name.as_str().to_owned()),
                    ordinal: source_ordinal,
                },
            );
        }

        let actual = types
            .expression(input.expression).unwrap_or_else(|| panic!("map_construction_inputs requires checked expression type or node, expression: {expression:?}"));

        recovered |= input.is_recovered;

        let surface = surfaces[surface_index];

        if !actual.is_recovered()
            && actual.ty() != surface.ty()
            && !numeric_literal_accepts_type(request, input.expression, surface.ty())
        {
            return ConstructionInputMapping::Rejected(SelectionConstructionInputRejection::Type {
                name: input.name.map(|name| name.as_str().to_owned()),
                ordinal: source_ordinal,
                expected: surface.ty(),
                actual: actual.ty(),
            });
        }

        supplied[surface_index] = true;

        values.push(SelectedConstructionInput::Explicit {
            expression: input.expression,
            input: surface.input(),
            ty: surface.ty(),
            ordinal: surface.ordinal(),
        });
    }

    for (index, surface) in surfaces.into_iter().enumerate() {
        if supplied[index] {
            continue;
        }

        let Some(provider) = surface.default() else {
            let ordinal = super::super::capacity::selection_ordinal_u64(index);

            return ConstructionInputMapping::Rejected(
                SelectionConstructionInputRejection::Missing {
                    name: surface.name().as_str().to_owned(),
                    ordinal,
                    expected: surface.ty(),
                },
            );
        };

        values.push(SelectedConstructionInput::Default {
            input: surface.input(),
            provider,
            ty: surface.ty(),
            ordinal: surface.ordinal(),
        });
    }

    ConstructionInputMapping::Mapped(MappedConstructionInputs { values, recovered })
}

fn construction_inputs<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
) -> Vec<SourceConstructionInput<'_>>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expression) = request.view().expression(expression) else {
        panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in construction_inputs"
        );
    };

    let inputs = match expression {
        BoundExpression::StructConstruction(construction) => construction
            .fields()
            .iter()
            .map(|field| SourceConstructionInput {
                expression: field.expression(),
                name: field.name(),
                is_recovered: field.is_recovered(),
            })
            .collect(),
        BoundExpression::Call(call) => call
            .arguments()
            .iter()
            .map(|argument| SourceConstructionInput {
                expression: argument.expression(),
                name: argument.name(),
                is_recovered: argument.is_recovered(),
            })
            .collect(),
        BoundExpression::BoxConstruction(construction) => construction
            .arguments()
            .iter()
            .map(|argument| SourceConstructionInput {
                expression: argument.expression(),
                name: argument.name(),
                is_recovered: argument.is_recovered(),
            })
            .collect(),
        BoundExpression::LeadingDotVariant(_)
        | BoundExpression::UnqualifiedVariant(_)
        | BoundExpression::MemberAccess(_) => Vec::new(),
        _ => panic!(
            "Semantic-selection inputs do not describe the requested bound unit or operation category. in construction_inputs"
        ),
    };

    inputs
}

fn construction_surface_is_valid(
    target: ConstructionTarget,
    surfaces: &[&ConstructionInputSurface],
) -> bool {
    let mut identities = BTreeSet::new();
    let mut names = BTreeSet::new();
    let mut ordinals = BTreeSet::new();
    let mut saw_named_only = false;

    surfaces.iter().all(|surface| {
        saw_named_only |= surface.position() == CallablePosition::NamedOnly;

        identities.insert(surface.input())
            && names.insert(surface.name())
            && ordinals.insert(surface.ordinal())
            && (!saw_named_only || surface.position() == CallablePosition::NamedOnly)
            && target.accepts_input(surface.input())
            && surface
                .default()
                .is_none_or(|provider| surface.input().accepts_default(provider))
    })
}
