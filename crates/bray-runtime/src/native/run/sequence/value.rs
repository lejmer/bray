use super::NativeHostSequence;
use crate::FrameProgress;
use crate::native::run::{NativeActivation, NativeRun};
use crate::native::storage::NativeStorage;
use crate::product::ProductEntry;
use bray_runtime_abi::NativeValueCleanup;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum ValuePhase {
    Dormant,
    Broadcast,
    Start,
    Activation,
    Complete,
    Failed,
}

pub(in crate::native) struct NativeValueSequence {
    error_offset: usize,
    broadcast: Option<NativeValueCleanup>,
    pub(super) lifecycle: NativeValueCleanup,
    pub(super) phase: ValuePhase,
    failed_activation: Option<Box<NativeActivation>>,
    rejected_frame: Option<crate::native::frame::NativeFrameTransfer>,
    storage: NativeStorage,
    _entry: ProductEntry,
}

impl NativeValueSequence {
    pub(in crate::native) fn new(
        storage: NativeStorage,
        error_offset: usize,
        broadcast: Option<NativeValueCleanup>,
        lifecycle: NativeValueCleanup,
        entry: ProductEntry,
    ) -> Self {
        Self {
            storage,
            error_offset,
            broadcast,
            lifecycle,
            phase: ValuePhase::Dormant,
            failed_activation: None,
            rejected_frame: None,
            _entry: entry,
        }
    }

    pub(super) fn contain(&mut self, activation: Option<Box<NativeActivation>>) {
        self.phase = ValuePhase::Failed;

        if activation.is_some() {
            assert!(
                self.failed_activation.is_none(),
                "failed value retains one active chain"
            );

            self.failed_activation = activation;
        }
    }
}

impl NativeRun {
    pub(super) fn contain_rejected_value_frame(
        &self,
        frame: crate::native::frame::NativeFrameTransfer,
    ) {
        let mut frame = Some(frame);

        {
            let mut state = self.lock_state();

            if let Some(NativeHostSequence::Value(sequence)) = state.sequence.as_deref_mut() {
                assert!(
                    sequence.rejected_frame.is_none(),
                    "one rejected lifecycle frame remains owned"
                );

                sequence.rejected_frame = frame.take();
                sequence.phase = ValuePhase::Failed;
            }
        }

        // Static finalizers retain their existing rollback behavior, outside the run lock.
        drop(frame);
    }

    pub(in crate::native::run) fn step_value_sequence(&self) -> Option<FrameProgress<usize>> {
        let (phase, value, broadcast, lifecycle) = {
            let state = self.lock_state();

            let Some(NativeHostSequence::Value(sequence)) = state.sequence.as_deref() else {
                return Some(FrameProgress::RuntimeFailure);
            };

            (
                sequence.phase,
                sequence
                    .storage
                    .pointer()
                    .wrapping_add(sequence.error_offset)
                    .addr(),
                sequence.broadcast,
                sequence.lifecycle,
            )
        };

        let next = crate::native::incident::with_incident_owner(&self.terminal, || match phase {
            ValuePhase::Dormant | ValuePhase::Failed => None,
            ValuePhase::Complete => None,
            ValuePhase::Broadcast => {
                if let Some(broadcast) = broadcast {
                    for incident in crate::native::value_cleanup::run(broadcast, value) {
                        self.terminal.record_cleanup_incident(incident);
                    }
                }

                Some(ValuePhase::Start)
            }
            ValuePhase::Start => Some(if self.start_value_frame(lifecycle, value) {
                ValuePhase::Activation
            } else {
                ValuePhase::Failed
            }),
            ValuePhase::Activation => Some(ValuePhase::Failed),
        });

        let Some(next) = next else {
            return Some(if phase == ValuePhase::Complete {
                FrameProgress::Completed(0)
            } else {
                FrameProgress::RuntimeFailure
            });
        };

        let mut state = self.lock_state();

        let Some(NativeHostSequence::Value(sequence)) = state.sequence.as_deref_mut() else {
            unreachable!()
        };

        sequence.phase = next;

        None
    }

    fn start_value_frame(&self, cleanup: NativeValueCleanup, value: usize) -> bool {
        crate::native::value_cleanup::start(cleanup, value, |incident| {
            self.terminal.record_cleanup_incident(incident);
        })
        .is_some_and(|frame| self.install_host_frame(frame, &self.terminal))
    }
}
