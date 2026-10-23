use bray_bound_tree::{BoundCallResult, BoundExpressionId, ConversionTarget, SelectedConversion};
use bray_ir::{
    MirBlockId, MirCall, MirCallIntrinsic, MirCallTarget, MirCallableReference, MirOperand,
    MirOperationKind, MirSourceAnchor,
};
use bray_symbols::CallableAbi;

use crate::lowering::{LoweringError, lowerer::Lowerer};

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
            ConversionTarget::Composite(children)
                if self.conversion_needs_control_flow(conversion) =>
            {
                let retained = self.input_temporaries.len();

                let result = self.convert_composite_operand(
                    expression, current, &source, operand, conversion, children,
                );

                self.input_temporaries.truncate(retained);

                result
            }
            ConversionTarget::CallableContract
            | ConversionTarget::BuiltInScalar
            | ConversionTarget::CVariadicPromotion
            | ConversionTarget::Composite(_) => self
                .push_typed_value_operation(
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

    fn conversion_needs_control_flow(&self, conversion: &SelectedConversion) -> bool {
        let mut pending = vec![conversion];

        while let Some(conversion) = pending.pop() {
            match conversion.target() {
                ConversionTarget::Trait { .. } | ConversionTarget::TraitConstraint { .. } => {
                    return true;
                }
                ConversionTarget::Composite(children) => {
                    if matches!(
                        self.input
                            .semantic_values()
                            .type_data(conversion.source_type())
                            .as_ref(),
                        bray_symbols::TypeData::Array { .. } | bray_symbols::TypeData::Nullable(_)
                    ) {
                        return true;
                    }

                    pending.extend(children.iter());
                }
                ConversionTarget::Identity
                | ConversionTarget::CallableContract
                | ConversionTarget::NullablePresent
                | ConversionTarget::BuiltInScalar
                | ConversionTarget::CVariadicPromotion => {}
            }
        }

        false
    }
}
