use bray_runtime_abi::{
    NativeRunOutcome, NativeRunResultLayout, NativeRunState, NativeRuntimeStatus, NativeTaskHandle,
};

use super::state::with_runtime;

pub(super) fn destroy_terminal_task(
    task: NativeTaskHandle,
    cleanup: Option<&'static bray_runtime_abi::NativeTaskTerminalCleanup>,
) -> NativeRuntimeStatus {
    if cleanup.is_some_and(|cleanup| !cleanup.is_valid()) {
        return NativeRuntimeStatus::INVALID_ARGUMENT;
    }

    with_runtime(|runtime| {
        if let Some(cleanup) = cleanup {
            let incidents = runtime.consume_task_outcome(task, |outcome| {
                if outcome.state() == NativeRunState::COMPLETED {
                    return cleanup.value().map_or_else(Vec::new, |value| {
                        super::value_cleanup::run(*value, outcome.payload())
                    });
                }

                if outcome.state() == NativeRunState::PANICKED {
                    let Some(outcome) =
                        bray_runtime_abi::NativeBrayCallOutcome::panicked(outcome.payload())
                    else {
                        return vec![crate::incident::OwnedCleanupIncident::panic(Box::new(
                            NativeRuntimeStatus::INVALID_ARGUMENT,
                        ))];
                    };

                    return crate::incident::OwnedCleanupIncident::boundary(
                        outcome,
                        cleanup.panics(),
                    )
                    .into_iter()
                    .collect();
                }

                Vec::new()
            });

            let incidents = match incidents {
                Ok(incidents) => incidents.unwrap_or_default(),
                Err(status) => return status,
            };

            super::incident::retain_cleanup_incidents(incidents);
        }

        runtime.destroy_task(task)
    })
    .unwrap_or_else(|status| status)
}

pub(super) fn transfer_outcome(
    outcome: NativeRunOutcome,
    destination: usize,
    layout: NativeRunResultLayout,
) -> Result<(), NativeRuntimeStatus> {
    let status = layout.transfer(destination, &outcome);

    if status.is_success() {
        Ok(())
    } else {
        Err(status)
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{
        NativeRunOutcome, NativeRunResultLayout, NativeRunState, NativeRuntimeStatus,
    };

    use super::transfer_outcome;

    #[test]
    fn terminal_outcomes_transfer_through_the_concrete_callback() {
        for state in [
            NativeRunState::COMPLETED,
            NativeRunState::PANICKED,
            NativeRunState::CANCELLED,
        ] {
            let outcome = NativeRunOutcome::new(state, 8);

            let layout =
                NativeRunResultLayout::new(16, 8, crate::test_support::record_run_result_transfer);

            assert_eq!(transfer_outcome(outcome, 16, layout), Ok(()));

            assert_eq!(
                crate::test_support::take_run_result_transfer(),
                Some((16, outcome))
            );
        }
    }

    #[test]
    fn unresolved_and_failed_outcomes_preserve_ownership_and_runtime_status() {
        let layout =
            NativeRunResultLayout::new(16, 8, crate::test_support::record_run_result_transfer);

        crate::test_support::take_run_result_transfer();

        for (state, payload, expected) in [
            (NativeRunState::PENDING, 0, NativeRuntimeStatus::PENDING),
            (
                NativeRunState::RUNTIME_FAILURE,
                0,
                NativeRuntimeStatus::RUNTIME_FAILURE,
            ),
            (
                NativeRunState::COMPLETED,
                0,
                NativeRuntimeStatus::INVALID_ARGUMENT,
            ),
        ] {
            assert_eq!(
                transfer_outcome(NativeRunOutcome::new(state, payload), 16, layout),
                Err(expected)
            );

            assert_eq!(crate::test_support::take_run_result_transfer(), None);
        }
    }
}
