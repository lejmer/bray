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
