use bray_bound_tree::{
    BoundBlockId, BoundBlockItem, BoundControlTransferKind, BoundExpression, BoundExpressionId,
    BoundStructuredExpressionKind, CheckedExpressionTypes, SelectedIterationSource,
};
use bray_diagnostics::{
    DiagnosticArrayGeneratorCardinalityProblem, DiagnosticArrayLength, DiagnosticType,
    DiagnosticYieldCardinality,
};
use bray_symbols::{ConstantTermData, ConstantTermId, ConstantValueKind, TypeData};

use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

pub(super) struct UnprovenArrayGenerator {
    pub(super) expression: BoundExpressionId,
    pub(super) problem: DiagnosticArrayGeneratorCardinalityProblem,
}

pub(super) fn unproven_array_generators<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    iteration_sources: &[SelectedIterationSource],
) -> Result<Vec<UnprovenArrayGenerator>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if iteration_sources.is_empty() {
        return Ok(Vec::new());
    }

    let mut unproven = Vec::new();

    for entry in types.entries() {
        let expression_id = entry.expression();

        let Some(BoundExpression::Structured(expression)) =
            request.view().expression(expression_id)
        else {
            continue;
        };

        if expression.kind() != BoundStructuredExpressionKind::ArrayGenerator
            || expression.is_recovered()
        {
            continue;
        }

        if let Some(problem) =
            array_generator_problem(request, types, iteration_sources, expression_id, expression)?
        {
            unproven.push(UnprovenArrayGenerator {
                expression: expression_id,
                problem,
            });
        }
    }

    Ok(unproven)
}

fn array_generator_problem<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    iteration_sources: &[SelectedIterationSource],
    expression_id: BoundExpressionId,
    expression: &bray_bound_tree::BoundStructuredExpression,
) -> Result<Option<DiagnosticArrayGeneratorCardinalityProblem>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(result) = types
        .expression(expression_id)
        .filter(|result| !result.is_recovered())
    else {
        return Ok(None);
    };

    let result_data = request
        .semantic_values()
        .type_data(result.ty())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let TypeData::Array {
        element,
        length: result_length,
    } = result_data.as_ref()
    else {
        return Err(CheckerInfrastructureError::InvalidExpressionTypeInput {
            expression: expression_id,
        });
    };

    let Some(iteration_id) = expression.operands().first().copied() else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let Some(BoundExpression::Generator(iteration)) = request.view().expression(iteration_id)
    else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let Some(selection) = iteration_sources
        .iter()
        .find(|selection| selection.expression() == iteration_id)
    else {
        return Err(CheckerInfrastructureError::InvalidSemanticSelectionInput);
    };

    let source = diagnostic_type(request, selection.source_type())?;
    let element = diagnostic_type(request, *element)?;
    let required = diagnostic_array_length(request, *result_length)?;

    let target = expression.origin().source_anchor().syntax();
    let summary = block_summary(request, iteration.body(), target);

    if let Some(actual) = summary.cardinality_problem() {
        let source_length = selection
            .exact_count()
            .map(|length| diagnostic_array_length(request, length))
            .transpose()?
            .unwrap_or(DiagnosticArrayLength::Symbolic);

        return Ok(Some(
            DiagnosticArrayGeneratorCardinalityProblem::YieldCountNotExact {
                element,
                source_length,
                required,
                actual,
            },
        ));
    }

    let Some(source_length) = selection.exact_count() else {
        return Ok(Some(
            DiagnosticArrayGeneratorCardinalityProblem::SourceCountUnavailable {
                source,
                element,
                required,
            },
        ));
    };

    let diagnostic_source_length = diagnostic_array_length(request, source_length)?;

    if source_length != *result_length {
        return Ok(Some(
            DiagnosticArrayGeneratorCardinalityProblem::SourceLengthMismatch {
                source,
                element,
                source_length: diagnostic_source_length,
                required,
            },
        ));
    }

    Ok(None)
}

fn diagnostic_type<C>(
    request: CheckerUnitView<'_, C>,
    ty: bray_symbols::TypeId,
) -> Result<DiagnosticType, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    crate::diagnostic::diagnostic_type(request.context(), ty)
}

fn diagnostic_array_length<C>(
    request: CheckerUnitView<'_, C>,
    term: ConstantTermId,
) -> Result<DiagnosticArrayLength, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let term = request
        .semantic_values()
        .constant_term_data(term)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let exact = match term.as_ref() {
        ConstantTermData::IntegerLiteral { value, .. } => value.to_u64(),
        ConstantTermData::Value(value) => {
            let value = request
                .semantic_values()
                .constant_value_data(*value)
                .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

            match value.kind() {
                ConstantValueKind::Integer(value) => value.to_u64(),
                _ => None,
            }
        }
        _ => None,
    };

    Ok(exact.map_or(
        DiagnosticArrayLength::Symbolic,
        DiagnosticArrayLength::Exact,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum YieldCount {
    Zero,
    One,
    Many,
}

impl YieldCount {
    const fn plus(self, other: Self) -> Self {
        match (self, other) {
            (Self::Zero, value) | (value, Self::Zero) => value,
            (Self::One, Self::One)
            | (Self::One, Self::Many)
            | (Self::Many, Self::One)
            | (Self::Many, Self::Many) => Self::Many,
        }
    }
}

#[derive(Clone, Copy, Debug)]
struct YieldSummary {
    normal: [bool; 3],
    continues: [bool; 3],
    has_break: bool,
    is_unknown: bool,
}

impl YieldSummary {
    const fn normal() -> Self {
        Self {
            normal: [true, false, false],
            continues: [false; 3],
            has_break: false,
            is_unknown: false,
        }
    }

    const fn unknown() -> Self {
        Self {
            normal: [false; 3],
            continues: [false; 3],
            has_break: false,
            is_unknown: true,
        }
    }

    fn then(self, next: Self) -> Self {
        let mut normal = [false; 3];
        let mut continues = self.continues;

        for (left_index, left_present) in self.normal.into_iter().enumerate() {
            if !left_present {
                continue;
            }

            for (right_index, right_present) in next.normal.into_iter().enumerate() {
                if right_present {
                    normal[count_index(index_count(left_index).plus(index_count(right_index)))] =
                        true;
                }
            }

            for (right_index, right_present) in next.continues.into_iter().enumerate() {
                if right_present {
                    continues
                        [count_index(index_count(left_index).plus(index_count(right_index)))] =
                        true;
                }
            }
        }

        Self {
            normal,
            continues,
            has_break: self.has_break || next.has_break,
            is_unknown: self.is_unknown || next.is_unknown,
        }
    }

    const fn merge(self, other: Self) -> Self {
        Self {
            normal: merge_counts(self.normal, other.normal),
            continues: merge_counts(self.continues, other.continues),
            has_break: self.has_break || other.has_break,
            is_unknown: self.is_unknown || other.is_unknown,
        }
    }

    fn with_transfer(self, kind: BoundControlTransferKind) -> Self {
        match kind {
            BoundControlTransferKind::Yield => Self {
                normal: add_one(self.normal),
                ..self
            },
            BoundControlTransferKind::Continue => Self {
                normal: [false; 3],
                continues: merge_counts(self.continues, self.normal),
                ..self
            },
            BoundControlTransferKind::Break => Self {
                normal: [false; 3],
                has_break: self.has_break || self.normal.into_iter().any(|present| present),
                ..self
            },
            BoundControlTransferKind::Return => Self {
                normal: [false; 3],
                ..self
            },
        }
    }

    fn cardinality_problem(self) -> Option<DiagnosticYieldCardinality> {
        if self.is_unknown {
            return Some(DiagnosticYieldCardinality::Unknown);
        }

        if self.has_break {
            return Some(DiagnosticYieldCardinality::Break);
        }

        let possible = [
            self.normal[0] || self.continues[0],
            self.normal[1] || self.continues[1],
            self.normal[2] || self.continues[2],
        ];

        match possible {
            [false, true, false] => None,
            [true, false, false] => Some(DiagnosticYieldCardinality::Zero),
            [false, false, true] => Some(DiagnosticYieldCardinality::Multiple),
            [true, true, false] => Some(DiagnosticYieldCardinality::ZeroOrOne),
            [false, true, true] => Some(DiagnosticYieldCardinality::OneOrMultiple),
            [true, false, true] => Some(DiagnosticYieldCardinality::ZeroOrMultiple),
            [true, true, true] => Some(DiagnosticYieldCardinality::ZeroOneOrMultiple),
            [false, false, false] => Some(DiagnosticYieldCardinality::Unknown),
        }
    }
}

fn block_summary<C>(
    request: CheckerUnitView<'_, C>,
    block: BoundBlockId,
    target: bray_declarations::SyntaxAnchor,
) -> YieldSummary
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(block) = request.view().block(block) else {
        return YieldSummary::unknown();
    };

    let mut summary = YieldSummary::normal();

    for item in block.items() {
        let expression = match item {
            BoundBlockItem::LocalBinding(binding) => binding.initializer(),
            BoundBlockItem::LocalConstant(constant) => constant.initializer(),
            BoundBlockItem::Expression(expression) => *expression,
        };

        summary = summary.then(expression_summary(request, expression, target));
    }

    summary
}

fn expression_summary<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    target: bray_declarations::SyntaxAnchor,
) -> YieldSummary
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expression) = request.view().expression(expression) else {
        return YieldSummary::unknown();
    };

    match expression {
        BoundExpression::ControlTransfer(transfer) => {
            let operand = transfer
                .operand()
                .map_or_else(YieldSummary::normal, |operand| {
                    expression_summary(request, operand, target)
                });

            if transfer.target() == Some(target) {
                operand.with_transfer(transfer.kind())
            } else if transfer.kind() == BoundControlTransferKind::Return {
                operand.with_transfer(BoundControlTransferKind::Return)
            } else {
                YieldSummary::normal()
            }
        }
        BoundExpression::For(expression) => {
            let source = expression_summary(request, expression.source(), target);

            if block_contains_target(request, expression.body(), target)
                || expression
                    .else_body()
                    .is_some_and(|block| block_contains_target(request, block, target))
            {
                source.then(YieldSummary::unknown())
            } else {
                source
            }
        }
        BoundExpression::Generator(expression) => {
            let source = expression_summary(request, expression.source(), target);

            if expression.region() == target
                || block_contains_target(request, expression.body(), target)
            {
                source.then(YieldSummary::unknown())
            } else {
                source
            }
        }
        BoundExpression::Match(expression) => {
            let subject = expression_summary(request, expression.subject(), target);

            let arms = expression.arms().iter().fold(None, |summary, arm| {
                let guard = arm.guard().map_or_else(YieldSummary::normal, |guard| {
                    expression_summary(request, guard, target)
                });

                let arm = guard.then(block_summary(request, arm.body(), target));

                Some(summary.map_or(arm, |summary: YieldSummary| summary.merge(arm)))
            });

            subject.then(arms.unwrap_or_else(YieldSummary::unknown))
        }
        BoundExpression::Structured(expression)
            if expression.kind() == BoundStructuredExpressionKind::Conditional =>
        {
            let condition =
                sequential_expressions(request, expression.operands().iter().copied(), target);

            let mut branches = expression
                .blocks()
                .iter()
                .map(|block| block_summary(request, *block, target));

            let Some(first) = branches.next() else {
                return condition.then(YieldSummary::unknown());
            };

            let mut alternatives = branches.fold(first, YieldSummary::merge);

            if expression.blocks().len() < 2 {
                alternatives = alternatives.merge(YieldSummary::normal());
            }

            condition.then(alternatives)
        }
        BoundExpression::Structured(expression)
            if matches!(
                expression.kind(),
                BoundStructuredExpressionKind::While | BoundStructuredExpressionKind::Loop
            ) && expression
                .blocks()
                .iter()
                .any(|block| block_contains_target(request, *block, target)) =>
        {
            sequential_expressions(request, expression.operands().iter().copied(), target)
                .then(YieldSummary::unknown())
        }
        _ => sequential_expressions(request, expression.child_expressions(), target),
    }
}

fn sequential_expressions<C>(
    request: CheckerUnitView<'_, C>,
    expressions: impl IntoIterator<Item = BoundExpressionId>,
    target: bray_declarations::SyntaxAnchor,
) -> YieldSummary
where
    C: CheckerRequestContext + ?Sized,
{
    expressions
        .into_iter()
        .fold(YieldSummary::normal(), |summary, expression| {
            summary.then(expression_summary(request, expression, target))
        })
}

fn block_contains_target<C>(
    request: CheckerUnitView<'_, C>,
    block: BoundBlockId,
    target: bray_declarations::SyntaxAnchor,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(block) = request.view().block(block) else {
        return true;
    };

    block.items().iter().any(|item| {
        let expression = match item {
            BoundBlockItem::LocalBinding(binding) => binding.initializer(),
            BoundBlockItem::LocalConstant(constant) => constant.initializer(),
            BoundBlockItem::Expression(expression) => *expression,
        };

        expression_contains_target(request, expression, target)
    })
}

fn expression_contains_target<C>(
    request: CheckerUnitView<'_, C>,
    expression: BoundExpressionId,
    target: bray_declarations::SyntaxAnchor,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expression) = request.view().expression(expression) else {
        return true;
    };

    if let BoundExpression::ControlTransfer(transfer) = expression
        && transfer.target() == Some(target)
    {
        return true;
    }

    expression
        .child_expressions()
        .into_iter()
        .any(|child| expression_contains_target(request, child, target))
        || expression
            .child_blocks()
            .into_iter()
            .any(|block| block_contains_target(request, block, target))
}

const fn index_count(index: usize) -> YieldCount {
    match index {
        0 => YieldCount::Zero,
        1 => YieldCount::One,
        _ => YieldCount::Many,
    }
}

const fn count_index(count: YieldCount) -> usize {
    match count {
        YieldCount::Zero => 0,
        YieldCount::One => 1,
        YieldCount::Many => 2,
    }
}

fn add_one(counts: [bool; 3]) -> [bool; 3] {
    let mut result = [false; 3];

    for (index, present) in counts.into_iter().enumerate() {
        if present {
            result[count_index(index_count(index).plus(YieldCount::One))] = true;
        }
    }

    result
}

const fn merge_counts(left: [bool; 3], right: [bool; 3]) -> [bool; 3] {
    [
        left[0] || right[0],
        left[1] || right[1],
        left[2] || right[2],
    ]
}

#[cfg(test)]
mod tests {
    use super::{BoundControlTransferKind, YieldSummary};

    #[test]
    fn alternatives_prove_exactly_one_yield_only_when_every_path_has_one() {
        let one = YieldSummary::normal().with_transfer(BoundControlTransferKind::Yield);

        assert!(one.merge(one).cardinality_problem().is_none());

        assert!(
            one.merge(YieldSummary::normal())
                .cardinality_problem()
                .is_some()
        );

        assert!(one.then(one).cardinality_problem().is_some());
    }
}
