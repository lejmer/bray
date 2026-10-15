use std::cell::Cell;

use bray_runtime_abi::{
    NativeReportRecords, NativeRunOutcome, NativeRunState, NativeThreadOperationCallback,
};

use crate::outgoing::OutgoingRecords;

type Callback = extern "C" fn(usize, &mut NativeRunOutcome, &mut NativeReportRecords);
type Entry = extern "C" fn(Callback, usize, &mut NativeRunOutcome) -> u32;

#[expect(
    unsafe_code,
    reason = "native tests exercise the linked production Bray entry provider"
)]
unsafe extern "C" {
    safe fn bray_runtime_synchronous_root_execution(
        callback: Callback,
        context: usize,
        outcome: &mut NativeRunOutcome,
    ) -> u32;
    pub(crate) safe fn bray_runtime_foreign_callback_execution(
        callback: Callback,
        context: usize,
        outcome: &mut NativeRunOutcome,
    ) -> u32;
}

thread_local! {
    static CURRENT: Cell<Option<NativeThreadOperationCallback>> = const { Cell::new(None) };
}

pub(super) extern "C" fn root(
    callback: NativeThreadOperationCallback,
    context: usize,
) -> NativeRunOutcome {
    execute(bray_runtime_synchronous_root_execution, callback, context)
}

pub(super) extern "C" fn foreign(
    callback: NativeThreadOperationCallback,
    context: usize,
) -> NativeRunOutcome {
    execute(bray_runtime_foreign_callback_execution, callback, context)
}

fn execute(
    entry: Entry,
    callback: NativeThreadOperationCallback,
    context: usize,
) -> NativeRunOutcome {
    let _isolation = super::state::test_runtime_isolation();

    let Ok(_bridge) = super::bridge::Scope::enter() else {
        return super::outgoing::allocation_failure();
    };

    let _callback = CallbackScope(CURRENT.replace(Some(callback)));
    let mut outcome = NativeRunOutcome::new(NativeRunState::RUNTIME_FAILURE, 4);
    let _status = entry(invoke, context, &mut outcome);

    outcome
}

struct CallbackScope(Option<NativeThreadOperationCallback>);

impl Drop for CallbackScope {
    fn drop(&mut self) {
        CURRENT.set(self.0);
    }
}

extern "C" fn invoke(
    context: usize,
    destination: &mut NativeRunOutcome,
    records: &mut NativeReportRecords,
) {
    let chain = std::mem::take(records);
    let mut admitted = OutgoingRecords::from_parts(chain.head, chain.tail, chain.count);

    if crate::frame::reserve_rust_panic_backings(&mut admitted).is_err() {
        *destination = super::outgoing::allocation_failure();
        return;
    }

    let callback = CURRENT
        .get()
        .expect("test callback remains installed through its synchronous native call");

    let mut published = NativeRunOutcome::new(NativeRunState::COMPLETED, 0);
    let result = crate::root::contain_callback(|| callback(context, &mut published), &mut admitted);

    *destination = match result {
        crate::RunOutcome::Completed(()) => published,
        crate::RunOutcome::Cancelled if published.state() == NativeRunState::PANICKED => published,
        crate::RunOutcome::Cancelled => NativeRunOutcome::new(NativeRunState::CANCELLED, 0),
        crate::RunOutcome::Panicked(panic) => {
            super::outgoing::append_panic(&mut published, panic, &mut admitted);

            published
        }
    };

    let (head, tail, count) = admitted.into_parts();

    *records = NativeReportRecords { head, tail, count };
}
