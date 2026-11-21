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

#[test]
#[ignore = "tier:proof"]
fn aarch64_linux_native_bootstrap_preserves_owner_and_registration_contracts() {
    assert_bootstrap_contracts(NativeTarget::Aarch64LinuxGnu);
}

#[test]
#[ignore = "tier:proof"]
fn aarch64_windows_native_bootstrap_preserves_owner_and_registration_contracts() {
    assert_bootstrap_contracts(NativeTarget::Aarch64WindowsMsvc);
}

fn assert_bootstrap_contracts(target: NativeTarget) {
    let compilation = production_host_compilation_for_target(
        "module app;",
        SelectedTarget::for_native(target),
    );

    let mut failures = Vec::new();

    for name in [
        "owner_entry_scope", "dispatch_ready", "dispatch_task", "start_frame", "prepare_start", "register_task", "commit_start",
        "origin_context", "worker_context", "owned_context", "borrow_execution", "view_context", "context",
        "pending_observation", "take_ready", "acquire_ready", "worker.acquire_ready", "acquire_worker_ready",
        "record_worker_failure", "retire_driver", "retire_driver_resources", "retain_in_arena", "retention",
        "create_native_provider", "install_binding",
    ] {
        let key = if name == "start_frame" {
            source_type_callable_member_body_key(&compilation, name)
        } else if name == "owner_entry_scope" {
            source_function_body_key(&compilation, name)
        } else if name == "worker.acquire_ready" {
            source_module_function_body_key(&compilation, &["bray", "runtime", "bootstrap", "worker"], "acquire_ready")
        } else if matches!(name, "retain_in_arena" | "retention") {
            source_module_function_body_key(&compilation, &["bray", "runtime", "bootstrap", "task"], name)
        } else {
            source_module_function_body_key(&compilation, &["bray", "runtime", "bootstrap", "execution"], name)
        };

        eprintln!("{} checking {name}", target.as_str());

        let properties = compilation.execution_properties(key.clone()).expect("native bootstrap contracts are checked");

        eprintln!("{} lowering {name}", target.as_str());

        let lowered = compilation.lowered_unit(key).expect("native bootstrap body lowers");

        if std::env::var_os("BRA_694_TRACE_BOOTSTRAP").is_some() {
            for diagnostic in properties.diagnostics().iter().chain(lowered.diagnostics().iter()) {
                eprintln!("bootstrap {name} {:?}", diagnostic.kind());

                let spans = diagnostic.primary_span().into_iter().chain(diagnostic.related_locations().iter().map(|location| location.span()));

                for span in spans {
                    if let Some(source) = compilation.source(span.source_id()) {
                        eprintln!("  {:?} {:?}: {:?}", source.origin(), span.range(), source.text_slice(span.range()).map(|slice| slice.chars().take(220).collect::<String>()));
                    }
                }
            }
        }

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
