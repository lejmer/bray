use bray_bound_tree::BoundBlockId;
use bray_ir::{
    MirAsyncOperation, MirBlockId, MirBlockKind, MirCleanupEdge, MirCleanupPhase, MirEdge,
    MirOperand, MirOperationKind, MirSourceAnchor, MirTaskTerminalState, MirTerminatorKind,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeId;

use super::LoweringError;
use super::lowerer::Lowerer;

enum CleanupDestination {
    Goto(MirBlockId),
    Return,
    PropagatePanic,
    Unreachable,
}

enum CleanupEntry {
    Ordinary,
    Panic(MirOperand),
    Cancellation,
}

enum TerminalState {
    Completed(TypeId),
    Panicked(TypeId),
    Cancelled,
}

impl Lowerer<'_> {
    pub(super) fn suspension_cleanup_edge(
        &mut self,
        source: &MirSourceAnchor,
    ) -> Result<MirCleanupEdge, LoweringError> {
        let plans = self.cleanup_plans(0)?;

        let cancellation = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        let lifecycle = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        self.push_cleanup_operations(
            cancellation,
            source,
            MirCleanupPhase::TaskCancellation,
            &plans,
        )?;

        self.builder.set_terminator(
            cancellation,
            Self::retained_source(source),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(lifecycle, []),
            )),
        )?;

        self.push_cleanup_operations(
            lifecycle,
            source,
            MirCleanupPhase::LifecycleResolution,
            &plans,
        )?;

        let terminal = self.terminal_state_block(source, TerminalState::Cancelled)?;

        self.builder.set_terminator(
            lifecycle,
            Self::retained_source(source),
            MirTerminatorKind::Goto(MirEdge::new(terminal, [])),
        )?;

        Ok(MirCleanupEdge::new(
            MirCleanupPhase::TaskCancellation,
            MirEdge::new(cancellation, []),
        ))
    }

    pub(super) fn finish_scope(
        &mut self,
        scope: BoundBlockId,
        current: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<MirBlockId, LoweringError> {
        if !self.scope_has_cleanup(scope)? {
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
        )?;

        Ok(continuation)
    }

    pub(super) fn finish_exit_to_block(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        scope_depth: usize,
        target: MirBlockId,
        value: Option<(MirOperand, TypeId)>,
    ) -> Result<(), LoweringError> {
        self.finish_cleanup(
            current,
            source,
            scope_depth,
            CleanupEntry::Ordinary,
            CleanupDestination::Goto(target),
            value,
        )
    }

    pub(super) fn finish_return(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        mut value: Option<(MirOperand, TypeId)>,
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
        )
    }

    pub(super) fn finish_panic(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        report: MirOperand,
        report_type: TypeId,
        catch: Option<MirBlockId>,
        scope_depth: usize,
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
        )
    }

    pub(super) fn finish_cancellation(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<(), LoweringError> {
        let destination = if self.input.unit_kind().protected_frame().is_some() {
            CleanupDestination::Goto(self.terminal_state_block(source, TerminalState::Cancelled)?)
        } else {
            CleanupDestination::Unreachable
        };

        self.finish_cleanup(
            current,
            source,
            0,
            CleanupEntry::Cancellation,
            destination,
            None,
        )
    }

    fn terminal_state_block(
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

        self.builder.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Async(MirAsyncOperation::PublishTerminalState {
                state,
                runtime: self.runtime_reference(RuntimeAbiRole::TerminalPublication),
            }),
            None,
        )?;

        self.builder.set_terminator(
            block,
            Self::retained_source(source),
            MirTerminatorKind::Return(None),
        )?;

        Ok(block)
    }

    fn finish_cleanup(
        &mut self,
        current: MirBlockId,
        source: &MirSourceAnchor,
        scope_depth: usize,
        entry: CleanupEntry,
        destination: CleanupDestination,
        value: Option<(MirOperand, TypeId)>,
    ) -> Result<(), LoweringError> {
        let plans = self.cleanup_plans(scope_depth)?;

        if plans.iter().all(|plan| {
            plan.cancellation_broadcast().is_empty() && plan.lifecycle_resolution().is_empty()
        }) {
            return self.set_direct_exit(current, source, entry, destination, value);
        }

        let cancellation = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::CleanupBroadcast,
        )?;

        let lifecycle = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let cancellation_value = self.cleanup_parameter(cancellation, source, value.as_ref())?;
        let lifecycle_value = self.cleanup_parameter(lifecycle, source, value.as_ref())?;

        let entry_value = value
            .as_ref()
            .map(|(value, _)| Self::retained_operand(value));

        self.push_cleanup_operations(
            cancellation,
            source,
            MirCleanupPhase::TaskCancellation,
            &plans,
        )?;

        self.builder.set_terminator(
            cancellation,
            Self::retained_source(source),
            MirTerminatorKind::ContinueCleanup(MirCleanupEdge::new(
                MirCleanupPhase::LifecycleResolution,
                MirEdge::new(lifecycle, cancellation_value.map(MirOperand::Value)),
            )),
        )?;

        self.push_cleanup_operations(
            lifecycle,
            source,
            MirCleanupPhase::LifecycleResolution,
            &plans,
        )?;

        self.set_destination(
            lifecycle,
            source,
            destination,
            lifecycle_value.map(MirOperand::Value),
        )?;

        let edge = MirCleanupEdge::new(
            MirCleanupPhase::TaskCancellation,
            MirEdge::new(cancellation, entry_value),
        );

        let terminator = match entry {
            CleanupEntry::Ordinary => MirTerminatorKind::BeginCleanup(edge),
            CleanupEntry::Panic(report) => MirTerminatorKind::Panic {
                report,
                cleanup: edge,
            },
            CleanupEntry::Cancellation => MirTerminatorKind::CancelCurrentRun { cleanup: edge },
        };

        self.builder
            .set_terminator(current, Self::retained_source(source), terminator)?;

        Ok(())
    }

    fn cleanup_plans(
        &self,
        scope_depth: usize,
    ) -> Result<Vec<bray_bound_tree::AsyncScopeExitPlan>, LoweringError> {
        let mut plans = Vec::new();

        for scope in self
            .active_scopes
            .get(scope_depth..)
            .unwrap_or_default()
            .iter()
            .rev()
        {
            let Some(plan) = self
                .input
                .async_facts()
                .scope_exits()
                .iter()
                .find(|plan| plan.scope() == *scope)
            else {
                if self.scope_requires_cleanup_plan(*scope) {
                    return Err(LoweringError::MissingCleanupPlan(*scope));
                }

                continue;
            };

            plans.push(plan.clone());
        }

        Ok(plans)
    }

    fn scope_has_cleanup(&self, scope: BoundBlockId) -> Result<bool, LoweringError> {
        let Some(plan) = self
            .input
            .async_facts()
            .scope_exits()
            .iter()
            .find(|plan| plan.scope() == scope)
        else {
            if self.scope_requires_cleanup_plan(scope) {
                return Err(LoweringError::MissingCleanupPlan(scope));
            }

            return Ok(false);
        };

        Ok(!plan.cancellation_broadcast().is_empty() || !plan.lifecycle_resolution().is_empty())
    }

    fn scope_requires_cleanup_plan(&self, scope: BoundBlockId) -> bool {
        self.input
            .storage_flow()
            .exits()
            .iter()
            .any(|exit| exit.scope() == scope)
    }

    fn push_cleanup_operations(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        phase: MirCleanupPhase,
        plans: &[bray_bound_tree::AsyncScopeExitPlan],
    ) -> Result<(), LoweringError> {
        for access in plans.iter().flat_map(|plan| match phase {
            MirCleanupPhase::TaskCancellation => plan.cancellation_broadcast(),
            MirCleanupPhase::LifecycleResolution => plan.lifecycle_resolution(),
        }) {
            let place = self.place_for_access(*access)?;

            self.builder.push_operation(
                block,
                Self::retained_source(source),
                MirOperationKind::Cleanup { phase, place },
                None,
            )?;
        }

        Ok(())
    }

    fn cleanup_parameter(
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

                self.builder.set_terminator(
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

                self.builder
                    .set_terminator(current, Self::retained_source(source), terminator)?;

                Ok(())
            }
        }
    }

    fn set_destination(
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
            CleanupDestination::Unreachable => MirTerminatorKind::Unreachable,
        };

        self.builder
            .set_terminator(block, Self::retained_source(source), terminator)?;

        Ok(())
    }
}
