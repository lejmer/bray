use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundExpressionId, BoundStructuredExpressionKind,
    BoundWalkControl, BoundWalkEvent, BoundWalkOutcome, CheckedExpressionTypes,
    SelectedPropagation, SelectedPropagationBoundary, SemanticSelection, SemanticSelectionEntry,
    walk_bound_unit_view,
};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticNote, DiagnosticNoteKind, DiagnosticPropagationProblem, DiagnosticType, SeverityKind,
};
use bray_symbols::{GenericArgument, TypeData, TypeId};

use crate::diagnostic::{diagnostic_id, expression_span};
use crate::representation::type_representation;
use crate::{
    CheckerInfrastructureError, CheckerQueryError, CheckerRequestContext, CheckerUnitRoot,
    CheckerUnitView,
};

use super::built_in_conversion_plan;

#[derive(Clone, Copy)]
struct ResultBoundary {
    target: SelectedPropagationBoundary,
    ty: TypeId,
}

pub(crate) fn select_propagations<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
) -> Result<Option<(Vec<SemanticSelectionEntry>, DiagnosticBag)>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let block_owners = crate::unit::expression_block_owners(
        request,
        types.entries().iter().map(|entry| entry.expression()),
    )?;

    let mut events = Vec::new();

    let outcome = walk_bound_unit_view(request.view(), walk_root(request), |event| {
        if request.is_cancelled() {
            return BoundWalkControl::Stop;
        }

        events.push(event);

        BoundWalkControl::Continue
    });

    match outcome {
        BoundWalkOutcome::Completed => {}
        BoundWalkOutcome::Stopped => return Ok(None),
        BoundWalkOutcome::MissingNode(node) => {
            return Err(CheckerInfrastructureError::InvalidBoundNode { node }.into());
        }
    }

    let mut boundaries = Vec::new();
    let mut entered_blocks = Vec::new();
    let mut entries = Vec::new();
    let mut diagnostics = DiagnosticBag::new();

    for event in events {
        match event {
            BoundWalkEvent::Enter(AnyBoundNodeId::Block(block)) => {
                let boundary = block_owners
                    .get(&block)
                    .and_then(|owner| types.expression(*owner))
                    .filter(|result| !result.is_recovered())
                    .and_then(|result| {
                        request.view().block(block).map(|block| ResultBoundary {
                            target: SelectedPropagationBoundary::YieldRegion(
                                block.origin().source_anchor().syntax(),
                            ),
                            ty: result.ty(),
                        })
                    });

                entered_blocks.push(boundary.is_some());

                if let Some(boundary) = boundary {
                    boundaries.push(boundary);
                }
            }
            BoundWalkEvent::Exit(AnyBoundNodeId::Block(_)) => {
                if entered_blocks.pop().unwrap_or(false) {
                    boundaries.pop();
                }
            }
            BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) => {
                let Some(selection) = select_propagation(request, types, expression, &boundaries)?
                else {
                    continue;
                };

                match selection {
                    Ok(selection) => entries.push(SemanticSelectionEntry::new(
                        expression,
                        SemanticSelection::Propagation(selection),
                    )),
                    Err(problem) => diagnostics.add(missing_boundary_diagnostic(
                        request,
                        expression,
                        problem,
                        diagnostics.len(),
                    )?),
                }
            }
            BoundWalkEvent::Enter(AnyBoundNodeId::Pattern(_) | AnyBoundNodeId::CallableBody(_))
            | BoundWalkEvent::Exit(
                AnyBoundNodeId::Expression(_)
                | AnyBoundNodeId::Pattern(_)
                | AnyBoundNodeId::CallableBody(_),
            ) => {}
        }
    }

    Ok(Some((entries, diagnostics)))
}

fn select_propagation<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    expression: BoundExpressionId,
    boundaries: &[ResultBoundary],
) -> Result<
    Option<Result<SelectedPropagation, DiagnosticPropagationProblem>>,
    CheckerQueryError<C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(BoundExpression::Structured(structured)) = request.view().expression(expression)
    else {
        return Ok(None);
    };

    if !matches!(
        structured.kind(),
        BoundStructuredExpressionKind::NullablePropagation
            | BoundStructuredExpressionKind::ResultPropagation
    ) {
        return Ok(None);
    }

    let Some(operand) = structured.operands().first().copied() else {
        return Ok(None);
    };

    let Some(operand_type) = types.expression(operand) else {
        return Ok(None);
    };

    if operand_type.is_recovered() {
        return Ok(None);
    }

    if structured.kind() == BoundStructuredExpressionKind::NullablePropagation {
        let TypeData::Nullable(_) = request
            .semantic_values()
            .type_data(operand_type.ty())
            .as_ref()
        else {
            return Ok(None);
        };

        return Ok(Some(
            match select_nullable_boundary(request, types, boundaries)? {
                Some(boundary) => Ok(SelectedPropagation::Nullable {
                    boundary: boundary.target,
                    result_type: boundary.ty,
                }),
                None => Err(DiagnosticPropagationProblem::NullableBoundaryUnavailable {
                    operand: diagnostic_type(request, operand_type.ty())?,
                    available_boundaries: diagnostic_boundary_types(request, types, boundaries)?
                        .into_boxed_slice(),
                }),
            },
        ));
    }

    let representation = type_representation(request, operand_type.ty());

    match representation {
        Some(RepresentationRole::RunResult) => Ok(Some(Ok(SelectedPropagation::CurrentRun))),
        Some(RepresentationRole::Result) => {
            let Some(error_type) = named_type_arguments(request, operand_type.ty())
                .get(1)
                .copied()
            else {
                return Ok(None);
            };

            Ok(Some(
                match select_result_boundary(request, types, boundaries, error_type)? {
                    Ok((boundary, conversion)) => Ok(SelectedPropagation::Result {
                        boundary: boundary.target,
                        result_type: boundary.ty,
                        error_conversion: conversion,
                    }),
                    Err(available_errors) => {
                        Err(DiagnosticPropagationProblem::ResultBoundaryUnavailable {
                            source_error: diagnostic_type(request, error_type)?,
                            available_errors: diagnostic_types(request, available_errors)?
                                .into_boxed_slice(),
                        })
                    }
                },
            ))
        }
        _ => Ok(None),
    }
}

fn select_nullable_boundary<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    boundaries: &[ResultBoundary],
) -> Result<Option<ResultBoundary>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for boundary in boundaries.iter().rev().copied() {
        if is_nullable(request, boundary.ty) {
            return Ok(Some(boundary));
        }
    }

    let Some(ty) = types.callable_result_type() else {
        return Ok(None);
    };

    Ok(is_nullable(request, ty).then_some(ResultBoundary {
        target: SelectedPropagationBoundary::Callable,
        ty,
    }))
}

fn select_result_boundary<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    boundaries: &[ResultBoundary],
    error_type: TypeId,
) -> Result<
    Result<(ResultBoundary, bray_bound_tree::SelectedConversion), Vec<TypeId>>,
    CheckerQueryError<C::UpstreamError>,
>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut available_errors = Vec::new();

    for boundary in boundaries
        .iter()
        .rev()
        .copied()
        .chain(types.callable_result_type().map(|ty| ResultBoundary {
            target: SelectedPropagationBoundary::Callable,
            ty,
        }))
    {
        if type_representation(request, boundary.ty) != Some(RepresentationRole::Result) {
            continue;
        }

        let Some(target_error) = named_type_arguments(request, boundary.ty).get(1).copied() else {
            continue;
        };

        if !available_errors.contains(&target_error) {
            available_errors.push(target_error);
        }

        if let Some(conversion) = built_in_conversion_plan(request, error_type, target_error)? {
            return Ok(Ok((boundary, conversion)));
        }
    }

    Ok(Err(available_errors))
}

fn diagnostic_boundary_types<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    boundaries: &[ResultBoundary],
) -> Result<Vec<DiagnosticType>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    diagnostic_types(
        request,
        boundaries
            .iter()
            .rev()
            .map(|boundary| boundary.ty)
            .chain(types.callable_result_type()),
    )
}

fn diagnostic_types<C>(
    request: CheckerUnitView<'_, C>,
    types: impl IntoIterator<Item = TypeId>,
) -> Result<Vec<DiagnosticType>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut diagnostics = Vec::new();

    for ty in types {
        let ty = diagnostic_type(request, ty)?;

        if !diagnostics.contains(&ty) {
            diagnostics.push(ty);
        }
    }

    Ok(diagnostics)
}

fn diagnostic_type<C>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> Result<DiagnosticType, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    crate::diagnostic::diagnostic_type(request.context(), ty)
}

fn named_type_arguments<C>(request: CheckerUnitView<'_, C>, ty: TypeId) -> Vec<TypeId>
where
    C: CheckerRequestContext + ?Sized,
{
    let data = request.semantic_values().type_data(ty);

    let TypeData::Named { substitution, .. } = data.as_ref() else {
        return Vec::new();
    };

    let substitution = request
        .semantic_values()
        .generic_substitution_data(*substitution);

    substitution
        .bindings()
        .iter()
        .filter_map(|binding| match binding.argument() {
            GenericArgument::Type(ty) => Some(ty),
            GenericArgument::Constant(_) => None,
        })
        .collect()
}

fn is_nullable<C>(request: CheckerUnitView<'_, C>, ty: TypeId) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    let data = request.semantic_values().type_data(ty);

    matches!(data.as_ref(), TypeData::Nullable(_))
}

const fn walk_root<C>(request: CheckerUnitView<'_, C>) -> AnyBoundNodeId
where
    C: CheckerRequestContext + ?Sized,
{
    match request.root() {
        CheckerUnitRoot::CallableBody(body) => AnyBoundNodeId::CallableBody(body),
        CheckerUnitRoot::Expression(expression) => AnyBoundNodeId::Expression(expression),
        CheckerUnitRoot::ExpressionSequence(block) => AnyBoundNodeId::Block(block),
    }
}

fn missing_boundary_diagnostic<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    problem: DiagnosticPropagationProblem,
    index: usize,
) -> Result<Diagnostic, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let span = expression_span(request, expression)?;

    Ok(Diagnostic::new(
        diagnostic_id(index),
        DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::IncompatiblePropagationBoundary,
        span,
    ))
    .with_arg(DiagnosticArg::propagation_problem(problem))
    .with_note(DiagnosticNote::new(
        DiagnosticNoteKind::PropagationBoundaryMustMatch,
    )))
}
