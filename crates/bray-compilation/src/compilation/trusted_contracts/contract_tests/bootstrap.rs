use bray_target::NativeTarget;

use super::fixtures::production_host_compilation_for_target;
use crate::SelectedTarget;
use crate::test_support::{source_function_body_key, source_module_function_body_key, source_type_callable_member_body_key};

#[test]
#[ignore = "tier:proof"]
fn linux_native_bootstrap_preserves_owner_and_registration_contracts() {
    assert_bootstrap_contracts(NativeTarget::X86_64LinuxGnu);
}

#[test]
#[ignore = "tier:proof"]
fn windows_native_bootstrap_preserves_owner_and_registration_contracts() {
    assert_bootstrap_contracts(NativeTarget::X86_64WindowsMsvc);
}

fn assert_bootstrap_contracts(target: NativeTarget) {
    let compilation = production_host_compilation_for_target(
        "module app;",
        SelectedTarget::for_native(target),
    );

    let mut failures = Vec::new();

    for name in [
        "owner_entry_scope", "dispatch_ready", "start_frame", "prepare_start",
        "origin_context", "worker_context", "owned_context", "borrow_execution", "view_context", "context",
        "pending_observation", "take_ready", "acquire_ready", "acquire_worker_ready",
        "record_worker_failure", "retire_driver", "retain_in_arena", "retention",
    ] {
        let key = if name == "start_frame" {
            source_type_callable_member_body_key(&compilation, name)
        } else if name == "owner_entry_scope" {
            source_function_body_key(&compilation, name)
        } else if matches!(name, "retain_in_arena" | "retention") {
            source_module_function_body_key(&compilation, &["bray", "runtime", "bootstrap", "task"], name)
        } else {
            source_module_function_body_key(&compilation, &["bray", "runtime", "bootstrap", "execution"], name)
        };

        let started = std::time::Instant::now();

        eprintln!("[bootstrap] body-start target={} name={name}", target.as_str());

        let properties = compilation.execution_properties(key.clone()).expect("native bootstrap contracts are checked");
        let lowered = compilation.lowered_unit(key).expect("native bootstrap body lowers");

        eprintln!("[bootstrap] body-finish target={} name={name} elapsed_ms={}", target.as_str(), started.elapsed().as_millis());

        if properties.diagnostics().has_errors() {
            failures.push(format!("{name} contracts: {:#?}", properties.diagnostics()));
        }

        if lowered.diagnostics().has_errors() {
            failures.push(format!("{name}: {:#?}", lowered.diagnostics()));
        } else {
            assert!(lowered.value().is_some(), "{name} produces a lowered body");
        }
    }

    assert!(failures.is_empty(), "{}: {}", target.as_str(), failures.join("\n"));
}
