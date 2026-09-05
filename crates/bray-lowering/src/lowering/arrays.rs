use bray_bound_tree::{CheckedMemoryOperationKind, StorageAccessId, StorageCleanupProjectionKind};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBinaryOperator, MirBlockId, MirCleanupPhase, MirEdge, MirMemoryOperation, MirOperand,
    MirOperationKind, MirPlace, MirProjection, MirProjectionKind, MirSourceAnchor, MirStorageKind,
    MirStoreKind, MirTerminatorKind, MirUnitBuildError, MirValueId,
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
                            .ok_or(LoweringError::UnsupportedStorageAccess(access))?;

                    (block, value) = self.push_guarded_cleanup(
                        block,
                        source,
                        phase,
                        Self::retained_place(&place),
                        Some(guard),
                        part.plan.release(),
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
                        .ok_or(LoweringError::UnsupportedStorageAccess(access))?;

                    let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

                    let (guard, _) =
                        part_guard_for_place(release, boolean, &self.ownership_place(&place))
                            .ok_or(LoweringError::UnsupportedStorageAccess(access))?;

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
                        .ok_or(LoweringError::UnsupportedStorageAccess(access))?;

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

        let counter = self.builder.push_storage(
            Self::retained_source(source),
            MirStorageKind::Local,
            usize_type,
        )?;

        let counter = MirPlace::new(counter, [], usize_type);

        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: Self::retained_place(&counter),
                value: length,
            },
            None,
        )?;

        let kind = self.builder.block_kind(block)?;

        let condition = self
            .builder
            .push_block(Self::retained_source(source), kind)?;

        let body = self
            .builder
            .push_block(Self::retained_source(source), kind)?;

        let continuation = self
            .builder
            .push_block(Self::retained_source(source), kind)?;

        let forwarded = value.map(|(value, ty)| (MirOperand::Value(value), ty));
        let condition_value = self.cleanup_parameter(condition, source, forwarded.as_ref())?;
        let body_value = self.cleanup_parameter(body, source, forwarded.as_ref())?;

        let continuation_value =
            self.cleanup_parameter(continuation, source, forwarded.as_ref())?;

        self.set_terminator(
            block,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(
                condition,
                value.map(|(value, _)| MirOperand::Value(value)),
            )),
        )?;

        let zero = self.integer_operand(usize_type, 0)?;

        let nonempty = self.push_cleanup_value(
            condition,
            source,
            MirOperationKind::Binary {
                operator: MirBinaryOperator::GreaterThan,
                left: MirOperand::Copy(Self::retained_place(&counter)),
                right: zero,
            },
            boolean,
        )?;

        self.set_terminator(
            condition,
            Self::retained_source(source),
            MirTerminatorKind::Branch {
                condition: nonempty,
                then_edge: MirEdge::new(body, condition_value.map(MirOperand::Value)),
                else_edge: MirEdge::new(continuation, condition_value.map(MirOperand::Value)),
            },
        )?;

        let one = self.integer_operand(usize_type, 1)?;

        let index = self.push_cleanup_value(
            body,
            source,
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Subtract,
                left: MirOperand::Copy(Self::retained_place(&counter)),
                right: one,
            },
            usize_type,
        )?;

        self.push_operation(
            body,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: MirStoreKind::Assign,
                destination: Self::retained_place(&counter),
                value: index,
            },
            None,
        )?;

        let element_place = MirPlace::new(
            place.storage(),
            place
                .projections()
                .iter()
                .cloned()
                .chain([MirProjection::new(
                    MirProjectionKind::Index(MirOperand::Copy(counter)),
                    place.ty(),
                    element,
                )]),
            element,
        );

        let (completed, completed_value) = self.push_part_cleanup(
            body,
            source,
            phase,
            access,
            element_place,
            parts,
            depth + 1,
            body_value.zip(value.map(|(_, ty)| ty)),
        )?;

        self.set_terminator(
            completed,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(
                condition,
                completed_value.map(|(value, _)| MirOperand::Value(value)),
            )),
        )?;

        Ok((
            continuation,
            continuation_value.zip(value.map(|(_, ty)| ty)),
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

        Ok(MirOperand::Value(commit.result().ok_or(
            MirUnitBuildError::MissingOperationResult(commit.operation()),
        )?))
    }
}
