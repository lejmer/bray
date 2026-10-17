use bray_bound_tree::LifecycleCallable;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCallableReference, MirCleanupPhase, MirEdge, MirOperand,
    MirOperationKind, MirPlace, MirSourceAnchor, MirTerminatorKind,
};

use crate::cleanup_outcome::CleanupOutcome;
use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;

impl Lowerer<'_> {
    pub(super) fn push_cleanup_action(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        place: bray_ir::MirPlace,
        release: Option<bray_bound_tree::StorageProtocolCall>,
        completed: bool,
    ) -> Result<MirBlockId, LoweringError> {
        let inherited = self
            .destruction_argument
            .as_ref()
            .filter(|(receiver, _)| {
                phase == MirCleanupPhase::LifecycleResolution
                    && release.is_none()
                    && matches!(
                        self.cleanup_occurrence,
                        Some(bray_bound_tree::BoundExecutionSite::Cleanup(_))
                    )
                    && self.storages.get(receiver) == Some(&place.storage())
            })
            .map(|(_, argument)| argument.clone());

        let Some(inherited) = inherited else {
            return self
                .push_ordinary_cleanup_action(block, source, phase, place, release, completed);
        };

        let abandoned = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let ordinary = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let continuation = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        self.set_terminator(
            block,
            Self::retained_source(source),
            MirTerminatorKind::Branch {
                condition: inherited.clone(),
                then_edge: MirEdge::new(abandoned, []),
                else_edge: MirEdge::new(ordinary, []),
            },
        )?;

        self.push_operation(
            abandoned,
            Self::retained_source(source),
            MirOperationKind::Destroy {
                place: Self::retained_place(&place),
                abandoned: Some(inherited),
            },
            None,
        )?;

        let abandoned = if let Some(outcome) = &self.cleanup_outcome {
            outcome.check(&mut self.builder, abandoned, source)?
        } else {
            self.check_ordinary_cleanup(abandoned, source, None, false, None)?
                .0
        };

        let ordinary =
            self.push_ordinary_cleanup_action(ordinary, source, phase, place, release, completed)?;

        for end in [abandoned, ordinary] {
            self.set_terminator(
                end,
                Self::retained_source(source),
                MirTerminatorKind::Goto(MirEdge::new(continuation, [])),
            )?;
        }

        Ok(continuation)
    }

    fn push_ordinary_cleanup_action(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        place: bray_ir::MirPlace,
        release: Option<bray_bound_tree::StorageProtocolCall>,
        completed: bool,
    ) -> Result<MirBlockId, LoweringError> {
        let owner = place.ty();
        let completed = completed || self.input.type_finalizer_is_complete(owner);

        if release.is_none()
            && phase == MirCleanupPhase::LifecycleResolution
            && self.input.unit_kind().protected_frame().is_some()
            && self.type_representation(owner)
                == Some(bray_compiler_known::RepresentationRole::Future)
        {
            return self.push_inactive_cleanup(block, source, place);
        }

        if release.is_none()
            && !completed
            && phase == MirCleanupPhase::LifecycleResolution
            && let Some(callable) = self.input.cleanup_type(owner).and_then(|ty| ty.finalizer())
            && callable.execution == bray_symbols::CallableExecution::Asynchronous
        {
            return self.push_async_finalizer(block, source, place, callable);
        }

        if let Some(release) = release {
            self.set_storage_initialized(block, source, &place, false)?;
            self.push_storage_protocol_call(block, source, &place, release, true)?;
        } else {
            self.push_operation(
                block,
                Self::retained_source(source),
                if completed && phase == MirCleanupPhase::LifecycleResolution {
                    MirOperationKind::Destroy {
                        place,
                        abandoned: None,
                    }
                } else {
                    MirOperationKind::Cleanup { phase, place }
                },
                None,
            )?;
        }

        if let Some(outcome) = &self.cleanup_outcome {
            Ok(outcome.check(&mut self.builder, block, source)?)
        } else {
            Ok(self
                .check_ordinary_cleanup(block, source, release.map(|_| owner), false, None)?
                .0)
        }
    }

    fn push_inactive_cleanup(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
    ) -> Result<MirBlockId, LoweringError> {
        let occurrence = self
            .cleanup_occurrence
            .expect("inactive capture cleanup retains its ownership-end occurrence");

        let local = self.cleanup_outcome.is_none();

        let outcome = match &self.cleanup_outcome {
            // Nested resolution shares the enclosing outcome's immutable storage paths.
            Some(outcome) => outcome.clone(),
            None => self.create_cleanup_outcome(block, source)?,
        };

        let awaited = self.lower_awaited_frame(
            occurrence,
            MirOperand::Move(place),
            None,
            block,
            Self::retained_source(source),
        )?;

        let block = awaited
            .block
            .expect("inactive capture cleanup resumes its enclosing owner");

        let block = outcome.check_abandonment(&mut self.builder, block, source)?;

        if local {
            self.finish_finalizer_outcome(block, source, &outcome)
        } else {
            Ok(block)
        }
    }

    pub(super) fn push_async_finalizer(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: LifecycleCallable,
    ) -> Result<MirBlockId, LoweringError> {
        let occurrence = self
            .cleanup_occurrence
            .expect("async finalization retains its ownership-end occurrence");

        let local = self.cleanup_outcome.is_none();

        let outcome = match &self.cleanup_outcome {
            // Nested resolution shares the enclosing outcome's immutable storage paths.
            Some(outcome) => outcome.clone(),
            None => self.create_cleanup_outcome(block, source)?,
        };

        let invocation = callable.invocation_result(
            self.input.semantic_values(),
            self.input.available_compiler_known_symbols(),
        )?;

        let receiver = self.input.semantic_values().type_data(callable.receiver);

        let borrow = match receiver.as_ref() {
            bray_symbols::TypeData::Borrow { kind, .. } => Some(*kind),
            _ => None,
        };

        let value = crate::lifecycle_call::lower_lifecycle_call(
            MirCallableReference::new(callable.callable, callable.abi),
            callable.receiver,
            Self::retained_place(&place),
            borrow,
            invocation,
            true,
            None,
            |operation, ty| {
                self.push_operation(block, Self::retained_source(source), operation, ty)
            },
        )?;

        let awaited = self.lower_awaited_frame(
            occurrence,
            MirOperand::Value(value),
            Some(callable.result),
            block,
            Self::retained_source(source),
        )?;

        let block = awaited
            .block
            .expect("shielded finalization resumes cleanup");

        let value = awaited
            .value
            .expect("shielded finalization publishes its completion result");

        let payloads = crate::operand::result_payloads(
            self.input.semantic_values(),
            self.input.available_compiler_known_symbols(),
            callable.result,
        );

        let (block, abandoned) = outcome.retain_finalizer_result(
            &mut self.builder,
            block,
            source,
            value,
            (
                MirCallableReference::new(callable.callable, callable.abi),
                invocation,
            ),
            payloads,
        )?;

        // Keep the owner broadcastable throughout the awaited mutation. Destruction
        // retires its guard only after every finalizer outcome has reached this point.
        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Destroy {
                place,
                abandoned: Some(abandoned),
            },
            None,
        )?;

        let block = outcome.check(&mut self.builder, block, source)?;

        if local {
            self.finish_finalizer_outcome(block, source, &outcome)
        } else {
            Ok(block)
        }
    }

    pub(in crate::lowering) fn finish_finalizer_outcome(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        outcome: &CleanupOutcome,
    ) -> Result<MirBlockId, LoweringError> {
        // Failure handlers own their report storage independently of this local outcome.
        let (panicked, cancelled, report) = self
            .cleanup_failure_targets
            .clone()
            .expect("ordinary finalization retains the surrounding cleanup failure targets");

        let panic = self.builder.push_block(
            Self::retained_source(source),
            self.builder.block_kind(block),
        )?;

        self.push_operation(
            panic,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: bray_ir::MirStoreKind::Initialize,
                destination: report,
                value: outcome.report(),
            },
            None,
        )?;

        self.set_terminator(
            panic,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(panicked, [])),
        )?;

        Ok(outcome.dispatch(
            &mut self.builder,
            block,
            source,
            panic,
            cancelled,
            &mut None,
        )?)
    }
}
