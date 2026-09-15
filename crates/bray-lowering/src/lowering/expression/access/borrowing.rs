use crate::lowering::LoweringError;
use crate::lowering::block::LoweredExpression;
use crate::lowering::lowerer::Lowerer;
use bray_bound_tree::{BoundExpressionId, SelectedReceiver, StorageAccessPurpose};
use bray_ir::{
    MirBlockId, MirOperand, MirOperationKind, MirPlace, MirProjection, MirProjectionKind,
};
use bray_symbols::{BorrowKind, ConstantValueKind, ReceiverMode, TypeData, TypeId};

impl Lowerer<'_> {
    pub(in crate::lowering::expression) fn lower_borrow(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundStructuredExpression,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let kind = expression
            .borrow_kind()
            .ok_or(LoweringError::UnsupportedExpression(id))?;

        let source = self.source(expression.origin());
        let result_type = self.expression_type(id)?;

        let result_data = self.input.semantic_values().type_data(result_type);

        let TypeData::Borrow {
            kind: result_kind,
            target,
        } = result_data.as_ref()
        else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        if *result_kind != kind {
            return Err(LoweringError::UnsupportedExpression(id));
        }

        let [operand] = expression.operands() else {
            return Err(LoweringError::UnsupportedExpression(id));
        };

        if let Some(value) = self.static_string_literal_borrow(*operand, kind, result_type) {
            return Ok(LoweredExpression::continuing(current, Some(value), source));
        }

        self.lower_storage_borrow(id, id, current, kind, *target, result_type, source)
    }

    pub(in crate::lowering::expression) fn lower_call_receiver(
        &mut self,
        receiver: &SelectedReceiver,
        current: MirBlockId,
    ) -> Result<(LoweredExpression, TypeId), LoweringError> {
        let target = receiver.target_type();

        let kind = match receiver.mode() {
            ReceiverMode::Shared => BorrowKind::Shared,
            ReceiverMode::Mutable => BorrowKind::Mutable,
            ReceiverMode::Consuming | ReceiverMode::ConsumingMutable => {
                return self
                    .lower_expression(receiver.expression(), current)
                    .map(|lowered| (lowered, target));
            }
        };

        let result_type = receiver.input_type(self.input.semantic_values())?;

        let source = self.expression_source(receiver.expression())?;

        self.lower_storage_borrow(
            receiver.expression(),
            receiver.expression(),
            current,
            kind,
            target,
            result_type,
            source,
        )
        .map(|lowered| (lowered, result_type))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "borrow lowering requires both source occurrences and the checked borrow types"
    )]
    fn lower_storage_borrow(
        &mut self,
        access_expression: BoundExpressionId,
        initialization_expression: BoundExpressionId,
        current: MirBlockId,
        kind: BorrowKind,
        target: TypeId,
        result_type: TypeId,
        source: bray_ir::MirSourceAnchor,
    ) -> Result<LoweredExpression, LoweringError> {
        let decision = self.storage_decision(access_expression, |purpose| {
            purpose == StorageAccessPurpose::Borrow(kind)
        })?;

        self.lower_materialized_access_place_with(
            initialization_expression,
            decision.access(),
            current,
            |lowerer, current, place| {
                let mut projections = place.projections().to_vec();
                let mut place_type = place.ty();

                let parameter_type = lowerer
                    .input
                    .storage_plan()
                    .root_identity(decision.access())
                    .and_then(|identity| lowerer.input.storage_plan().storage_type(identity));

                let parameter_borrow = if let Some(ty) = parameter_type {
                    let data = lowerer.input.semantic_values().type_data(ty);

                    match data.as_ref() {
                        TypeData::Borrow { target, .. } => Some((ty, *target)),
                        _ => None,
                    }
                } else {
                    None
                };

                let already_dereferenced = projections
                    .first()
                    .is_some_and(|projection| projection.kind() == &MirProjectionKind::Dereference);

                if let Some((parameter_type, reached_type)) = parameter_borrow
                    && !already_dereferenced
                {
                    projections.insert(
                        0,
                        MirProjection::new(
                            MirProjectionKind::Dereference,
                            parameter_type,
                            reached_type,
                        ),
                    );

                    place_type = reached_type;
                }

                place_type =
                    lowerer.append_reached_dereference(place_type, target, &mut projections);

                let place = MirPlace::new(place.storage(), projections, place_type);

                let commit = lowerer.push_operation(
                    current,
                    Self::retained_source(&source),
                    MirOperationKind::Borrow { kind, place },
                    Some(result_type),
                )?;

                let value = commit
                    .result()
                    .map(MirOperand::Value)
                    .ok_or(LoweringError::MissingOperationResult(access_expression))?;

                Ok(LoweredExpression::continuing(current, Some(value), source))
            },
        )
    }

    pub(in crate::lowering::expression) fn lower_implicit_shared_borrow(
        &mut self,
        parent: BoundExpressionId,
        operand: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        self.lower_implicit_borrow(parent, operand, current, BorrowKind::Shared)
    }

    pub(in crate::lowering::expression) fn lower_implicit_borrow(
        &mut self,
        parent: BoundExpressionId,
        operand: BoundExpressionId,
        current: MirBlockId,
        kind: BorrowKind,
    ) -> Result<LoweredExpression, LoweringError> {
        let target = self.expression_type(operand)?;

        let result_type = self
            .input
            .semantic_values()
            .intern_type(TypeData::Borrow { kind, target })?;

        let source = self.expression_source(operand)?;

        if let Some(value) = self.static_string_literal_borrow(operand, kind, result_type) {
            return Ok(LoweredExpression::continuing(current, Some(value), source));
        }

        self.lower_storage_borrow(operand, parent, current, kind, target, result_type, source)
    }

    fn static_string_literal_borrow(
        &self,
        expression: BoundExpressionId,
        kind: BorrowKind,
        result_type: TypeId,
    ) -> Option<MirOperand> {
        if kind != BorrowKind::Shared {
            return None;
        }

        let Some(value) = self.input.literal_values().expression(expression) else {
            return None;
        };

        let data = self.input.semantic_values().constant_value_data(value);

        let representation = self.input.semantic_values().type_data(result_type);

        let TypeData::Borrow { target, .. } = representation.as_ref() else {
            return None;
        };

        let ConstantValueKind::String(_) = data.kind() else {
            return None;
        };

        if data.ty() != *target {
            return None;
        }

        Some(MirOperand::Constant {
            value,
            ty: result_type,
        })
    }
}
