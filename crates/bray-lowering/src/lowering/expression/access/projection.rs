use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;
use crate::lowering::projection::static_projection_kind;
use bray_bound_tree::{BoundExpressionId, StorageIdentityId, StorageProjection};
use bray_ir::{MirBlockId, MirOperand, MirProjection, MirProjectionKind};
use bray_symbols::{TypeData, TypeId};

impl Lowerer<'_> {
    pub(super) fn append_entry_dereference(
        &self,
        identity: StorageIdentityId,
        reached_type: TypeId,
        has_no_explicit_projections: bool,
        projections: &mut Vec<MirProjection>,
    ) -> TypeId {
        let source_type = self.storage_identity_type(identity);

        if self.input.storage_plan().identity_type(identity).is_none()
            || has_no_explicit_projections && source_type == reached_type
        {
            return source_type;
        }

        let data = self.input.semantic_values().type_data(source_type);

        let TypeData::Borrow { target, .. } = data.as_ref() else {
            return source_type;
        };

        projections.push(MirProjection::new(
            MirProjectionKind::Dereference,
            source_type,
            *target,
        ));

        *target
    }

    pub(super) fn append_reached_dereference(
        &self,
        source_type: TypeId,
        reached_type: TypeId,
        projections: &mut Vec<MirProjection>,
    ) -> TypeId {
        if source_type == reached_type {
            return source_type;
        }

        let data = self.input.semantic_values().type_data(source_type);

        let TypeData::Borrow { target, .. } = data.as_ref() else {
            return source_type;
        };

        if *target != reached_type {
            return source_type;
        }

        projections.push(MirProjection::new(
            MirProjectionKind::Dereference,
            source_type,
            reached_type,
        ));

        reached_type
    }

    pub(super) fn projection_result_type(
        &self,
        identity: StorageIdentityId,
        projections: &[StorageProjection],
    ) -> TypeId {
        let plan = self.input.storage_plan();

        plan.access_at(identity, projections)
            .and_then(|access| plan.access(access))
            .map(bray_bound_tree::StorageAccess::reached_type)
            .unwrap_or_else(|| {
                panic!(
                    "lowering contract violation: MissingStorageIdentityRecord {value:?}",
                    value = identity
                )
            })
    }

    pub(super) fn lower_projection(
        &mut self,
        projection: StorageProjection,
        mut current: MirBlockId,
    ) -> Result<(MirBlockId, MirProjectionKind), LoweringError> {
        let kind = match static_projection_kind(projection) {
            Some(kind) => kind,
            None => match projection {
                StorageProjection::Element(selector) => {
                    let (continuation, selector) = self.lower_selector(selector, current)?;

                    current = continuation;

                    MirProjectionKind::Index(selector)
                }
                StorageProjection::SliceRange { start, end } => {
                    let (continuation, start) = self.lower_optional_selector(start, current)?;

                    let (continuation, end) = self.lower_optional_selector(end, continuation)?;

                    current = continuation;

                    MirProjectionKind::Slice { start, end }
                }
                _ => unreachable!("every static projection was handled above"),
            },
        };

        Ok((current, kind))
    }

    fn lower_optional_selector(
        &mut self,
        selector: Option<BoundExpressionId>,
        current: MirBlockId,
    ) -> Result<(MirBlockId, Option<MirOperand>), LoweringError> {
        match selector {
            Some(selector) => {
                let (current, value) = self.lower_selector(selector, current)?;

                Ok((current, Some(value)))
            }
            None => Ok((current, None)),
        }
    }

    fn lower_selector(
        &mut self,
        selector: BoundExpressionId,
        current: MirBlockId,
    ) -> Result<(MirBlockId, MirOperand), LoweringError> {
        let lowered = self.lower_expression(selector, current)?;
        let lowered = self.materialize_for_later_evaluation(selector, lowered)?;

        let Some(current) = lowered.block else {
            panic!(
                "lowering contract violation: UnsupportedExpression {value:?}",
                value = selector
            );
        };

        let Some(value) = lowered.value else {
            panic!(
                "lowering contract violation: MissingOperationResult {value:?}",
                value = selector
            );
        };

        let value = match value {
            // Index values are copied each time the already-established access path is used.
            MirOperand::Move(place) => MirOperand::Copy(place),
            value => value,
        };

        Ok((current, value))
    }
}
