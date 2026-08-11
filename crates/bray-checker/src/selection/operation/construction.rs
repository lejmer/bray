use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, CheckedExpressionTypes,
    ConstructionTarget, SelectedConstructionInput,
};
use bray_symbols::CallablePosition;

use crate::type_check::numeric_literal_accepts_type;
use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

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
) -> Result<ConstructionInputMapping, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let source = construction_inputs(request, expression)?;
    let mut surfaces = surfaces.iter().collect::<Vec<_>>();

    surfaces.sort_unstable_by_key(|surface| surface.ordinal());

    if !construction_surface_is_valid(target, &surfaces) {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    }

    let mut supplied = vec![false; surfaces.len()];
    let mut values = Vec::with_capacity(surfaces.len());
    let mut positional_index = 0;
    let mut saw_named = false;
    let mut recovered = false;

    for (source_ordinal, input) in source.into_iter().enumerate() {
        let source_ordinal = u64::try_from(source_ordinal)
            .map_err(|_| CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

        let surface_index = match input.name {
            Some(name) => {
                saw_named = true;

                let Some(index) = surfaces.iter().position(|surface| surface.name() == name) else {
                    return Ok(ConstructionInputMapping::Rejected(
                        SelectionConstructionInputRejection::UnknownName {
                            provided: name.as_str().to_owned(),
                            accepted: surfaces
                                .iter()
                                .map(|surface| surface.name().as_str().to_owned())
                                .collect(),
                        },
                    ));
                };

                Some(index)
            }
            None if saw_named => {
                return Ok(ConstructionInputMapping::Rejected(
                    SelectionConstructionInputRejection::PositionalAfterNamed,
                ));
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
            return Ok(ConstructionInputMapping::Rejected(
                SelectionConstructionInputRejection::PositionalUnavailable {
                    ordinal: source_ordinal,
                },
            ));
        };

        if supplied[surface_index] {
            return Ok(ConstructionInputMapping::Rejected(
                SelectionConstructionInputRejection::Duplicate {
                    name: input.name.map(|name| name.as_str().to_owned()),
                    ordinal: source_ordinal,
                },
            ));
        }

        let actual = types
            .expression(input.expression)
            .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

        recovered |= input.is_recovered;

        let surface = surfaces[surface_index];

        if !actual.is_recovered()
            && actual.ty() != surface.ty()
            && !numeric_literal_accepts_type(request, input.expression, surface.ty())?
        {
            return Ok(ConstructionInputMapping::Rejected(
                SelectionConstructionInputRejection::Type {
                    name: input.name.map(|name| name.as_str().to_owned()),
                    ordinal: source_ordinal,
                    expected: surface.ty(),
                    actual: actual.ty(),
                },
            ));
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
            let ordinal = u64::try_from(index)
                .map_err(|_| CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

            return Ok(ConstructionInputMapping::Rejected(
                SelectionConstructionInputRejection::Missing {
                    name: surface.name().as_str().to_owned(),
                    ordinal,
                    expected: surface.ty(),
                },
            ));
        };

        values.push(SelectedConstructionInput::Default {
            input: surface.input(),
            provider,
            ty: surface.ty(),
            ordinal: surface.ordinal(),
        });
    }

    Ok(ConstructionInputMapping::Mapped(MappedConstructionInputs {
        values,
        recovered,
    }))
}

fn construction_inputs<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
) -> Result<Vec<SourceConstructionInput<'_>>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expression) = request.view().expression(expression) else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
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
        BoundExpression::Structured(structured)
            if structured.kind() == BoundStructuredExpressionKind::TypeFormConstruction =>
        {
            structured
                .operands()
                .iter()
                .copied()
                .map(|expression| SourceConstructionInput {
                    expression,
                    name: None,
                    is_recovered: structured.is_recovered(),
                })
                .collect()
        }
        BoundExpression::LeadingDotVariant(_)
        | BoundExpression::UnqualifiedVariant(_)
        | BoundExpression::MemberAccess(_) => Vec::new(),
        _ => return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput),
    };

    Ok(inputs)
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
