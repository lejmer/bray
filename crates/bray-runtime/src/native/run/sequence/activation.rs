use crate::RunOutcome;
use crate::incident::OwnedCleanupIncident;
use crate::native::frame::{NativeFrameTransfer, NativeTerminalState};
use crate::native::run::NativeRun;
use bray_runtime_abi::{
    NativeBrayCallOutcome, NativeFrameEntry, NativeInactiveFrame, NativePanicReportCallbacks,
    NativeRunState,
};
use triomphe::Arc;

impl NativeRun {
    pub(super) fn install_host_frame(
        &self,
        frame: NativeInactiveFrame,
        terminal: &NativeTerminalState,
    ) -> bool {
        if frame.context() == 0 {
            terminal.record_cleanup_incident(OwnedCleanupIncident::runtime_failure());

            return false;
        }

        let mut transfer = NativeFrameTransfer::new(frame.into_protected(NativeFrameEntry::Body));

        let claim = crate::native::frames::claim(
            transfer.frame().context(),
            transfer.entry(),
            transfer.frame().metadata(),
        );

        let Ok(Some(claim)) = claim else {
            terminal.record_cleanup_incident(OwnedCleanupIncident::runtime_failure());
            self.contain_rejected_value_frame(transfer);

            return false;
        };

        // The domain's immutable alternatives bound every generated activation lane.
        let admitted = claim
            .reservation()
            .descriptor()
            .states()
            .iter()
            .all(|actual| {
                self.descriptor.states().iter().any(|allowed| {
                    actual.affinity() == allowed.affinity()
                        && actual.lane_requirements() == allowed.lane_requirements()
                })
            });

        if !admitted {
            terminal.record_cleanup_incident(OwnedCleanupIncident::runtime_failure());
            self.contain_rejected_value_frame(transfer);

            return false;
        }

        let activation = claim.install_activation(transfer.take());
        self.lock_state().current = Some(activation);

        true
    }

    pub(super) fn retain_host_outcome(
        &self,
        outcome: RunOutcome<usize>,
        terminal: &Arc<NativeTerminalState>,
        owner: &Arc<NativeTerminalState>,
        panics: NativePanicReportCallbacks,
    ) -> bool {
        let resolved = crate::native::incident::with_incident_owner(owner, || {
            let outcome = match crate::native::state::task_outcome(outcome, terminal) {
                Ok(outcome) => outcome,
                Err(panic) => {
                    owner.record_cleanup_incident(OwnedCleanupIncident::host(Box::new(panic)));

                    return false;
                }
            };

            let boundary = match outcome.state() {
                NativeRunState::COMPLETED => None,
                NativeRunState::PANICKED => NativeBrayCallOutcome::panicked(outcome.payload()),
                NativeRunState::CANCELLED => Some(NativeBrayCallOutcome::cancelled()),
                _ => {
                    owner.record_cleanup_incident(OwnedCleanupIncident::runtime_failure());

                    return false;
                }
            };

            if let Some(incident) =
                boundary.and_then(|outcome| OwnedCleanupIncident::boundary(outcome, panics))
            {
                owner.record_cleanup_incident(incident);
            }

            true
        });

        Self::retain_incidents(terminal, owner);

        resolved
    }
}
