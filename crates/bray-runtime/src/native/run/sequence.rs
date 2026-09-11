use std::panic::{AssertUnwindSafe, catch_unwind};

use bray_runtime_abi::{
    NativeBrayCallOutcome, NativeCleanupExecution, NativeFrameEntry, NativeRunState,
    NativeStaticFinalizer, NativeStaticFinalizerStatus, NativeStaticIdentity,
};
use triomphe::Arc;

use crate::incident::OwnedCleanupIncident;
use crate::product::{
    StaticCleanup, run_static_destroy, run_static_transition, run_synchronous_finalizer,
};
use crate::{FrameProgress, RunOutcome};

use super::super::frame::{NativeFrameTransfer, NativeTerminalState};
use super::NativeRun;

#[derive(Clone, Copy)]
enum StaticPhase {
    Prepare,
    Start,
    Activation,
    Destroy,
    Detach,
    Report,
}

pub(in crate::native) struct NativeStaticSequence {
    product: usize,
    entries: Vec<StaticCleanup>,
    next: usize,
    phase: StaticPhase,
    terminal: Arc<NativeTerminalState>,
    completed: fn(usize, NativeStaticIdentity, usize),
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
    pub(super) fn fail_static_activation(&self, incident: OwnedCleanupIncident) -> bool {
        let (activation, terminal) = {
            let mut state = self.lock_state();

            if state.current.is_none() {
                return false;
            }

            let Some(sequence) = state.sequence.as_mut() else {
                return false;
            };

            sequence.phase = StaticPhase::Destroy;
            let terminal = Arc::clone(&sequence.terminal);

            (state.current.take(), terminal)
        };

        terminal.record_cleanup_incident(incident);

        // Resolve the invalid activation's ownership, then resume the remaining host phases.
        Self::release_chain(activation, &terminal);

        true
    }

    pub(in crate::native) fn install_sequence(&self, sequence: Box<NativeStaticSequence>) {
        let mut state = self.lock_state();

        assert!(
            state.current.is_none() && state.sequence.is_none(),
            "host sequence must install before dispatch"
        );

        state.sequence = Some(sequence);
    }

    pub(super) fn step_sequence(&self) -> Option<FrameProgress<usize>> {
        let (entry, phase, terminal) = {
            let state = self.lock_state();

            let Some(sequence) = &state.sequence else {
                return Some(FrameProgress::RuntimeFailure);
            };

            let Some(entry) = sequence.entries.get(sequence.next).copied() else {
                return Some(FrameProgress::Completed(0));
            };

            (entry, sequence.phase, Arc::clone(&sequence.terminal))
        };

        // Keep the phase installed, but hold no run or scheduler lock during callbacks.
        let next = super::super::incident::with_incident_owner(&terminal, || {
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

                        let sequence = state
                            .sequence
                            .as_ref()
                            .expect("dispatch retains its host sequence");

                        (sequence.product, sequence.completed)
                    };

                    completed(product, entry.identity, count);

                    StaticPhase::Prepare
                }
            }
        });

        let mut state = self.lock_state();

        let sequence = state
            .sequence
            .as_mut()
            .expect("dispatch retains its host sequence");

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
        let mut frame = super::super::inactive_frame_output();
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

        let mut transfer = NativeFrameTransfer::new(frame.into_protected(NativeFrameEntry::Body));

        let claim = super::super::frames::claim(
            transfer.frame().context(),
            transfer.entry(),
            transfer.frame().metadata(),
        );

        let Ok(Some(mut claim)) = claim else {
            terminal.record_cleanup_incident(OwnedCleanupIncident::runtime_failure());

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

            return false;
        }

        let activation = claim.install_activation(transfer.take());
        self.lock_state().current = Some(activation);

        true
    }

    pub(super) fn finish_static_activation(
        &self,
        outcome: RunOutcome<usize>,
        terminal: &Arc<NativeTerminalState>,
    ) {
        let (finalizer, owner) = {
            let state = self.lock_state();

            let sequence = state
                .sequence
                .as_ref()
                .expect("host completion retains its sequence");

            (
                sequence.entries[sequence.next].finalizer,
                Arc::clone(&sequence.terminal),
            )
        };

        super::super::incident::with_incident_owner(&owner, || {
            match super::super::state::task_outcome(outcome, terminal) {
                Ok(outcome) => {
                    let boundary = match outcome.state() {
                        NativeRunState::COMPLETED => None,
                        NativeRunState::PANICKED => {
                            NativeBrayCallOutcome::panicked(outcome.payload())
                        }
                        NativeRunState::CANCELLED => Some(NativeBrayCallOutcome::cancelled()),
                        _ => {
                            owner.record_cleanup_incident(OwnedCleanupIncident::runtime_failure());

                            None
                        }
                    };

                    if let Some(incident) = boundary.and_then(|outcome| {
                        OwnedCleanupIncident::boundary(outcome, finalizer.panics())
                    }) {
                        owner.record_cleanup_incident(incident);
                    }
                }
                Err(panic) => {
                    owner.record_cleanup_incident(OwnedCleanupIncident::host(Box::new(panic)))
                }
            }

            Self::retain_incidents(terminal, &owner);
        });

        let activation = {
            let mut state = self.lock_state();

            state
                .sequence
                .as_mut()
                .expect("host completion retains its sequence")
                .phase = StaticPhase::Destroy;

            state.current.take()
        };

        drop(activation);
    }
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{NativeBrayCallOutcome, NativeFrameExit, NativeFrameMetadata,
        NativeFrameProgress, NativeFrameProgressKind, NativeProtectedFrame, NativeRuntimeConfiguration,
        NativeRuntimeStatus, NativeStaticIdentity};
    use crate::native::state::{initialize, shutdown, with_runtime, NativeRunReservation};
    use crate::native::frame::{NativeFrame, NativeTerminalState};
    use crate::native::run::NativeActivationReservation;
    use crate::product::StaticCleanup;
    use std::cell::Cell;
    thread_local! { static DESTROYED: Cell<usize> = const { Cell::new(0) }; }
    fn metadata() -> NativeFrameMetadata {
        NativeFrameMetadata::new([163; 32], 1, 1, 1, 0, 1, crate::test_support::native_origin_frame_state)
    }
    extern "C-unwind" fn invalid(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::YIELDED, 99, 0)
    }
    extern "C-unwind" fn ready(_: usize) -> NativeFrameProgress {
        NativeFrameProgress::new(NativeFrameProgressKind::COMPLETED, 0, 0)
    }
    extern "C-unwind" fn ignore(_: usize) {}
    extern "C-unwind" fn lifecycle(_: usize, _: NativeFrameExit) {}
    extern "C-unwind" fn complete(_: usize, _: usize) {}
    extern "C" fn transition() {}
    extern "C-unwind" fn destroy() -> NativeBrayCallOutcome {
        DESTROYED.set(DESTROYED.get() + 1);

        NativeBrayCallOutcome::completed()
    }
    extern "C-unwind" fn start(_: usize, _: &mut NativeBrayCallOutcome) -> bray_runtime_abi::NativeStaticFinalizerStatus {
        panic!("a NONE finalizer is never started");
    }
    extern "C-unwind" fn unexpected(_: usize) -> NativeRuntimeStatus { panic!("no provider panic"); }
    fn completed(_: usize, _: NativeStaticIdentity, count: usize) {
        assert_eq!(count, usize::from(DESTROYED.get() == 1));
    }
    #[test]
    fn invalid_activation_drains_static_destruction_and_remaining_entries() {
        for depth in [0, 64] {
        assert!(initialize(NativeRuntimeConfiguration::new(1, 1)).is_success());
        DESTROYED.set(0);

        let entry = StaticCleanup { identity: NativeStaticIdentity::new([163; 32]), order: 0,
            prepare: transition, destroy, detach: transition,
            finalizer: bray_runtime_abi::NativeStaticFinalizer::new(bray_runtime_abi::NativeCleanupExecution::NONE,
                None, start, crate::test_support::panic_callbacks(unexpected, unexpected)) };

        with_runtime(|runtime| {
            let mut reservation = NativeRunReservation::prepare(NativeFrame::checked_descriptor(&metadata()).unwrap(),
                NativeTerminalState::reserve().unwrap(), crate::task::TaskAdmissionKind::Continuation).unwrap();

            reservation.reserve(&runtime.scheduler).unwrap();

            let sequence = Box::new(super::NativeStaticSequence::new(0, vec![entry; 2],
                NativeTerminalState::reserve().unwrap(), completed));

            reservation.run().install_sequence(sequence);

            reservation.run().install_root(NativeActivationReservation::prepare(&metadata()).unwrap().install(
                NativeProtectedFrame::new(0, metadata(), invalid, invalid, ignore, lifecycle, complete, ignore)));

            if depth != 0 {
                reservation.run().lock_state().current.as_mut().unwrap().state =
                    bray_runtime_model::ProtectedFrameStateId::new(99);
            }

            for _ in 0..depth {
                let mut child = NativeActivationReservation::prepare(&metadata()).unwrap().install(
                    NativeProtectedFrame::new(0, metadata(), ready, ready, ignore, lifecycle, complete, ignore));

                let mut state = reservation.run().lock_state();
                child.parent = state.current.take();
                state.current = Some(child);
            }

            let handle = runtime.allocate_continuation().task().unwrap();
            assert!(runtime.start_run(handle, reservation).is_success());
            let outcome = runtime.resolve_task(handle);
            assert_eq!(outcome.state(), bray_runtime_abi::NativeRunState::COMPLETED);
            assert_eq!(DESTROYED.get(), 2);
            assert!(runtime.destroy_task(handle).is_success());
        }).unwrap();

        assert!(shutdown().is_success());
        }
    }
}
