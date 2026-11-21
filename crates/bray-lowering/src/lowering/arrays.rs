use std::ops::Range;
use std::sync::Arc;

use bray_bound_tree::{CheckedMemoryOperationKind, StorageAccessId, StorageCleanupProjectionKind};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirCleanupPhase, MirMemoryOperation, MirOperand, MirOperationKind, MirPlace,
    MirProjection, MirProjectionKind, MirSourceAnchor, MirValueId,
};
use bray_symbols::{BorrowKind, TypeData, TypeId};

use super::LoweringError;
use super::cleanup::CleanupStep;
use super::initialization::{InitializedPart, part_guard_for_place};
use super::lowerer::Lowerer;
use super::projection::static_cleanup_projection_kind;

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub(super) struct CleanupPartCursor {
    access: StorageAccessId,
    place: MirPlace,
    parts: Arc<[InitializedPart]>,
    range: Range<usize>,
    depth: usize,
    end: Option<MirPlace>,
}

impl CleanupPartCursor {
    pub(super) fn new(access: StorageAccessId, place: MirPlace, parts: Arc<[InitializedPart]>, depth: usize) -> Self {
        let range = 0..parts.len();

        Self { access, place, parts, range, depth, end: None }
    }

    pub(super) fn with_range(mut self, range: Range<usize>) -> Self {
        self.range = range;

        self
    }

    fn select(&self, place: MirPlace, range: Range<usize>, depth: usize) -> Self {
        Self { access: self.access, place, parts: Arc::clone(&self.parts), range, depth, end: None }
    }
}

impl Lowerer<'_> {
    pub(super) fn push_part_cleanup(
        &mut self,
        mut block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        mut cursor: CleanupPartCursor,
        mut value: Option<(MirValueId, TypeId)>,
        present: Option<&MirPlace>,
    ) -> Result<(MirBlockId, Option<(MirValueId, TypeId)>), LoweringError> {
        let access = cursor.access;
        let depth = cursor.depth;
        let occurrence = self.cleanup_occurrence;

        while let Some(part) = cursor.parts[cursor.range.clone()].first() {
            let place = &cursor.place;

            let Some(projection) = part.plan.projections().get(depth).copied() else {
                let active = match phase {
                    MirCleanupPhase::TaskCancellation => part.plan.phases().includes_cancellation(),
                    MirCleanupPhase::LifecycleResolution => part.plan.phases().includes_lifecycle(),
                };

                if active {
                    let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

                    let (guard, _) = part_guard_for_place(
                        part,
                        boolean,
                        &self.ownership_place(place),
                    )
                    .unwrap_or_else(|| {
                        panic!(
                            "lowering contract violation: UnsupportedStorageAccess {value:?}",
                            value = access
                        )
                    });

                    (block, value) = if let Some(present) = present {
                        self.guarded_cleanup_region(block, source, Self::retained_place(place), Some(guard), value,
                            |lowerer, reached, _, value| {
                                lowerer.set_boolean_guard(reached, source, present, true)?;

                                Ok((reached, value))
                            })?
                    } else {
                        let tail = cursor.range.start + 1..cursor.range.end;

                        self.with_cleanup_remainder(
                            || (!tail.is_empty()).then(|| CleanupStep::parts(occurrence,
                                cursor.select(Self::retained_place(place), tail, depth), source)),
                            |lowerer| lowerer.push_guarded_cleanup(
                                block, source, phase, Self::retained_place(place), Some(guard),
                                part.plan.release(), false, value,
                            ),
                        )?
                    };
                }

                cursor.range.start += 1;
                continue;
            };

            let count = cursor.parts[cursor.range.clone()]
                .iter()
                .take_while(|part| part.plan.projections().get(depth) == Some(&projection))
                .count();

            let group = cursor.range.start..cursor.range.start + count;
            let tail = group.end..cursor.range.end;

            if present.is_some() && matches!(projection.projection(), StorageCleanupProjectionKind::OwnedTarget(_)) {
                // The following release guard already records an owner duty.
                // Selecting cleanup must not acquire a backing-storage loan.
                cursor.range.start = group.end;
                continue;
            }

            let end = cursor.end.take();
            let place = &cursor.place;
            let next = group.end;

            (block, value) = self.with_cleanup_remainder(
                || (!tail.is_empty() && present.is_none()).then(|| CleanupStep::parts(occurrence,
                    cursor.select(Self::retained_place(place), tail, depth), source)),
                |lowerer| lowerer.push_part_cleanup_group(
                    block, source, phase, &cursor, group, projection, value, present, end,
                ),
            )?;

            cursor.range.start = next;
        }

        Ok((block, value))
    }

    #[expect(clippy::too_many_arguments, reason = "the current group retains its actual place and resumed range")]
    fn push_part_cleanup_group(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        cursor: &CleanupPartCursor,
        group: Range<usize>,
        projection: bray_bound_tree::StorageCleanupProjection,
        value: Option<(MirValueId, TypeId)>,
        present: Option<&MirPlace>,
        end: Option<MirPlace>,
    ) -> Result<(MirBlockId, Option<(MirValueId, TypeId)>), LoweringError> {
        let place = &cursor.place;
        let access = cursor.access;
        let depth = cursor.depth;

        match projection.projection() {
                StorageCleanupProjectionKind::OwnedTarget(call) => {
                    let release = cursor.parts[group.end..cursor.range.end]
                        .first()
                        .filter(|part| {
                            part.plan.release().is_some() && part.plan.projections().len() == depth
                        })
                        .unwrap_or_else(|| {
                            panic!(
                                "lowering contract violation: UnsupportedStorageAccess {value:?}",
                                value = access
                            )
                        });

                    let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

                    let (guard, _) = part_guard_for_place(
                        release,
                        boolean,
                        &self.ownership_place(place),
                    )
                    .unwrap_or_else(|| {
                        panic!(
                            "lowering contract violation: UnsupportedStorageAccess {value:?}",
                            value = access
                        )
                    });

                    self.guarded_cleanup_region(
                        block,
                        source,
                        Self::retained_place(place),
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
                                cursor.select(target, group, depth + 1),
                                value,
                                present,
                            )
                        },
                    )
                }
                StorageCleanupProjectionKind::Component(_)
                | StorageCleanupProjectionKind::UnionPayloadElement { .. } => {
                    let kind = static_cleanup_projection_kind(projection.projection())
                        .unwrap_or_else(|| {
                            panic!(
                                "lowering contract violation: UnsupportedStorageAccess {value:?}",
                                value = access
                            )
                        });

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
                        cursor.select(projected, group, depth + 1),
                        value,
                        present,
                    )
                }
                StorageCleanupProjectionKind::ArrayElements(_) => self.push_array_part_cleanup(
                    block,
                    source,
                    phase,
                    cursor.select(Self::retained_place(place), group, depth),
                    projection.result_type(),
                    value,
                    present,
                    end,
                ),
        }
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
        cursor: CleanupPartCursor,
        element: TypeId,
        value: Option<(MirValueId, TypeId)>,
        present: Option<&MirPlace>,
        end: Option<MirPlace>,
    ) -> Result<(MirBlockId, Option<(MirValueId, TypeId)>), LoweringError> {
        let usize_type = self.representation_type(RepresentationRole::ScalarUsize)?;
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

        let place = &cursor.place;

        let length = match end {
            Some(end) => MirOperand::Copy(end),
            None => self.sequence_length(block, source, place)?,
        };

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

        let occurrence = self.cleanup_occurrence;

        let (completed, completed_value) = self.with_cleanup_remainder(
            || {
                if present.is_some() { return None; }

                let mut remaining = cursor.clone();

                remaining.end = Some(Self::retained_place(&cleanup.counter));

                Some(CleanupStep::parts(occurrence, remaining, source))
            },
            |lowerer| lowerer.push_part_cleanup(
                cleanup.body, source, phase,
                cursor.select(element_place, cursor.range.clone(), cursor.depth + 1),
                cleanup.body_value.zip(value.map(|(_, ty)| ty)), present,
            ),
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
    pub(super) fn sequence_length(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: &MirPlace,
    ) -> Result<MirOperand, LoweringError> {
        let usize_type = self.representation_type(RepresentationRole::ScalarUsize)?;

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

        self.push_cleanup_value(
            block,
            source,
            MirOperationKind::Memory(Box::new(MirMemoryOperation::new(
                CheckedMemoryOperationKind::SequenceLength,
                [borrowed],
                [borrow_type],
                Some(usize_type),
            ))),
            usize_type,
        )
    }

    fn push_cleanup_value(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        kind: MirOperationKind,
        ty: TypeId,
    ) -> Result<MirOperand, LoweringError> {
        let commit = self.push_operation(block, Self::retained_source(source), kind, Some(ty))?;

        Ok(MirOperand::Value(commit.result().expect(
            "value-producing MIR operation must publish a result",
        )))
    }
}
