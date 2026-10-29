use bray_bound_tree::{
    BoundExpressionId, BoundStructuredExpression, BoundStructuredExpressionKind,
};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBinaryOperator, MirBlockId, MirOperand, MirOperationKind, MirPlace, MirSourceAnchor,
    MirStoreKind,
};

use crate::lowering::LoweringError;
use crate::lowering::block::LoweredExpression;
use crate::lowering::lowerer::{Lowerer, YieldTarget};

impl Lowerer<'_> {
    pub(in crate::lowering::expression) fn lower_generator_observation(
        &mut self,
        producer: BoundExpressionId,
        iteration: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let source = self.expression_source(producer);
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

        let every = self.materialize_synthetic_place(
            current,
            Self::retained_source(&source),
            self.boolean_operand(boolean, true),
            boolean,
        )?;

        let some = self.materialize_synthetic_place(
            current,
            Self::retained_source(&source),
            self.boolean_operand(boolean, false),
            boolean,
        )?;

        let folds = [every, some];

        self.generator_observations.insert(producer, folds.clone());

        self.yield_targets.push(YieldTarget::GeneratorObservation {
            syntax: self
                .input
                .unit()
                .view()
                .expression(producer)
                .expect("observed producer belongs to its unit")
                .origin()
                .source_anchor()
                .syntax(),
            folds,
        });

        let lowered = self.lower_expression(iteration, current);

        self.yield_targets.pop();

        let lowered = lowered?;

        Ok(LoweredExpression {
            block: lowered.block,
            value: None,
            source,
        })
    }

    pub(in crate::lowering::expression) fn lower_generator_fold(
        &mut self,
        id: BoundExpressionId,
        producer: BoundExpressionId,
        expression: &BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let [operand] = expression.operands() else {
            panic!("observed generator fold has its sole operand");
        };

        let lowered = self.lower_expression(*operand, current)?;

        let Some(current) = lowered.block else {
            return Ok(lowered);
        };

        let ordinal =
            usize::from(expression.kind() == BoundStructuredExpressionKind::BooleanAnyFold);

        let place = self
            .generator_observations
            .get(&producer)
            .expect("eager production precedes every generator fold")[ordinal]
            .clone();

        Ok(LoweredExpression::continuing(
            current,
            Some(MirOperand::Copy(place)),
            self.expression_source(id),
        ))
    }

    pub(in crate::lowering::expression) fn push_generator_observation(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: &MirSourceAnchor,
        folds: [MirPlace; 2],
        value: MirOperand,
    ) -> Result<(), LoweringError> {
        for (destination, operator) in folds
            .into_iter()
            .zip([MirBinaryOperator::BitwiseAnd, MirBinaryOperator::BitwiseOr])
        {
            let ty = destination.ty();

            let updated = self.push_typed_value_operation(
                expression,
                current,
                Self::retained_source(source),
                MirOperationKind::Binary {
                    operator,
                    left: MirOperand::Copy(destination.clone()),
                    right: Self::retained_operand(&value),
                },
                ty,
            )?;

            self.push_operation(
                current,
                Self::retained_source(source),
                MirOperationKind::Store {
                    kind: MirStoreKind::Assign,
                    destination,
                    value: updated,
                },
                None,
            )?;
        }

        Ok(())
    }
}
