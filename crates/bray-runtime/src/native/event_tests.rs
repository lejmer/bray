use bray_runtime_abi::{NativeRuntimeConfiguration, NativeRuntimeStatus};

use super::export::{
    bray_runtime_execution_shutdown, bray_runtime_task_event_creation,
    bray_runtime_task_event_destruction, bray_runtime_task_event_signal,
};

#[test]
fn unattached_signal_preserves_attachment_and_rejects_retired_execution_names() {
    let _isolation = super::test_runtime_isolation();

    assert!(super::fixtures::startup(NativeRuntimeConfiguration::new(1, 1)).is_success());

    let identity = bray_runtime_task_event_creation();

    assert_ne!(identity, 0);

    let status = std::thread::spawn(move || {
        assert!(crate::bootstrap::current_runtime_thread().is_none());
        assert!(super::state::retain_current_runtime().expect("unattached lookup succeeds").is_none());

        let status = bray_runtime_task_event_signal(identity);

        assert!(super::state::retain_current_runtime().expect("signal leaves no execution binding").is_none());
        assert!(crate::bootstrap::current_runtime_thread().is_none());

        status
    }).join().expect("the unattached signal thread returns");

    assert_eq!(status, NativeRuntimeStatus::SUCCESS);
    assert!(bray_runtime_execution_shutdown().is_success());
    assert_eq!(bray_runtime_task_event_signal(identity), NativeRuntimeStatus::INVALID_ARGUMENT);
    assert!(super::fixtures::startup(NativeRuntimeConfiguration::new(1, 1)).is_success());

    let replacement = bray_runtime_task_event_creation();

    assert_ne!(replacement, 0);
    assert_ne!(replacement, identity);
    assert_eq!(bray_runtime_task_event_signal(identity), NativeRuntimeStatus::INVALID_ARGUMENT);
    assert!(bray_runtime_task_event_destruction(replacement).is_success());
    assert!(bray_runtime_execution_shutdown().is_success());
}
