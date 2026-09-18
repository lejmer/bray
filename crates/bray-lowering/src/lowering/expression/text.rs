use bray_bound_tree::{BoundExpressionId, BoundOperator, SelectedArgument};
use bray_compiler_known::ImplementationHook;
use bray_ir::{
    MirBlockId, MirOperand, MirOperationKind, MirSourceAnchor, MirTextOperation,
    MirTextOperationKind, MirUnaryOperator,
};

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

pub(super) const fn text_operation_kind(hook: ImplementationHook) -> Option<MirTextOperationKind> {
    match hook {
        ImplementationHook::StringScalarCount => Some(MirTextOperationKind::ScalarCount),
        ImplementationHook::StringIsEmpty => Some(MirTextOperationKind::IsEmpty),
        ImplementationHook::StringEquals => Some(MirTextOperationKind::Equals),
        ImplementationHook::StringScalarAt => Some(MirTextOperationKind::ScalarAt),
        ImplementationHook::StringScalarSlice => Some(MirTextOperationKind::ScalarSlice),
        ImplementationHook::StringUtf8 => Some(MirTextOperationKind::Utf8),
        ImplementationHook::StringFromUtf8 => Some(MirTextOperationKind::FromUtf8),
        ImplementationHook::CharacterScalarValue => {
            Some(MirTextOperationKind::CharacterScalarValue)
        }
        ImplementationHook::CharacterFromScalarValue => {
            Some(MirTextOperationKind::CharacterFromScalarValue)
        }
        ImplementationHook::CharacterUtf8Length => Some(MirTextOperationKind::CharacterUtf8Length),
        ImplementationHook::CharacterUtf8Byte => Some(MirTextOperationKind::CharacterUtf8Byte),
        ImplementationHook::CharacterIsAlphabetic => {
            Some(MirTextOperationKind::CharacterIsAlphabetic)
        }
        ImplementationHook::CharacterIsNumeric => Some(MirTextOperationKind::CharacterIsNumeric),
        ImplementationHook::CharacterIsWhitespace => {
            Some(MirTextOperationKind::CharacterIsWhitespace)
        }
        _ => None,
    }
}
impl Lowerer<'_> {
    pub(super) fn lower_string_equality(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        operator: BoundOperator,
        operand_type: bray_symbols::TypeId,
        operands: [MirOperand; 2],
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let result_type = self.expression_type(id);

        let equal = self.push_typed_value_operation(
            id,
            current,
            Self::retained_source(&source),
            MirOperationKind::Text(MirTextOperation::new(
                MirTextOperationKind::Equals,
                operands,
                [operand_type, operand_type],
                Some(result_type),
            )),
            result_type,
        )?;

        if operator == BoundOperator::Equal {
            return Ok((current, equal));
        }

        let not_equal = self.push_typed_value_operation(
            id,
            current,
            Self::retained_source(&source),
            MirOperationKind::Unary {
                operator: MirUnaryOperator::Not,
                operand: equal,
            },
            result_type,
        )?;

        Ok((current, not_equal))
    }

    pub(super) fn lower_text_call(
        &mut self,
        id: BoundExpressionId,
        mut current: MirBlockId,
        source: MirSourceAnchor,
        selection: &bray_bound_tree::SelectedCall,
        kind: MirTextOperationKind,
    ) -> Result<LoweredExpression, LoweringError> {
        let capacity = selection.arguments().len() + usize::from(selection.receiver().is_some());
        let mut operands = Vec::with_capacity(capacity);
        let mut operand_types = Vec::with_capacity(capacity);

        if let Some(receiver) = selection.receiver() {
            let (lowered, operand_type) = self.lower_call_receiver(receiver, current)?;

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            current = continuation;

            let Some(operand) = lowered.value else {
                panic!(
                    "lowering contract violation: MissingOperationResult {value:?}",
                    value = receiver.expression()
                );
            };

            operands.push(operand);
            operand_types.push(operand_type);
        }

        for argument in selection.arguments() {
            let SelectedArgument::Explicit {
                expression,
                conversion,
                reborrow,
                ..
            } = argument
            else {
                panic!(
                    "lowering contract violation: MissingSemanticSelection {value:?}",
                    value = id
                );
            };

            let lowered =
                self.lower_text_operand(id, *expression, current, &source, conversion, *reborrow)?;

            let Some(continuation) = lowered.block else {
                return Ok(lowered);
            };

            current = continuation;

            let Some(operand) = lowered.value else {
                panic!(
                    "lowering contract violation: MissingOperationResult {value:?}",
                    value = *expression
                );
            };

            operands.push(operand);
            operand_types.push(conversion.target_type());
        }

        let result_type = self.expression_type(id);

        let commit = self.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::Text(MirTextOperation::new(
                kind,
                operands,
                operand_types,
                Some(result_type),
            )),
            Some(result_type),
        )?;

        let result = commit.result().map(MirOperand::Value).unwrap_or_else(|| {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = id
            )
        });

        Ok(LoweredExpression::continuing(current, Some(result), source))
    }

    fn lower_text_operand(
        &mut self,
        call: BoundExpressionId,
        expression: BoundExpressionId,
        current: MirBlockId,
        source: &MirSourceAnchor,
        conversion: &bray_bound_tree::SelectedConversion,
        reborrow: Option<bray_symbols::BorrowKind>,
    ) -> Result<LoweredExpression, LoweringError> {
        let lowered = self.lower_call_argument(expression, reborrow, current)?;

        let Some(current) = lowered.block else {
            return Ok(lowered);
        };

        let Some(operand) = lowered.value else {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = expression
            );
        };

        let (current, operand) = self.convert_operand(
            call,
            current,
            Self::retained_source(source),
            operand,
            conversion,
        )?;

        Ok(LoweredExpression::continuing(
            current,
            Some(operand),
            Self::retained_source(source),
        ))
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::ImplementationHook;
    use bray_ir::MirTextOperationKind;

    use super::text_operation_kind;

    #[test]
    fn recognized_text_hooks_map_to_text_operations() {
        let cases = [
            (
                ImplementationHook::StringScalarCount,
                MirTextOperationKind::ScalarCount,
            ),
            (
                ImplementationHook::StringIsEmpty,
                MirTextOperationKind::IsEmpty,
            ),
            (
                ImplementationHook::StringEquals,
                MirTextOperationKind::Equals,
            ),
            (
                ImplementationHook::StringScalarAt,
                MirTextOperationKind::ScalarAt,
            ),
            (
                ImplementationHook::StringScalarSlice,
                MirTextOperationKind::ScalarSlice,
            ),
            (ImplementationHook::StringUtf8, MirTextOperationKind::Utf8),
            (
                ImplementationHook::StringFromUtf8,
                MirTextOperationKind::FromUtf8,
            ),
            (
                ImplementationHook::CharacterScalarValue,
                MirTextOperationKind::CharacterScalarValue,
            ),
            (
                ImplementationHook::CharacterFromScalarValue,
                MirTextOperationKind::CharacterFromScalarValue,
            ),
            (
                ImplementationHook::CharacterUtf8Length,
                MirTextOperationKind::CharacterUtf8Length,
            ),
            (
                ImplementationHook::CharacterUtf8Byte,
                MirTextOperationKind::CharacterUtf8Byte,
            ),
            (
                ImplementationHook::CharacterIsAlphabetic,
                MirTextOperationKind::CharacterIsAlphabetic,
            ),
            (
                ImplementationHook::CharacterIsNumeric,
                MirTextOperationKind::CharacterIsNumeric,
            ),
            (
                ImplementationHook::CharacterIsWhitespace,
                MirTextOperationKind::CharacterIsWhitespace,
            ),
        ];

        for (hook, expected) in cases {
            assert_eq!(text_operation_kind(hook), Some(expected));
        }

        assert_eq!(text_operation_kind(ImplementationHook::AddressOf), None);
    }
}
