use bray_bound_tree::{
    BoundCallResult, BoundExpression, BoundExpressionId, BoundOperator,
    BoundStructuredExpressionKind, SemanticSelection, StorageIdentity,
};
use bray_compiler_known::ImplementationHook;
use bray_ir::{MirOperand, MirOperationKind, MirPlace, MirStorageKind, MirStoreKind};
use bray_symbols::{CallableAbi, TypeId};

use super::super::super::LoweringError;
use super::super::super::block::LoweredExpression;
use super::super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(in crate::lowering) fn later_evaluation_may_change_block(
        &self,
        expressions: impl IntoIterator<Item = BoundExpressionId>,
    ) -> Result<bool, LoweringError> {
        let mut pending = expressions.into_iter().collect::<Vec<_>>();

        while let Some(expression) = pending.pop() {
            let bound = self
                .input
                .unit()
                .view()
                .expression(expression)
                .ok_or_else(|| LoweringError::MissingBoundNode(expression.into()))?;

            let changes_block = match bound {
                BoundExpression::Binary(binary) => matches!(
                    binary.operator(),
                    BoundOperator::LogicalAnd | BoundOperator::LogicalOr
                ),
                BoundExpression::Structured(structured) => !matches!(
                    structured.kind(),
                    BoundStructuredExpressionKind::Unit
                        | BoundStructuredExpressionKind::Absence
                        | BoundStructuredExpressionKind::Tuple
                        | BoundStructuredExpressionKind::Array
                        | BoundStructuredExpressionKind::RepeatedArray
                        | BoundStructuredExpressionKind::Range
                        | BoundStructuredExpressionKind::Borrow
                ),
                BoundExpression::Block(_)
                | BoundExpression::Await(_)
                | BoundExpression::Assignment(_)
                | BoundExpression::ControlTransfer(_)
                | BoundExpression::For(_)
                | BoundExpression::Match(_)
                | BoundExpression::Generator(_) => true,
                _ => false,
            };

            // MIR values are block-local. Earlier operands must survive every later branch, not only a checked call.
            if changes_block {
                return Ok(true);
            }

            let may_check = match self.input.semantic_selections().expression(expression) {
                Some(SemanticSelection::Call(selection)) => {
                    selection.evaluates_defaults()
                        || (selection.abi() == CallableAbi::Bray
                            && matches!(
                                selection.resolution().result(),
                                BoundCallResult::Immediate(_)
                            )
                            && selection.implementation_hook().is_none())
                }
                Some(SemanticSelection::Operation(operation)) => {
                    operation.may_propagate_synchronous_panic()
                }
                Some(SemanticSelection::Iteration(selection)) => !matches!(
                    self.input
                        .available_compiler_known_symbols()
                        .symbol_implementation(selection.iterate().definition().symbol()),
                    Some(
                        ImplementationHook::RangeSharedIterate
                            | ImplementationHook::RangeMoveIterate
                    )
                ),
                Some(
                    SemanticSelection::Reference(_)
                    | SemanticSelection::CallableReference(_)
                    | SemanticSelection::StaticReference(_)
                    | SemanticSelection::Predicate(_)
                    | SemanticSelection::Propagation(_),
                )
                | None => false,
            };

            if may_check {
                return Ok(true);
            }

            pending.extend(bound.child_expressions());
        }

        Ok(false)
    }

    pub(super) fn materialize_temporary(
        &mut self,
        expression: BoundExpressionId,
        lowered: LoweredExpression,
    ) -> Result<LoweredExpression, LoweringError> {
        let ty = self.expression_type(expression)?;

        self.materialize_temporary_when(expression, lowered, ty, false)
    }

    pub(in crate::lowering) fn materialize_for_later_evaluation(
        &mut self,
        expression: BoundExpressionId,
        lowered: LoweredExpression,
    ) -> Result<LoweredExpression, LoweringError> {
        let ty = self.expression_type(expression)?;

        self.materialize_temporary_when(expression, lowered, ty, true)
    }

    pub(in crate::lowering) fn materialize_typed_for_later_evaluation(
        &mut self,
        expression: BoundExpressionId,
        lowered: LoweredExpression,
        ty: TypeId,
    ) -> Result<LoweredExpression, LoweringError> {
        self.materialize_temporary_when(expression, lowered, ty, true)
    }

    fn materialize_temporary_when(
        &mut self,
        expression: BoundExpressionId,
        lowered: LoweredExpression,
        ty: TypeId,
        required: bool,
    ) -> Result<LoweredExpression, LoweringError> {
        let Some(current) = lowered.block else {
            return Ok(lowered);
        };

        let Some(value) = lowered.value else {
            return Ok(lowered);
        };

        let storage = self.input.storage_plan();

        let temporary = storage
            .identity_entries()
            .find_map(|(identity, model)| {
                matches!(model, StorageIdentity::Temporary(owner) if owner == expression)
                    .then_some(identity)
                    .filter(|identity| storage.storage_type(*identity) == Some(ty))
            })
            .or_else(|| {
                // Transparent expressions retain their operand's checked storage identity.
                storage
                    .expression_plans(expression)
                    .filter(|plan| storage.is_root_access(plan.access()))
                    .filter_map(|plan| storage.root_identity(plan.access()))
                    .find(|identity| {
                        matches!(
                            storage.identity(*identity),
                            Some(StorageIdentity::Temporary(_))
                        ) && storage.storage_type(*identity) == Some(ty)
                    })
            });

        let Some(temporary) = temporary else {
            if required {
                return self.materialize_synthetic_temporary(current, lowered.source, value, ty);
            }

            return Ok(LoweredExpression::continuing(
                current,
                Some(value),
                lowered.source,
            ));
        };

        let requires_lifecycle_storage = self
            .input
            .requires_lifecycle_storage(temporary);

        if !required && !requires_lifecycle_storage {
            return Ok(LoweredExpression::continuing(
                current,
                Some(value),
                lowered.source,
            ));
        }

        let origin = self
            .input
            .unit()
            .view()
            .expression(expression)
            .map(BoundExpression::origin)
            .ok_or_else(|| LoweringError::MissingBoundNode(expression.into()))?;

        let place = self.place_for_identity(temporary, ty, origin)?;

        if value.reads_from(&place) {
            self.initialized_temporaries.insert(temporary);

            return Ok(LoweredExpression::continuing(
                current,
                Some(value),
                lowered.source,
            ));
        }

        self.push_operation(
            current,
            Self::retained_source(&lowered.source),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: place.clone(),
                value,
            },
            None,
        )?;

        self.initialized_temporaries.insert(temporary);

        Ok(LoweredExpression::continuing(
            current,
            Some(MirOperand::Move(place)),
            lowered.source,
        ))
    }

    pub(in crate::lowering) fn materialize_synthetic_temporary(
        &mut self,
        current: bray_ir::MirBlockId,
        source: bray_ir::MirSourceAnchor,
        value: MirOperand,
        ty: TypeId,
    ) -> Result<LoweredExpression, LoweringError> {
        let storage = self.builder.push_storage(
            Self::retained_source(&source),
            MirStorageKind::Temporary,
            ty,
        )?;

        let place = MirPlace::new(storage, [], ty);

        self.push_operation(
            current,
            Self::retained_source(&source),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: place.clone(),
                value,
            },
            None,
        )?;

        Ok(LoweredExpression::continuing(
            current,
            Some(MirOperand::Move(place)),
            source,
        ))
    }
}
