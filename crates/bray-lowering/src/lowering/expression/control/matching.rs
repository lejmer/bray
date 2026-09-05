use bray_bound_tree::{BoundExpressionId, BoundMatchExpression};
use bray_ir::{
    MirBlockId, MirBlockKind, MirEdge, MirOperand, MirOperationKind, MirPlace, MirStorageKind,
    MirStoreKind, MirTerminatorKind,
};

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

        let Some(current) = subject.block else {
            return Ok(subject);
        };

        let Some(subject) = subject.value else {
            return Err(LoweringError::MissingOperationResult(expression.subject()));
        };

        let source = self.source(expression.origin());
        let subject_type = self.expression_type(expression.subject())?;
        let subject = self.materialize_match_subject(subject, subject_type, current, &source)?;

        let (join, result, result_type) = self.push_result_join(id, expression.origin())?;

        let coverage = self
            .input
            .patterns()
            .match_coverage(id)
            .ok_or(LoweringError::UnsupportedExpression(id))?;

        let mut candidate = current;

        for (index, arm) in expression.arms().iter().copied().enumerate() {
            let ordinal = match u32::try_from(index) {
                Ok(ordinal) => ordinal,
                Err(_) => {
                    return Err(LoweringError::MatchArmOrdinalUnrepresentable {
                        expression: id,
                        ordinal: index,
                    });
                }
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

            self.lower_pattern_branch(
                arm.pattern(),
                Self::retained_operand(&subject),
                candidate,
                matched,
                next,
            )?;

            let body_entry = match arm.guard() {
                Some(guard) => self.lower_match_guard(guard, matched, next, &source)?,
                None => matched,
            };

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

        self.builder
            .set_terminator(candidate, Self::retained_source(&source), terminator)?;

        if !self.builder.has_incoming_edge(join)? {
            self.builder.set_terminator(
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
            return Err(LoweringError::UnsupportedExpression(guard));
        };

        let Some(condition) = guard_value.value else {
            return Err(LoweringError::MissingOperationResult(guard));
        };

        let body = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        self.builder.set_terminator(
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
        subject: MirOperand,
        subject_type: bray_symbols::TypeId,
        current: MirBlockId,
        source: &bray_ir::MirSourceAnchor,
    ) -> Result<MirOperand, LoweringError> {
        let subject = match subject {
            MirOperand::Move(place) => return Ok(MirOperand::Copy(place)),
            subject @ (MirOperand::Constant { .. }
            | MirOperand::Immediate { .. }
            | MirOperand::Copy(_)) => return Ok(subject),
            MirOperand::Value(value) => MirOperand::Value(value),
        };

        let storage = self.builder.push_storage(
            Self::retained_source(source),
            MirStorageKind::Temporary,
            subject_type,
        )?;

        let place = MirPlace::new(storage, [], subject_type);

        self.builder.push_operation(
            current,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: Self::retained_place(&place),
                value: subject,
            },
            None,
        )?;

        Ok(MirOperand::Copy(place))
    }
}
