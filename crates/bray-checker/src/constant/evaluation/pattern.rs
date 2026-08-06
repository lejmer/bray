use bray_bound_tree::{
    BoundExpressionId, BoundMatchExpression, BoundPatternId, BoundPatternKind, PatternPredicate,
    PatternProjection,
};
use bray_symbols::{
    AnyLocalSymbolId, ConstantTermData, ConstantTermId, ConstantValueId, ConstantValueKind,
};

use crate::constant::literal::parse_literal;
use crate::representation::type_representation;
use crate::{CheckerInfrastructureError, CheckerRequestContext};

use super::engine::Evaluator;
use super::flow::EvaluationFlow;
use super::support::EvaluationFailure;

impl<'view, 'input, 'types, C> Evaluator<'view, 'input, 'types, C>
where
    C: CheckerRequestContext + ?Sized,
{
    pub(super) fn evaluate_match(
        &mut self,
        expression: BoundExpressionId,
        matched: &BoundMatchExpression,
    ) -> Result<EvaluationFlow, EvaluationFailure> {
        let subject = self.evaluate(matched.subject())?;
        let subject_value = self.closed_value(subject, expression)?;
        let result_type = self.expression_type(expression)?;

        for arm in matched.arms() {
            self.observe_cancellation()?;

            // Failed patterns and guards must not leak arm-local constant bindings.
            let outer_locals = self.locals.clone();

            if !self.pattern_matches(expression, arm.pattern(), subject, subject_value)? {
                self.locals = outer_locals;

                continue;
            }

            if let Some(guard) = arm.guard()
                && !self.boolean_value(guard)?
            {
                self.locals = outer_locals;

                continue;
            }

            return self.evaluate_block(arm.body(), result_type);
        }

        Err(EvaluationFailure::invalid_expression(expression))
    }

    fn pattern_matches(
        &mut self,
        owner: BoundExpressionId,
        pattern: BoundPatternId,
        subject: ConstantTermId,
        subject_value: ConstantValueId,
    ) -> Result<bool, EvaluationFailure> {
        let pattern_node = self
            .request
            .view()
            .pattern(pattern)
            .ok_or(EvaluationFailure::invalid_input())?;

        if pattern_node.kind() == BoundPatternKind::Alternative {
            for alternative in pattern_node.children() {
                let outer_locals = self.locals.clone();

                if self.pattern_matches(owner, *alternative, subject, subject_value)? {
                    return Ok(true);
                }

                self.locals = outer_locals;
            }

            return Ok(false);
        }

        let facts = self
            .input
            .pattern_facts()
            .ok_or(EvaluationFailure::invalid_input())?;

        let checked = facts
            .pattern(pattern)
            .ok_or(EvaluationFailure::invalid_input())?;

        if checked.is_recovered()
            || !self.predicate_matches(owner, pattern_node, checked.test(), subject_value)?
        {
            return Ok(false);
        }

        for binding in pattern_node.bindings() {
            self.locals
                .insert(AnyLocalSymbolId::from(*binding), subject);
        }

        for child in pattern_node.children() {
            let child_fact = facts
                .pattern(*child)
                .ok_or(EvaluationFailure::invalid_input())?;

            let (child_term, child_value) =
                self.project_pattern_subject(subject_value, child_fact.projection())?;

            if !self.pattern_matches(owner, *child, child_term, child_value)? {
                return Ok(false);
            }
        }

        for entry in pattern_node.entries() {
            let Some(binding) = entry.binding() else {
                continue;
            };

            let binding_fact = facts
                .binding_type(binding)
                .ok_or(EvaluationFailure::invalid_input())?;

            let (value, _) =
                self.project_pattern_subject(subject_value, binding_fact.projection())?;

            self.locals.insert(AnyLocalSymbolId::from(binding), value);
        }

        Ok(true)
    }

    fn predicate_matches(
        &mut self,
        owner: BoundExpressionId,
        pattern: &bray_bound_tree::BoundPattern,
        predicate: Option<PatternPredicate>,
        subject: ConstantValueId,
    ) -> Result<bool, EvaluationFailure> {
        let subject_id = subject;
        let subject = self.constant_value(subject)?;

        Ok(match predicate {
            None
            | Some(
                PatternPredicate::ProductShape(_)
                | PatternPredicate::TupleShape(_)
                | PatternPredicate::ArrayShape(_),
            ) => true,
            Some(PatternPredicate::NullableAbsent) => {
                matches!(subject.kind(), ConstantValueKind::NullableAbsent)
            }
            Some(PatternPredicate::NullablePresent) => {
                matches!(subject.kind(), ConstantValueKind::NullablePresent(_))
            }
            Some(PatternPredicate::ActiveUnionVariant(variant)) => {
                matches!(
                    subject.kind(),
                    ConstantValueKind::Union {
                        variant: active,
                        ..
                    } if *active == variant
                )
            }
            Some(PatternPredicate::Literal(literal)) => {
                let literal = literal.literal();

                let source = self
                    .request
                    .source(pattern.origin().source_anchor())
                    .map_err(EvaluationFailure::Infrastructure)?;

                let spelling = source.text_for_range(literal.range()).ok_or_else(|| {
                    EvaluationFailure::Infrastructure(
                        CheckerInfrastructureError::InvalidSourceRange {
                            span: bray_source::SourceSpan::new(
                                source.span().source_id(),
                                literal.range(),
                            ),
                        },
                    )
                })?;

                self.budget.charge_literal(owner, spelling.len())?;

                let representation = type_representation(self.request, pattern.input_type())
                    .map_err(EvaluationFailure::Infrastructure)?
                    .ok_or_else(|| EvaluationFailure::invalid_expression(owner))?;

                let expected = parse_literal(literal.kind(), spelling, representation, || {
                    self.request
                        .selected_target()
                        .machine()
                        .pointer_width_bits()
                })
                .map_err(|error| EvaluationFailure::literal(owner, error))?;

                subject.kind() == &expected
            }
            Some(PatternPredicate::Constant(expected)) => {
                let Some(expected) = self.term_value(expected)? else {
                    return Err(EvaluationFailure::invalid_expression(owner));
                };

                crate::constant::constant_values_equal(
                    self.request.semantic_values(),
                    subject_id,
                    expected,
                )
                .map_err(EvaluationFailure::Infrastructure)?
            }
            Some(PatternPredicate::OwnedTarget) => {
                return Err(EvaluationFailure::invalid_expression(owner));
            }
        })
    }

    fn project_pattern_subject(
        &self,
        subject: ConstantValueId,
        projection: Option<PatternProjection>,
    ) -> Result<(ConstantTermId, ConstantValueId), EvaluationFailure> {
        let subject_data = self.constant_value(subject)?;

        let value = match (projection, subject_data.kind()) {
            (None, _) => subject,
            (Some(PatternProjection::ProductField(field)), ConstantValueKind::Product(fields)) => {
                fields
                    .iter()
                    .find(|entry| *entry.field() == field)
                    .map(|entry| *entry.value())
                    .ok_or(EvaluationFailure::invalid_input())?
            }
            (
                Some(PatternProjection::TupleElement(ordinal)),
                ConstantValueKind::Tuple(elements),
            )
            | (
                Some(PatternProjection::ElementFromStart(ordinal)),
                ConstantValueKind::Array(elements),
            ) => elements
                .get(ordinal.raw() as usize)
                .copied()
                .ok_or(EvaluationFailure::invalid_input())?,
            (
                Some(PatternProjection::ElementFromEnd(ordinal)),
                ConstantValueKind::Array(elements),
            ) => {
                let index = elements
                    .len()
                    .checked_sub(ordinal.raw() as usize + 1)
                    .ok_or(EvaluationFailure::invalid_input())?;

                elements[index]
            }
            (
                Some(PatternProjection::ActiveUnionPayloadField { variant, field }),
                ConstantValueKind::Union {
                    variant: active,
                    fields,
                },
            ) if *active == variant => fields
                .iter()
                .find(|entry| *entry.field() == field)
                .map(|entry| *entry.value())
                .ok_or(EvaluationFailure::invalid_input())?,
            (Some(PatternProjection::NullableValue), ConstantValueKind::NullablePresent(value)) => {
                *value
            }
            (Some(PatternProjection::OwnedTarget), _) | (Some(_), _) => {
                return Err(EvaluationFailure::invalid_input());
            }
        };

        let term = self.intern_term(ConstantTermData::Value(value))?;

        Ok((term, value))
    }

    fn boolean_value(&mut self, expression: BoundExpressionId) -> Result<bool, EvaluationFailure> {
        let value = self.evaluate(expression)?;
        let value = self.closed_value(value, expression)?;
        let value = self.constant_value(value)?;

        match value.kind() {
            ConstantValueKind::Boolean(value) => Ok(*value),
            _ => Err(EvaluationFailure::invalid_expression(expression)),
        }
    }
}
