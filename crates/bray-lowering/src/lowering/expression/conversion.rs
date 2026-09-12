use bray_bound_tree::{BoundCallResult, BoundExpressionId, ConversionTarget, SelectedConversion};
use bray_ir::{
    MirBlockId, MirCall, MirCallIntrinsic, MirCallTarget, MirCallableReference, MirOperand,
    MirOperationKind, MirSourceAnchor,
};
use bray_symbols::{CallableAbi, TypeId};

use super::super::LoweringError;
use super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering) fn convert_operand(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        operand: MirOperand,
        conversion: &SelectedConversion,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        match conversion.target() {
            ConversionTarget::Identity => Ok((current, operand)),
            ConversionTarget::NullablePresent => {
                let value = self.push_nullable_present(
                    expression,
                    current,
                    source,
                    operand,
                    conversion.target_type(),
                )?;

                Ok((current, value))
            }
            ConversionTarget::CallableContract
            | ConversionTarget::BuiltInScalar
            | ConversionTarget::CVariadicPromotion
            | ConversionTarget::Composite(_) => self
                .push_converted_value(
                    expression,
                    current,
                    source,
                    MirOperationKind::Convert {
                        operand,
                        conversion: conversion.clone(),
                    },
                    conversion.target_type(),
                )
                .map(|value| (current, value)),
            ConversionTarget::Trait {
                fulfillment,
                requirement,
                witness,
                ..
            } => self.push_checked_call(
                expression,
                current,
                source,
                MirCall::protocol(
                    MirCallTarget::Direct(MirCallableReference::new(
                        *fulfillment,
                        CallableAbi::Bray,
                    )),
                    BoundCallResult::Immediate(conversion.target_type()),
                    [operand],
                    [bray_bound_tree::SelectedImplementationWitness::new(
                        *requirement,
                        *witness,
                    )],
                ),
                conversion.target_type(),
            ),
            ConversionTarget::TraitConstraint {
                member, dispatch, ..
            } => self.push_checked_call(
                expression,
                current,
                source,
                MirCall::protocol(
                    MirCallTarget::Direct(MirCallableReference::new(*member, CallableAbi::Bray)),
                    BoundCallResult::Immediate(conversion.target_type()),
                    [operand],
                    [],
                )
                .with_trait_dispatch(*dispatch)
                .with_intrinsic(MirCallIntrinsic::Conversion(conversion.target_type())),
                conversion.target_type(),
            ),
        }
    }

    fn push_converted_value(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        operation: MirOperationKind,
        result_type: TypeId,
    ) -> Result<MirOperand, LoweringError> {
        let commit = self.push_operation(current, source, operation, Some(result_type))?;

        commit
            .result()
            .map(MirOperand::Value)
            .ok_or(LoweringError::MissingOperationResult(expression))
    }
}
