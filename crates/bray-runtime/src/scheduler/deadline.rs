use std::time::Duration;

use bray_runtime_abi::{
    NativeDeadlineReference, NativeEventCallback, NativePlatformStatus, NativeRuntimeStatus,
    NativeSchedulerReference,
};

use super::contract::SchedulerError;

// Every call borrows the actual Source allocation retained by SchedulerData.
#[expect(
    unsafe_code,
    reason = "the Bray provider owns deadline storage and transitions"
)]
unsafe extern "C" {
    #[link_name = "bray_runtime_deadline_creation"]
    safe fn creation(
        scheduler: &NativeSchedulerReference,
        limit: usize,
        output: &mut NativeDeadlineReference,
    ) -> u32;
    #[link_name = "bray_runtime_deadline_destruction"]
    safe fn destruction(reference: &NativeDeadlineReference);
    #[link_name = "bray_runtime_deadline_registration"]
    safe fn registration(
        reference: &NativeDeadlineReference,
        deadline: u64,
        scope: u64,
        callback: &NativeEventCallback,
        output: &mut u64,
    ) -> u32;
    #[link_name = "bray_runtime_deadline_withdrawal"]
    safe fn withdrawal(reference: &NativeDeadlineReference, identity: u64) -> u32;
    #[link_name = "bray_runtime_deadline_scope_withdrawal"]
    safe fn scope_withdrawal(reference: &NativeDeadlineReference, scope: u64);
    #[link_name = "bray_runtime_deadline_dispatch"]
    safe fn dispatch(reference: &NativeDeadlineReference, now: u64) -> NativeRuntimeStatus;
    #[link_name = "bray_runtime_deadline_next"]
    safe fn next(reference: &NativeDeadlineReference, output: &mut u64) -> u32;
    #[link_name = "bray_runtime_deadline_membership_count"]
    safe fn membership_count(reference: &NativeDeadlineReference) -> usize;
    #[link_name = "bray_platform_clock_monotonic_now"]
    safe fn monotonic_now(output: &mut u64) -> NativePlatformStatus;
}

#[derive(Debug)]
pub(super) struct BrayDeadlines {
    reference: NativeDeadlineReference,
    dispatch: fn(&NativeDeadlineReference) -> Result<(), SchedulerError>,
    remaining: fn(&NativeDeadlineReference) -> Result<Option<Duration>, SchedulerError>,
    count: fn(&NativeDeadlineReference) -> usize,
    withdraw: fn(&NativeDeadlineReference, u64),
    withdraw_scope: fn(&NativeDeadlineReference, u64),
    destroy: fn(&NativeDeadlineReference),
}

impl BrayDeadlines {
    pub(super) fn new(
        scheduler: &NativeSchedulerReference,
        limit: usize,
    ) -> Result<Self, SchedulerError> {
        let mut reference = NativeDeadlineReference::default();

        let status = crate::context::with_independent_execution_context(|| {
            creation(scheduler, limit, &mut reference)
        });

        match status {
            0 => Ok(Self {
                reference,
                dispatch: dispatch_elapsed,
                remaining: remaining_duration,
                count: deadline_count,
                withdraw: withdraw_registration,
                withdraw_scope: withdraw_task_scope,
                destroy: destroy_deadlines,
            }),
            1 => Err(SchedulerError::TimerBackingUnavailable),
            4 => Err(SchedulerError::TimersClosed),
            _ => panic!("Bray deadline creation returned unknown status {status}"),
        }
    }

    pub(super) fn register(
        &self,
        deadline: u64,
        scope: u64,
        callback: &NativeEventCallback,
    ) -> Result<u64, SchedulerError> {
        let mut identity = 0;

        let status = crate::context::with_independent_execution_context(|| {
            registration(&self.reference, deadline, scope, callback, &mut identity)
        });

        match status {
            0 => Ok(identity),
            1 => Err(SchedulerError::TimerBackingUnavailable),
            2 => Err(SchedulerError::TimerCapacityReached),
            4 => Err(SchedulerError::TimersClosed),
            8 => Err(SchedulerError::TimerIdentityExhausted),
            _ => panic!("Bray deadline registration returned unknown status {status}"),
        }
    }

    // Indirect service entry keeps optional deadline/clock policy out of callers
    // that never construct this owner. Formation installs the selected operations.
    pub(super) fn withdraw(&self, identity: u64) {
        (self.withdraw)(&self.reference, identity);
    }

    pub(super) fn withdraw_scope(&self, scope: u64) {
        (self.withdraw_scope)(&self.reference, scope);
    }

    pub(super) fn dispatch(&self) -> Result<(), SchedulerError> {
        (self.dispatch)(&self.reference)
    }

    pub(super) fn remaining(&self) -> Result<Option<Duration>, SchedulerError> {
        (self.remaining)(&self.reference)
    }

    pub(super) fn count(&self) -> usize {
        (self.count)(&self.reference)
    }
}

impl Drop for BrayDeadlines {
    fn drop(&mut self) {
        (self.destroy)(&self.reference);
    }
}

fn withdraw_registration(reference: &NativeDeadlineReference, identity: u64) {
    // Release may retire the last mandatory runtime owner, outside all locks.
    let _ = crate::context::with_independent_execution_context(|| withdrawal(reference, identity));
}

fn withdraw_task_scope(reference: &NativeDeadlineReference, scope: u64) {
    crate::context::with_independent_execution_context(|| scope_withdrawal(reference, scope));
}

fn dispatch_elapsed(reference: &NativeDeadlineReference) -> Result<(), SchedulerError> {
    if next_deadline(reference).is_none() {
        return Ok(());
    }

    let now = clock_now()?;
    let status = crate::context::with_independent_execution_context(|| dispatch(reference, now));

    if status.is_success() {
        Ok(())
    } else {
        Err(SchedulerError::TimerCallback(status))
    }
}

fn next_deadline(reference: &NativeDeadlineReference) -> Option<u64> {
    let mut deadline = 0;

    let status =
        crate::context::with_independent_execution_context(|| next(reference, &mut deadline));

    match status {
        0 => None,
        1 => Some(deadline),
        _ => panic!("Bray deadline observation returned unknown status {status}"),
    }
}

fn remaining_duration(
    reference: &NativeDeadlineReference,
) -> Result<Option<Duration>, SchedulerError> {
    next_deadline(reference)
        .map(|deadline| clock_now().map(|now| Duration::from_nanos(deadline.saturating_sub(now))))
        .transpose()
}

fn deadline_count(reference: &NativeDeadlineReference) -> usize {
    crate::context::with_independent_execution_context(|| membership_count(reference))
}

fn destroy_deadlines(reference: &NativeDeadlineReference) {
    crate::context::with_independent_execution_context(|| destruction(reference));
}

pub(crate) fn deadline_after(duration: Duration) -> Result<u64, SchedulerError> {
    let nanos = u64::try_from(duration.as_nanos()).map_err(|_| SchedulerError::DeadlineOverflow)?;

    clock_now()?
        .checked_add(nanos)
        .ok_or(SchedulerError::DeadlineOverflow)
}

fn clock_now() -> Result<u64, SchedulerError> {
    let mut now = 0;
    let status = crate::context::with_independent_execution_context(|| monotonic_now(&mut now));

    if status.category() == 0 {
        Ok(now)
    } else {
        Err(SchedulerError::TimerClock(status))
    }
}
