use super::model::CleanupOutcome;
use bray_bound_tree::BoundCallResult;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCapacityError, MirCleanupEdge, MirCleanupPhase, MirCleanupState,
    MirEdge, MirImmediateValue, MirOperand, MirOperationKind, MirPlace, MirRuntimeReference,
    MirSourceAnchor, MirStorageKind, MirStoreKind, MirTerminatorKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeId;

impl CleanupOutcome {
    /// Collects a completed finalizer's active error while retaining every failed-call outcome.
    #[expect(
        clippy::too_many_arguments,
        reason = "finalizer result retention joins its producer, owner, payload types, and report policy"
    )]
    pub(crate) fn retain_finalizer_result(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        value: MirOperand,
        owner: &MirPlace,
        producer: (bray_ir::MirCallableReference, BoundCallResult),
        payloads: Option<([TypeId; 2], bray_symbols::CompilerKnownResultRepresentation)>,
        reports: bool,
    ) -> Result<(MirBlockId, MirOperand), MirCapacityError> {
        let (completed, value, failed) =
            self.check_completion(builder, block, source, Some(value), reports)?;

        let value = value.expect("completed finalizer retains its result");
        let continuation = builder.push_block(source.clone(), builder.block_kind(block))?;

        let abandoned = builder.push_block_parameter(continuation, source.clone(), self.boolean)?;

        builder.set_terminator(
            failed,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(
                continuation,
                [Self::boolean(true, self.boolean)],
            )),
        );

        if let Some((types, representation)) = payloads {
            let (branch, [(success, _), (error, incident)]) = crate::operand::branch_result(
                builder,
                completed,
                source,
                value,
                types,
                representation,
            )?;

            builder.set_terminator(completed, source.clone(), branch);

            let error = self.retain_incident(builder, error, source, incident, producer)?;

            for branch in [success, error] {
                builder.set_terminator(
                    branch,
                    source.clone(),
                    MirTerminatorKind::Goto(MirEdge::new(
                        continuation,
                        [Self::boolean(branch == error, self.boolean)],
                    )),
                );
            }
        } else {
            builder.set_terminator(
                completed,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(
                    continuation,
                    [Self::boolean(false, self.boolean)],
                )),
            );
        }

        let pending = builder.push_block(source.clone(), builder.block_kind(continuation))?;
        let destruction = builder.push_block(source.clone(), builder.block_kind(continuation))?;

        let disposition =
            builder.push_block_parameter(destruction, source.clone(), self.boolean)?;

        builder.set_terminator(
            continuation,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Value(abandoned),
                then_edge: MirEdge::new(pending, []),
                else_edge: MirEdge::new(destruction, [Self::boolean(false, self.boolean)]),
            },
        );

        // Finalizer mutation can introduce child runs after the earlier broadcast.
        // Resolve that current graph before terminal-only destruction starts.
        let pending = self.quiesce(builder, pending, source, owner)?;

        builder.set_terminator(
            pending,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(
                destruction,
                [Self::boolean(true, self.boolean)],
            )),
        );

        Ok((destruction, MirOperand::Value(disposition)))
    }

    /// Resolves an error's child work before transferring its still-owned payload.
    pub(crate) fn retain_incident(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        incident: MirOperand,
        producer: (bray_ir::MirCallableReference, BoundCallResult),
    ) -> Result<MirBlockId, MirCapacityError> {
        let ty = builder.operand_type(&incident);
        let storage = builder.push_storage(source.clone(), MirStorageKind::Temporary, ty)?;
        let place = MirPlace::new(storage, [], ty);

        builder.push_operation(
            block,
            source.clone(),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: place.clone(),
                value: incident,
            },
            None,
        )?;

        let block = self.quiesce(builder, block, source, &place)?;
        let initialize = builder.push_block(source.clone(), builder.block_kind(block))?;
        let transfer = builder.push_block(source.clone(), builder.block_kind(block))?;

        self.switch(
            builder,
            block,
            source,
            [(MirCleanupState::Normal, MirEdge::new(initialize, []))],
            MirEdge::new(transfer, []),
        );

        self.store(
            builder,
            initialize,
            source,
            &self.report,
            MirOperand::Immediate {
                value: MirImmediateValue::EmptyPanicReport,
                ty: self.report.ty(),
            },
        )?;

        self.store_state(builder, initialize, source, MirCleanupState::Incidents)?;

        builder.set_terminator(
            initialize,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(transfer, [])),
        );

        builder.push_operation(
            transfer,
            source.clone(),
            MirOperationKind::Async(bray_ir::MirAsyncOperation::TransferCleanupIncident {
                incident: place,
                report: self.report.clone(),
                producer: producer.0,
                invocation: producer.1,
                runtime: MirRuntimeReference::new(
                    RuntimeAbiRole::CleanupIncidentTransfer,
                    self.runtime_abi,
                ),
            }),
            None,
        )?;

        Ok(transfer)
    }

    fn quiesce(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: &MirPlace,
    ) -> Result<MirBlockId, MirCapacityError> {
        let broadcast = builder.push_block(source.clone(), MirBlockKind::CleanupBroadcast)?;
        let resolution = builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::BeginCleanup(MirCleanupEdge::new(
                MirCleanupPhase::TaskCancellation,
                MirEdge::new(broadcast, []),
            )),
        );

        builder.push_operation(
            broadcast,
            source.clone(),
            MirOperationKind::Cleanup {
                collection: None,
                phase: MirCleanupPhase::TaskCancellation,
                place: place.clone(),
            },
            None,
        )?;

        let broadcast = self.check(builder, broadcast, source, false)?;

        builder.set_terminator(
            broadcast,
            source.clone(),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(resolution, []),
            )),
        );

        builder.push_operation(
            resolution,
            source.clone(),
            MirOperationKind::Quiesce {
                place: place.clone(),
                collection: None,
            },
            None,
        )?;

        // Quiescence retains storage on every outcome. Its incidents remain owned
        // by the enclosing cleanup before destruction or payload transfer proceeds.
        self.check(builder, resolution, source, false)
    }
}
