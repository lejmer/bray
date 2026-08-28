use bray_bound_tree::StorageProjection;
use bray_ir::{MirProjection, MirProjectionKind};
use bray_symbols::{TypeData, TypeId};

use super::super::LoweringError;
use super::super::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn append_projection_dereferences(
        &self,
        projection: StorageProjection,
        mut source_type: TypeId,
        projections: &mut Vec<MirProjection>,
    ) -> Result<TypeId, LoweringError> {
        if projection == StorageProjection::OwnedTarget {
            return Ok(source_type);
        }

        loop {
            let data = self
                .input
                .semantic_values()
                .type_data(source_type)
                .map_err(|_| LoweringError::SemanticValueUnavailable)?;

            let TypeData::Borrow { target, .. } = data.as_ref() else {
                return Ok(source_type);
            };

            projections.push(MirProjection::new(
                MirProjectionKind::Dereference,
                source_type,
                *target,
            ));

            source_type = *target;
        }
    }
}
