use bray_bound_tree::BoundExpressionId;
use bray_compiler_known::{ImplementationHook, RepresentationRole};
use bray_ir::{
    MirBinaryOperator, MirBlockId, MirMemoryOperation, MirOperationKind, MirSourceAnchor,
};

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum SequenceOperationKind {
    Length,
    IsEmpty,
}

pub(super) const fn sequence_operation_kind(
    hook: ImplementationHook,
) -> Option<SequenceOperationKind> {
    match hook {
        ImplementationHook::SequenceLength => Some(SequenceOperationKind::Length),
        ImplementationHook::SequenceIsEmpty => Some(SequenceOperationKind::IsEmpty),
        _ => None,
    }
}

impl Lowerer<'_> {
    pub(super) fn lower_sequence_call(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
        source: MirSourceAnchor,
        selection: &bray_bound_tree::SelectedCall,
        kind: SequenceOperationKind,
    ) -> Result<LoweredExpression, LoweringError> {
        let receiver = selection
            .receiver()
            .filter(|_| selection.arguments().is_empty())
            .ok_or(LoweringError::MissingSemanticSelection(id))?;

        let (lowered, operand_type) = self.lower_call_receiver(receiver, current)?;

        let Some(current) = lowered.block else {
            return Ok(lowered);
        };

        let Some(receiver) = lowered.value else {
            return Err(LoweringError::MissingOperationResult(receiver.expression()));
        };

        let usize_type = self.representation_type(RepresentationRole::ScalarUsize)?;

        let length = self.push_typed_value_operation(
            id,
            current,
            Self::retained_source(&source),
            MirOperationKind::Memory(MirMemoryOperation::new(
                bray_bound_tree::CheckedMemoryOperationKind::SequenceLength,
                [receiver],
                [operand_type],
                Some(usize_type),
            )),
            usize_type,
        )?;

        if kind == SequenceOperationKind::Length {
            return Ok(LoweredExpression::continuing(current, Some(length), source));
        }

        let zero = self.integer_operand(usize_type, 0)?;
        let result_type = self.expression_type(id)?;

        let empty = self.push_typed_value_operation(
            id,
            current,
            Self::retained_source(&source),
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Equal,
                left: length,
                right: zero,
            },
            result_type,
        )?;

        Ok(LoweredExpression::continuing(current, Some(empty), source))
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::ImplementationHook;

    use super::{SequenceOperationKind, sequence_operation_kind};

    #[test]
    fn sequence_hooks_map_to_sequence_operations() {
        assert_eq!(
            sequence_operation_kind(ImplementationHook::SequenceLength),
            Some(SequenceOperationKind::Length)
        );

        assert_eq!(
            sequence_operation_kind(ImplementationHook::SequenceIsEmpty),
            Some(SequenceOperationKind::IsEmpty)
        );

        assert_eq!(sequence_operation_kind(ImplementationHook::AddressOf), None);
    }
}
