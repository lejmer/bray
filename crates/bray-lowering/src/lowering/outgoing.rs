use super::LoweringError;
use super::lowerer::Lowerer;
use bray_bound_tree::BoundExpressionId;
use bray_ir::{MirBlockId, MirOperationKind, MirSourceAnchor};
use bray_symbols::TypeId;

impl Lowerer<'_> {
    pub(in crate::lowering) fn admit_outgoing_owner(
        &mut self,
        expression: BoundExpressionId,
        block: MirBlockId,
        source: &MirSourceAnchor,
        ty: TypeId,
    ) -> Result<MirBlockId, LoweringError> {
        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::AdmitOutgoing {
                ty,
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
        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::DischargeOutgoing {
                ty,
                runtime: self
                    .runtime_reference(bray_runtime_interface::RuntimeAbiRole::OutgoingDischarge),
            },
            None,
        )?;

        Ok(())
    }
}
