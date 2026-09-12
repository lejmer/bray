use bray_bound_tree::{AnyBoundNodeId, BoundBlockId};
use bray_ir::{
    MirAsyncOperation, MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge,
    MirOperand, MirOperationKind, MirSourceAnchor, MirTaskTerminalState, MirTerminatorKind,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeId;

use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;
use crate::plan::ScopeExitCleanupStatus;

pub(super) enum CleanupDestination {
    Goto(MirBlockId),
    Return,
    PropagatePanic,
    PropagateCancellation,
}

enum CleanupEntry {
    Ordinary,
    Panic(MirOperand),
    Cancellation,
}

pub(super) enum TerminalState {
    Completed(TypeId),
    Panicked(TypeId),
    Cancelled,
}

impl Lowerer<'_> {
    pub(in crate::lowering) fn finish_scope(
        &mut self,
        scope: BoundBlockId,
        current: MirBlockId,
        source: &MirSourceAnchor,
        exit: AnyBoundNodeId,
    ) -> Result<MirBlockId, LoweringError> {
        if !matches!(
            self.scope_cleanup_status(scope, exit)?,
            ScopeExitCleanupStatus::Cleanup
        ) {
            return Ok(current);
        }

        let continuation = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        self.finish_cleanup(
            current,
            source,
            self.active_scopes.len().saturating_sub(1),
            CleanupEntry::Ordinary,
            CleanupDestination::Goto(continuation),
            None,
            exit,
        )?;

        Ok(continuation)
    }

    pub(in crate::lowering) fn finish_exit_to_block(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        scope_depth: usize,
        target: MirBlockId,
        value: Option<(MirOperand, TypeId)>,
        exit: AnyBoundNodeId,
    ) -> Result<(), LoweringError> {
        self.finish_cleanup(
            current,
            source,
            scope_depth,
            CleanupEntry::Ordinary,
            CleanupDestination::Goto(target),
            value,
            exit,
        )
    }

    pub(in crate::lowering) fn finish_return(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        mut value: Option<(MirOperand, TypeId)>,
        exit: AnyBoundNodeId,
    ) -> Result<(), LoweringError> {
        let destination = if self.input.unit_kind().protected_frame().is_some() {
            let result_type = self
                .input
                .expression_types()
                .callable_result_type()
                .ok_or(LoweringError::MissingCallableResultType)?;

            if value.is_none() {
                value = Some((self.unit_operand(result_type), result_type));
            }

            CleanupDestination::Goto(
                self.terminal_state_block(source, TerminalState::Completed(result_type))?,
            )
        } else {
            CleanupDestination::Return
        };

        self.finish_cleanup(
            current,
            source,
            0,
            CleanupEntry::Ordinary,
            destination,
            value,
            exit,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "panic cleanup requires the complete transfer and cleanup context"
    )]
    pub(in crate::lowering) fn finish_panic(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        report: MirOperand,
        report_type: TypeId,
        catch: Option<MirBlockId>,
        scope_depth: usize,
        exit: AnyBoundNodeId,
    ) -> Result<(), LoweringError> {
        let destination = match (catch, self.input.unit_kind().protected_frame()) {
            (Some(catch), _) => CleanupDestination::Goto(catch),
            (None, Some(_)) => CleanupDestination::Goto(
                self.terminal_state_block(source, TerminalState::Panicked(report_type))?,
            ),
            (None, None) => CleanupDestination::PropagatePanic,
        };

        self.finish_cleanup(
            current,
            source,
            scope_depth,
            CleanupEntry::Panic(Self::retained_operand(&report)),
            destination,
            Some((report, report_type)),
            exit,
        )
    }

    pub(in crate::lowering) fn finish_cancellation(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        exit: AnyBoundNodeId,
    ) -> Result<(), LoweringError> {
        let destination = if self.input.unit_kind().protected_frame().is_some() {
            CleanupDestination::Goto(self.terminal_state_block(source, TerminalState::Cancelled)?)
        } else {
            CleanupDestination::PropagateCancellation
        };

        self.finish_cleanup(
            current,
            source,
            0,
            CleanupEntry::Cancellation,
            destination,
            None,
            exit,
        )
    }

    pub(super) fn terminal_state_block(
        &mut self,
        source: &MirSourceAnchor,
        terminal: TerminalState,
    ) -> Result<MirBlockId, LoweringError> {
        let block = self
            .builder
            .push_block(Self::retained_source(source), MirBlockKind::Ordinary)?;

        let state = match terminal {
            TerminalState::Completed(result_type) => {
                let result = self.builder.push_block_parameter(
                    block,
                    Self::retained_source(source),
                    result_type,
                )?;

                MirTaskTerminalState::Completed(MirOperand::Value(result))
            }
            TerminalState::Panicked(report_type) => {
                let report = self.builder.push_block_parameter(
                    block,
                    Self::retained_source(source),
                    report_type,
                )?;

                MirTaskTerminalState::Panicked(MirOperand::Value(report))
            }
            TerminalState::Cancelled => MirTaskTerminalState::Cancelled,
        };

        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Async(MirAsyncOperation::PublishTerminalState {
                state,
                runtime: self.runtime_reference(RuntimeAbiRole::TerminalPublication),
            }),
            None,
        )?;

        self.set_terminator(
            block,
            Self::retained_source(source),
            MirTerminatorKind::Return(None),
        )?;

        Ok(block)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "cleanup finalization requires the complete transfer and cleanup context"
    )]
    fn finish_cleanup(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        scope_depth: usize,
        entry: CleanupEntry,
        destination: CleanupDestination,
        value: Option<(MirOperand, TypeId)>,
        exit: AnyBoundNodeId,
    ) -> Result<(), LoweringError> {
        let plans = self.cleanup_plans(scope_depth, exit)?;

        if plans.iter().all(|plan| !plan.has_cleanup()) {
            return self.set_direct_exit(current, source, entry, destination, value);
        }

        match entry {
            CleanupEntry::Panic(report) => {
                return self.finish_abnormal_cleanup(
                    current,
                    source,
                    Some(report),
                    destination,
                    &plans,
                    None,
                );
            }
            CleanupEntry::Cancellation => {
                return self.finish_abnormal_cleanup(
                    current,
                    source,
                    None,
                    destination,
                    &plans,
                    None,
                );
            }
            CleanupEntry::Ordinary => {}
        }

        self.finish_ordinary_cleanup(current, source, destination, value, exit, &plans)
    }

    pub(super) fn cleanup_plans(
        &self,
        scope_depth: usize,
        exit: AnyBoundNodeId,
    ) -> Result<Vec<bray_bound_tree::AsyncScopeExitPlan>, LoweringError> {
        self.input
            .lowering_plans()
            .cleanup_plans(&self.active_scopes, scope_depth, exit)
            .map_err(Into::into)
    }

    fn scope_cleanup_status(
        &self,
        scope: BoundBlockId,
        exit: AnyBoundNodeId,
    ) -> Result<ScopeExitCleanupStatus, LoweringError> {
        self.input
            .lowering_plans()
            .scope_cleanup_status(scope, exit)
            .map_err(Into::into)
    }

    pub(super) fn push_cleanup_operations(
        &mut self,
        mut block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        plans: &[bray_bound_tree::AsyncScopeExitPlan],
        mut value: Option<(bray_ir::MirValueId, TypeId)>,
        failures: &std::collections::BTreeMap<BoundBlockId, (MirBlockId, MirBlockId, TypeId)>,
    ) -> Result<(MirBlockId, Option<(bray_ir::MirValueId, TypeId)>), LoweringError> {
        for plan in plans {
            self.cleanup_failure_targets = failures.get(&plan.scope()).copied();

            for access in match phase {
                MirCleanupPhase::TaskCancellation => plan.cancellation_broadcast(),
                MirCleanupPhase::LifecycleResolution => plan.lifecycle_resolution(),
            } {
                let place = self.place_for_access(*access, false)?;

                let state = self.initialization_guards.get(&place.storage());
                let guard = state.map(|state| Self::retained_place(&state.guard));

                // Cleanup construction mutates the lowerer while retaining shared checked paths and flag types.
                let parts = state
                    .map(|state| {
                        state
                            .parts
                            .iter()
                            .map(|part| crate::lowering::initialization::InitializedPart {
                                plan: part.plan,
                                guard: Self::retained_place(&part.guard),
                                array_types: std::sync::Arc::clone(&part.array_types),
                            })
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();

                if parts.is_empty() {
                    (block, value) =
                        self.push_guarded_cleanup(block, source, phase, place, guard, None, value)?;
                } else {
                    (block, value) = self.guarded_cleanup_region(
                        block,
                        source,
                        place,
                        guard,
                        value,
                        |lowerer, block, place, value| {
                            lowerer.push_part_cleanup(
                                block, source, phase, *access, place, &parts, 0, value,
                            )
                        },
                    )?;
                }
            }
        }

        self.cleanup_failure_targets = None;

        Ok((block, value))
    }

    pub(in crate::lowering) fn push_guarded_cleanup(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        place: bray_ir::MirPlace,
        guard: Option<bray_ir::MirPlace>,
        release: Option<bray_bound_tree::StorageProtocolCall>,
        value: Option<(bray_ir::MirValueId, TypeId)>,
    ) -> Result<(MirBlockId, Option<(bray_ir::MirValueId, TypeId)>), LoweringError> {
        self.guarded_cleanup_region(
            block,
            source,
            place,
            guard,
            value,
            |lowerer, block, place, value| {
                lowerer.push_cleanup_action(block, source, phase, place, release)?;

                let block = if let Some(outcome) = &lowerer.cleanup_outcome {
                    outcome.check(&mut lowerer.builder, block, source)?
                } else {
                    lowerer.check_ordinary_cleanup(block, source)?
                };

                Ok((block, value))
            },
        )
    }

    pub(in crate::lowering) fn guarded_cleanup_region(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: bray_ir::MirPlace,
        guard: Option<bray_ir::MirPlace>,
        value: Option<(bray_ir::MirValueId, TypeId)>,
        body: impl FnOnce(
            &mut Self,
            MirBlockId,
            bray_ir::MirPlace,
            Option<(bray_ir::MirValueId, TypeId)>,
        )
            -> Result<(MirBlockId, Option<(bray_ir::MirValueId, TypeId)>), LoweringError>,
    ) -> Result<(MirBlockId, Option<(bray_ir::MirValueId, TypeId)>), LoweringError> {
        if let Some(guard) = guard {
            let kind = self.builder.block_kind(block)?;

            let perform = self
                .builder
                .push_block(Self::retained_source(source), kind)?;

            let continuation = self
                .builder
                .push_block(Self::retained_source(source), kind)?;

            let forwarded = value.map(|(value, ty)| (MirOperand::Value(value), ty));
            let perform_value = self.cleanup_parameter(perform, source, forwarded.as_ref())?;

            let continuation_value =
                self.cleanup_parameter(continuation, source, forwarded.as_ref())?;

            self.set_terminator(
                block,
                Self::retained_source(source),
                MirTerminatorKind::Branch {
                    condition: MirOperand::Copy(guard),
                    then_edge: MirEdge::new(
                        perform,
                        value.map(|(value, _)| MirOperand::Value(value)),
                    ),
                    else_edge: MirEdge::new(
                        continuation,
                        value.map(|(value, _)| MirOperand::Value(value)),
                    ),
                },
            )?;

            let (perform, perform_value) = self.guard_cleanup_payloads(
                perform,
                continuation,
                source,
                &place,
                perform_value.zip(value.map(|(_, ty)| ty)),
            )?;

            let (perform, perform_value) = body(self, perform, place, perform_value)?;

            self.set_terminator(
                perform,
                Self::retained_source(source),
                MirTerminatorKind::Goto(MirEdge::new(
                    continuation,
                    perform_value.map(|(value, _)| MirOperand::Value(value)),
                )),
            )?;

            return Ok((
                continuation,
                continuation_value.zip(value.map(|(_, ty)| ty)),
            ));
        }

        body(self, block, place, value)
    }

    fn push_cleanup_action(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        place: bray_ir::MirPlace,
        release: Option<bray_bound_tree::StorageProtocolCall>,
    ) -> Result<(), LoweringError> {
        if let Some(release) = release {
            self.set_storage_initialized(block, source, &place, false)?;
            self.push_storage_protocol_call(block, source, &place, release)?;
        } else {
            self.push_operation(
                block,
                Self::retained_source(source),
                MirOperationKind::Cleanup { phase, place },
                None,
            )?;
        }

        Ok(())
    }

    pub(in crate::lowering) fn cleanup_parameter(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        value: Option<&(MirOperand, TypeId)>,
    ) -> Result<Option<bray_ir::MirValueId>, LoweringError> {
        value
            .map(|(_, ty)| {
                self.builder
                    .push_block_parameter(block, Self::retained_source(source), *ty)
                    .map_err(Into::into)
            })
            .transpose()
    }

    fn set_direct_exit(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        entry: CleanupEntry,
        destination: CleanupDestination,
        value: Option<(MirOperand, TypeId)>,
    ) -> Result<(), LoweringError> {
        match entry {
            CleanupEntry::Ordinary => {
                self.set_destination(current, source, destination, value.map(|(value, _)| value))
            }
            entry @ (CleanupEntry::Panic(_) | CleanupEntry::Cancellation) => {
                let cancellation = self.builder.push_block(
                    Self::retained_source(source),
                    MirBlockKind::CleanupBroadcast,
                )?;

                let lifecycle = self.builder.push_block(
                    Self::retained_source(source),
                    MirBlockKind::LifecycleResolution,
                )?;

                let cancellation_value =
                    self.cleanup_parameter(cancellation, source, value.as_ref())?;

                let lifecycle_value = self.cleanup_parameter(lifecycle, source, value.as_ref())?;

                self.set_terminator(
                    cancellation,
                    Self::retained_source(source),
                    MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                        MirCleanupPhase::LifecycleResolution,
                        MirEdge::new(lifecycle, cancellation_value.map(MirOperand::Value)),
                    )),
                )?;

                self.set_destination(
                    lifecycle,
                    source,
                    destination,
                    lifecycle_value.map(MirOperand::Value),
                )?;

                let edge = MirCleanupEdge::new(
                    MirCleanupPhase::TaskCancellation,
                    MirEdge::new(cancellation, value.map(|(value, _)| value)),
                );

                let terminator = match entry {
                    CleanupEntry::Panic(report) => MirTerminatorKind::Panic {
                        report,
                        cleanup: edge,
                    },
                    CleanupEntry::Cancellation => {
                        MirTerminatorKind::CancelCurrentRun { cleanup: edge }
                    }
                    CleanupEntry::Ordinary => unreachable!(),
                };

                self.set_terminator(current, Self::retained_source(source), terminator)?;

                Ok(())
            }
        }
    }

    pub(super) fn set_destination(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        destination: CleanupDestination,
        value: Option<MirOperand>,
    ) -> Result<(), LoweringError> {
        let terminator = match destination {
            CleanupDestination::Goto(target) => {
                MirTerminatorKind::Goto(MirEdge::new(target, value))
            }
            CleanupDestination::Return => MirTerminatorKind::Return(value),
            CleanupDestination::PropagatePanic => MirTerminatorKind::PropagatePanic {
                report: value.ok_or(LoweringError::SemanticValueUnavailable)?,
                runtime: self.runtime_reference(RuntimeAbiRole::PanicPropagation),
            },
            CleanupDestination::PropagateCancellation => MirTerminatorKind::PropagateCancellation {
                runtime: self.runtime_reference(RuntimeAbiRole::CurrentRunCancellationPropagation),
            },
        };

        self.set_terminator(block, Self::retained_source(source), terminator)?;

        Ok(())
    }
}
