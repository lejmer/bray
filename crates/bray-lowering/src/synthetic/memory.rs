use bray_bound_tree::{CheckedMemoryOperationKind, MemoryLayoutQueryKind};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirMemoryOperation, MirOperand, MirOperationKind, MirSourceAnchor,
    MirUnitBuildError, MirUnitBuilder,
};
use bray_symbols::TypeId;

use super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn push_memory_layout(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        element: TypeId,
    ) -> Result<[MirOperand; 2], C::Error> {
        let usize_type = self
            .context
            .representation_type(RepresentationRole::ScalarUsize)?;

        let query = |builder: &mut MirUnitBuilder, kind| -> Result<MirOperand, C::Error> {
            push_memory(
                builder,
                block,
                source,
                CheckedMemoryOperationKind::LayoutQuery { ty: element, kind },
                [],
                Some(usize_type),
            )
            .map_err(|cause| self.mir_error(source, cause))?
            .ok_or_else(|| SyntheticLoweringError::MissingTypeResult(element).into())
        };

        Ok([
            query(builder, MemoryLayoutQueryKind::Size)?,
            query(builder, MemoryLayoutQueryKind::Alignment)?,
        ])
    }
}

pub(super) fn push_memory(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    kind: CheckedMemoryOperationKind,
    operands: impl IntoIterator<Item = MirOperand>,
    result: Option<TypeId>,
) -> Result<Option<MirOperand>, MirUnitBuildError> {
    let operands: Vec<_> = operands.into_iter().collect();

    let types = operands
        .iter()
        .map(|operand| builder.operand_type(operand))
        .collect::<Result<Vec<_>, _>>()?;

    let operation = MirMemoryOperation::new(kind, operands, types, result);

    Ok(builder
        .push_operation(
            block,
            source.clone(),
            MirOperationKind::Memory(operation),
            result,
        )?
        .result()
        .map(MirOperand::Value))
}
