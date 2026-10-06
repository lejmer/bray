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
    let source = concat!(
        "module app;\n",
        "func main()\n",
        "{\n",
        "    let pointer = core.memory.null<i32>();\n",
        "}\n",
    );

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
    let source = concat!(
        "trusted module app;\n",
        "trusted func main() uses(manual_alloc)\n",
        "{\n",
        "    let pointer = trusted core.memory.allocate(bytes = 16, align = 4);\n",
        "}\n",
    );

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
        concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "    let pointer = core.memory.null<i32>();\n",
            "}\n",
        ),
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
    let compilation = compilation(concat!(
        "trusted module app;\n",
        "trusted func main() -> i32\n",
        "    uses(device_memory, intrinsic, raw_memory, unchecked_alias, unchecked_init)\n",
        "{\n",
        "    return trusted core.target.assembly<((i32, i32), ), (i32, )>(\n",
        "        template = \"\",\n",
        "        constraints = \"=reg,reg\",\n",
        "        clobbers = \"\",\n",
        "        features = \"\",\n",
        "        options = 0,\n",
        "        inputs = ((1, 2),),\n",
        "    ).0;\n",
        "}\n",
    ));

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
            concat!(
                "trusted module app;\n",
                "trusted func main() -> u8 uses(raw_memory)\n",
                "{\n",
                "    let pointer = core.memory.null<u8>();\n",
                "    return core.memory.read<u8>(pointer);\n",
                "}\n",
            ),
            DiagnosticKind::CheckingMissingTrustedMemoryGuarantees,
        ),
        (
            concat!(
                "trusted module app;\n",
                "trusted func main() -> u8 uses(raw_memory)\n",
                "{\n",
                "    let pointer = core.memory.null<u8>();\n",
                "    return trusted core.memory.read<u8>(pointer);\n",
                "}\n",
            ),
            DiagnosticKind::CheckingUninitializedRawStorage,
        ),
        (
            concat!(
                "trusted module app;\n",
                "trusted func main() uses(manual_alloc, raw_memory, unchecked_init)\n",
                "{\n",
                "    let pointer = trusted core.memory.allocate(bytes = 0, align = 1);\n",
                "\n",
                "    trusted core.memory.deallocate(pointer = pointer, bytes = 0, align = 1);\n",
                "    trusted core.memory.write<u8>(pointer, 1);\n",
                "}\n",
            ),
            DiagnosticKind::CheckingMemoryOperationAfterDeallocation,
        ),
        (
            concat!(
                "trusted module app;\n",
                "trusted func main() uses(manual_alloc, raw_memory, unchecked_init)\n",
                "{\n",
                "    let pointer = trusted core.memory.allocate(bytes = 1, align = 1);\n",
                "\n",
                "    trusted core.memory.write<u8>(pointer, 1);\n",
                "    trusted core.memory.deallocate(pointer = pointer, bytes = 1, align = 1);\n",
                "}\n",
            ),
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
        concat!(
            "trusted module app;\n",
            "trusted func main(pos condition: bool) uses(manual_alloc, raw_memory, unchecked_init)\n",
            "{\n",
            "    let allocated = trusted core.memory.allocate(bytes = 0, align = 1);\n",
            "    let forwarded = allocated;\n",
            "    let pointer = forwarded;\n",
            "    if condition\n",
            "    {\n",
            "        trusted core.memory.deallocate(pointer = pointer, bytes = 0, align = 1);\n",
            "    }\n",
            "    else\n",
            "    {\n",
            "        trusted core.memory.deallocate(pointer = pointer, bytes = 0, align = 1);\n",
            "    }\n",
            "    trusted core.memory.write<u8>(pointer, 1);\n",
            "}\n",
        ),
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
