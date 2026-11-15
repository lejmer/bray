use std::sync::Arc;

use bray_runtime_abi::{NativeEventReference, NativeRuntimeStatus};

use crate::context::with_independent_execution_context as independent;
use crate::event::Event;
use crate::event::native;

use super::state::NativeRuntimeCore;

pub(super) fn create(owner: &Arc<NativeRuntimeCore>) -> usize {
    independent(|| {
        let mut reference = NativeEventReference::default();
        let identity = native::creation(owner.identity, &mut reference);

        if identity != 0 {
            native::release(&reference);
        }

        identity
    })
}

pub(super) fn event(owner: &Arc<NativeRuntimeCore>, identity: usize) -> Option<Event> {
    Event::acquire(owner.identity, identity)
}

pub(super) fn signal(identity: usize) -> NativeRuntimeStatus {
    independent(|| native::signal(identity))
}

pub(super) fn destroy(owner: &Arc<NativeRuntimeCore>, identity: usize) -> NativeRuntimeStatus {
    independent(|| native::destruction(owner.identity, identity))
}

pub(super) fn clear(owner: &NativeRuntimeCore) {
    independent(|| native::clear(owner.identity));
}

#[cfg(test)]
mod tests {
    use bray_runtime_abi::{NativeRuntimeConfiguration, NativeRuntimeStatus};

    use crate::native::implementation::{
        bray_runtime_execution_shutdown, bray_runtime_main_thread_lane_startup,
        bray_runtime_task_event_creation, bray_runtime_task_event_destruction,
        bray_runtime_task_event_signal,
    };

    #[test]
    fn unattached_signal_preserves_attachment_and_rejects_retired_runtime_names() {
        let _isolation = crate::native::test_runtime_isolation();

        assert_eq!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(1, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let identity = bray_runtime_task_event_creation();

        assert_ne!(identity, 0);

        let retained = super::super::state::with_runtime(|runtime| {
            super::event(&runtime.core, identity).expect("the creating runtime owns its event")
        })
        .expect("the creating runtime remains active");

        let status = std::thread::spawn(move || {
            assert!(crate::bootstrap::current_runtime_thread().is_none());
            assert!(super::super::state::current_runtime().is_none());

            let status = bray_runtime_task_event_signal(identity);

            assert!(super::super::state::current_runtime().is_none());
            assert!(crate::bootstrap::current_runtime_thread().is_none());

            status
        })
        .join()
        .expect("the unattached signal thread returns");

        assert_eq!(status, NativeRuntimeStatus::SUCCESS);
        assert!(retained.observation().1);
        assert_eq!(bray_runtime_execution_shutdown(), NativeRuntimeStatus::SUCCESS);

        assert_eq!(
            bray_runtime_task_event_signal(identity),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            bray_runtime_main_thread_lane_startup(NativeRuntimeConfiguration::new(1, 1)),
            NativeRuntimeStatus::SUCCESS
        );

        let replacement = bray_runtime_task_event_creation();

        assert_ne!(replacement, 0);
        assert_ne!(replacement, identity);

        assert_eq!(
            bray_runtime_task_event_signal(identity),
            NativeRuntimeStatus::INVALID_ARGUMENT
        );

        assert_eq!(
            bray_runtime_task_event_destruction(replacement),
            NativeRuntimeStatus::SUCCESS
        );

        assert_eq!(bray_runtime_execution_shutdown(), NativeRuntimeStatus::SUCCESS);
        drop(retained);
    }
}
