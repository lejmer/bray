use bray_bound_tree::{BoundExpressionId, BoundMatchExpression};
use bray_ir::{MirBlockId, MirBlockKind, MirEdge, MirOperand, MirTerminatorKind};

use super::super::super::LoweringError;
use super::super::super::block::LoweredExpression;
use super::super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering::expression) fn lower_match(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundMatchExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let subject = self.lower_expression(expression.subject(), current)?;
        let subject = self.materialize_match_subject(expression.subject(), subject)?;

        let Some(current) = subject.block else {
            return Ok(subject);
        };

        let Some(subject) = subject.value else {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = expression.subject()
            );
        };

        let source = self.source(expression.origin());

        let (join, result, result_type) = self.push_result_join(id, expression.origin())?;

        let coverage = self.input.patterns().match_coverage(id).unwrap_or_else(|| {
            panic!(
                "lowering contract violation: UnsupportedExpression {value:?}",
                value = id
            )
        });

        let mut candidate = current;

        for (index, arm) in expression.arms().iter().copied().enumerate() {
            let ordinal = match u32::try_from(index) {
                Ok(ordinal) => ordinal,
                Err(_) => panic!(
                    "lowering contract violation: match arm {index} for {id:?} must fit the MIR ordinal"
                ),
            };

            if coverage.unreachable_arms().contains(&ordinal) {
                continue;
            }

            let matched = self
                .builder
                .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

            let next = self
                .builder
                .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

            let guarded = arm.guard().is_some();

            if guarded {
                self.guard_bindings.push(std::collections::BTreeMap::new());
            }

            self.lower_pattern_branch(
                arm.pattern(),
                Self::retained_operand(&subject),
                candidate,
                matched,
                next,
            )?;

            let mut body_entry = match arm.guard() {
                Some(guard) => self.lower_match_guard(guard, matched, next, &source)?,
                None => matched,
            };

            if guarded {
                self.guard_bindings.pop();

                let committed = self
                    .builder
                    .push_block(Self::retained_source(&source), MirBlockKind::Ordinary)?;

                // Guards cannot change the subject, so the same structural branch selects
                // the owning bindings without storing or consuming them on a rejected arm.
                self.lower_pattern_branch(
                    arm.pattern(),
                    Self::retained_operand(&subject),
                    body_entry,
                    committed,
                    next,
                )?;

                body_entry = committed;
            }

            let body = self.lower_yielding_block(
                arm.body(),
                body_entry,
                join,
                result_type,
                self.active_scopes.len(),
            )?;

            self.finish_result_edge(body, join, result_type)?;

            candidate = next;
        }

        let terminator = if coverage.is_exhaustive() {
            MirTerminatorKind::Unreachable
        } else {
            MirTerminatorKind::Goto(MirEdge::new(join, [self.unit_operand(result_type)]))
        };

        self.set_terminator(candidate, Self::retained_source(&source), terminator)?;

        if !self.builder.has_incoming_edge(join) {
            self.set_terminator(
                join,
                Self::retained_source(&source),
                MirTerminatorKind::Unreachable,
            )?;

            return Ok(LoweredExpression::terminated(source));
        }

        Ok(LoweredExpression::continuing(
            join,
            Some(MirOperand::Value(result)),
            source,
        ))
    }

    fn lower_match_guard(
        &mut self,
        guard: BoundExpressionId,
        current: MirBlockId,
        unmatched: MirBlockId,
        source: &bray_ir::MirSourceAnchor,
    ) -> Result<MirBlockId, LoweringError> {
        let guard_value = self.lower_expression(guard, current)?;

        let Some(current) = guard_value.block else {
            panic!(
                "lowering contract violation: UnsupportedExpression {value:?}",
                value = guard
            );
        };

        let Some(condition) = guard_value.value else {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = guard
            );
        };

        let body = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        self.set_terminator(
            current,
            Self::retained_source(source),
            MirTerminatorKind::Branch {
                condition,
                then_edge: MirEdge::new(body, []),
                else_edge: MirEdge::new(unmatched, []),
            },
        )?;

        Ok(body)
    }

    pub(super) fn materialize_match_subject(
        &mut self,
        expression: BoundExpressionId,
        mut subject: LoweredExpression,
    ) -> Result<LoweredExpression, LoweringError> {
        if matches!(subject.value, Some(MirOperand::Value(_))) {
            // Observed bindings resolve through the checked temporary's identity, not a synthetic copy.
            subject = self.materialize_for_later_evaluation(expression, subject)?;
        }

        subject.value = subject.value.map(|value| match value {
            MirOperand::Move(place) => MirOperand::Copy(place),
            value => value,
        });

        Ok(subject)
    }
}
