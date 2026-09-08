use bray_bound_tree::{
    BoundExpressionId, SelectedOperation, StorageAccessPurpose, StorageIdentity,
};
use bray_ir::{
    MirBlockId, MirOperand, MirOperationKind, MirPlace, MirProjection, MirProjectionKind,
};
use bray_symbols::{AnySymbolId, TypeData};

use super::super::LoweringError;
use super::super::block::LoweredExpression;
use super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn lower_member_access(
        &mut self,
        id: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        match self.input.semantic_selections().expression(id) {
            Some(bray_bound_tree::SemanticSelection::Operation(
                SelectedOperation::Construction(_),
            )) => self.lower_construction(id, current),
            Some(bray_bound_tree::SemanticSelection::Operation(SelectedOperation::Member(
                target,
            ))) => match target.member() {
                AnySymbolId::UnionVariant(_) => self.lower_construction(id, current),
                AnySymbolId::StructField(_) | AnySymbolId::UnionPayloadField(_) => {
                    self.lower_storage_operand(id, current)
                }
                _ => Err(LoweringError::UnsupportedExpression(id)),
            },
            None => self.lower_storage_operand(id, current),
            _ => Err(LoweringError::UnsupportedExpression(id)),
        }
    }

    pub(super) fn lower_storage_operand(
        &mut self,
        expression: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<LoweredExpression, LoweringError> {
        let expression_type = self.expression_type(expression)?;

        let decision = self
            .storage_decision_reaching(expression, expression_type, |purpose| {
                matches!(
                    purpose,
                    StorageAccessPurpose::Read
                        | StorageAccessPurpose::Copy
                        | StorageAccessPurpose::Move
                        | StorageAccessPurpose::ValueTransfer
                )
            })
            .or_else(|error| match error {
                LoweringError::MissingStorageAccess(_) => {
                    self.storage_decision_reaching(expression, expression_type, |purpose| {
                        matches!(
                            purpose,
                            StorageAccessPurpose::Member
                                | StorageAccessPurpose::Index
                                | StorageAccessPurpose::Slice
                                | StorageAccessPurpose::Projection
                        )
                    })
                }
                _ => Err(error),
            })?;

        let source = self.expression_source(expression)?;

        self.lower_access_place_with(
            expression,
            decision.access(),
            current,
            |lowerer, block, mut place| {
                let entry_borrow = lowerer
                    .input
                    .storage_plan()
                    .root_identity(decision.access())
                    .and_then(|identity| lowerer.input.storage_plan().identity(identity))
                    .is_some_and(StorageIdentity::is_parameter);

                if entry_borrow && place.projections().is_empty() {
                    let expression_type = lowerer.expression_type(expression)?;

                    let place_data = lowerer.input.semantic_values().type_data(place.ty())?;

                    if let TypeData::Borrow { kind, target } = place_data.as_ref()
                        && expression_type == place.ty()
                    {
                        let target = MirPlace::new(
                            place.storage(),
                            [MirProjection::new(
                                MirProjectionKind::Dereference,
                                place.ty(),
                                *target,
                            )],
                            *target,
                        );

                        let value = lowerer.push_value_operation(
                            expression,
                            block,
                            Self::retained_source(&source),
                            MirOperationKind::Borrow {
                                kind: *kind,
                                place: target,
                            },
                        )?;

                        return Ok(LoweredExpression::continuing(block, Some(value), source));
                    }

                    if let TypeData::Borrow { target, .. } = place_data.as_ref()
                        && expression_type == *target
                    {
                        place = MirPlace::new(
                            place.storage(),
                            [MirProjection::new(
                                MirProjectionKind::Dereference,
                                place.ty(),
                                *target,
                            )],
                            *target,
                        );
                    }
                }

                let borrowed = matches!(
                    lowerer
                        .input
                        .semantic_values()
                        .type_data(place.ty())?
                        .as_ref(),
                    TypeData::Borrow { .. }
                );

                let operand = match decision.purpose() {
                    StorageAccessPurpose::Move if borrowed => MirOperand::Copy(place),
                    StorageAccessPurpose::Move => MirOperand::Move(place),
                    StorageAccessPurpose::Read
                    | StorageAccessPurpose::Copy
                    | StorageAccessPurpose::ValueTransfer
                    | StorageAccessPurpose::Member
                    | StorageAccessPurpose::Index
                    | StorageAccessPurpose::Slice
                    | StorageAccessPurpose::Projection => MirOperand::Copy(place),
                    _ => {
                        return Err(LoweringError::UnsupportedStorageAccess(decision.access()));
                    }
                };

                Ok(LoweredExpression::continuing(block, Some(operand), source))
            },
        )
    }
}
