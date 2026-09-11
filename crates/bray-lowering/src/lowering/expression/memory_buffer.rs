use bray_bound_tree::{BoundExpressionId, CheckedMemoryOperationKind};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirMemoryOperation, MirOperand, MirOperationKind, MirPlace, MirProjectionKind,
    MirSourceAnchor, MirStorageKind, MirStoreKind,
};
use bray_symbols::{BorrowKind, TypeData, TypeId};

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn lower_raw_buffer_cleanup(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: MirSourceAnchor,
        kind: CheckedMemoryOperationKind,
        operands: Vec<MirOperand>,
    ) -> Result<LoweredExpression, LoweringError> {
        let (element, replaces) = match kind {
            CheckedMemoryOperationKind::RawBufferRelease { element } => (element, false),
            CheckedMemoryOperationKind::RawBufferReplace { element } => (element, true),
            _ => return Err(LoweringError::MissingSemanticSelection(expression)),
        };

        let mut places = operands
            .into_iter()
            .map(|operand| self.raw_buffer_place(block, &source, operand))
            .collect::<Result<Vec<_>, _>>()?;

        if places.len() != if replaces { 2 } else { 1 } {
            return Err(LoweringError::MissingSemanticSelection(expression));
        }

        let replacement = replaces.then(|| places.pop()).flatten();

        let destination = places
            .pop()
            .ok_or(LoweringError::MissingSemanticSelection(expression))?;

        let result_type = self.expression_type(expression)?;
        let unit = self.unit_operand(result_type);

        let retained = std::iter::once(destination.storage())
            .chain(replacement.as_ref().map(MirPlace::storage));

        self.push_cleanup_operation(
            block,
            source.clone(),
            MirOperationKind::Destroy(destination.clone()),
            retained,
        )?;

        let (completed, result) =
            self.finish_typed_call_panic_check(expression, block, &source, &unit, result_type)?;

        if let Some(replacement) = replacement {
            self.push_operation(
                completed,
                source.clone(),
                MirOperationKind::Store {
                    kind: MirStoreKind::Assign,
                    destination,
                    value: MirOperand::Move(replacement.clone()),
                },
                None,
            )?;

            self.reset_transferred_buffer(completed, &source, replacement, element)?;
        }

        Ok(LoweredExpression::continuing(
            completed,
            Some(result),
            source,
        ))
    }

    fn raw_buffer_place(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        operand: MirOperand,
    ) -> Result<MirPlace, LoweringError> {
        let ty = self.builder.operand_type(&operand)?;
        let data = self.input.semantic_values().type_data(ty)?;

        let TypeData::Borrow {
            kind: BorrowKind::Mutable,
            target,
        } = data.as_ref()
        else {
            return Err(LoweringError::SemanticValueUnavailable);
        };

        let storage = self
            .builder
            .push_storage(source.clone(), MirStorageKind::Temporary, ty)?;

        let place = MirPlace::new(storage, [], ty);

        self.push_operation(
            block,
            source.clone(),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: place.clone(),
                value: operand,
            },
            None,
        )?;

        Ok(place.project(MirProjectionKind::Dereference, *target))
    }

    fn reset_transferred_buffer(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        element: TypeId,
    ) -> Result<(), LoweringError> {
        let pointer = self.unary_representation_type(RepresentationRole::RawPointer, element)?;
        let integer = self.representation_type(RepresentationRole::ScalarUsize)?;
        let zero = crate::operand::integer_constant(self.input.semantic_values(), integer, 0)?;

        let null = self.push_operation(
            block,
            source.clone(),
            MirOperationKind::Memory(MirMemoryOperation::new(
                CheckedMemoryOperationKind::Null { pointee: element },
                [],
                [],
                Some(pointer),
            )),
            Some(pointer),
        )?;

        let null = null
            .result()
            .ok_or(bray_ir::MirUnitBuildError::MissingOperationResult(
                null.operation(),
            ))?;

        crate::raw_buffer::reset_raw_buffer(
            &mut self.builder,
            block,
            source,
            place,
            MirOperand::Value(null),
            zero,
        )?;

        Ok(())
    }
}
