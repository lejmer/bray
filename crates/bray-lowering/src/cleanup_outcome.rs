use bray_bound_tree::BoundCallResult;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCall, MirCallPanicEdge, MirCallTarget, MirEdge, MirImmediateValue,
    MirOperand, MirOperationKind, MirPlace, MirRuntimeReference, MirSourceAnchor, MirStorageKind,
    MirStoreKind, MirTerminatorKind, MirUnitBuildError, MirUnitBuilder,
};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeAbiVersion};
use bray_symbols::TypeId;

/// Determines whether cancellation is an incident or the requested cleanup result.
pub(crate) enum CleanupCancellation {
    Propagate,
    Resolved,
}

/// Retains cleanup incidents until every selected obligation has been resolved.
pub(crate) struct CleanupOutcome {
    panicked: MirPlace,
    cancelled: MirPlace,
    report: MirPlace,
    unit: TypeId,
    runtime_abi: RuntimeAbiVersion,
}

// MIR instructions own their provenance and place paths. Clones below share the Arc-backed
// paths while this builder retains the same storage identities for later incident branches.
impl CleanupOutcome {
    pub(crate) fn new(
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        boolean: TypeId,
        report: TypeId,
        unit: TypeId,
        runtime_abi: RuntimeAbiVersion,
    ) -> Result<Self, MirUnitBuildError> {
        // Every generated operation retains the same Arc-backed source provenance.
        let mut storage = |ty| {
            builder
                .push_storage(source.clone(), MirStorageKind::Temporary, ty)
                .map(|id| MirPlace::new(id, [], ty))
        };

        let outcome = Self {
            panicked: storage(boolean)?,
            cancelled: storage(boolean)?,
            report: storage(report)?,
            unit,
            runtime_abi,
        };

        for flag in [&outcome.panicked, &outcome.cancelled] {
            outcome.store(builder, block, source, flag, Self::boolean(false, boolean))?;
        }

        outcome.shield(builder, block, source, RuntimeAbiRole::CleanupShieldEnter)?;

        Ok(outcome)
    }

    pub(crate) fn report(&self) -> MirOperand {
        // The move operand owns its path while this builder retains the storage identity.
        MirOperand::Move(self.report.clone())
    }

    pub(crate) fn initialize_panic(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        report: MirOperand,
    ) -> Result<(), MirUnitBuildError> {
        self.store(builder, block, source, &self.report, report)?;

        self.store(
            builder,
            block,
            source,
            &self.panicked,
            Self::boolean(true, self.panicked.ty()),
        )
    }

    pub(crate) fn end_shield(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<(), MirUnitBuildError> {
        self.shield(builder, block, source, RuntimeAbiRole::CleanupShieldLeave)
    }

    pub(crate) fn initialize_cancellation(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<(), MirUnitBuildError> {
        self.store(
            builder,
            block,
            source,
            &self.cancelled,
            Self::boolean(true, self.cancelled.ty()),
        )
    }

    /// Retains a completed payload and accepts cancellation requested by automatic owner cleanup.
    pub(crate) fn resolve_completion(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        result: MirPlace,
        contract: (bray_ir::MirRunResultVariants, TypeId),
    ) -> Result<(MirBlockId, MirBlockId, MirPlace), MirUnitBuildError> {
        let (variants, completion) = contract;

        let payload = result.project(
            bray_ir::MirProjectionKind::ActiveUnionPayloadElement {
                variant: variants.completed(),
                ordinal: bray_symbols::SymbolOrdinal::new(0),
            },
            completion,
        );

        let (completed, finished) = self.resolve_run_result(
            builder,
            block,
            source,
            result,
            (variants, CleanupCancellation::Resolved),
        )?;

        Ok((completed, finished, payload))
    }

    /// Moves failed run outcomes into the retained incident state and exposes the completed branch.
    pub(crate) fn resolve_run_result(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        result: MirPlace,
        result_contract: (bray_ir::MirRunResultVariants, CleanupCancellation),
    ) -> Result<(MirBlockId, MirBlockId), MirUnitBuildError> {
        let (variants, cancellation) = result_contract;

        let kind = builder.block_kind(block)?;
        let completed = builder.push_block(source.clone(), kind)?;
        let failed = builder.push_block(source.clone(), kind)?;
        let panicked = builder.push_block(source.clone(), kind)?;
        let cancelled = builder.push_block(source.clone(), kind)?;
        let finished = builder.push_block(source.clone(), kind)?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::PatternBranch {
                subject: MirOperand::Copy(result.clone()),
                predicate: bray_ir::MirPatternPredicate::ActiveUnionVariant(variants.completed()),
                matched: MirEdge::new(completed, []),
                unmatched: MirEdge::new(failed, []),
            },
        )?;

        builder.set_terminator(
            failed,
            source.clone(),
            MirTerminatorKind::PatternBranch {
                subject: MirOperand::Copy(result.clone()),
                predicate: bray_ir::MirPatternPredicate::ActiveUnionVariant(variants.panicked()),
                matched: MirEdge::new(panicked, []),
                unmatched: MirEdge::new(cancelled, []),
            },
        )?;

        let report = result.project(
            bray_ir::MirProjectionKind::ActiveUnionPayloadElement {
                variant: variants.panicked(),
                ordinal: bray_symbols::SymbolOrdinal::new(0),
            },
            self.report.ty(),
        );

        self.retain_panic(
            builder,
            panicked,
            source,
            MirOperand::Move(report),
            finished,
        )?;

        if matches!(cancellation, CleanupCancellation::Propagate) {
            self.initialize_cancellation(builder, cancelled, source)?;
        }

        builder.set_terminator(
            cancelled,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(finished, [])),
        )?;

        Ok((completed, finished))
    }

    /// Adds incident continuations after the block's last fallible cleanup operation.
    pub(crate) fn check(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<MirBlockId, MirUnitBuildError> {
        let kind = builder.block_kind(block)?;
        let completed = builder.push_block(source.clone(), kind)?;

        self.check_into(
            builder,
            block,
            source,
            MirEdge::new(completed, []),
            completed,
        )?;

        Ok(completed)
    }

    /// Stores a successful call's result and exposes the continuation where that storage is initialized.
    pub(crate) fn check_value(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        value: bray_ir::MirValueId,
        failed: MirBlockId,
    ) -> Result<(MirBlockId, MirPlace), MirUnitBuildError> {
        let ty = builder.operand_type(&MirOperand::Value(value))?;
        let kind = builder.block_kind(block)?;
        let completed = builder.push_block(source.clone(), kind)?;
        let parameter = builder.push_block_parameter(completed, source.clone(), ty)?;
        let storage = builder.push_storage(source.clone(), MirStorageKind::Temporary, ty)?;
        let place = MirPlace::new(storage, [], ty);

        self.check_into(
            builder,
            block,
            source,
            MirEdge::new(completed, [MirOperand::Value(value)]),
            failed,
        )?;

        self.store(
            builder,
            completed,
            source,
            &place,
            MirOperand::Value(parameter),
        )?;

        Ok((completed, place))
    }

    /// Retains an incident before taking the failure path, without using a failed call's value.
    pub(crate) fn check_into(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        completed: MirEdge,
        failed: MirBlockId,
    ) -> Result<(), MirUnitBuildError> {
        let kind = builder.block_kind(block)?;
        let panicked = builder.push_block(source.clone(), kind)?;
        let cancelled = builder.push_block(source.clone(), kind)?;
        let report = builder.push_block_parameter(panicked, source.clone(), self.report.ty())?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::CheckCallOutcome {
                completed,
                panicked: MirCallPanicEdge::new(panicked, self.report.ty()),
                cancelled: MirEdge::new(cancelled, []),
            },
        )?;

        self.store(
            builder,
            cancelled,
            source,
            &self.cancelled,
            Self::boolean(true, self.cancelled.ty()),
        )?;

        builder.set_terminator(
            cancelled,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(failed, [])),
        )?;

        self.retain_panic(builder, panicked, source, MirOperand::Value(report), failed)
    }

    pub(crate) fn retain_allocation_failure(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        completed: MirBlockId,
    ) -> Result<(), MirUnitBuildError> {
        let report =
            crate::frame_creation::allocation_panic(builder, block, source, self.report.ty())?;

        self.retain_panic(builder, block, source, report, completed)
    }

    fn retain_panic(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        report: MirOperand,
        completed: MirBlockId,
    ) -> Result<(), MirUnitBuildError> {
        let kind = builder.block_kind(block)?;
        let primary = builder.push_block(source.clone(), kind)?;
        let suppressed = builder.push_block(source.clone(), kind)?;

        let primary_report =
            builder.push_block_parameter(primary, source.clone(), self.report.ty())?;

        let suppressed_report =
            builder.push_block_parameter(suppressed, source.clone(), self.report.ty())?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Copy(self.panicked.clone()),
                // Only the selected edge transfers the newly reported incident.
                then_edge: MirEdge::new(suppressed, [report.clone()]),
                else_edge: MirEdge::new(primary, [report]),
            },
        )?;

        self.store(
            builder,
            primary,
            source,
            &self.report,
            MirOperand::Value(primary_report),
        )?;

        self.store(
            builder,
            primary,
            source,
            &self.panicked,
            Self::boolean(true, self.panicked.ty()),
        )?;

        builder.set_terminator(
            primary,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(completed, [])),
        )?;

        let merged = builder.push_operation(
            suppressed,
            source.clone(),
            MirOperationKind::Call(MirCall::protocol(
                MirCallTarget::Runtime(MirRuntimeReference::new(
                    RuntimeAbiRole::PanicReportSuppression,
                    self.runtime_abi,
                )),
                BoundCallResult::Immediate(self.report.ty()),
                [self.report(), MirOperand::Value(suppressed_report)],
                [],
            )),
            Some(self.report.ty()),
        )?;

        let value = merged
            .result()
            .ok_or(MirUnitBuildError::MissingOperationResult(
                merged.operation(),
            ))?;

        self.store(
            builder,
            suppressed,
            source,
            &self.report,
            MirOperand::Value(value),
        )?;

        builder.set_terminator(
            suppressed,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(completed, [])),
        )
    }

    /// Forwards collected failures through the caller's existing outcome continuations.
    pub(crate) fn forward(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        edges: (&MirEdge, MirCallPanicEdge, &MirEdge),
    ) -> Result<(), MirUnitBuildError> {
        let (completed_edge, panicked, cancelled_edge) = edges;

        let panic_bridge = builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;

        let cancellation_bridge =
            builder.push_block(source.clone(), MirBlockKind::LifecycleResolution)?;

        let completed = self.dispatch(builder, block, source, panic_bridge, cancellation_bridge)?;

        // Preserve the caller's ownership arguments while forwarding only the newly collected failure.
        for (block, edge) in [
            (completed, completed_edge.clone()),
            (
                panic_bridge,
                MirEdge::new(panicked.target(), [self.report()]),
            ),
            (cancellation_bridge, cancelled_edge.clone()),
        ] {
            builder.set_terminator(block, source.clone(), MirTerminatorKind::Goto(edge))?;
        }

        Ok(())
    }

    /// Transfers this child's failures to its parent and resumes the parent's remaining cleanup.
    pub(crate) fn retain_into(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        parent: &Self,
    ) -> Result<MirBlockId, MirUnitBuildError> {
        let kind = builder.block_kind(block)?;
        let panicked = builder.push_block(source.clone(), kind)?;
        let cancelled = builder.push_block(source.clone(), kind)?;
        let completed = self.dispatch(builder, block, source, panicked, cancelled)?;

        parent.retain_panic(builder, panicked, source, self.report(), completed)?;
        parent.initialize_cancellation(builder, cancelled, source)?;

        builder.set_terminator(
            cancelled,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(completed, [])),
        )?;

        Ok(completed)
    }

    /// Branches to caller-owned completion paths, with panic taking precedence.
    pub(crate) fn dispatch(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        panicked: MirBlockId,
        cancelled: MirBlockId,
    ) -> Result<MirBlockId, MirUnitBuildError> {
        self.end_shield(builder, block, source)?;
        let kind = builder.block_kind(block)?;
        let no_panic = builder.push_block(source.clone(), kind)?;
        let completed = builder.push_block(source.clone(), kind)?;

        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Copy(self.panicked.clone()),
                then_edge: MirEdge::new(panicked, []),
                else_edge: MirEdge::new(no_panic, []),
            },
        )?;

        builder.set_terminator(
            no_panic,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: MirOperand::Copy(self.cancelled.clone()),
                then_edge: MirEdge::new(cancelled, []),
                else_edge: MirEdge::new(completed, []),
            },
        )?;

        Ok(completed)
    }

    fn shield(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        role: RuntimeAbiRole,
    ) -> Result<(), MirUnitBuildError> {
        builder.push_operation(
            block,
            source.clone(),
            MirOperationKind::Call(MirCall::protocol(
                MirCallTarget::Runtime(MirRuntimeReference::new(role, self.runtime_abi)),
                BoundCallResult::Immediate(self.unit),
                [],
                [],
            )),
            Some(self.unit),
        )?;

        Ok(())
    }

    fn store(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        destination: &MirPlace,
        value: MirOperand,
    ) -> Result<(), MirUnitBuildError> {
        builder.push_operation(
            block,
            source.clone(),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: destination.clone(),
                value,
            },
            None,
        )?;

        Ok(())
    }

    const fn boolean(value: bool, ty: TypeId) -> MirOperand {
        MirOperand::Immediate {
            value: MirImmediateValue::Boolean(value),
            ty,
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_ir::{
        MirBlockKind, MirImmediateValue, MirOperand, MirOperationKind, MirPlace,
        MirRunResultVariants, MirSourceAnchor, MirStorageKind, MirTerminatorKind, MirUnitBuilder,
        MirUnitKind,
    };
    use bray_symbols::{SemanticValueStore, SymbolId, TypeData, UnionVariantSymbolId};

    use super::{CleanupCancellation, CleanupOutcome};

    #[test]
    fn requested_capture_resolution_does_not_cancel_the_owning_run() {
        for (cancellation, expected) in [
            (CleanupCancellation::Propagate, 1),
            (CleanupCancellation::Resolved, 0),
        ] {
            let values = SemanticValueStore::try_new().unwrap();
            let ty = values.intern_type(TypeData::tuple([])).unwrap();
            let bound = bray_testing::test_bound_unit(916);
            let source = MirSourceAnchor::from(bound.key().source());
            let target = bray_testing::test_mir_target();
            let abi = target.runtime_abi();

            let mut builder =
                MirUnitBuilder::for_bound(bound.identity(), MirUnitKind::Synchronous, target);

            let entry = builder
                .push_block(source.clone(), MirBlockKind::Ordinary)
                .unwrap();

            let outcome =
                CleanupOutcome::new(&mut builder, entry, &source, ty, ty, ty, abi).unwrap();

            let result = builder
                .push_storage(source.clone(), MirStorageKind::Parameter(0), ty)
                .unwrap();

            let variants = MirRunResultVariants::new(
                UnionVariantSymbolId::from_symbol_id(SymbolId::new(1)),
                UnionVariantSymbolId::from_symbol_id(SymbolId::new(2)),
                UnionVariantSymbolId::from_symbol_id(SymbolId::new(3)),
            );

            let (completed, finished) = outcome
                .resolve_run_result(
                    &mut builder,
                    entry,
                    &source,
                    MirPlace::new(result, [], ty),
                    (variants, cancellation),
                )
                .unwrap();

            for block in [completed, finished] {
                builder
                    .set_terminator(block, source.clone(), MirTerminatorKind::Return(None))
                    .unwrap();
            }

            let unit = builder.finish(entry).unwrap();

            let cancellations = unit
                .operations()
                .iter()
                .filter(|operation| {
                    matches!(operation.kind(),
                        MirOperationKind::Store { destination, value: MirOperand::Immediate {
                            value: MirImmediateValue::Boolean(true), ..
                        }, .. } if destination == &outcome.cancelled
                    )
                })
                .count();

            assert_eq!(cancellations, expected);
        }
    }
}
