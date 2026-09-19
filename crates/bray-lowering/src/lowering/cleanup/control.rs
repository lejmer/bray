use bray_bound_tree::{AnyBoundNodeId, BoundBlockId};
use bray_ir::{
    MirAsyncOperation, MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge,
    MirOperand, MirOperationKind, MirSourceAnchor, MirTaskTerminalState, MirTerminatorKind,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeId;

use crate::input::ScopeExitCleanupStatus;
use crate::lowering::LoweringError;
use crate::lowering::inputs::{InputExit, InputTemporary};
use crate::lowering::lowerer::Lowerer;

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::lowering) enum CleanupDestination {
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

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(in crate::lowering) enum TerminalState {
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
            self.scope_cleanup_status(scope, exit),
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
                .unwrap_or_else(|| {
                    panic!(
                        "lowering cleanup contract violated: protected-frame exit {exit:?} has no callable result type"
                    )
                });

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
        if let Some(block) = self.terminal_state_blocks.get(&terminal).copied() {
            return Ok(block);
        }

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

        self.terminal_state_blocks.insert(terminal, block);

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
        let plans = self.cleanup_plans(scope_depth, exit);

        let input_exit = match &entry {
            CleanupEntry::Ordinary => InputExit::Scope(scope_depth),
            CleanupEntry::Panic(_) => self.abnormal_input_exit(destination),
            CleanupEntry::Cancellation => InputExit::All,
        };

        if self.input_cleanup(input_exit).is_empty() && plans.iter().all(|plan| !plan.has_cleanup())
        {
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

        self.finish_ordinary_cleanup(
            current,
            source,
            destination,
            value,
            exit,
            &plans,
            input_exit,
        )
    }

    pub(super) fn cleanup_plans(
        &self,
        scope_depth: usize,
        exit: AnyBoundNodeId,
    ) -> Vec<bray_bound_tree::AsyncScopeExitPlan> {
        self.input
            .cleanup_plans(&self.active_scopes, scope_depth, exit)
    }

    fn scope_cleanup_status(
        &self,
        scope: BoundBlockId,
        exit: AnyBoundNodeId,
    ) -> ScopeExitCleanupStatus {
        self.input.scope_cleanup_status(scope, exit)
    }

    pub(super) fn push_cleanup_operations(
        &mut self,
        mut block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        plans: &[bray_bound_tree::AsyncScopeExitPlan],
        temporaries: &[InputTemporary],
        mut value: Option<(bray_ir::MirValueId, TypeId)>,
        failures: &std::collections::BTreeMap<
            BoundBlockId,
            (MirBlockId, MirBlockId, bray_ir::MirPlace),
        >,
    ) -> Result<(MirBlockId, Option<(bray_ir::MirValueId, TypeId)>), LoweringError> {
        let mut temporaries = temporaries.iter().rev().peekable();

        for plan in plans {
            self.cleanup_failure_targets = failures.get(&plan.scope()).cloned();

            let depth = self
                .active_scopes
                .iter()
                .position(|scope| *scope == plan.scope())
                .unwrap_or_else(|| {
                    panic!(
                        "lowering contract violation: MissingBoundNode {value:?}",
                        value = plan.scope()
                    )
                })
                + 1;

            block = self.push_input_cleanup(block, phase, &mut temporaries, depth)?;

            for access in match phase {
                MirCleanupPhase::TaskCancellation => plan.cancellation_broadcast(),
                MirCleanupPhase::LifecycleResolution => plan.lifecycle_resolution(),
            } {
                let completed = self.input.finalizer_is_complete(plan.exit(), *access);

                (block, value) =
                    self.push_cleanup_access(block, source, phase, *access, completed, value)?;
            }
        }

        block = self.push_input_cleanup(block, phase, &mut temporaries, 0)?;
        self.cleanup_failure_targets = None;

        Ok((block, value))
    }

    pub(super) fn push_cleanup_access(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        access: bray_bound_tree::StorageAccessId,
        completed: bool,
        value: Option<(bray_ir::MirValueId, TypeId)>,
    ) -> Result<(MirBlockId, Option<(bray_ir::MirValueId, TypeId)>), LoweringError> {
        let place = self.place_for_access(access, false)?;

        self.push_cleanup_place(block, source, phase, access, place, completed, value)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "cleanup construction requires the resolved place and its checked context"
    )]
    pub(super) fn push_cleanup_place(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        access: bray_bound_tree::StorageAccessId,
        place: bray_ir::MirPlace,
        completed: bool,
        value: Option<(bray_ir::MirValueId, TypeId)>,
    ) -> Result<(MirBlockId, Option<(bray_ir::MirValueId, TypeId)>), LoweringError> {
        let cleanup_source = self.cleanup_source(access);
        let state = self.initialization_guards.get(&place.storage());
        let guard = state.map(|state| Self::retained_place(&state.guard));

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
            return self.push_guarded_cleanup(
                block,
                &cleanup_source,
                phase,
                place,
                guard,
                None,
                completed,
                value,
            );
        }

        self.guarded_cleanup_region(
            block,
            source,
            place,
            guard,
            value,
            |lowerer, block, place, value| {
                lowerer.push_part_cleanup(
                    block,
                    &cleanup_source,
                    phase,
                    access,
                    place,
                    &parts,
                    0,
                    value,
                )
            },
        )
    }

    pub(super) fn cleanup_source(
        &self,
        access: bray_bound_tree::StorageAccessId,
    ) -> MirSourceAnchor {
        let source = self
            .input
            .storage_plan()
            .access(access)
            .unwrap_or_else(|| {
                panic!("lowering cleanup contract violated: missing storage access {access:?}")
            })
            .source();

        self.source(bray_bound_tree::BoundNodeOrigin::source(source))
    }

    pub(in crate::lowering) fn push_guarded_cleanup(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        place: bray_ir::MirPlace,
        guard: Option<bray_ir::MirPlace>,
        release: Option<bray_bound_tree::StorageProtocolCall>,
        completed: bool,
        value: Option<(bray_ir::MirValueId, TypeId)>,
    ) -> Result<(MirBlockId, Option<(bray_ir::MirValueId, TypeId)>), LoweringError> {
        self.guarded_cleanup_region(
            block,
            source,
            place,
            guard,
            value,
            |lowerer, block, place, value| {
                let owner = place.ty();
                lowerer.push_cleanup_action(block, source, phase, place, release, completed)?;

                let block = if let Some(outcome) = &lowerer.cleanup_outcome {
                    outcome.check(&mut lowerer.builder, block, source)?
                } else {
                    lowerer.check_ordinary_cleanup(block, source, release.map(|_| owner))?
                };

                if release.is_some() {
                    lowerer.discharge_outgoing_owner(block, source, owner)?;
                }

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
            let kind = self.builder.block_kind(block);

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
        completed: bool,
    ) -> Result<(), LoweringError> {
        if let Some(release) = release {
            self.set_storage_initialized(block, source, &place, false)?;
            self.push_storage_protocol_call(block, source, &place, release, true)?;
        } else {
            self.push_operation(
                block,
                Self::retained_source(source),
                if completed && phase == MirCleanupPhase::LifecycleResolution {
                    MirOperationKind::Destroy(place)
                } else {
                    MirOperationKind::Cleanup { phase, place }
                },
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
                report: value.unwrap_or_else(|| {
                    panic!(
                        "lowering cleanup contract violated: panic propagation from block {block:?} requires a report operand"
                    )
                }),
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
