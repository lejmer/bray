use bray_diagnostics::{DiagnosticKind};
use bray_symbols::{DependencyMutationKind, DependencyStorageEffect};
use crate::test_support::{source_function_body_key};
use super::fixtures::production_host_compilation;

#[test]
fn production_byte_copy_does_not_publish_its_input_addresses() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), "module probe;");

    let key = source_function_body_key(&compilation, "copy_spare_raw_bytes");
    let lowered = compilation.lowered_unit(key).expect("production byte copy lowers");

    let mir = lowered.value().as_ref().and_then(bray_lowering::LoweredUnit::mir)
        .expect("admitted byte copy has MIR");

    assert!(mir.operations().iter().any(|operation| matches!(operation.kind(),
        bray_ir::MirOperationKind::Memory(memory) if matches!(memory.kind(),
            bray_bound_tree::CheckedMemoryOperationKind::Copy { kind: bray_bound_tree::MemoryCopyKind::Overlapping, .. }))),
        "the production wrapper must execute its checked byte copy");

    let dependencies = crate::compilation::result_dependencies::source_function_dependencies(
        &compilation, "copy_spare_raw_bytes");

    assert!(!dependencies.effects().iter().any(|effect| matches!(effect,
        DependencyStorageEffect::Escape
        | DependencyStorageEffect::Direct { kind: DependencyMutationKind::Escape, .. }
        | DependencyStorageEffect::Indirect { kind: DependencyMutationKind::Escape, .. })),
        "{:?}", dependencies.effects());
}

#[test]
fn absent_buffer_cleanup_preserves_unrelated_input_facts() {
    for value in ["none", "maybe_buffer()"] {
        let source = format!(r#"
            trusted module probe;

            using core.memory;

            using std.memory;

            extern trusted func maybe_buffer() -> std.memory.RawBuffer<u8>?
                executes(pure, total);

            extern trusted func observe(pos pointer: DataPointer<u8>)
                requires(trusted core.memory.valid_read<u8>(pointer = pointer, count = 1))
                executes(pure, total);

            trusted func caller(pos pointer: DataPointer<u8>)
                requires(trusted core.memory.valid_read<u8>(pointer = pointer, count = 1))
            {{
                {{
                    let buffer: std.memory.RawBuffer<u8>? = {value};
                }}

                observe(pointer);
            }}
        "#);

        let compilation = crate::test_support::production_standard_library_compilation(
            crate::SelectedTarget::baseline(), &source);

        let key = source_function_body_key(&compilation, "caller");

        let lowered = compilation.lowered_unit(key)
            .expect("conditional buffer disposal retains its checked transition");

        assert!(!lowered.diagnostics().has_errors(), "{value}: {:?}", lowered.diagnostics());
    }
}

#[test]
fn production_awaited_writer_does_not_return_its_completed_receiver_loan() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), "module probe;");

    let key = crate::test_support::source_type_callable_member_body_key(&compilation, "into_sink_async");

    let lowered = compilation.lowered_unit(key)
        .expect("the consuming async writer publishes its checked completion");

    assert!(!lowered.diagnostics().has_errors(), "{:?}", lowered.diagnostics());

    let key = crate::test_support::source_type_callable_member_body_key(&compilation, "into_sink_async");
    let flow = compilation.storage_flow(key).expect("the writer publishes its portable reports");

    let dependencies = flow.value().dependencies();

    for reports in dependencies.captured_values().iter().filter_map(|outcome| outcome.report.as_deref())
        .chain(dependencies.failure_outcomes().into_iter().flatten().filter_map(|row| row.report.as_deref())) {
        assert!(!reports.iter().any(|requirement| matches!(requirement,
            bray_symbols::DependencyRequirement::Direct { subject, kind: bray_symbols::DependencyRequirementKind::StorageAlive }
                if subject.subject_root() == bray_symbols::DependencySubjectRoot::Receiver)),
            "a completed writer must not export its invocation receiver loan: {reports:?}");
    }
}

#[test]
fn production_native_paths_preserve_their_admitted_input_contracts() {
    let mut failures = Vec::new();

    for target in [bray_target::NativeTarget::X86_64LinuxGnu, bray_target::NativeTarget::X86_64WindowsMsvc] {
        let compilation = crate::test_support::production_standard_library_compilation(
            crate::SelectedTarget::for_native(target), "module probe;");

        for name in ["prepare_native_path", "prepare_file_open", "prepare_dynamic_library_open_path", "platform_path_rename"] {
            let key = source_function_body_key(&compilation, name);

            let lowered = compilation.lowered_unit(key.clone())
                .expect("the native path wrapper publishes its checked source contract");

            if lowered.diagnostics().has_errors() {
                failures.push((target, name, "lowering", lowered.diagnostics().clone()));
            }

            let output = compilation.source_output_unit(key, &crate::CancellationToken::new())
                .expect("the native path wrapper completes the production Source-output boundary");

            if output.has_errors() {
                failures.push((target, name, "Source output", output.diagnostic_bag()));
            }
        }
    }

    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn production_formatted_bytes_retain_the_buffer_owner() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), "module probe;");

    let lowered = compilation.lowered_unit(source_function_body_key(&compilation, "bytes"))
        .expect("production formatted bytes reports its checked source contract");

    assert!(!lowered.diagnostics().has_errors(), "{:?}", lowered.diagnostics());
}

#[test]
fn production_buffer_views_retain_external_storage() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), "module probe;");

    let symbols = compilation.symbol_graph().expect("production symbols are available");
    let mut failures = Vec::new();

    for name in ["as_slice", "as_slice_mut"] {
        let key = crate::test_support::source_type_callable_member_body_key(&compilation, name);
        let member = symbols.symbol_for_key(key.declared_owner()).expect("buffer method is declared");
        let owner = symbols.containing_symbol(member).expect("buffer method has its owning type");

        assert_eq!(symbols.member_name(owner).map(|name| name.as_str()), Some("Buffer"));

        let lowered = compilation.lowered_unit(key)
            .expect("production buffer view reports its checked source contract");

        if lowered.diagnostics().has_errors() {
            failures.push((name, lowered.diagnostics().clone()));
        }
    }

    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn production_dynamic_outputs_preserve_their_write_contracts() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), "module probe;");

    let mut failures = Vec::new();

    for name in [
        "platform_dynamic_library_open_path",
        "platform_dynamic_library_open_system",
        "platform_dynamic_library_symbol",
    ] {
        let lowered = compilation.lowered_unit(source_function_body_key(&compilation, name))
            .expect("production dynamic operation reports its checked source contract");

        if lowered.diagnostics().has_errors() {
            failures.push((name, lowered.diagnostics().clone()));
        }
    }

    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn production_buffer_range_contracts_survive_pointer_staging() {
    let compilation = production_host_compilation("module probe;");
    let mut failures = Vec::new();

    for name in ["fill_spare_bytes", "copy_spare_bytes", "copy_bytes_from_pointer"] {
        let lowered = compilation.lowered_unit(source_function_body_key(&compilation, name))
            .expect("production buffer operation reports its checked source contract");

        if lowered.diagnostics().has_errors() {
            failures.push((name, lowered.diagnostics().clone()));
        }
    }

    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn production_channel_storage_publishes_its_taken_pending_value() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), "module app;");

    let key = crate::test_support::source_type_callable_member_body_key(&compilation, "store_message");
    let properties = compilation.execution_properties(key.clone()).expect("channel storage is certified");
    let body = compilation.lowered_unit(key).expect("channel storage lowers");

    assert!(!properties.diagnostics().has_errors(), "{:?}", properties.diagnostics());
    assert!(!body.diagnostics().has_errors(), "{:?}", body.diagnostics());
}

#[test]
fn production_bounded_channel_creation_retains_its_cleanup_evidence() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), "module app;");

    let key = source_function_body_key(&compilation, "bounded");
    let properties = compilation.execution_properties(key.clone()).expect("channel creation is certified");
    let body = compilation.lowered_unit(key.clone()).expect("channel creation lowers");

    let output = compilation.source_output_unit(key, &crate::CancellationToken::new())
        .expect("channel creation publishes Source output");

    assert!(!properties.diagnostics().has_errors(), "{:?}", properties.diagnostics());
    assert!(!body.diagnostics().has_errors(), "{:?}", body.diagnostics());
    assert!(!output.has_errors(), "{:?}", output.diagnostic_bag());
}

#[test]
fn production_channel_endpoints_preserve_loop_and_cleanup_authority() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), "module app;");

    let symbols = compilation.symbol_graph().expect("production symbols are available");
    let mut failures = Vec::new();
    let mut bodies = 0;

    for key in compilation.declared_unit_keys_for_test().expect("production bodies are declared") {
        if key.kind() != bray_bound_tree::BoundUnitKind::CallableBody {
            continue;
        }

        let Some(owner) = symbols.symbol_for_key(key.declared_owner()) else { continue; };

        let member = symbols.member_name(owner).map(|name| name.as_str());
        let ty = symbols.containing_symbol(owner).and_then(|ty| symbols.member_name(ty)).map(|name| name.as_str());

        if !matches!((ty, member), (Some("Sender"), Some("send")) | (Some("Receiver"), Some("receive"))) {
            continue;
        }

        bodies += 1;

        let properties = compilation.execution_properties(key.clone()).expect("endpoint execution is certified");

        if properties.diagnostics().has_errors() {
            failures.push((key, properties.diagnostics().clone()));
        } else {
            let body = compilation.lowered_unit(key.clone()).expect("endpoint body lowers");

            if body.diagnostics().has_errors() { failures.push((key, body.diagnostics().clone())); }
        }
    }

    assert_eq!(bodies, 2, "both actual endpoint bodies must be selected");
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
fn production_byte_reads_keep_their_loop_entry_evidence() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), "module app;");

    let key = source_function_body_key(&compilation, "read_native_u64");
    let properties = compilation.execution_properties(key.clone()).expect("native byte reads are certified");
    let body = compilation.lowered_unit(key).expect("native byte reads lower");

    assert!(!properties.diagnostics().has_errors(), "{:?}", properties.diagnostics());
    assert!(!body.diagnostics().has_errors(), "{:?}", body.diagnostics());
}

#[test]
fn bounded_collection_storage_effects_keep_their_physical_scope() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), "module app;");

    for name in ["hash_table_clear_storage", "ordered_tree_clear_storage"] {
        let dependencies = crate::compilation::result_dependencies::source_function_dependencies(&compilation, name);

        assert!(!dependencies.effects().iter().any(|effect| matches!(effect,
            bray_symbols::DependencyStorageEffect::UnboundedMutation { kind, ambient, .. }
                if !*ambient || !matches!(kind, bray_symbols::DependencyMutationKind::Dispose(_) | bray_symbols::DependencyMutationKind::Cleanup(_)))),
            "{name}: {:?}", dependencies.effects());
    }
}

#[test]
fn production_collection_destructors_release_transferred_backing() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), "module app;");

    let symbols = compilation.symbol_graph().expect("production symbols are available");
    let mut failures = Vec::new();
    let mut bodies = 0;

    for key in compilation.declared_unit_keys_for_test().expect("production bodies are declared") {
        if key.kind() != bray_bound_tree::BoundUnitKind::CallableBody
            || key.declared_owner().kind() != bray_symbols::SymbolKind::Destructor
            || !symbols.symbol_for_key(key.declared_owner()).and_then(|owner| symbols.containing_symbol(owner))
                .and_then(|owner| symbols.member_name(owner))
                .is_some_and(|name| matches!(name.as_str(), "HashTable" | "OrderedTree")) {
            continue;
        }

        bodies += 1;

        let properties = compilation.execution_properties(key.clone()).expect("collection cleanup is certified");
        let body = compilation.lowered_unit(key.clone()).expect("collection cleanup lowers");

        if properties.diagnostics().has_errors() || body.diagnostics().has_errors() {
            failures.push((key, properties.diagnostics().clone(), body.diagnostics().clone()));
        }
    }

    assert_eq!(bodies, 2, "both actual collection destructors must be selected");
    assert!(failures.is_empty(), "{failures:?}");
}

#[test]
#[ignore = "tier:proof"]
fn production_buffer_views_keep_nested_allocations_bounded() {
    let compilation = production_host_compilation("module probe;");

    for name in ["slot_at", "record"] {
        let key = source_function_body_key(&compilation, name);

        let flow = compilation
            .storage_flow(key)
            .expect("production getter retains its storage analysis");

        assert!(
            !flow.diagnostics().has_errors(),
            "{name}: {:?}",
            flow.diagnostics()
        );

        assert!(!flow.value().is_recovered(), "{name}: {:?}", flow.value());

        assert!(
            flow.value()
                .dependencies()
                .preserves_storage_on_normal_return(),
            "{name}: {:?}",
            flow.value().dependencies()
        );
    }
}

#[test]
fn production_spare_fill_preserves_its_descriptor_bound() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::baseline(), "module app;");

    let key = source_function_body_key(&compilation, "fill_spare_bytes");
    let properties = compilation.execution_properties(key.clone()).expect("production fill is certified");
    let body = compilation.lowered_unit(key).expect("production fill lowers");

    assert!(!properties.diagnostics().has_errors(), "{:?}", properties.diagnostics());
    assert!(!body.diagnostics().has_errors(), "{:?}", body.diagnostics());
}

#[test]
fn production_windows_environment_storage_retains_its_cleanup_evidence() {
    let compilation = crate::test_support::production_standard_library_compilation(
        crate::SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc), "module app;");

    let key = source_function_body_key(&compilation, "environment_storage");
    let properties = compilation.execution_properties(key.clone()).expect("environment storage is certified");
    let body = compilation.lowered_unit(key.clone()).expect("environment storage lowers");

    let output = compilation.source_output_unit(key, &crate::CancellationToken::new())
        .expect("environment storage publishes Source output");

    assert!(!properties.diagnostics().has_errors(), "{:?}", properties.diagnostics());
    assert!(!body.diagnostics().has_errors(), "{:?}", body.diagnostics());
    assert!(!output.has_errors(), "{:?}", output.diagnostic_bag());
}

#[test]
#[ignore = "tier:proof"]
fn cleanup_backing_requires_the_live_allocation_and_its_exact_layout() {
    for (body, valid) in [
        ("deallocate_bytes(pointer, 64, align = 32);", true),
        ("deallocate_bytes(pointer, 65, align = 32);", false),
        ("deallocate_bytes(pointer, 64, align = 16);", false),
        (
            "let copied = pointer; deallocate_bytes(pointer, 64, align = 32); deallocate_bytes(copied, 64, align = 32);",
            false,
        ),
        (
            "deallocate_bytes(pointer, 64, align = 32); let next = trusted allocate(64, align = 32); if !std.memory.is_null<u8>(next) { deallocate_bytes(pointer, 64, align = 32); deallocate_bytes(next, 64, align = 32); }",
            false,
        ),
    ] {
        let caller = format!(
            r#"
            trusted internal module bray.runtime.bootstrap.heap;

            using std.memory;
            func probe()
            {{
                let pointer = trusted allocate(64, align = 32);
                if !std.memory.is_null<u8>(pointer) {{ {body} }}
            }}
            "#,
        );

        let compilation = production_host_compilation(&caller);

        let lowered = compilation
            .lowered_unit(source_function_body_key(
                &compilation,
                "probe",
            ))
            .expect("fixture callable must lower successfully");

        assert_eq!(
            !lowered.diagnostics().has_errors(),
            valid,
            "{body}: {lowered:#?}"
        );

        if !valid {
            bray_testing::assert_goal_state_diagnostic_kind(
                lowered.diagnostics(),
                DiagnosticKind::CheckingTrustedObligationNotProven,
            );
        }
    }
}

#[test]
fn allocation_disposal_retires_its_backing_without_reaching_another_owner() {
    for (selected, valid) in [("retained.pointer", true), ("retired", false)] {
        let source = format!(r#"
            trusted module app;

            using core.memory;
            using std.memory;

            extern func observe(pos pointer: DataPointer<u8>)
                requires(trusted core.memory.valid_write<u8>(pointer = pointer, count = 1))
                executes(pure, total);

            trusted func caller()
                uses(manual_alloc)
            {{
                let retained = trusted std.memory.allocate(
                    {{
                        bytes = 1,
                        align = 1,
                    }}
                );
                observe(retained.pointer);

                let retired: DataPointer<u8> =
                {{
                    let victim = trusted std.memory.allocate(
                        {{
                            bytes = 1,
                            align = 1,
                        }}
                    );

                    yield victim.pointer;
                }};

                observe({selected});
            }}
        "#);

        let compilation = crate::test_support::production_standard_library_compilation(
            crate::SelectedTarget::baseline(), &source);

        let checked = compilation.lowered_unit(source_function_body_key(&compilation, "caller"))
            .expect("allocation disposal retains its checked transition");

        bray_testing::assert_goal_state_diagnostic_outcome(checked.diagnostics(),
            (!valid).then_some(DiagnosticKind::CheckingTrustedObligationNotProven), &source);

        if !valid {
            let source_id = source_function_body_key(&compilation, "caller").source().syntax().source_id();

            assert!(checked.diagnostics().iter()
                .filter(|diagnostic| diagnostic.kind() == DiagnosticKind::CheckingTrustedObligationNotProven)
                .all(|diagnostic| super::fixtures::diagnostic_at_call(diagnostic, source_id, &source, "observe(")),
                "retired allocation must fail at its observation: {:?}", checked.diagnostics());
        }
    }
}
