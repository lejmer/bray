use bray_bound_tree::BoundFutureConstruction;
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirFrameState, MirGeneratedLifecycleRole, MirOperand, MirOperationKind, MirPlace,
    MirRunResultVariants, MirSourceAnchor, MirUnitBuildError,
};
use bray_symbols::{BorrowKind, TypeData, TypeId};

use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn prepare_cleanup_await(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        role: MirGeneratedLifecycleRole,
        place: &MirPlace,
        owner: Option<crate::cleanup_await::CleanupOwner>,
    ) -> Result<
        (
            MirBlockId,
            Option<MirBlockId>,
            crate::cleanup_await::CleanupAwait,
            TypeId,
        ),
        LoweringError,
    > {
        let ty = place.ty();

        match owner {
            Some(crate::cleanup_await::CleanupOwner::Future { entry, completion }) => {
                let (operand, completion) = if entry == bray_ir::MirFrameEntry::CaptureQuiescence {
                    (
                        MirOperand::Copy(Self::retained_place(place)),
                        self.representation_type(RepresentationRole::Unit)?,
                    )
                } else {
                    (MirOperand::Move(Self::retained_place(place)), completion)
                };

                return Ok((
                    block,
                    None,
                    crate::cleanup_await::CleanupAwait::Frame(operand, entry),
                    completion,
                ));
            }
            Some(crate::cleanup_await::CleanupOwner::Task { completion }) => {
                return Ok((
                    block,
                    None,
                    crate::cleanup_await::CleanupAwait::Task(MirOperand::Copy(
                        Self::retained_place(place),
                    )),
                    completion,
                ));
            }
            None => {}
        }

        let receiver = self.input.semantic_values().intern_type(TypeData::Borrow {
            kind: BorrowKind::Mutable,
            target: ty,
        })?;

        let borrow = self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Borrow {
                kind: BorrowKind::Mutable,
                place: Self::retained_place(place),
            },
            Some(receiver),
        )?;

        let receiver = MirOperand::Value(borrow.result().ok_or(
            MirUnitBuildError::MissingOperationResult(borrow.operation()),
        )?);

        let completion = self.representation_type(RepresentationRole::Unit)?;
        let future = self.unary_representation_type(RepresentationRole::Future, completion)?;
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;

        let (block, rejected, future) = crate::cleanup_await::create_lifecycle_frame(
            &mut self.builder,
            block,
            source,
            role,
            ty,
            receiver,
            BoundFutureConstruction::new(completion, future),
            boolean,
        )?;

        Ok((
            block,
            Some(rejected),
            crate::cleanup_await::CleanupAwait::Frame(
                MirOperand::Move(future),
                bray_ir::MirFrameEntry::Body,
            ),
            completion,
        ))
    }

    pub(in crate::lowering::cleanup) fn await_cleanup_frame(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        awaited: crate::cleanup_await::CleanupAwait,
        completion: TypeId,
    ) -> Result<(MirBlockId, MirPlace, MirRunResultVariants), LoweringError> {
        let result = self.unary_representation_type(RepresentationRole::RunResult, completion)?;
        let representation = self.run_result_representation()?;

        let variants = MirRunResultVariants::new(
            representation.completed_variant,
            representation.panicked_variant,
            representation.cancelled_variant,
        );

        let state = self.next_frame_state()?;

        let (resume, result) = crate::cleanup_await::await_cleanup(
            &mut self.builder,
            block,
            source,
            state,
            awaited,
            result,
            variants,
        )?;

        self.retain_cleanup_state(state, resume)?;

        Ok((resume, result, variants))
    }

    pub(super) fn retain_cleanup_state(
        &mut self,
        state: bray_ir::MirFrameStateId,
        resume: MirBlockId,
    ) -> Result<(), LoweringError> {
        let mut storages =
            self.retained_storages(self.input.lowering_plans().frame_dependencies())?;

        storages.extend(self.cleanup_retained_storages.iter().copied());
        storages.sort_unstable();
        storages.dedup();

        self.frame_states.push(
            MirFrameState::new(state, resume, self.execution_lane_requirements(), storages)
                .with_affinity(self.frame_affinity()),
        );

        Ok(())
    }
}
