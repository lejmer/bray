use super::statics::{NativeStaticSequence, StaticPhase};
use super::value::{NativeValueSequence, ValuePhase};
use crate::incident::OwnedCleanupIncident;
use crate::native::frame::NativeTerminalState;
use crate::native::run::NativeRun;
use crate::{FrameProgress, RunOutcome};
use triomphe::Arc;

pub(in crate::native) enum NativeHostSequence {
    Static(NativeStaticSequence),
    Value(NativeValueSequence),
}

impl NativeRun {
    pub(in crate::native) fn install_sequence(&self, sequence: Box<NativeHostSequence>) {
        let mut state = self.lock_state();

        assert!(
            state.current.is_none() && state.sequence.is_none(),
            "host sequence must install before dispatch"
        );

        state.sequence = Some(sequence);
    }

    pub(in crate::native) fn begin_value_cleanup(&self) -> bool {
        let mut state = self.lock_state();

        let Some(NativeHostSequence::Value(sequence)) = state.sequence.as_deref_mut() else {
            return false;
        };

        if sequence.phase != ValuePhase::Dormant {
            return false;
        }

        sequence.phase = ValuePhase::Broadcast;

        true
    }

    pub(in crate::native::run) fn step_sequence(&self) -> Option<FrameProgress<usize>> {
        let value = {
            let state = self.lock_state();

            match state.sequence.as_deref() {
                Some(NativeHostSequence::Static(_)) => false,
                Some(NativeHostSequence::Value(_)) => true,
                None => return Some(FrameProgress::RuntimeFailure),
            }
        };

        if value {
            self.step_value_sequence()
        } else {
            self.step_static_sequence()
        }
    }

    pub(in crate::native::run) fn fail_host_activation(
        &self,
        incident: OwnedCleanupIncident,
    ) -> bool {
        let (activation, owner) = {
            let mut state = self.lock_state();

            if state.current.is_none() || state.sequence.is_none() {
                return false;
            }

            let activation = state.current.take();

            match state
                .sequence
                .as_deref_mut()
                .expect("host sequence remains installed")
            {
                NativeHostSequence::Static(sequence) => {
                    sequence.phase = StaticPhase::Destroy;

                    (activation, Arc::clone(&sequence.terminal))
                }
                NativeHostSequence::Value(sequence) => {
                    sequence.contain(activation);

                    (None, Arc::clone(&self.terminal))
                }
            }
        };

        owner.record_cleanup_incident(incident);

        // Static fields have a separate destruction phase. An unresolved value retains its chain.
        Self::release_chain(activation, &owner);

        true
    }

    pub(in crate::native::run) fn contain_value_activation(&self) -> bool {
        let mut state = self.lock_state();

        if !matches!(state.sequence.as_deref(), Some(NativeHostSequence::Value(sequence))
            if sequence.phase != ValuePhase::Complete)
        {
            return false;
        }

        let activation = state.current.take();

        let Some(NativeHostSequence::Value(sequence)) = state.sequence.as_deref_mut() else {
            unreachable!()
        };

        sequence.contain(activation);

        true
    }

    pub(in crate::native::run) fn finish_host_activation(
        &self,
        outcome: RunOutcome<usize>,
        terminal: &Arc<NativeTerminalState>,
    ) {
        let (panics, owner) = {
            let state = self.lock_state();

            match state
                .sequence
                .as_deref()
                .expect("host completion retains its sequence")
            {
                NativeHostSequence::Static(sequence) => (
                    sequence.entries[sequence.next].finalizer.panics(),
                    Arc::clone(&sequence.terminal),
                ),
                NativeHostSequence::Value(sequence) => {
                    (sequence.lifecycle.panics(), Arc::clone(&self.terminal))
                }
            }
        };

        let resolved = self.retain_host_outcome(outcome, terminal, &owner, panics);

        let activation = {
            let mut state = self.lock_state();
            let activation = state.current.take();

            match state
                .sequence
                .as_deref_mut()
                .expect("host completion retains its sequence")
            {
                NativeHostSequence::Static(sequence) => {
                    sequence.phase = StaticPhase::Destroy;

                    activation
                }
                NativeHostSequence::Value(sequence) if resolved => {
                    sequence.phase = ValuePhase::Complete;

                    activation
                }
                NativeHostSequence::Value(sequence) => {
                    sequence.contain(activation);

                    None
                }
            }
        };

        drop(activation);
    }
}
