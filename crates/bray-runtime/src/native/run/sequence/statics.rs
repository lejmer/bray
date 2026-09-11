use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeBrayCallOutcome, NativeCleanupExecution, NativeStaticFinalizer,
    NativeStaticFinalizerStatus, NativeStaticIdentity,
};
use triomphe::Arc;

use crate::FrameProgress;
use crate::incident::OwnedCleanupIncident;
use crate::product::{
    StaticCleanup, run_static_destroy, run_static_transition, run_synchronous_finalizer,
};

use super::NativeHostSequence;
use crate::native::frame::NativeTerminalState;
use crate::native::run::NativeRun;

#[derive(Clone, Copy)]
pub(super) enum StaticPhase {
    Prepare,
    Start,
    Activation,
    Destroy,
    Detach,
    Report,
}

pub(in crate::native) struct NativeStaticSequence {
    pub(super) product: usize,
    pub(super) entries: Vec<StaticCleanup>,
    pub(super) next: usize,
    pub(super) phase: StaticPhase,
    pub(super) terminal: Arc<NativeTerminalState>,
    pub(super) completed: fn(usize, NativeStaticIdentity, usize),
}

impl NativeStaticSequence {
    pub(in crate::native) fn new(
        product: usize,
        entries: Vec<StaticCleanup>,
        terminal: Arc<NativeTerminalState>,
        completed: fn(usize, NativeStaticIdentity, usize),
    ) -> Self {
        Self {
            product,
            entries,
            next: 0,
            phase: StaticPhase::Prepare,
            terminal,
            completed,
        }
    }
}

impl NativeRun {
    pub(in crate::native::run) fn step_static_sequence(&self) -> Option<FrameProgress<usize>> {
        let (entry, phase, terminal) = {
            let state = self.lock_state();

            let Some(NativeHostSequence::Static(sequence)) = state.sequence.as_deref() else {
                return Some(FrameProgress::RuntimeFailure);
            };

            let Some(entry) = sequence.entries.get(sequence.next).copied() else {
                return Some(FrameProgress::Completed(0));
            };

            (entry, sequence.phase, Arc::clone(&sequence.terminal))
        };

        // Keep the phase installed, but hold no run or scheduler lock during callbacks.
        let next = crate::native::incident::with_incident_owner(&terminal, || {
            let record = |incident| terminal.record_cleanup_incident(incident);

            match phase {
                StaticPhase::Prepare => {
                    if let Some(incident) = run_static_transition(entry.prepare) {
                        record(incident);
                    }

                    StaticPhase::Start
                }
                StaticPhase::Start => match entry.finalizer.execution() {
                    NativeCleanupExecution::ASYNCHRONOUS => {
                        if self.start_static_frame(entry.finalizer, &terminal) {
                            StaticPhase::Activation
                        } else {
                            StaticPhase::Destroy
                        }
                    }
                    NativeCleanupExecution::SYNCHRONOUS => {
                        for incident in run_synchronous_finalizer(entry.finalizer) {
                            record(incident);
                        }

                        StaticPhase::Destroy
                    }
                    NativeCleanupExecution::NONE => StaticPhase::Destroy,
                    _ => {
                        record(OwnedCleanupIncident::runtime_failure());

                        StaticPhase::Destroy
                    }
                },
                StaticPhase::Activation => {
                    record(OwnedCleanupIncident::runtime_failure());

                    StaticPhase::Destroy
                }
                StaticPhase::Destroy => {
                    if let Some(incident) =
                        run_static_destroy(entry.destroy, entry.finalizer.panics())
                    {
                        record(incident);
                    }

                    StaticPhase::Detach
                }
                StaticPhase::Detach => {
                    if let Some(incident) = run_static_transition(entry.detach) {
                        record(incident);
                    }

                    StaticPhase::Report
                }
                StaticPhase::Report => {
                    let mut count = 0usize;

                    loop {
                        let incidents = terminal.take_cleanup_incidents();

                        if incidents.is_empty() {
                            break;
                        }

                        count = count.saturating_add(incidents.len());

                        for incident in incidents {
                            let _ = incident.report();
                        }
                    }

                    let (product, completed) = {
                        let state = self.lock_state();

                        let NativeHostSequence::Static(sequence) = state
                            .sequence
                            .as_deref()
                            .expect("dispatch retains its host sequence")
                        else {
                            unreachable!()
                        };

                        (sequence.product, sequence.completed)
                    };

                    completed(product, entry.identity, count);

                    StaticPhase::Prepare
                }
            }
        });

        let mut state = self.lock_state();

        let NativeHostSequence::Static(sequence) = state
            .sequence
            .as_deref_mut()
            .expect("dispatch retains its host sequence")
        else {
            unreachable!()
        };

        sequence.phase = next;

        if matches!(phase, StaticPhase::Report) {
            sequence.next += 1;
        }

        None
    }

    fn start_static_frame(
        &self,
        finalizer: NativeStaticFinalizer,
        terminal: &NativeTerminalState,
    ) -> bool {
        let mut frame = crate::native::inactive_frame_output();
        let destination = (&raw mut frame).addr();
        let mut outcome = NativeBrayCallOutcome::completed();

        let result = catch_unwind(AssertUnwindSafe(|| {
            (finalizer.start())(destination, &mut outcome)
        }));

        if let Some(incident) = OwnedCleanupIncident::boundary(outcome, finalizer.panics()) {
            terminal.record_cleanup_incident(incident);

            if let Err(payload) = result {
                terminal.record_cleanup_incident(OwnedCleanupIncident::host(payload));
            }

            return false;
        }

        match result {
            Ok(NativeStaticFinalizerStatus::SUCCESS) => {}
            Ok(_) => {
                terminal.record_cleanup_incident(OwnedCleanupIncident::runtime_failure());

                return false;
            }
            Err(payload) => {
                terminal.record_cleanup_incident(OwnedCleanupIncident::host(payload));

                return false;
            }
        }

        self.install_host_frame(frame, terminal)
    }
}
