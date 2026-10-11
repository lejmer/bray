use super::support::artifacts::{generated_artifacts, generated_artifacts_of_kind};
use super::support::compilation::{
    codegen_compilation, codegen_compilation_for_sources_target_with_source_roles,
    test_product_identity,
};
use super::support::linker::{test_linked_product, test_linker};
use super::support::runtime::{
    runtime_artifact, runtime_artifact_with_roles, runtime_native_plan_for_product,
};
use crate::compilation::product::codegen::NativeProductPlanningError;
use crate::{SelectedTarget, WorkerBudget};
use bray_codegen::{BackendArtifactKind, CodegenLinkage};
use bray_ir::MirUnitKey;
use bray_runtime_interface::{
    ExecutableHostContractBuildError, RuntimeAbiRole, RuntimeCompatibilityError,
};
use bray_symbols::ProductKind;
use bray_testing::TemporaryFile;
use std::sync::Arc;

#[test]
fn demanded_runtime_roles_cannot_disappear_from_product_validation() {
    let (_, compilation) = codegen_compilation(include_str!(
        "../../../../../../../../xtask/fixtures/native-execution/sync-panic.bray"
    ));

    let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");

    let roles = RuntimeAbiRole::ALL
        .into_iter()
        .filter(|role| *role != RuntimeAbiRole::PanicPropagation);

    let runtime = runtime_artifact_with_roles(&compilation, archive.path(), roles);

    let product = test_product_identity();

    let result = compilation.native_product_plan(
        product,
        crate::BuildConfiguration::Development,
        Some(runtime),
        [],
        Some((&test_linker(), bray_linker::LinkedProductKind::Executable)),
    );

    let Err(error) = result else {
        panic!("incomplete runtime must fail product validation");
    };

    assert!(
        matches!(
            error.as_ref(),
            NativeProductPlanningError::InvalidExecutableHost(
                ExecutableHostContractBuildError::IncompatibleRuntime(
                    RuntimeCompatibilityError::MissingRole(RuntimeAbiRole::PanicPropagation)
                )
            )
        ),
        "{error:?}"
    );
}

fn runtime_native_plan_with_source_roles(
    sources: &[&str],
    product_kind: ProductKind,
    target: SelectedTarget,
    runtime_roles: impl IntoIterator<Item = bray_runtime_interface::RuntimeRoleSourceBinding>,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    Arc<crate::compilation::product::codegen::NativeProductPlan>,
) {
    let (backend, compilation) = codegen_compilation_for_sources_target_with_source_roles(
        sources,
        product_kind,
        target,
        &[],
        [],
        runtime_roles,
        WorkerBudget::serial(),
    );

    let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
    let runtime = runtime_artifact(&compilation, archive.path());

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            Some(runtime),
            [],
            Some((&test_linker(), test_linked_product(product_kind))),
        )
        .unwrap_or_else(|error| panic!("runtime source-role plan must resolve: {error:?}"));

    (backend, plan)
}

#[test]
fn library_runtime_defaults_coalesce_with_consumer_generated_bodies() {
    let (backend, plan) = runtime_native_plan_for_product(
        "module app; public func answer(pos value: i32 = 42) -> i32 { return value; }",
        ProductKind::Library,
    );

    let defaults = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::symbols)
        .filter_map(|symbol| match symbol.key() {
            bray_codegen::CodegenSymbolKey::Instance(instance)
                if matches!(instance.template(), MirUnitKey::Bound(unit)
                        if unit.kind() == bray_bound_tree::BoundUnitKind::RuntimeDefault) =>
            {
                Some((symbol, instance))
            }
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(defaults.len(), 1);

    let (symbol, instance) = defaults[0];

    assert_eq!(symbol.linkage(), CodegenLinkage::LinkOnce);

    let compatibility = plan
        .units()
        .iter()
        .find_map(|unit| unit.key().compatibility(instance))
        .expect("default body must retain its partition compatibility");

    assert_eq!(compatibility.linkage(), CodegenLinkage::LinkOnce);

    let backend_ir = generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

    assert!(backend_ir.iter().any(|bytes| {
        String::from_utf8_lossy(bytes).lines().any(|line| {
            line.starts_with("define weak_odr ") && line.contains(symbol.name().as_str())
        })
    }));
}

#[test]
fn runtime_source_imports_share_generated_runtime_calls() {
    let source = r#"
            trusted module app;
            @abi(c)
            extern trusted internal func reserve(pos count: usize, pos outcome: RawPointer<u8>) uses(foreign_call);
            struct Guard { destruct() {} }
            public trusted func invoke(pos outcome: RawPointer<u8>) uses(foreign_call) {
                let guard = Guard {};
                trusted reserve(1, outcome);
            }
        "#;

    let role = RuntimeAbiRole::OutgoingAdmission;

    let binding = bray_runtime_interface::RuntimeRoleSourceBinding::try_new(role, "app.reserve")
        .unwrap_or_else(|| panic!("runtime source binding must validate"));

    let (backend, plan) = runtime_native_plan_with_source_roles(
        &[source],
        ProductKind::Library,
        SelectedTarget::baseline(),
        [binding],
    );

    assert!(plan.mappings().iter().any(|mappings| mappings.symbols().iter().any(|symbol| matches!(symbol.key(), bray_codegen::CodegenSymbolKey::Runtime(reference) if reference.role() == role))));

    assert!(plan.mappings().iter().all(|mappings| {
        mappings
            .symbols()
            .iter()
            .filter(|symbol| symbol.name().as_str() == role.native_symbol().unwrap())
            .count()
            <= 1
    }));

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
fn runtime_source_bindings_export_and_retain_the_canonical_role_symbol() {
    let source = concat!(
        "trusted module app;\n",
        "@abi(c)\n",
        "trusted internal func attachment_identity(pos descriptor: RawPointer<u8>) -> u64\n",
        "{\n",
        "    let _: RawPointer<u8> = descriptor;\n",
        "    return 7;\n",
        "}\n",
    );

    let role = RuntimeAbiRole::ThreadAttachmentIdentity;

    let binding =
        bray_runtime_interface::RuntimeRoleSourceBinding::try_new(role, "app.attachment_identity")
            .unwrap_or_else(|| panic!("runtime source binding must validate"));

    let (backend, plan) = runtime_native_plan_with_source_roles(
        &[source],
        ProductKind::Library,
        SelectedTarget::baseline(),
        [binding],
    );

    let symbol = role
        .native_symbol()
        .unwrap_or_else(|| panic!("runtime role must have a native symbol"));

    assert!(plan.mappings().iter().any(|mappings| {
        mappings.symbols().iter().any(|mapping| {
            mapping.name().as_str() == symbol && mapping.linkage() == CodegenLinkage::Export
        })
    }));

    assert!(
        plan.preservation_roots()
            .any(|root| root.as_str() == symbol)
    );

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
fn bootstrap_status_entries_contain_body_failures_without_attaching() {
    for target in bray_target::NativeTarget::ALL {
        for body in [
            "return 73;",
            "panic(\"registration failed\");",
            "let outcome: RunResult<unit> = Cancelled; try outcome; return 0;",
        ] {
            let source = format!(
                r#"
            trusted module app;
            @abi(c)
            trusted internal func register(pos callback: RawPointer<u8>) -> u32
            {{ let _ = callback; {body} }}
        "#
            );

            let role = RuntimeAbiRole::ThreadAttachmentCleanup;

            let binding =
                bray_runtime_interface::RuntimeRoleSourceBinding::try_new(role, "app.register")
                    .unwrap();

            let (backend, plan) = runtime_native_plan_with_source_roles(
                &[&source],
                ProductKind::Library,
                SelectedTarget::for_native(target),
                [binding],
            );

            let symbol = role.native_symbol().unwrap();

            let mapping = plan
                .mappings()
                .iter()
                .flat_map(bray_codegen::CodegenMappings::symbols)
                .find(|mapping| {
                    mapping
                        .native_entry()
                        .is_some_and(|entry| entry.name().as_str() == symbol)
                })
                .expect("bootstrap status operation has a native containment boundary");

            assert_eq!(
                mapping.native_entry().unwrap().kind(),
                bray_codegen::CodegenNativeEntryKind::RuntimeStatus
            );

            assert!(mapping.signature().has_panic_report_context());

            let artifacts =
                generated_artifacts_of_kind(&backend, &plan, BackendArtifactKind::BackendIr);

            let ir = artifacts
                .iter()
                .map(|artifact| String::from_utf8_lossy(artifact))
                .collect::<String>();

            assert!(
                ir.lines()
                    .any(|line| line.starts_with("define ") && line.contains(symbol))
            );

            assert!(
                !ir.contains(
                    RuntimeAbiRole::ForeignCallbackExecution
                        .native_symbol()
                        .unwrap()
                )
            );

            assert!(
                generated_artifacts(&backend, &plan)
                    .iter()
                    .all(|artifact| !artifact.is_empty())
            );
        }
    }
}

#[test]
#[cfg(all(windows, target_arch = "x86_64"))]
fn bootstrap_status_cancellation_returns_native_failure_without_reporting_a_panic() {
    let source = r#"
        trusted module app;
        @abi(c)
        trusted internal func register(pos callback: RawPointer<u8>) -> u32 {
            let _ = callback;
            let outcome: RunResult<unit> = Cancelled;
            try outcome;
            return 0;
        }
    "#;

    let role = RuntimeAbiRole::ThreadAttachmentCleanup;

    let binding =
        bray_runtime_interface::RuntimeRoleSourceBinding::try_new(role, "app.register").unwrap();

    let (backend, plan) = runtime_native_plan_with_source_roles(
        &[source],
        ProductKind::Library,
        SelectedTarget::for_native(bray_target::NativeTarget::X86_64WindowsMsvc),
        [binding],
    );

    let directory = tempfile::tempdir().unwrap();
    let mut objects = Vec::new();

    for (index, bytes) in generated_artifacts(&backend, &plan).into_iter().enumerate() {
        let path = directory.path().join(format!("unit-{index}.obj"));

        std::fs::write(&path, bytes).unwrap();
        objects.push(path);
    }

    let caller = format!(
        r#"
        static unsigned reports;
        unsigned {report}(void* value) {{ (void)value; ++reports; return 0; }}
        extern unsigned {entry}(void* callback);
        int main(void) {{
            unsigned status = {entry}(0);
            return status == {failure} && reports == 0 ? 0 : 1;
        }}
        "#,
        report = RuntimeAbiRole::PanicReporting.native_symbol().unwrap(),
        entry = role.native_symbol().unwrap(),
        failure = bray_runtime_abi::NativeRuntimeStatus::RUNTIME_FAILURE.code(),
    );

    let source = directory.path().join("caller.c");
    let executable = directory.path().join("caller.exe");

    std::fs::write(&source, caller).unwrap();

    let compiler = std::path::Path::new(bray_codegen_llvm::COMPILED_LLVM_PREFIX.unwrap())
        .join("bin/clang.exe");

    let linked = std::process::Command::new(compiler)
        .args(["--target=x86_64-pc-windows-msvc", "-fuse-ld=lld"])
        .arg(&source)
        .args(objects)
        .arg("-o")
        .arg(&executable)
        .output()
        .unwrap();

    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );

    let result = std::process::Command::new(executable).status().unwrap();

    assert_eq!(result.code(), Some(0));
}

#[test]
fn runtime_source_bindings_retain_referenced_static_atomic_storage() {
    let source = concat!(
        "trusted module app;\n",
        "internal static STATE: core.atomic.Atomic<u32> = core.atomic.initialize<u32>(0);\n",
        "@abi(c)\n",
        "trusted internal func initialize(pos worker_capacity: usize, pos timer_capacity: usize) -> u32\n",
        "{\n",
        "    let _: usize = timer_capacity;\n",
        "    if worker_capacity == 0\n",
        "    {\n",
        "        let _: u32 = core.atomic.load<u32, 1>(&STATE);\n",
        "    }\n",
        "    return core.atomic.load<u32, 1>(&STATE);\n",
        "}\n",
    );

    let role = RuntimeAbiRole::RuntimeInitialization;

    let binding = bray_runtime_interface::RuntimeRoleSourceBinding::try_new(role, "app.initialize")
        .unwrap_or_else(|| panic!("runtime source binding must validate"));

    let (backend, plan) = runtime_native_plan_with_source_roles(
        &[source],
        ProductKind::Library,
        SelectedTarget::baseline(),
        [binding],
    );

    let symbol = role
        .native_symbol()
        .unwrap_or_else(|| panic!("runtime role must have a native symbol"));

    assert!(
        plan.preservation_roots()
            .any(|root| root.as_str() == symbol)
    );

    assert!(plan.mappings().iter().any(|mappings| {
        mappings
            .static_storages()
            .iter()
            .any(bray_codegen::CodegenStaticStorageMapping::defines_storage)
    }));

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}
