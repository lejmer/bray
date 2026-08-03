use bray_bound_tree::{BoundExpression, BoundExpressionId, StorageIdentity};
use bray_ir::{MirOperand, MirOperationKind, MirStoreKind};

use super::super::super::LoweringError;
use super::super::super::block::LoweredExpression;
use super::super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn materialize_temporary(
        &mut self,
        expression: BoundExpressionId,
        lowered: LoweredExpression,
    ) -> Result<LoweredExpression, LoweringError> {
        let Some(current) = lowered.block else {
            return Ok(lowered);
        };

        let Some(value) = lowered.value else {
            return Ok(lowered);
        };

        let temporary =
            self.input
                .storage_plan()
                .identity_entries()
                .find_map(|(identity, model)| {
                    matches!(model, StorageIdentity::Temporary(owner) if owner == expression)
                        .then_some(identity)
                });

        let Some(temporary) = temporary else {
            return Ok(LoweredExpression::continuing(
                current,
                Some(value),
                lowered.source,
            ));
        };

        let requires_lifecycle_storage = self
            .input
            .async_facts()
            .scope_exits()
            .iter()
            .flat_map(bray_bound_tree::AsyncScopeExitPlan::lifecycle_resolution)
            .any(|access| self.input.storage_plan().root_identity(*access) == Some(temporary));

        if !requires_lifecycle_storage {
            return Ok(LoweredExpression::continuing(
                current,
                Some(value),
                lowered.source,
            ));
        }

        let ty = self.expression_type(expression)?;

        let origin = self
            .input
            .unit()
            .view()
            .expression(expression)
            .map(BoundExpression::origin)
            .ok_or_else(|| LoweringError::MissingBoundNode(expression.into()))?;

        let place = self.place_for_identity(temporary, ty, origin)?;

        if matches!(
            &value,
            MirOperand::Copy(existing) | MirOperand::Move(existing) if existing == &place
        ) {
            return Ok(LoweredExpression::continuing(
                current,
                Some(value),
                lowered.source,
            ));
        }

        self.builder.push_operation(
            current,
            Self::retained_source(&lowered.source),
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
            lowered.source,
        ))
    }
}
