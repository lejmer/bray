use bray_bound_tree::{CheckedMemoryOperationKind, StorageAccessId, StorageCleanupProjectionKind};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirCleanupPhase, MirMemoryOperation, MirOperand, MirOperationKind, MirPlace,
    MirProjection, MirProjectionKind, MirSourceAnchor, MirValueId,
};
use bray_symbols::{BorrowKind, TypeData, TypeId};

use super::LoweringError;
use super::initialization::{InitializedPart, part_guard_for_place};
use super::lowerer::Lowerer;
use super::projection::static_cleanup_projection_kind;

impl Lowerer<'_> {
    #[expect(
        clippy::too_many_arguments,
        reason = "recursive cleanup retains its checked access, path depth, phase and pending exit value"
    )]
    pub(super) fn push_part_cleanup(
        &mut self,
        mut block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        access: StorageAccessId,
        place: MirPlace,
        mut parts: &[InitializedPart<'_>],
        depth: usize,
        mut value: Option<(MirValueId, TypeId)>,
    ) -> Result<(MirBlockId, Option<(MirValueId, TypeId)>), LoweringError> {
        while let Some(part) = parts.first() {
            let Some(projection) = part.plan.projections().get(depth).copied() else {
                let active = match phase {
                    MirCleanupPhase::TaskCancellation => part.plan.phases().includes_cancellation(),
                    MirCleanupPhase::LifecycleResolution => part.plan.phases().includes_lifecycle(),
                };

                if active {
                    let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

                    let (guard, _) =
                        part_guard_for_place(part, boolean, &self.ownership_place(&place))
                            .unwrap_or_else(|| panic!("lowering contract violation: UnsupportedStorageAccess {value:?}", value = access));

                    (block, value) = self.push_guarded_cleanup(
                        block,
                        source,
                        phase,
                        Self::retained_place(&place),
                        Some(guard),
                        part.plan.release(),
                        false,
                        value,
                    )?;
                }

                parts = &parts[1..];
                continue;
            };

            let count = parts
                .iter()
                .take_while(|part| part.plan.projections().get(depth) == Some(&projection))
                .count();

            let (group, remaining) = parts.split_at(count);

            (block, value) = match projection.projection() {
                StorageCleanupProjectionKind::OwnedTarget(call) => {
                    let release = remaining
                        .first()
                        .filter(|part| {
                            part.plan.release().is_some() && part.plan.projections().len() == depth
                        })
                        .unwrap_or_else(|| panic!("lowering contract violation: UnsupportedStorageAccess {value:?}", value = access));

                    let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

                    let (guard, _) =
                        part_guard_for_place(release, boolean, &self.ownership_place(&place))
                            .unwrap_or_else(|| panic!("lowering contract violation: UnsupportedStorageAccess {value:?}", value = access));

                    self.guarded_cleanup_region(
                        block,
                        source,
                        Self::retained_place(&place),
                        Some(guard),
                        value,
                        |lowerer, block, place, value| {
                            let target = lowerer.project_owned_target(
                                block,
                                source,
                                &place,
                                call,
                                projection.result_type(),
                            )?;

                            lowerer.push_part_cleanup(
                                block,
                                source,
                                phase,
                                access,
                                target,
                                group,
                                depth + 1,
                                value,
                            )
                        },
                    )?
                }
                StorageCleanupProjectionKind::Component(_)
                | StorageCleanupProjectionKind::UnionPayloadElement { .. } => {
                    let kind = static_cleanup_projection_kind(projection.projection())
                        .unwrap_or_else(|| panic!("lowering contract violation: UnsupportedStorageAccess {value:?}", value = access));

                    let projected = MirPlace::new(
                        place.storage(),
                        place
                            .projections()
                            .iter()
                            .cloned()
                            .chain([MirProjection::new(
                                kind,
                                projection.source_type(),
                                projection.result_type(),
                            )]),
                        projection.result_type(),
                    );

                    self.push_part_cleanup(
                        block,
                        source,
                        phase,
                        access,
                        projected,
                        group,
                        depth + 1,
                        value,
                    )?
                }
                StorageCleanupProjectionKind::ArrayElements(_) => self.push_array_part_cleanup(
                    block,
                    source,
                    phase,
                    access,
                    &place,
                    group,
                    depth,
                    projection.result_type(),
                    value,
                )?,
            };

            parts = remaining;
        }

        Ok((block, value))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "array traversal preserves the surrounding checked cleanup path and pending exit value"
    )]
    fn push_array_part_cleanup(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        access: StorageAccessId,
        place: &MirPlace,
        parts: &[InitializedPart<'_>],
        depth: usize,
        element: TypeId,
        value: Option<(MirValueId, TypeId)>,
    ) -> Result<(MirBlockId, Option<(MirValueId, TypeId)>), LoweringError> {
        let usize_type = self.representation_type(RepresentationRole::ScalarUsize)?;
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

        let borrow_type = self.input.semantic_values().intern_type(TypeData::Borrow {
            kind: BorrowKind::Shared,
            target: place.ty(),
        })?;

        let borrowed = self.push_cleanup_value(
            block,
            source,
            MirOperationKind::Borrow {
                kind: BorrowKind::Shared,
                place: Self::retained_place(place),
            },
            borrow_type,
        )?;

        let length = self.push_cleanup_value(
            block,
            source,
            MirOperationKind::Memory(MirMemoryOperation::new(
                CheckedMemoryOperationKind::SequenceLength,
                [borrowed],
                [borrow_type],
                Some(usize_type),
            )),
            usize_type,
        )?;

        let zero = crate::operand::integer_constant(self.input.semantic_values(), usize_type, 0)?;
        let one = crate::operand::integer_constant(self.input.semantic_values(), usize_type, 1)?;

        let cleanup = crate::cleanup_loop::ReverseCleanupLoop::new(
            &mut self.builder,
            block,
            source,
            length,
            boolean,
            [zero, one],
            value.map(|(value, _)| MirOperand::Value(value)),
        )?;

        // The indexed child shares the loop's counter storage and the parent's projection path.
        let element_place = MirPlace::new(
            place.storage(),
            place
                .projections()
                .iter()
                .cloned()
                .chain([MirProjection::new(
                    MirProjectionKind::Index(MirOperand::Copy(cleanup.counter.clone())),
                    place.ty(),
                    element,
                )]),
            element,
        );

        let (completed, completed_value) = self.push_part_cleanup(
            cleanup.body,
            source,
            phase,
            access,
            element_place,
            parts,
            depth + 1,
            cleanup.body_value.zip(value.map(|(_, ty)| ty)),
        )?;

        cleanup.close(
            &mut self.builder,
            completed,
            source,
            completed_value.map(|(value, _)| MirOperand::Value(value)),
        )?;

        Ok((
            cleanup.continuation,
            cleanup.continuation_value.zip(value.map(|(_, ty)| ty)),
        ))
    }
    fn push_cleanup_value(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        kind: MirOperationKind,
        ty: TypeId,
    ) -> Result<MirOperand, LoweringError> {
        let commit = self.push_operation(block, Self::retained_source(source), kind, Some(ty))?;

        Ok(MirOperand::Value(
            commit
                .result()
                .expect("value-producing MIR operation must publish a result"),
        ))
    }
}
