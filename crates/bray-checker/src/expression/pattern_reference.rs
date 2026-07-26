use std::collections::BTreeSet;

use bray_bound_tree::{
    AnyBoundNodeId, BoundExpression, BoundReferenceTarget, BoundUnresolvedReferenceKind,
    BoundWalkControl, BoundWalkEvent, CheckedPatternFacts, SemanticSelection,
    SemanticSelectionEntry, walk_bound_unit_view,
};
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticBag, DiagnosticKind, DiagnosticNameKind, SeverityKind,
};
use bray_symbols::TypeData;

use crate::diagnostic::{diagnostic_id, expression_span};
use crate::{
    CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView, ExpressionTypeEvidence,
};

#[derive(Default)]
pub(super) struct PreparedPatternReferences {
    pub(super) deferred: BTreeSet<bray_bound_tree::BoundExpressionId>,
    pub(super) evidence: Vec<ExpressionTypeEvidence>,
    pub(super) selections: Vec<SemanticSelectionEntry>,
    pub(super) diagnostics: DiagnosticBag,
}

pub(super) fn pattern_binding_reference_expressions<C>(
    request: CheckerUnitView<'_, C>,
) -> Result<BTreeSet<bray_bound_tree::BoundExpressionId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut expressions = BTreeSet::new();
    let mut missing = None;

    walk_bound_unit_view(request.view(), request.unit().root(), |event| {
        let BoundWalkEvent::Enter(AnyBoundNodeId::Expression(expression)) = event else {
            return BoundWalkControl::Continue;
        };

        match request.view().expression(expression) {
            Some(BoundExpression::PatternReference(_)) => {
                expressions.insert(expression);
            }
            Some(BoundExpression::Name(_))
                if expression_uses_pattern_binding(request, expression) =>
            {
                expressions.insert(expression);
            }
            Some(_) => {}
            None => {
                missing = Some(expression);

                return BoundWalkControl::Stop;
            }
        }

        BoundWalkControl::Continue
    });

    if let Some(expression) = missing {
        return Err(CheckerInfrastructureError::InvalidBoundNode {
            node: expression.into(),
        });
    }

    Ok(expressions)
}

pub(super) fn prepare_pattern_binding_references<C>(
    request: CheckerUnitView<'_, C>,
    facts: &CheckedPatternFacts,
    expressions: BTreeSet<bray_bound_tree::BoundExpressionId>,
) -> Result<PreparedPatternReferences, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let error_type = request
        .semantic_values()
        .intern_type(TypeData::Error)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let mut evidence = Vec::with_capacity(expressions.len());
    let mut selections = Vec::with_capacity(expressions.len());
    let mut diagnostics = Vec::new();

    for expression in &expressions {
        let Some(bound) = request.view().expression(*expression) else {
            return Err(CheckerInfrastructureError::InvalidBoundNode {
                node: (*expression).into(),
            });
        };

        let binding_id = match bound {
            BoundExpression::Name(name) => {
                let BoundReferenceTarget::Local(bray_symbols::AnyLocalSymbolId::Binding(binding)) =
                    name.target()
                else {
                    return Err(CheckerInfrastructureError::InvalidBoundNode {
                        node: (*expression).into(),
                    });
                };

                binding
            }
            BoundExpression::PatternReference(reference) => reference.binding(),
            _ => {
                return Err(CheckerInfrastructureError::InvalidBoundNode {
                    node: (*expression).into(),
                });
            }
        };

        if let Some(binding_type) = facts.binding_type(binding_id) {
            evidence.push(ExpressionTypeEvidence::new(*expression, binding_type.ty()));

            if matches!(bound, BoundExpression::PatternReference(_)) {
                selections.push(SemanticSelectionEntry::new(
                    *expression,
                    SemanticSelection::Reference(BoundReferenceTarget::Local(
                        binding_type.binding().into(),
                    )),
                ));
            }

            continue;
        }

        let BoundExpression::PatternReference(reference) = bound else {
            evidence.push(ExpressionTypeEvidence::new(*expression, error_type));

            continue;
        };

        let Some(pattern) = facts.pattern(reference.pattern()) else {
            return Err(CheckerInfrastructureError::InvalidBoundNode {
                node: reference.pattern().into(),
            });
        };

        if pattern.target().is_none() || pattern.is_recovered() {
            evidence.push(ExpressionTypeEvidence::new(*expression, error_type));

            continue;
        }

        evidence.push(ExpressionTypeEvidence::new(*expression, error_type));

        if let Some(kind) = diagnostic_kind(
            reference.unresolved_kind(),
            reference.candidates().is_empty(),
        ) {
            let span = expression_span(request, *expression)?;

            let mut diagnostic =
                Diagnostic::new(diagnostic_id(diagnostics.len()), kind, SeverityKind::Error)
                    .with_primary_span(span)
                    .with_arg(DiagnosticArg::referenced_name(reference.name().as_str()));

            if kind == DiagnosticKind::BindingWrongNameKind {
                diagnostic = diagnostic
                    .with_arg(DiagnosticArg::expected_name_kind(DiagnosticNameKind::Value));
            }

            diagnostics.push(diagnostic);
        }
    }

    Ok(PreparedPatternReferences {
        deferred: expressions,
        evidence,
        selections,
        diagnostics: DiagnosticBag::from(diagnostics),
    })
}

fn expression_uses_pattern_binding<C>(
    request: CheckerUnitView<'_, C>,
    expression: bray_bound_tree::BoundExpressionId,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    matches!(
        request.view().expression(expression),
        Some(BoundExpression::Name(name))
            if matches!(
                name.target(),
                BoundReferenceTarget::Local(bray_symbols::AnyLocalSymbolId::Binding(_))
            )
    )
}

const fn diagnostic_kind(
    kind: BoundUnresolvedReferenceKind,
    has_no_candidates: bool,
) -> Option<DiagnosticKind> {
    match kind {
        BoundUnresolvedReferenceKind::NotFound => Some(DiagnosticKind::BindingUnresolvedName),
        BoundUnresolvedReferenceKind::WrongKind => Some(DiagnosticKind::BindingWrongNameKind),
        BoundUnresolvedReferenceKind::Ambiguous => Some(DiagnosticKind::BindingAmbiguousName),
        BoundUnresolvedReferenceKind::Inaccessible => Some(DiagnosticKind::BindingInaccessibleName),
        BoundUnresolvedReferenceKind::Malformed if has_no_candidates => None,
        BoundUnresolvedReferenceKind::Malformed => Some(DiagnosticKind::BindingMalformedName),
    }
}
