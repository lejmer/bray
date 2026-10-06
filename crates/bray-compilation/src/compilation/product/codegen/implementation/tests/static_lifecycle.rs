use super::support::artifacts::generated_artifacts;
use super::support::compilation::{codegen_compilation_for_product, test_product_identity};
use super::support::dependencies::{
    GenericDependencyFixture, generic_consumer_for_target_with_source,
    generic_dependency_from_fixture,
};
use super::support::linker::test_linker;
use super::support::runtime::{
    runtime_artifact, runtime_native_plan, runtime_native_plan_for_target,
};
use crate::compilation::product::codegen::NativeProductPlanningError;
use crate::{CancellationToken, SelectedTarget};
use bray_codegen::CodegenLinkage;
use bray_ir::{MirHelperReference, MirUnitKey};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeCapability};
use bray_symbols::{ProductKind, StaticStorageDuration};
use bray_target::NativeTarget;
use bray_testing::TemporaryFile;
use std::collections::BTreeSet;
use std::sync::Arc;

#[test]
fn library_preserves_main_thread_cleanup_without_selecting_a_runtime() {
    let source = concat!(
        "module app;\n",
        "public struct Resource { mut state: i32; }\n",
        "impl Resource {\n",
        "    async finalize() requires(main_thread_execution()) { self.state = 2; }\n",
        "    destruct() {}\n",
        "}\n",
        "public static RESOURCE: Resource = Resource { state = 1 };\n",
    );

    let (_, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            Some((
                &test_linker(),
                bray_linker::LinkedProductKind::StaticLibrary,
            )),
        )
        .unwrap_or_else(|error| {
            panic!("library must preserve cleanup obligations for its consumer: {error:?}")
        });

    assert!(
        plan.native_statics()
            .iter()
            .any(bray_native_artifact::NativeStatic::requires_main_thread)
    );

    let (_, executable) = runtime_native_plan(&format!(
        "{source}\nfunc main() -> i32 {{ return RESOURCE.state; }}\n"
    ));

    assert!(
        executable
            .executable_host()
            .unwrap()
            .requirements()
            .requires_role(RuntimeAbiRole::RuntimeInitialization)
    );

    let host = executable
        .units()
        .iter()
        .flat_map(|unit| unit.instances())
        .find(|instance| {
            matches!(
                instance.mir().kind(),
                bray_ir::MirUnitKind::ExecutableHost(_)
            )
        })
        .expect("executable must lower its host");

    assert!(matches!(
        host.mir()
            .operations()
            .first()
            .map(bray_ir::MirOperation::kind),
        Some(bray_ir::MirOperationKind::Host(
            bray_ir::MirHostOperation::InitializeRuntime { .. }
        ))
    ));

    assert!(
        executable
            .executable_host()
            .unwrap()
            .requirements()
            .capabilities()
            .contains(&RuntimeCapability::MainThreadLane)
    );
}

#[test]
fn shared_libraries_own_hosts_while_archives_contribute_storage() {
    let source = r#"
            module app;

            public static VALUE: i32 = 42;
        "#;

    let (_, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

    let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
    let runtime = runtime_artifact(&compilation, archive.path());
    let linker = test_linker();

    let plans = [
        bray_linker::LinkedProductKind::StaticLibrary,
        bray_linker::LinkedProductKind::SharedLibrary,
    ]
    .map(|kind| {
        compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                Some(runtime.clone()),
                [],
                Some((&linker, kind)),
            )
            .expect("library output must prepare with its host ownership")
    });

    assert!(!Arc::ptr_eq(&plans[0], &plans[1]));
    assert!(!plans[0].product_host().unwrap().is_final_image());
    assert!(plans[1].product_host().unwrap().is_final_image());

    assert!(
        plans[0]
            .preservation_roots()
            .all(|symbol| symbol.as_str() != bray_codegen::LINKED_PRODUCT_HOST_SYMBOL)
    );

    assert!(
        plans[1]
            .preservation_roots()
            .any(|symbol| symbol.as_str() == bray_codegen::LINKED_PRODUCT_HOST_SYMBOL)
    );
}

#[test]
fn shared_library_runtime_roles_do_not_require_static_storage() {
    let source = r#"
            module app;

            public func fail()
            {
                panic("library failure");
            }
        "#;

    let (_, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

    let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
    let runtime = runtime_artifact(&compilation, archive.path());
    let linker = test_linker();

    assert!(matches!(compilation.native_product_plan(
            test_product_identity(), crate::BuildConfiguration::Development,
            None, [], Some((&linker, bray_linker::LinkedProductKind::SharedLibrary)),
        ), Err(error) if matches!(error.as_ref(), NativeProductPlanningError::MissingRuntime)));

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            Some(runtime),
            [],
            Some((&linker, bray_linker::LinkedProductKind::SharedLibrary)),
        )
        .expect("shared library runtime roles must select their implementation");

    assert!(plan.product_host().is_none());
    assert!(plan.link().is_some());
}

#[test]
fn public_library_static_contributes_a_linked_host_table_entry() {
    let source = concat!(
        "module app;\n",
        "\n",
        "public static EXPORTED_VALUE: i32 = 42;\n",
    );

    let (backend, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

    let plan = compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            Some((
                &test_linker(),
                bray_linker::LinkedProductKind::StaticLibrary,
            )),
        )
        .unwrap_or_else(|error| panic!("static library plan must resolve: {error:?}"));

    let mappings = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::static_storages)
        .collect::<Vec<_>>();

    assert_eq!(mappings.len(), 1);

    assert_eq!(
        mappings[0].instance().duration(),
        StaticStorageDuration::Product
    );

    assert_eq!(plan.static_instances(), [mappings[0].instance().clone()]);

    assert!(
        !plan.native_statics()[0].requires_host(),
        "plain exported storage needs no executable host"
    );

    let marker = b"bray.static.host.";

    assert!(generated_artifacts(&backend, &plan).iter().any(|artifact| {
        artifact
            .windows(marker.len())
            .any(|candidate| candidate == marker)
    }));
}

#[test]
fn private_lifecycle_static_is_a_retained_product_root() {
    let source = concat!(
        "module app;\n",
        "internal struct Resource {}\n",
        "impl Resource\n",
        "{\n",
        "    finalize()\n",
        "    {\n",
        "    }\n",
        "}\n",
        "internal static HIDDEN_RESOURCE: Resource = Resource {};\n",
        "func main()\n",
        "{\n",
        "}\n",
    );

    let compilation =
        crate::test_support::compilation_with_product(source, ProductKind::Executable);

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let cancellation = CancellationToken::new();

    let target = compilation
        .selected_target()
        .target()
        .codegen_target()
        .unwrap_or_else(|error| panic!("test codegen target must validate: {error:?}"));

    let semantic = compilation
        .product_semantics()
        .unwrap_or_else(|error| panic!("test product plan must resolve: {error:?}"));

    let roots = compilation
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .unwrap_or_else(|error| panic!("test roots must resolve: {error:?}"));

    assert_eq!(roots.len(), 2);

    let dependency = generic_dependency_from_fixture(
        true,
        false,
        GenericDependencyFixture {
            source,
            runtime_frames: None,
            executable_templates: 3,
            platform_service: None,
        },
    );

    let consumer = generic_consumer_for_target_with_source(
        dependency,
        SelectedTarget::baseline(),
        r#"
            module consumer;
            using example.dependency.app;
            func main() { example.dependency.app.main(); }
        "#,
    );

    assert!(
        consumer.check_diagnostics().is_empty(),
        "{:#?}",
        consumer.check_diagnostics()
    );

    let semantic = consumer
        .product_semantics()
        .expect("template consumer must select its product");

    let roots = consumer
        .product_root_instances(semantic.value(), None, &target, &cancellation)
        .expect("template consumer must preserve the library lifecycle contract");

    assert_eq!(
        roots.len(),
        1,
        "publication roots must not materialize unused private library statics"
    );
}

#[test]
fn static_mapping_retains_selected_lifecycle_helpers() {
    let source = concat!(
        "module app;\n",
        "struct Resource { mut state: i32; }\n",
        "impl Resource\n",
        "{\n",
        "    finalize() { self.state = 2; }\n",
        "    destruct() { self.state = 3; }\n",
        "}\n",
        "static RESOURCE: Resource = Resource { state = 1 };\n",
        "func main() {}\n",
    );

    let (backend, plan) = runtime_native_plan(source);

    let mapping = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::static_storages)
        .find(|mapping| mapping.finalization().is_some() || mapping.destroy().is_some())
        .unwrap_or_else(|| panic!("lifecycle-bearing static mapping must be retained"));

    assert!(mapping.finalization().is_some());
    assert!(mapping.destroy().is_some());
    assert!(mapping.outgoing_capacity() >= 2);

    let lifecycle_symbols = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::symbols)
        .filter(|symbol| {
            matches!(
                symbol.key(),
                bray_codegen::CodegenSymbolKey::Instance(instance)
                    if matches!(instance.template(), MirUnitKey::GeneratedLifecycle(_))
            )
        })
        .collect::<Vec<_>>();

    assert!(!lifecycle_symbols.is_empty());

    assert!(
        lifecycle_symbols
            .iter()
            .any(|symbol| symbol.linkage() == CodegenLinkage::LinkOnce)
    );

    assert!(lifecycle_symbols.iter().all(|symbol| {
        matches!(
            symbol.linkage(),
            CodegenLinkage::LinkOnce | CodegenLinkage::Import
        )
    }));

    assert!(!generated_artifacts(&backend, &plan).is_empty());
}

#[test]
fn static_admission_counts_repeated_constant_owners() {
    let (_, plan) = runtime_native_plan(
        r#"
            module app;
            struct Resource { value: i32; destruct() {} }
            static ONE: Resource = Resource { value = 1 };
            static TWO: [Resource; 2] = [Resource { value = 1 }, Resource { value = 1 }];
            func main() {}
        "#,
    );

    let capacities = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::static_storages)
        .map(bray_codegen::CodegenStaticStorageMapping::outgoing_capacity)
        .filter(|capacity| *capacity != 0)
        .collect::<BTreeSet<_>>();

    let capacities = capacities.into_iter().collect::<Vec<_>>();

    assert_eq!(capacities.len(), 2);
    assert_eq!(capacities[1], capacities[0] * 2);
}

#[test]
fn static_host_orders_dependencies_reached_only_by_finalization() {
    for finalizer in ["", "impl Provider { finalize() {} }\n"] {
        let source = concat!(
            "module app;\n",
            "struct Provider { mut state: i32; }\n",
            "static PROVIDER: Provider = Provider { state = 1 };\n",
            "static UNRELATED: i32 = 7;\n",
            "struct Consumer {}\n",
            "impl Consumer\n",
            "{\n",
            "    finalize() { if PROVIDER.state == 1 {} }\n",
            "}\n",
            "static CONSUMER: Consumer = Consumer {};\n",
            "func main() {}\n",
        );

        let (_, plan) = runtime_native_plan(&format!("{source}\n{finalizer}"));

        let host = plan
            .product_host()
            .unwrap_or_else(|| panic!("lifecycle-bearing statics must retain a product host"));

        let owner = plan
            .mappings()
            .iter()
            .find(|mapping| {
                mapping
                    .static_storages()
                    .iter()
                    .any(bray_codegen::CodegenStaticStorageMapping::defines_storage)
            })
            .map(bray_codegen::CodegenMappings::unit)
            .unwrap_or_else(|| panic!("lifecycle-bearing statics must define storage"));

        assert_eq!(host.owner(), owner);

        let consumer = host
            .statics()
            .iter()
            .find(|entry| !entry.dependencies().is_empty())
            .unwrap_or_else(|| panic!("finalizer-only static dependency must be retained"));

        let [provider] = consumer.dependencies() else {
            panic!("consumer must retain exactly one finalizer-only provider");
        };

        let provider = host
            .statics()
            .iter()
            .find(|entry| entry.identity() == *provider)
            .unwrap_or_else(|| panic!("provider must remain in the product host"));

        assert!(consumer.order() < provider.order());
        assert_eq!(host.statics().len(), 2);
    }
}

#[test]
fn static_mapping_retains_asynchronous_fallible_finalizer() {
    let source = concat!(
        "module app;\n",
        "struct Resource { mut state: i32; }\n",
        "impl Resource\n",
        "{\n",
        "    async finalize() -> Result<unit, i32>\n",
        "    {\n",
        "        self.state = 2;\n",
        "        return await finish();\n",
        "    }\n",
        "    destruct() { self.state = 3; }\n",
        "}\n",
        "async func finish() -> Result<unit, i32> { return Error(42); }\n",
        "static RESOURCE: Resource = Resource { state = 1 };\n",
        "func main() {}\n",
    );

    let (backend, plan) = runtime_native_plan(source);

    assert!(
        !plan
            .units()
            .iter()
            .flat_map(bray_codegen::CodegenUnit::mir_units)
            .any(|mir| matches!(
                mir.source(),
                bray_ir::MirSourceOrigin::GeneratedLifecycle(MirHelperReference::Finalize(_))
            ))
    );

    let finalization = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::static_storages)
        .find_map(bray_codegen::CodegenStaticStorageMapping::finalization)
        .unwrap_or_else(|| panic!("asynchronous static finalizer must be retained"));

    assert_eq!(
        finalization.execution(),
        bray_symbols::CallableExecution::Asynchronous
    );

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );

    let (windows_backend, windows_plan) = runtime_native_plan_for_target(
        source,
        ProductKind::Executable,
        SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
    );

    assert!(
        generated_artifacts(&windows_backend, &windows_plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}
