use std::collections::BTreeSet;

use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundStructuredExpressionKind, CheckedExpressionTypes,
    ConstructionTarget, SelectedConstructionInput,
};
use bray_symbols::CallablePosition;

use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

use super::super::ConstructionInputSurface;

pub(super) struct MappedConstructionInputs {
    pub(super) values: Vec<SelectedConstructionInput>,
    pub(super) recovered: bool,
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
) -> Result<Option<MappedConstructionInputs>, CheckerInfrastructureError>
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

    for input in source {
        let surface_index = match input.name {
            Some(name) => {
                saw_named = true;
                surfaces.iter().position(|surface| surface.name() == name)
            }
            None if saw_named => return Ok(None),
            None => {
                let index = positional_index;
                positional_index += 1;

                surfaces.get(index).and_then(|surface| {
                    (surface.position() == CallablePosition::PositionalOrNamed).then_some(index)
                })
            }
        };

        let Some(surface_index) = surface_index else {
            return Ok(None);
        };

        if supplied[surface_index] {
            return Ok(None);
        }

        let actual = types
            .expression(input.expression)
            .ok_or(CheckerInfrastructureError::InvalidSemanticSelectionInput)?;

        recovered |= input.is_recovered || actual.is_recovered();

        if !actual.is_recovered() && actual.ty() != surfaces[surface_index].ty() {
            return Ok(None);
        }

        supplied[surface_index] = true;

        values.push(SelectedConstructionInput::Explicit {
            expression: input.expression,
            input: surfaces[surface_index].input(),
            ordinal: surfaces[surface_index].ordinal(),
        });
    }

    for (index, surface) in surfaces.into_iter().enumerate() {
        if supplied[index] {
            continue;
        }

        let Some(provider) = surface.default() else {
            return Ok(None);
        };

        values.push(SelectedConstructionInput::Default {
            input: surface.input(),
            provider,
            ordinal: surface.ordinal(),
        });
    }

    Ok(Some(MappedConstructionInputs { values, recovered }))
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
        BoundExpression::LeadingDotVariant(_) | BoundExpression::MemberAccess(_) => Vec::new(),
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
