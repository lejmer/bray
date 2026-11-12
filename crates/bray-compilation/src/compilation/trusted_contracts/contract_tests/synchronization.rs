use bray_diagnostics::{DiagnosticKind};
use crate::compilation::result_dependencies::{source_function_dependencies};
use crate::test_support::{source_function_body_key};
use super::fixtures::{body_belongs_to, production_sync_compilation};

#[test]
#[ignore = "tier:proof"]
fn production_mutex_requires_acquisition_authority() {
    for (body, valid) in [
        (
            "let mut guard: std.sync.MutexGuard<u32> = { mutex = &lock, acquisition = internal std.sync.mutex.Marker {} }; let value = guard.get();",
            false,
        ),
        (
            "let mut guard = lock.lock(); let value = guard.get();",
            true,
        ),
        (
            "let candidate = lock.try_lock(); match consume candidate { case ?guard { let mut held = guard; let value = held.get(); } case none {} }",
            true,
        ),
    ] {
        let caller = format!(
            r#"
            trusted module app;
            using internal std.sync;
            func production_probe() requires(blocking_execution()) {{
                let lock = std.sync.Mutex<u32>(7);
                {body}
            }}
        "#
        );

        let compilation = production_sync_compilation(&caller);

        let checked = compilation
            .lowered_unit(source_function_body_key(&compilation, "production_probe"))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !checked.diagnostics().has_errors(),
            valid,
            "{caller}: {:?}",
            checked.diagnostics()
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                checked.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
#[ignore = "tier:proof"]
fn production_synchronization_declarations_check_their_own_bodies() {
    let compilation = production_sync_compilation(
        r#"module app;

using core.target;
"#,
    );

    for key in compilation
        .declared_unit_keys_for_test()
        .expect("fixture declarations must resolve their units")
    {
        if key.kind() != bray_bound_tree::BoundUnitKind::CallableBody {
            continue;
        }

        if !body_belongs_to(
            &compilation,
            &key,
            &[
                &["std", "sync", "spin_lock"],
                &["std", "sync", "mutex"],
                &["std", "sync", "rw_lock"],
                &["std", "sync", "condition"],
            ],
            &[
                "SpinLock",
                "SpinLockGuard",
                "Mutex",
                "MutexGuard",
                "RwLock",
                "RwLockReadGuard",
                "RwLockWriteGuard",
                "ConditionVariable",
            ],
        ) {
            continue;
        }

        let certified = compilation
            .execution_properties(key.clone())
            .expect("fixture callable must resolve its execution properties");

        assert!(
            !certified.diagnostics().has_errors(),
            "{key:?}: {:?}",
            certified.diagnostics()
        );

        let checked = compilation
            .lowered_unit(key.clone())
            .expect("fixture callable must lower successfully");

        assert!(
            !checked.diagnostics().has_errors(),
            "{key:?}: {:?}",
            checked.diagnostics()
        );
    }
}

#[test]
#[ignore = "tier:proof"]
fn barrier_preserves_its_nested_mutex_authority() {
    let compilation = production_sync_compilation(
        r#"
        module app;

        using std.sync;
        func caller() requires(blocking_execution()) {
            match consume std.sync.Barrier(participants = 1) {
                case Ok(barrier) { _ = barrier.wait(); }
                case Error(_) {}
            }
        }
    "#,
    );

    let checked = compilation
        .lowered_unit(source_function_body_key(&compilation, "caller"))
        .expect("fixture callable must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );

    {
        let name = "result_marker";

        let checked = compilation
            .execution_properties(crate::test_support::source_type_callable_member_body_key(
                &compilation,
                name,
            ))
            .expect("fixture callable must resolve its execution properties");

        assert!(
            !checked.diagnostics().has_errors(),
            "{:?}",
            checked.diagnostics()
        );
    }
}

#[test]
#[ignore = "tier:proof"]
fn production_standard_input_keeps_buffer_authority() {
    let application = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
        "module app;",
    );

    let checked = application
        .lowered_unit(source_function_body_key(
            &application,
            "read_standard_input",
        ))
        .expect("fixture callable must lower successfully");

    assert!(
        !checked.diagnostics().has_errors(),
        "{:?}",
        checked.diagnostics()
    );
}

#[test]
#[ignore = "tier:proof"]
fn production_owner_transport_and_async_observations_preserve_authority() {
    let application = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
        r#"
        module app;

        using std.channel;
        using std.fs;
        using std.io;
        using std.path;
        using std.sync;
        using std.testing;
        static ONCE: std.sync.Once<u32> = { prepared = std.sync.Once<u32>.prepare() };
        func initialized_value() -> u32 { return 37; }
        func static_once_case() requires(blocking_execution()) {
            _ = catch { _ = ONCE.get_or_init(initialized_value); };
            _ = ONCE.get_or_init(initialized_value);
        }
        func pair() {
            let (sender, receiver) = std.testing.assert_ok<
                (std.channel.Sender<u32>, std.channel.Receiver<u32>), std.channel.ChannelError
            >(std.channel.bounded<u32>(capacity = 1));
            sender.close();
            receiver.close();
        }
        func barrier_case() requires(blocking_execution()) {
            let barrier = std.testing.assert_ok<std.sync.Barrier, std.sync.BarrierError>(
                std.sync.Barrier(participants = 1));
            _ = barrier.wait();
        }
        func once_case(pos callback: func() -> u32) requires(blocking_execution()) {
            let once = std.sync.Once<u32>.empty();
            _ = once.get_or_init(callback);
        }
        async func channel_wait_case() {
            let (sender, receiver) = std.testing.assert_ok<
                (std.channel.Sender<u32>, std.channel.Receiver<u32>), std.channel.ChannelError
            >(std.channel.bounded<u32>(capacity = 1));
            _ = catch { _ = await sender.send(1); };
        }
        func completed_file_case(pos path: &std.path.Path) -> Result<unit, std.io.IoError>
            requires(blocking_execution()) {
            let options = std.fs.OpenOptions(access = std.fs.FileAccess.ReadWrite,
                creation = std.fs.FileCreation.CreateNew);
            let mut file = std.testing.assert_ok<std.fs.File, std.io.IoError>(
                std.fs.File.open(path, options = options));
            let mut buffer: bytes<1> = [0];
            _ = file.read(&mut buffer[..]);
            _ = file.write(&buffer[..]);
            std.testing.assert_ok<unit, std.io.IoError>(file.close());
            assert(file.is_complete());
            std.testing.assert_ok<unit, std.io.IoError>(std.fs.remove_file(path));
            return Ok(unit);
        }
        func completed_directory_case(pos path: &std.path.Path) -> Result<unit, std.io.IoError>
            requires(blocking_execution()) {
            let mut directory = std.testing.assert_ok<std.fs.Directory, std.io.IoError>(
                std.fs.Directory.open(path));
            let first = std.testing.assert_ok<std.fs.DirectoryEntry?, std.io.IoError>(
                directory.next());
            std.testing.assert_ok<unit, std.io.IoError>(directory.close());
            assert(directory.is_complete());
            return Ok(unit);
        }
        async func worker(pos lock: &std.sync.Mutex<u32>)
            requires(blocking_execution(), trusted std.sync.mutex.valid(&lock.instance))
            ensures(trusted std.sync.mutex.valid(&lock.instance)) {
            let guard = lock.lock();
        }
        async func task_case() requires(blocking_execution()) {
            let lock = std.sync.Mutex<u32>(0);
            let task = worker(&lock).start();
            try await task.join();
            let guard = lock.lock();
        }
        "#,
    );

    for name in ["assert_ok", "assert_error", "assert_present"] {
        let summary = crate::compilation::result_dependencies::source_function_dependencies(
            &application,
            name,
        );

        assert!(!summary.owned_result().is_empty(), "{name}: {summary:?}");
    }

    let failures = [
        "pair",
        "barrier_case",
        "once_case",
        "task_case",
        "static_once_case",
        "channel_wait_case",
        "completed_file_case",
        "completed_directory_case",
    ]
    .into_iter()
    .filter_map(|name| {
        let checked = application
            .lowered_unit(source_function_body_key(&application, name))
            .expect("fixture callable must lower successfully");

        checked.diagnostics().has_errors().then(|| {
            format!(
                "{name}: {:?}. Source: {:?}",
                checked.diagnostics(),
                crate::test_support::diagnostic_source_fragments(
                    &application,
                    checked.diagnostics()
                )
            )
        })
    })
    .collect::<Vec<_>>();

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
#[ignore = "tier:proof"]
fn production_synchronization_api_cleanup_checks() {
    let names = [
        "synchronization_owners_guard_and_publish_values",
        "concurrency_stress_mutex_contention_and_panic_cleanup_preserve_progress",
        "concurrency_model_reader_writer_lock_admits_a_pending_writer_before_later_readers",
        "concurrency_stress_barrier_reuses_its_generation_across_contended_rounds",
    ];

    let mut source = include_str!("../../../../../../standard-library/std/tests/api/sync.bray")
        .replace("@test", "");

    for name in names {
        source = source.replace(
            &format!("func {name}()"),
            &format!("func {name}() requires(blocking_execution())"),
        );
    }

    let application = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
        &source,
    );

    let failures = names
        .into_iter()
        .filter_map(|name| {
            let checked = application
                .lowered_unit(source_function_body_key(&application, name))
                .expect("fixture callable must lower successfully");

            checked
                .diagnostics()
                .has_errors()
                .then(|| format!("{name}: {:?}", checked.diagnostics()))
        })
        .collect::<Vec<_>>();

    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
