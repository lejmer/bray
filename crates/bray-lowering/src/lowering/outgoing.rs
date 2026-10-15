use super::LoweringError;
use super::lowerer::Lowerer;
use bray_bound_tree::BoundExpressionId;
use bray_ir::{MirBlockId, MirOperand, MirOperationKind, MirSourceAnchor};
use bray_symbols::TypeId;

impl Lowerer<'_> {
    pub(in crate::lowering) fn admit_outgoing_owner(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        ty: TypeId,
    ) -> Result<MirBlockId, LoweringError> {
        if self.input.cleanup_type(ty).is_some_and(|shape| {
            shape.cleanup() == bray_bound_tree::AsyncStorageCleanupRequirement::None
        }) {
            return Ok(block);
        }

        self.admit_outgoing(
            expression,
            block,
            source,
            bray_ir::MirOutgoingOwner::Value(ty),
        )
    }

    fn admit_outgoing(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        owner: bray_ir::MirOutgoingOwner,
    ) -> Result<MirBlockId, LoweringError> {
        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::AdmitOutgoing {
                owner,
                runtime: self
                    .runtime_reference(bray_runtime_interface::RuntimeAbiRole::OutgoingAdmission),
            },
            None,
        )?;

        let unit = self.representation_type(bray_compiler_known::RepresentationRole::Unit)?;

        let (block, _) = self.finish_typed_call_panic_check(
            expression,
            block,
            source,
            &self.unit_operand(unit),
            unit,
            None,
        )?;

        Ok(block)
    }

    pub(in crate::lowering) fn discharge_outgoing_owner(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        ty: TypeId,
    ) -> Result<(), LoweringError> {
        if self.input.cleanup_type(ty).is_some_and(|shape| {
            shape.cleanup() == bray_bound_tree::AsyncStorageCleanupRequirement::None
        }) {
            return Ok(());
        }

        self.discharge_outgoing(block, source, bray_ir::MirOutgoingOwner::Value(ty))
    }

    fn discharge_outgoing(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        owner: bray_ir::MirOutgoingOwner,
    ) -> Result<(), LoweringError> {
        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::DischargeOutgoing {
                owner,
                runtime: self
                    .runtime_reference(bray_runtime_interface::RuntimeAbiRole::OutgoingDischarge),
            },
            None,
        )?;

        Ok(())
    }

    pub(super) fn admit_scoped_outgoing(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        owner: bray_ir::MirOutgoingOwner,
    ) -> Result<(MirBlockId, bray_ir::MirPlace), LoweringError> {
        let block = self.admit_outgoing(expression, block, source, owner)?;
        let guard = self.register_scoped_outgoing(block, source, owner)?;

        Ok((block, guard))
    }

    pub(super) fn discharge_scoped_outgoing(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        owner: bray_ir::MirOutgoingOwner,
        guard: &bray_ir::MirPlace,
    ) -> Result<MirBlockId, LoweringError> {
        self.guarded_cleanup_region(
            block,
            source,
            Self::retained_place(guard),
            Some(Self::retained_place(guard)),
            None,
            |lowerer, block, guard, value| {
                lowerer.push_operation(
                    block,
                    Self::retained_source(source),
                    MirOperationKind::Store {
                        kind: bray_ir::MirStoreKind::Assign,
                        value: MirOperand::Immediate {
                            value: bray_ir::MirImmediateValue::Boolean(false),
                            ty: guard.ty(),
                        },
                        destination: guard,
                    },
                    None,
                )?;

                lowerer.discharge_outgoing(block, source, owner)?;

                Ok((block, value))
            },
        )
        .map(|(block, _)| block)
    }
}
