use bray_runtime_abi::{NativeRunOutcome, NativeRuntimeStatus};

use super::NativeRun;

struct ChildTransfer<'a> {
    run: &'a NativeRun,
    outcome: NativeRunOutcome,
    committed: bool,
}

impl Drop for ChildTransfer<'_> {
    fn drop(&mut self) {
        let removed = {
            let mut current = self.run.lock_current();

            // The child stays installed throughout the callback, excluding reentrant transfers.
            let parent = current.as_mut().expect("transfer retains its parent activation");

            if self.committed {
                parent.child.take()
            } else {
                parent.child.as_mut().expect("transfer retains its child").outcome = Some(self.outcome);

                None
            }
        };

        drop(removed);
    }
}

impl NativeRun {
    pub(in crate::native) fn transfer_child(
        &self,
        transfer: impl FnOnce(NativeRunOutcome) -> Result<(), NativeRuntimeStatus>,
    ) -> NativeRuntimeStatus {
        let outcome = {
            let mut current = self.lock_current();

            let Some(child) = current.as_mut().and_then(|parent| parent.child.as_mut()) else {
                return NativeRuntimeStatus::UNKNOWN_TASK;
            };

            let Some(outcome) = child.outcome.take() else {
                return NativeRuntimeStatus::PENDING;
            };

            outcome
        };

        let mut claim = ChildTransfer { run: self, outcome, committed: false };

        if let Err(status) = transfer(outcome) {
            return status;
        }

        claim.committed = true;

        NativeRuntimeStatus::SUCCESS
    }
}
