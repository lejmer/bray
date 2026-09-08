use bray_runtime_abi::{NativeRunOutcome, NativeRunState, NativeRuntimeStatus, NativeTaskHandle};

use super::core::{NativeRuntime, NativeTaskSlot, TerminalOutcome};

/// Pins a terminal value during an exclusive callback and restores ownership unless a move commits.
struct OutcomeTransfer<'a> {
    runtime: &'a NativeRuntime,
    task: NativeTaskHandle,
    outcome: NativeRunOutcome,
    committed: bool,
}

impl Drop for OutcomeTransfer<'_> {
    fn drop(&mut self) {
        let mut tasks = self
            .runtime
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        if let Some(NativeTaskSlot::Terminal { outcome, .. }) = tasks.get_mut(&self.task) {
            *outcome = if self.committed {
                TerminalOutcome::Consumed
            } else {
                TerminalOutcome::Available(self.outcome)
            };
        }
    }
}

impl NativeRuntime {
    pub(in crate::native) fn transfer_task_outcome(
        &self,
        task: NativeTaskHandle,
        transfer: impl FnOnce(NativeRunOutcome) -> Result<(), NativeRuntimeStatus>,
    ) -> Result<(), NativeRuntimeStatus> {
        let mut claim = self.claim_task_outcome(task)?;

        transfer(claim.outcome)?;
        claim.committed = true;

        Ok(())
    }

    pub(in crate::native) fn consume_task_outcome<T>(
        &self,
        task: NativeTaskHandle,
        consume: impl FnOnce(NativeRunOutcome) -> T,
    ) -> Result<Option<T>, NativeRuntimeStatus> {
        {
            let tasks = self
                .tasks
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);

            if matches!(
                tasks.get(&task),
                Some(NativeTaskSlot::Terminal {
                    outcome: TerminalOutcome::Consumed,
                    ..
                })
            ) {
                return Ok(None);
            }
        }

        let mut claim = self.claim_task_outcome(task)?;

        // Destruction owns the value even when its callback unwinds after partial teardown.
        claim.committed = true;

        Ok(Some(consume(claim.outcome)))
    }

    pub(in crate::native) fn borrow_task_completion(
        &self,
        task: NativeTaskHandle,
    ) -> Result<Option<std::num::NonZeroUsize>, NativeRuntimeStatus> {
        self.update_available_task_outcome(task, |outcome| {
            if outcome.state() != NativeRunState::COMPLETED {
                return Ok((TerminalOutcome::Available(outcome), None));
            }

            let address = std::num::NonZeroUsize::new(outcome.payload())
                .ok_or(NativeRuntimeStatus::INVALID_ARGUMENT)?;

            Ok((TerminalOutcome::Borrowed(outcome), Some(address)))
        })
    }

    pub(in crate::native) fn release_task_completion_borrow(
        &self,
        task: NativeTaskHandle,
    ) -> NativeRuntimeStatus {
        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(NativeTaskSlot::Terminal { outcome, .. }) = tasks.get_mut(&task) else {
            return NativeRuntimeStatus::UNKNOWN_TASK;
        };

        let TerminalOutcome::Borrowed(value) = outcome else {
            return NativeRuntimeStatus::INVALID_ARGUMENT;
        };

        *outcome = TerminalOutcome::Available(*value);

        NativeRuntimeStatus::SUCCESS
    }

    fn claim_task_outcome(
        &self,
        task: NativeTaskHandle,
    ) -> Result<OutcomeTransfer<'_>, NativeRuntimeStatus> {
        self.update_available_task_outcome(task, |outcome| {
            let claim = OutcomeTransfer {
                runtime: self,
                task,
                outcome,
                committed: false,
            };

            Ok((TerminalOutcome::Transferring, claim))
        })
    }

    fn update_available_task_outcome<T>(
        &self,
        task: NativeTaskHandle,
        update: impl FnOnce(NativeRunOutcome) -> Result<(TerminalOutcome, T), NativeRuntimeStatus>,
    ) -> Result<T, NativeRuntimeStatus> {
        let observed = self.observe(task);

        match observed.state() {
            NativeRunState::COMPLETED | NativeRunState::PANICKED | NativeRunState::CANCELLED => {}
            NativeRunState::PENDING => return Err(NativeRuntimeStatus::PENDING),
            NativeRunState::RUNTIME_FAILURE => {
                let code = u32::try_from(observed.payload())
                    .map_err(|_| NativeRuntimeStatus::INVALID_ARGUMENT)?;

                return Err(NativeRuntimeStatus::from_code(code));
            }
            _ => return Err(NativeRuntimeStatus::INVALID_ARGUMENT),
        }

        let mut tasks = self
            .tasks
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);

        let Some(NativeTaskSlot::Terminal { outcome, .. }) = tasks.get_mut(&task) else {
            return Err(NativeRuntimeStatus::UNKNOWN_TASK);
        };

        let available = match outcome {
            TerminalOutcome::Available(value) => *value,
            TerminalOutcome::Transferring | TerminalOutcome::Borrowed(_) => {
                return Err(NativeRuntimeStatus::PENDING);
            }
            TerminalOutcome::Consumed => return Err(NativeRuntimeStatus::INVALID_ARGUMENT),
        };

        let (replacement, result) = update(available)?;

        *outcome = replacement;

        Ok(result)
    }
}
