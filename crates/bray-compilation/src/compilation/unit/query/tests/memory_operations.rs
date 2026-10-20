use crate::test_support::{
    compilation, compilation_with_target_operations, only_call_selection, source_callable_body_key,
};
use bray_bound_tree::{CheckedMemoryOperationKind, SemanticSelection};
use bray_compiler_known::ImplementationHook;
use bray_diagnostics::{DiagnosticKind, DiagnosticRelatedLocationKind};
use bray_testing::assert_goal_state_diagnostic_kind;
use std::sync::Arc;

#[test]
fn memory_operations_publish_lazily_from_compiler_known_hooks() {
    let source = r#"module app;

using core.memory;
func main()
{
    let pointer = core.memory.null<i32>();
}
"#;

    let compilation = compilation_with_target_operations(source, true, false);

    let key = source_callable_body_key(&compilation);

    assert_eq!(
        compilation.state.memory_operations.is_published(&key),
        Ok(false)
    );

    let selections = match compilation.semantic_selections(key.clone()) {
        Ok(selections) => selections,
        Err(error) => panic!("semantic selections must publish: {error:?}"),
    };

    let selection = only_call_selection(selections.value());

    let SemanticSelection::Call(call) = selection.selection() else {
        panic!("null pointer call must select a call");
    };

    let Some(definition) = call.target().declaration() else {
        panic!("null pointer call must select a declaration");
    };

    assert_eq!(
        compilation
            .available_compiler_known_symbols()
            .symbol_implementation(definition.symbol()),
        Some(ImplementationHook::RawPointerNull)
    );

    let first = match compilation.memory_operations(key.clone()) {
        Ok(operations) => operations,
        Err(error) => panic!("memory operations must publish: {error:?}"),
    };

    assert!(
        first.diagnostics().is_empty(),
        "memory operation checking must be diagnostic-free: {:?}",
        first.diagnostics()
    );

    let [operation] = first.value().operations() else {
        panic!("null pointer call must publish one memory operation");
    };

    assert!(matches!(
        operation.kind(),
        CheckedMemoryOperationKind::Null { .. }
    ));

    let repeated = match compilation.memory_operations(key) {
        Ok(operations) => operations,
        Err(error) => panic!("repeated memory operations must publish: {error:?}"),
    };

    assert!(Arc::ptr_eq(&first, &repeated));

    let flow = match compilation.storage_flow(source_callable_body_key(&compilation)) {
        Ok(flow) => flow,
        Err(error) => panic!("storage flow must publish: {error:?}"),
    };

    let [decision] = flow.value().memory_operations() else {
        panic!("storage flow must retain the null operation decision");
    };

    assert_eq!(
        decision.status(),
        bray_bound_tree::MemoryOperationStatus::Valid
    );
}

#[test]
fn compiler_known_callable_parameters_supply_default_templates() {
    let source = r#"trusted module app;

using core.memory;
trusted func main() uses(manual_alloc)
{
    let pointer = trusted core.memory.allocate(bytes = 16, align = 4);
}
"#;

    let compilation = compilation_with_target_operations(source, true, true);

    let operations = match compilation.memory_operations(source_callable_body_key(&compilation)) {
        Ok(operations) => operations,
        Err(error) => panic!("allocation operation must publish: {error:?}"),
    };

    let [operation] = operations.value().operations() else {
        panic!("allocation call must publish one memory operation");
    };

    assert_eq!(operation.kind(), CheckedMemoryOperationKind::RawAllocate);

    let flow = compilation
        .storage_flow(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("allocation storage flow must publish: {error:?}"));

    assert!(flow.diagnostics().is_empty(), "{:?}", flow.diagnostics());
}

#[test]
fn unavailable_memory_operations_publish_structured_target_diagnostics() {
    let compilation = compilation_with_target_operations(
        r#"module app;

using core.memory;
func main()
{
    let pointer = core.memory.null<i32>();
}
"#,
        false,
        false,
    );

    let operations = match compilation.memory_operations(source_callable_body_key(&compilation)) {
        Ok(operations) => operations,
        Err(error) => panic!("memory operations must publish: {error:?}"),
    };

    assert!(operations.value().operations().is_empty());

    let mut diagnostics = operations.diagnostics().iter();

    let Some(diagnostic) = diagnostics.next() else {
        panic!("unavailable operation must publish one diagnostic");
    };

    assert!(diagnostics.next().is_none());

    assert_eq!(
        diagnostic.kind(),
        DiagnosticKind::CheckingTargetMemoryOperationUnavailable
    );

    assert_goal_state_diagnostic_kind(
        operations.diagnostics(),
        DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
    );
}

#[test]
fn invalid_target_control_contracts_publish_structured_diagnostics() {
    let compilation = compilation(
        r#"trusted module app;

using core.target;
trusted func main() -> i32
    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)
{
    return trusted core.target.assembly<((i32, i32), ), (i32, )>(
        template = "",
        constraints = "=reg,reg",
        clobbers = "",
        features = "",
        options = 0,
        inputs = ((1, 2),),
    ).0;
}
"#,
    );

    let operations = compilation
        .memory_operations(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("target-control memory operations must publish: {error:?}"));

    assert!(operations.value().operations().is_empty());

    assert_goal_state_diagnostic_kind(
        operations.diagnostics(),
        DiagnosticKind::CheckingInvalidTargetControlContract,
    );
}

#[test]
fn invalid_memory_obligations_publish_structured_semantic_diagnostics() {
    let cases = [
        (
            r#"trusted module app;

using core.memory;
trusted func main() -> u8 uses(raw_memory)
{
    let pointer = core.memory.null<u8>();
    return core.memory.read<u8>(pointer);
}
"#,
            DiagnosticKind::CheckingMissingTrustedMemoryGuarantees,
        ),
        (
            r#"trusted module app;

using core.memory;
trusted func main() -> u8 uses(raw_memory)
{
    let pointer = core.memory.null<u8>();
    return trusted core.memory.read<u8>(pointer);
}
"#,
            DiagnosticKind::CheckingUninitializedRawStorage,
        ),
        (
            r#"trusted module app;

using core.memory;
trusted func main() uses(manual_alloc, raw_memory, unchecked_init)
{
    let pointer = trusted core.memory.allocate(bytes = 0, align = 1);

    trusted core.memory.deallocate(pointer = pointer, bytes = 0, align = 1);
    trusted core.memory.write<u8>(pointer, 1);
}
"#,
            DiagnosticKind::CheckingMemoryOperationAfterDeallocation,
        ),
        (
            r#"trusted module app;

using core.memory;
trusted func main() uses(manual_alloc, raw_memory, unchecked_init)
{
    let pointer = trusted core.memory.allocate(bytes = 1, align = 1);

    trusted core.memory.write<u8>(pointer, 1);
    trusted core.memory.deallocate(pointer = pointer, bytes = 1, align = 1);
}
"#,
            DiagnosticKind::CheckingDeallocationWithOutstandingObligations,
        ),
    ];

    for (source, expected) in cases {
        let compilation = compilation_with_target_operations(source, true, true);

        let flow = compilation
            .storage_flow(source_callable_body_key(&compilation))
            .unwrap_or_else(|error| panic!("invalid memory storage flow must publish: {error:?}"));

        assert!(
            flow.diagnostics().by_kind(expected).next().is_some(),
            "{expected:?} must be reported: {:?}",
            flow.diagnostics()
        );

        match expected {
            DiagnosticKind::CheckingMissingTrustedMemoryGuarantees => {
                assert_goal_state_diagnostic_kind(
                    flow.diagnostics(),
                    DiagnosticKind::CheckingMissingTrustedMemoryGuarantees,
                );
            }
            DiagnosticKind::CheckingUninitializedRawStorage => {
                assert_goal_state_diagnostic_kind(
                    flow.diagnostics(),
                    DiagnosticKind::CheckingUninitializedRawStorage,
                );
            }
            DiagnosticKind::CheckingMemoryOperationAfterDeallocation => {
                assert_goal_state_diagnostic_kind(
                    flow.diagnostics(),
                    DiagnosticKind::CheckingMemoryOperationAfterDeallocation,
                );
            }
            DiagnosticKind::CheckingDeallocationWithOutstandingObligations => {
                assert_goal_state_diagnostic_kind(
                    flow.diagnostics(),
                    DiagnosticKind::CheckingDeallocationWithOutstandingObligations,
                );
            }
            _ => unreachable!("memory-obligation table contains only exact memory kinds"),
        }
    }
}

#[test]
fn invalidated_memory_reports_every_branch_deallocation_origin() {
    let compilation = compilation_with_target_operations(
        r#"trusted module app;

using core.memory;
trusted func main(pos condition: bool) uses(manual_alloc, raw_memory, unchecked_init)
{
    let allocated = trusted core.memory.allocate(bytes = 0, align = 1);
    let forwarded = allocated;
    let pointer = forwarded;
    if condition
    {
        trusted core.memory.deallocate(pointer = pointer, bytes = 0, align = 1);
    }
    else
    {
        trusted core.memory.deallocate(pointer = pointer, bytes = 0, align = 1);
    }
    trusted core.memory.write<u8>(pointer, 1);
}
"#,
        true,
        true,
    );

    let flow = compilation
        .storage_flow(source_callable_body_key(&compilation))
        .unwrap_or_else(|error| panic!("branch memory flow must publish: {error:?}"));

    let diagnostic = flow
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingMemoryOperationAfterDeallocation)
        .next()
        .unwrap_or_else(|| panic!("post-branch write must report invalidated storage"));

    let origins = diagnostic
        .related_locations()
        .iter()
        .filter(|related| related.kind() == DiagnosticRelatedLocationKind::DeallocationOrigin)
        .collect::<Vec<_>>();

    let allocations = diagnostic
        .related_locations()
        .iter()
        .filter(|related| related.kind() == DiagnosticRelatedLocationKind::AllocationOrigin)
        .collect::<Vec<_>>();

    assert_eq!(origins.len(), 2, "{diagnostic:#?}");
    assert_eq!(allocations.len(), 1, "{diagnostic:#?}");
    assert_ne!(origins[0].span(), origins[1].span());

    assert_goal_state_diagnostic_kind(
        flow.diagnostics(),
        DiagnosticKind::CheckingMemoryOperationAfterDeallocation,
    );
}

#[test]
fn same_named_source_callables_are_not_memory_operations() {
    let compilation = compilation_with_target_operations(
        concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let value: i32 = allocate();\n",
            "}\n",
            "func allocate() -> i32\n",
            "{\n",
            "    return 1;\n",
            "}\n",
        ),
        true,
        true,
    );

    let operations = match compilation.memory_operations(source_callable_body_key(&compilation)) {
        Ok(operations) => operations,
        Err(error) => panic!("source call memory operations must publish: {error:?}"),
    };

    assert!(operations.value().operations().is_empty());
    assert!(operations.diagnostics().is_empty());
}

#[test]
fn call_results_transfer_raw_state_only_from_checked_reference_inputs() {
    for (result, initialized) in [
        ("pointer", true),
        ("other", false),
        ("core.memory.null<u8>()", false),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            func forward(pos other: RawPointer<u8>, pos pointer: RawPointer<u8>) -> RawPointer<u8>
            {{
                return {result};
            }}
            trusted func main() -> u8 uses(manual_alloc, raw_memory, unchecked_init)
            {{
                let pointer = trusted core.memory.allocate(bytes = 1, align = 1);
                trusted core.memory.write<u8>(pointer, 1);
                let returned = forward(core.memory.null<u8>(), pointer);
                return trusted core.memory.read<u8>(returned);
            }}
        "#
        );

        let compilation = compilation_with_target_operations(&source, true, true);

        let forwarding = compilation
            .storage_flow(crate::test_support::source_function_body_key(
                &compilation,
                "forward",
            ))
            .unwrap();

        assert!(
            forwarding.value().dependencies().effects().is_empty(),
            "forwarding performs no caller-storage mutation: {:?}",
            forwarding.value().dependencies().effects()
        );

        let flow = compilation
            .storage_flow(crate::test_support::source_function_body_key(
                &compilation,
                "main",
            ))
            .unwrap_or_else(|error| panic!("returned pointer flow must publish: {error:?}"));

        assert_eq!(
            flow.diagnostics()
                .by_kind(DiagnosticKind::CheckingUninitializedRawStorage)
                .next()
                .is_none(),
            initialized,
            "result {result}: {:?}",
            flow.diagnostics()
        );

        let read = flow
            .value()
            .memory_operations()
            .last()
            .expect("read decision must publish");

        assert_eq!(
            read.status(),
            if initialized {
                bray_bound_tree::MemoryOperationStatus::Valid
            } else {
                bray_bound_tree::MemoryOperationStatus::UninitializedRawStorage
            }
        );
    }
}
#[test]
fn checked_writes_preserve_unrelated_raw_initialization() {
    for target in ["left", "right"] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            struct Cell {{ mut value: u8; }}
            func rewrite(pos cell: &mut Cell) {{ cell.value = 2; }}
            trusted func main() -> u8 uses(raw_memory, unchecked_init) {{
                let mut left: Cell = {{ value = 1 }};
                let mut right: Cell = {{ value = 1 }};
                let pointer = core.memory.address_of_mut(&mut left.value);
                rewrite(&mut {target});
                return trusted core.memory.read<u8>(pointer);
            }}
        "#
        );

        let compilation = compilation_with_target_operations(&source, true, true);
        let key = crate::test_support::source_function_body_key(&compilation, "main");
        let flow = compilation.storage_flow(key).unwrap();
        let status = flow.value().memory_operations().last().unwrap().status();

        assert_eq!(
            status,
            bray_bound_tree::MemoryOperationStatus::Valid,
            "{target}: {:?}",
            flow.diagnostics()
        );
    }
}

#[test]
fn raw_memory_aliases_share_consumption_and_retirement() {
    use bray_bound_tree::MemoryOperationStatus;

    for (body, expected) in [
        (
            r#"
            let pointer = trusted core.memory.allocate(bytes = 1, align = 1);
            let alias = pointer;
            trusted core.memory.deallocate(pointer = alias, bytes = 1, align = 1);
            trusted core.memory.write<u8>(pointer, 1);
        "#,
            MemoryOperationStatus::InvalidatedAllocation,
        ),
        (
            r#"
            let allocated = trusted core.memory.allocate(bytes = 1, align = 1);
            let pointer = trusted core.memory.reinterpret<Payload, u8>(allocated);
            let alias = pointer;
            trusted core.memory.write<Payload>(pointer, { value = 1 });
            let first = trusted core.memory.read<Payload>(alias);
            let second = trusted core.memory.read<Payload>(pointer);
        "#,
            MemoryOperationStatus::UninitializedRawStorage,
        ),
        (
            r#"
            let mut pointer = trusted core.memory.allocate(bytes = 1, align = 1);
            let alias = pointer;
            trusted core.memory.write<u8>(pointer, 1);
            pointer = core.memory.null<u8>();
            let value = trusted core.memory.read<u8>(alias);
        "#,
            MemoryOperationStatus::Valid,
        ),
        (
            r#"
            let pointer = trusted core.memory.allocate(bytes = 2, align = 1);
            let alias = core.memory.offset<u8>(pointer, elements = 1);
            trusted core.memory.deallocate(pointer = pointer, bytes = 2, align = 1);
            trusted core.memory.write<u8>(alias, 1);
        "#,
            MemoryOperationStatus::InvalidatedAllocation,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            struct Payload {{ value: u8; }}
            trusted func main() uses(manual_alloc, raw_memory, unchecked_init, layout_reinterpret)
            {{
                {body}
            }}
        "#
        );

        let compilation = compilation_with_target_operations(&source, true, true);

        let flow = compilation
            .storage_flow(crate::test_support::source_function_body_key(
                &compilation,
                "main",
            ))
            .expect("source alias flow must publish");

        let decision = flow
            .value()
            .memory_operations()
            .last()
            .expect("last memory operation must publish");

        assert_eq!(
            decision.status(),
            expected,
            "{body}: {:?}",
            flow.diagnostics()
        );
    }
}

#[test]
fn conditional_raw_initialization_retains_the_possible_cleanup_obligation() {
    let source = r#"
        trusted module app;

        using core.memory;
        trusted func main(pos initialize: bool) uses(manual_alloc, raw_memory, unchecked_init)
        {
            let pointer = trusted core.memory.allocate(bytes = 1, align = 1);
            if initialize
            {
                trusted core.memory.write<u8>(pointer, 1);
            }
            trusted core.memory.deallocate(pointer = pointer, bytes = 1, align = 1);
        }
    "#;

    let compilation = compilation_with_target_operations(source, true, true);

    let flow = compilation
        .storage_flow(source_callable_body_key(&compilation))
        .expect("conditional initialization flow must publish");

    assert_eq!(
        flow.value()
            .memory_operations()
            .last()
            .expect("deallocation decision")
            .status(),
        bray_bound_tree::MemoryOperationStatus::OutstandingObligations
    );

    let diagnostic = flow
        .diagnostics()
        .by_kind(DiagnosticKind::CheckingDeallocationWithOutstandingObligations)
        .next()
        .expect("possible value requires cleanup");

    assert!(
        diagnostic
            .related_locations()
            .iter()
            .any(|location| location.kind() == DiagnosticRelatedLocationKind::InitializationOrigin)
    );
}

#[test]
fn ordinary_ownership_observes_raw_target_transitions() {
    for (body, moved) in [
        (
            r#"
            let mut payload: Payload = { value = 1 };
            let pointer = core.memory.address_of_mut(&mut payload);
            let first = trusted core.memory.read<Payload>(pointer);
            let byte = trusted core.memory.reinterpret<u8, Payload>(pointer);
            trusted core.memory.write<u8>(byte, 2);
            let second = payload;
        "#,
            true,
        ),
        (
            r#"
            let mut payload: Payload = { value = 1 };
            let pointer = core.memory.address_of_mut(&mut payload);
            let alias = pointer;
            let first = trusted core.memory.read<Payload>(alias);
            let second = payload;
        "#,
            true,
        ),
        (
            r#"
            let mut payload: Payload = { value = 1 };
            let pointer = core.memory.address_of_mut(&mut payload);
            let first = trusted core.memory.read<Payload>(pointer);
            trusted core.memory.write<Payload>(pointer, { value = 2 });
            let second = payload;
        "#,
            false,
        ),
        (
            r#"
            let mut pair: Pair = { left = { value = 1 }, right = { value = 2 } };
            let pointer = core.memory.address_of_mut(&mut pair.left);
            let first = trusted core.memory.read<Payload>(pointer);
            let second = pair.right;
        "#,
            false,
        ),
        (
            r#"
            let mut pair: Pair = { left = { value = 1 }, right = { value = 2 } };
            let pointer = core.memory.address_of_mut(&mut pair.left);
            let first = trusted core.memory.read<Payload>(pointer);
            let second = pair.left;
        "#,
            true,
        ),
    ] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            struct Payload {{ value: u8; }}
            struct Pair {{ mut left: Payload; mut right: Payload; }}
            trusted func main() uses(raw_memory, unchecked_init, layout_reinterpret)
            {{
                {body}
            }}
        "#
        );

        let compilation = compilation_with_target_operations(&source, true, true);

        let flow = compilation
            .storage_flow(crate::test_support::source_function_body_key(
                &compilation,
                "main",
            ))
            .expect("mixed ordinary and raw ownership flow must publish");

        if !moved {
            assert!(
                !flow.diagnostics().has_errors(),
                "{body}: {:?}",
                flow.diagnostics()
            );
        }

        assert_eq!(
            flow.diagnostics()
                .by_kind(DiagnosticKind::CheckingUseOfMovedStorage)
                .next()
                .is_some(),
            moved,
            "{body}: {:?}",
            flow.diagnostics()
        );

        assert!(
            flow.value().memory_operations().iter().all(
                |operation| operation.status() == bray_bound_tree::MemoryOperationStatus::Valid
            ),
            "raw operations must succeed before checking later ownership: {:?}",
            flow.diagnostics()
        );
    }
}

#[test]
fn raw_moves_through_stored_borrows_reach_the_original_fields() {
    for (use_after_move, moved) in [("pair", true), ("pair.left", true), ("pair.right", false)] {
        let source = format!(
            r#"
            trusted module app;

            using core.memory;
            struct Payload {{ value: u8; }}
            struct Pair {{ mut left: Payload; mut right: Payload; }}
            trusted func main() uses(raw_memory, unchecked_init)
            {{
                let mut pair: Pair = {{ left = {{ value = 1 }}, right = {{ value = 2 }} }};
                let alias = &mut pair;
                let pointer = core.memory.address_of_mut(&mut alias.left);
                let first = trusted core.memory.read<Payload>(pointer);
                let second = {use_after_move};
            }}
        "#
        );

        let compilation = compilation_with_target_operations(&source, true, true);

        let flow = compilation
            .storage_flow(crate::test_support::source_function_body_key(
                &compilation,
                "main",
            ))
            .expect("derived physical field flow must publish");

        assert_eq!(
            flow.diagnostics()
                .by_kind(DiagnosticKind::CheckingUseOfMovedStorage)
                .next()
                .is_some(),
            moved,
            "{source}: {:?}",
            flow.diagnostics()
        );

        if !moved {
            assert!(
                !flow.diagnostics().has_errors(),
                "{source}: {:?}",
                flow.diagnostics()
            );
        }

        if use_after_move == "pair" {
            let key = crate::test_support::source_function_body_key(&compilation, "main");

            let storage = compilation
                .storage_plan(key.clone())
                .expect("derived target plan");

            let refinements = compilation
                .refinements(key)
                .expect("derived target effects");

            assert!(
                refinements.value().invalidations().values().any(|effect| {
                    matches!(effect, bray_bound_tree::StorageInvalidation::Observed { accesses, .. }
                    if accesses.values().flatten().any(|path| path.access_id(storage.value()).is_none()))
                }),
                "a raw alias must project its target into observations without requiring a source access"
            );
        }

        assert!(
            flow.value().memory_operations().iter().all(
                |operation| operation.status() == bray_bound_tree::MemoryOperationStatus::Valid
            ),
            "{:?}",
            flow.diagnostics()
        );
    }
}

#[test]
fn cleanup_without_external_mutation_preserves_raw_initialization() {
    let compilation = compilation_with_target_operations(
        r#"
        trusted module app;

        using core.memory;
        struct Guard { flag: bool; consume func destroy() {} }
        trusted func main() -> u8 uses(raw_memory, unchecked_init) {
            let mut value: u8 = 1;
            let pointer = core.memory.address_of_mut(&mut value);
            { let guard: Guard = { flag = false }; }
            return trusted core.memory.read<u8>(pointer);
        }
    "#,
        true,
        true,
    );

    let flow = compilation
        .storage_flow(crate::test_support::source_function_body_key(
            &compilation,
            "main",
        ))
        .unwrap();

    assert!(!flow.diagnostics().has_errors(), "{:?}", flow.diagnostics());

    assert_eq!(
        flow.value().memory_operations().last().unwrap().status(),
        bray_bound_tree::MemoryOperationStatus::Valid
    );
}
