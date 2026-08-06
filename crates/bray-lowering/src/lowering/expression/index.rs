use bray_bound_tree::{BoundExpression, BoundExpressionId, IndexTarget, SelectedOperation};
use bray_ir::{MirBlockId, MirCall, MirCallTarget, MirCallableReference, MirOperationKind};
use bray_symbols::CallableAbi;

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn lower_index(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let target = match self.selected_operation(id)? {
            SelectedOperation::Index { target, .. } => *target,
            _ => return Err(LoweringError::MissingSemanticSelection(id)),
        };

        match target {
            IndexTarget::ArrayElement
            | IndexTarget::SliceElement
            | IndexTarget::ArraySlice
            | IndexTarget::Slice => self.lower_storage_operand(id, current),
            IndexTarget::Custom {
                fulfillment,
                requirement,
                witness,
                ..
            } => self.lower_custom_index(
                id,
                current,
                fulfillment,
                None,
                Some((requirement, witness)),
            ),
            IndexTarget::TraitConstraint {
                member, dispatch, ..
            } => self.lower_custom_index(id, current, member, Some(dispatch), None),
        }
    }

    fn lower_custom_index(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        callable: bray_symbols::CallableInstanceData,
        dispatch: Option<bray_symbols::TraitConstraintDispatch>,
        witness: Option<(
            bray_symbols::ImplementationRequirementKey,
            bray_symbols::ImplementationInstanceId,
        )>,
    ) -> Result<LoweredExpression, LoweringError> {
        let operands = self
            .input
            .unit()
            .view()
            .expression(id)
            .and_then(|expression| match expression {
                BoundExpression::Structured(expression) => Some(expression.operands()),
                _ => None,
            })
            .ok_or_else(|| LoweringError::MissingBoundNode(id.into()))?
            .to_vec();

        let source = self.expression_source(id)?;
        let mut block = current;
        let mut arguments = Vec::with_capacity(operands.len());

        for operand in operands {
            let lowered = self.lower_expression(operand, block)?;

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            let Some(value) = lowered.value else {
                return Err(LoweringError::MissingOperationResult(operand));
            };

            block = continuation;
            arguments.push(value);
        }

        let witnesses = witness.into_iter().map(|(requirement, witness)| {
            bray_bound_tree::SelectedImplementationWitness::new(requirement, witness)
        });

        let mut call = MirCall::protocol(
            MirCallTarget::Direct(MirCallableReference::new(callable, CallableAbi::Bray)),
            bray_bound_tree::BoundCallResult::Immediate(self.expression_type(id)?),
            arguments,
            witnesses,
        );

        if let Some(dispatch) = dispatch {
            call = call.with_trait_dispatch(dispatch);
        }

        let value = self.push_value_operation(
            id,
            block,
            Self::retained_source(&source),
            MirOperationKind::Call(call),
        )?;

        Ok(LoweredExpression::continuing(block, Some(value), source))
    }
}
