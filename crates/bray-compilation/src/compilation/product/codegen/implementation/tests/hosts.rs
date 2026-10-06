use super::support::artifacts::{
    generated_artifacts, generated_artifacts_of_kind, generated_artifacts_of_kind_with_options,
};
use super::support::compilation::{codegen_compilation_for_product, test_product_identity};
use super::support::linker::test_linker;
use super::support::runtime::{
    runtime_artifact, runtime_native_plan, runtime_native_plan_for_sources_target,
    runtime_native_plan_for_sources_target_with_platform_overrides,
};
use crate::SelectedTarget;
use bray_base::NonEmptySharedStr;
use bray_codegen::{BackendArtifactKind, CodegenResultMapping, OptimizationLevel};
use bray_ir::MirHelperReference;
use bray_runtime_interface::{
    ExecutableEntryResult, ProtectedFrameOperation, RootExecution, RuntimeAbiRole,
    RuntimeRoleBinding, RuntimeRoleImplementation,
};
use bray_symbols::{NativeLinkKind, NativeLinkRequirement, ProductKind};
use bray_target::NativeTarget;
use bray_testing::TemporaryFile;

#[test]
fn asynchronous_executable_hosts_emit_complete_deterministic_native_units() {
    let (backend, plan) = runtime_native_plan(include_str!(
        "../../../../../../../../xtask/fixtures/native-execution/async-i32.bray"
    ));

    let host = plan
        .executable_host()
        .unwrap_or_else(|| panic!("async executable must own a host"));

    let RootExecution::Asynchronous { frame } = host.entries()[0].root() else {
        panic!("async executable host must retain a protected root frame");
    };

    assert_eq!(host.entries()[0].result(), ExecutableEntryResult::I32);

    let frame_unit = plan
        .units()
        .iter()
        .find(|unit| {
            unit.instances()
                .iter()
                .any(|instance| instance.protected_frame_identity() == Some(frame))
        })
        .unwrap_or_else(|| panic!("concrete root frame unit must be retained"));

    let frame_mapping = plan
        .units()
        .iter()
        .zip(plan.mappings())
        .find_map(|(unit, mappings)| (unit.key() == frame_unit.key()).then_some(mappings))
        .unwrap_or_else(|| panic!("root frame mappings must be retained"));

    let adapter = frame_mapping
        .symbol(&bray_codegen::CodegenSymbolKey::ProtectedFrame {
            frame,
            operation: ProtectedFrameOperation::MoveBeforeStart,
        })
        .map(bray_codegen::CodegenSymbolMapping::name);

    assert_eq!(host.entries()[0].root_frame_adapter(), adapter);

    for role in [
        RuntimeAbiRole::RootExecution,
        RuntimeAbiRole::RootCancellationRequest,
        RuntimeAbiRole::RootTerminalObservation,
        RuntimeAbiRole::RootCompletionResolution,
        RuntimeAbiRole::CleanupIncidentReporting,
        RuntimeAbiRole::PanicReporting,
        RuntimeAbiRole::EntryFailureReporting,
        RuntimeAbiRole::StructuredShutdown,
    ] {
        assert_eq!(
            host.role_binding(role)
                .map(RuntimeRoleBinding::implementation),
            Some(RuntimeRoleImplementation::BrayRuntime)
        );
    }

    let first = generated_artifacts(&backend, &plan);
    let second = generated_artifacts(&backend, &plan);

    assert_eq!(first, second);
    assert_eq!(first.len(), plan.units().len());
    assert!(first.iter().all(|artifact| !artifact.is_empty()));
}

#[test]
fn independently_started_tasks_emit_native_units() {
    let source = concat!(
        "module async_tasks;\n",
        "\n",
        "async func complete()\n",
        "{\n",
        "    return unit;\n",
        "}\n",
        "\n",
        "async func main()\n",
        "{\n",
        "    let first: Task<unit> = complete().start();\n",
        "    let second: Task<unit> = complete().start();\n",
        "\n",
        "    try await first.join();\n",
        "    try await second.join();\n",
        "}\n",
    );

    let (backend, compilation) = codegen_compilation_for_product(source, ProductKind::Executable);

    let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
    let runtime = runtime_artifact(&compilation, archive.path());

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            Some(runtime),
            [],
            Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
        )
        .unwrap_or_else(|error| panic!("started tasks must realize: {error:?}"));

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
fn asynchronous_unit_and_result_error_roots_emit_native_hosts() {
    let cases = [
        (
            include_str!("../../../../../../../../xtask/fixtures/native-execution/async-unit.bray"),
            false,
        ),
        (
            include_str!(
                "../../../../../../../../xtask/fixtures/native-execution/async-result-error.bray"
            ),
            true,
        ),
    ];

    for (source, fallible) in cases {
        let (backend, plan) = runtime_native_plan(source);

        let host = plan
            .executable_host()
            .unwrap_or_else(|| panic!("async executable must own a host"));

        assert!(matches!(
            host.entries()[0].root(),
            RootExecution::Asynchronous { .. }
        ));

        if fallible {
            let ExecutableEntryResult::Fallible { error, .. } = host.entries()[0].result() else {
                panic!("Result root must retain its concrete error type");
            };

            let helper_references = plan
                .mappings()
                .iter()
                .flat_map(bray_codegen::CodegenMappings::operations)
                .flat_map(bray_codegen::CodegenOperationMapping::helpers)
                .map(bray_codegen::CodegenHelperMapping::reference)
                .collect::<Vec<_>>();

            assert!(helper_references.contains(&&MirHelperReference::Finalize(error)));

            assert!(helper_references.contains(&&MirHelperReference::Destroy(error)));

            let ir = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr)
                .into_iter()
                .flatten()
                .collect::<Vec<_>>();

            let ir = String::from_utf8(ir)
                .unwrap_or_else(|error| panic!("LLVM IR must be UTF-8: {error}"));

            assert!(ir.contains("entry.failure"));
            assert!(ir.contains("bray_runtime_entry_failure_reporting"));
        } else {
            assert_eq!(host.entries()[0].result(), ExecutableEntryResult::Unit);
        }

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }
}

#[test]
fn synchronous_panics_emit_a_runtime_owned_host_boundary() {
    for target in [
        SelectedTarget::baseline(),
        SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
    ] {
        let (backend, plan) = runtime_native_plan_for_sources_target(
            &[include_str!(
                "../../../../../../../../xtask/fixtures/native-execution/sync-panic.bray"
            )],
            ProductKind::Executable,
            target,
            &[],
        );

        let host = plan
            .executable_host()
            .unwrap_or_else(|| panic!("executable must own a host"));

        assert_eq!(host.entries()[0].root(), RootExecution::Synchronous);

        for role in [
            RuntimeAbiRole::SynchronousRootExecution,
            RuntimeAbiRole::PanicReporting,
            RuntimeAbiRole::PanicPropagation,
        ] {
            assert_eq!(
                host.role_binding(role)
                    .map(RuntimeRoleBinding::implementation),
                Some(RuntimeRoleImplementation::BrayRuntime)
            );
        }

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }
}

#[test]
fn direct_checked_calls_forward_outcomes_without_copying_reports() {
    let source = r#"
            module app;

            func checked(pos value: i32) -> i32
            {
                assert(value != 0);

                return value;
            }

            func void_result(pos value: i32)
            {
                checked(value);
                checked(value);
            }

            func scalar_result(pos value: i32) -> i32
            {
                checked(value);

                return checked(value);
            }

            func indirect_result(pos value: i32) -> [i32; 8]
            {
                checked(value);
                checked(value);

                return [value; 8];
            }
        "#;

    let (backend, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .unwrap_or_else(|error| panic!("direct checked calls must realize: {error:?}"));

    let artifacts = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

    let ir = artifacts
        .iter()
        .map(|artifact| std::str::from_utf8(artifact).unwrap())
        .collect::<String>();

    assert!(
        ir.contains("call.outcome.state"),
        "checked calls must remain in the emitted IR: {ir}"
    );

    assert!(
        !ir.contains("call.outcome.report"),
        "direct propagation must not copy the report: {ir}"
    );

    let options = plan.options().with_optimization(OptimizationLevel::None);

    let artifacts = generated_artifacts_of_kind_with_options(
        &backend,
        &plan,
        BackendArtifactKind::BackendIr,
        &options,
    );

    let ir = artifacts
        .iter()
        .map(|artifact| std::str::from_utf8(artifact).unwrap())
        .collect::<String>();

    assert!(
        !ir.contains("call.outcome.report"),
        "unoptimized propagation must not copy the report: {ir}"
    );

    assert!(
        !ir.lines().any(|line| line.trim() == "unreachable"),
        "bypassed propagation blocks must not be emitted: {ir}"
    );

    let mut shared_returns = 0;

    for function in ir.split("\ndefine ").skip(1) {
        let count = function
            .lines()
            .take_while(|line| *line != "}")
            .filter(|line| line.starts_with("outcome.propagated:"))
            .count();

        assert!(
            count <= 1,
            "propagation exits must share one epilogue: {function}"
        );

        shared_returns += count;
    }

    assert!(
        shared_returns >= 3,
        "void, scalar and indirect callers must emit their shared epilogues: {ir}"
    );

    assert!(
        plan.mappings()
            .iter()
            .flat_map(|mappings| mappings.symbols())
            .any(|symbol| {
                matches!(
                    symbol.signature().result(),
                    CodegenResultMapping::Indirect { .. }
                )
            }),
        "the fixture must exercise an indirect result"
    );
}

#[test]
fn timed_synchronous_entries_repeat_inside_one_runtime_root() {
    let source = concat!(
        "trusted module app;\n",
        "@link(name = \"native\")\n",
        "@symbol(name = \"native_status\")\n",
        "@abi(c)\n",
        "extern trusted func native_status() -> i32 uses(foreign_call);\n",
        "trusted func main() -> i32 uses(foreign_call)\n",
        "{\n",
        "    return trusted native_status();\n",
        "}\n",
    );

    let native_link = NativeLinkRequirement::new(
        NonEmptySharedStr::try_new("native")
            .unwrap_or_else(|| panic!("native link name must be valid")),
        NativeLinkKind::Dynamic,
    );

    let (backend, plan) = runtime_native_plan_for_sources_target_with_platform_overrides(
        &[source],
        ProductKind::Executable,
        SelectedTarget::baseline(),
        &[native_link],
        [],
        [],
        crate::BuildConfiguration::TimedRelease {
            inner_iterations: std::num::NonZeroU64::new(3)
                .unwrap_or_else(|| panic!("timed test iteration count must be nonzero")),
        },
    );

    let backend_ir = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr)
        .into_iter()
        .map(|artifact| String::from_utf8_lossy(&artifact).into_owned())
        .collect::<String>();

    let callback = backend_ir
        .split("define private void @bray_host_synchronous_root_callback_0")
        .nth(1)
        .and_then(|tail| tail.split("\n}").next())
        .unwrap_or_else(|| panic!("timed executable must define its synchronous callback"));

    assert!(callback.contains(bray_runtime_abi::PERFORMANCE_INTERVAL_BEGIN_SYMBOL));
    assert!(callback.contains(bray_runtime_abi::PERFORMANCE_INTERVAL_END_SYMBOL));
    assert!(callback.contains("performance.iteration"));
    assert!(callback.contains("root.performance.succeeded"));

    let root_execution_symbol = RuntimeAbiRole::SynchronousRootExecution
        .native_symbol()
        .unwrap_or_else(|| panic!("synchronous root execution must have a native symbol"));

    assert!(!callback.contains(root_execution_symbol));

    let root_executions = backend_ir
        .lines()
        .filter(|line| line.contains(" call ") && line.contains(root_execution_symbol))
        .count();

    assert_eq!(root_executions, 1);
}
