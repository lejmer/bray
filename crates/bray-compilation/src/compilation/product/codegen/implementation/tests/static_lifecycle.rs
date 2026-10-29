use super::support::artifacts::{generated_artifacts, generated_artifacts_of_kind};
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
use bray_ir::MirUnitKey;
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
            .expect("fixture `executable_host` call must succeed")
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

    let operations = host.mir().operations();

    let position = |predicate: fn(&bray_ir::MirHostOperation) -> bool| {
        operations.iter().position(|operation| matches!(operation.kind(), bray_ir::MirOperationKind::Host(operation) if predicate(operation))).expect("fixture sequence must contain the selected entry")
    };

    let admitted =
        position(|operation| matches!(operation, bray_ir::MirHostOperation::BeginProductEntry));

    let materialized = position(|operation| {
        matches!(
            operation,
            bray_ir::MirHostOperation::MaterializeStatic { .. }
        )
    });

    let resolved = position(|operation| {
        matches!(
            operation,
            bray_ir::MirHostOperation::ResolveRootTerminal { .. }
        )
    });

    let retired =
        position(|operation| matches!(operation, bray_ir::MirHostOperation::BeginStaticCleanup));

    assert!(admitted < materialized && materialized < resolved && resolved < retired);

    assert!(
        executable
            .executable_host()
            .expect("fixture `executable_host` call must succeed")
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

    assert!(
        !plans[0]
            .product_host()
            .expect("fixture shared library must own an executable host")
            .is_final_image()
    );

    assert!(
        plans[1]
            .product_host()
            .expect("fixture `product_host` call must succeed")
            .is_final_image()
    );

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
fn shared_library_tls_retains_only_demanded_worker_execution() {
    let source = r#"
        module app;

        @thread_local
        internal static VALUE: i32 = 42;

        @abi(c)
        @symbol(name = "read_tls")
        func read() -> i32 { return VALUE; }
    "#;

    for cooperative in [false, true] {
        let (_, compilation) = codegen_compilation_for_product(source, ProductKind::Library);

        let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");
        let runtime = runtime_artifact(&compilation, archive.path());
        let linker = test_linker();

        let plan = compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                Some(runtime),
                cooperative.then_some(RuntimeCapability::CooperativeExecution),
                Some((&linker, bray_linker::LinkedProductKind::SharedLibrary)),
            )
            .expect("TLS library must preserve its selected execution requirements");

        assert!(plan.executable_host().is_none());

        assert_eq!(
            plan.product_host()
                .expect("TLS retains a product host")
                .execution(),
            cooperative.then_some(bray_symbols::CallableExecution::Synchronous),
        );
    }
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
            runtime_frames: &[],
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
    assert!(mapping.outgoing_requirements().len() >= 2);

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
        .map(|mapping| mapping.outgoing_requirements().len())
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
        plan.executable_host()
            .expect("fixture `executable_host` call must succeed")
            .requirements()
            .roles()
            .contains(&bray_runtime_interface::RuntimeAbiRole::CleanupStorageRelease,)
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

    let asynchronous = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::operations)
        .filter_map(bray_codegen::CodegenOperationMapping::outgoing_requirements)
        .flatten()
        .filter(|requirement| {
            requirement.execution() == bray_symbols::CallableExecution::Asynchronous
        })
        .collect::<Vec<_>>();

    assert!(
        !asynchronous.is_empty(),
        "async cleanup retains its physical requirement"
    );

    let bray_runtime_interface::ExecutableEntryResult::Fallible { ty, .. } = finalization.result()
    else {
        panic!("fixture must retain its fallible completion");
    };

    for requirement in asynchronous {
        assert_eq!(
            requirement.completion(),
            ty,
            "reserve the completion, not the lazy Future handle"
        );

        assert!(plan.mappings().iter().any(|mapping| {
            mapping
                .ty(ty)
                .and_then(bray_codegen::CodegenTypeMapping::layout)
                .is_some()
        }));
    }

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );

    for target in NativeTarget::ALL {
        let (backend, plan) = runtime_native_plan_for_target(
            source,
            ProductKind::Executable,
            SelectedTarget::for_native(target),
        );

        assert!(
            generated_artifacts(&backend, &plan)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }
}

#[test]
fn generic_static_failure_uses_its_selected_overaligned_requirement() {
    for (alignment, artifact) in [
        (64, bray_codegen::BackendArtifactKind::RelocatableObject),
        (4096, bray_codegen::BackendArtifactKind::BackendIr),
    ] {
        let source = r#"
        module app;
        @layout(stable, align = ALIGNMENT)
        struct Failure { value: u8; }
        struct Resource<T> {
            value: T;
            finalize() -> Result<unit, Failure> {
                return Error(Failure { value = 7 });
            }
            destruct() {}
        }
        static RESOURCE: Resource<i32> = { value = 42 };
        func main() {}
    "#
        .replace("ALIGNMENT", &alignment.to_string());

        let (backend, plan) = runtime_native_plan(&source);

        let (mapping, storage) = plan
            .mappings()
            .iter()
            .find_map(|mapping| {
                mapping
                    .static_storages()
                    .iter()
                    .find(|storage| storage.finalization().is_some())
                    .map(|storage| (mapping, storage))
            })
            .expect("generic resource must retain its static finalizer");

        let finalization = storage
            .finalization()
            .expect("fixture `finalization` call must succeed");

        let requirement = storage
            .outgoing_requirements()
            .iter()
            .find(|requirement| Some(requirement.callable()) == finalization.selected())
            .expect("retained error capture must select the same admitted source action");

        let layout = mapping
            .instance_ty(storage.owner(), requirement.completion())
            .and_then(bray_codegen::CodegenTypeMapping::layout)
            .expect("fixture `and_then` call must succeed");

        assert!(layout.size() >= alignment);
        assert_eq!(layout.alignment().get(), alignment);

        let roles = plan
            .executable_host()
            .expect("fixture `executable_host` call must succeed")
            .requirements()
            .roles();

        assert!(roles.contains(&RuntimeAbiRole::CleanupStorageClaim));
        assert!(roles.contains(&RuntimeAbiRole::CleanupStorageRelease));

        assert!(
            generated_artifacts_of_kind(&backend, &plan, artifact)
                .iter()
                .all(|artifact| !artifact.is_empty())
        );
    }
}

#[test]
fn static_incident_destroyer_initializes_both_outcomes_before_calling_cleanup() {
    for destructor in ["", "panic(\"destruction failed\");"] {
        let source = r#"
            module app;

            struct Failure {
                value: u8;
                destruct() {
                    DESTRUCTOR
                }
            }

            struct Resource {
                value: i32;
                finalize() -> Result<unit, Failure> {
                    return Error(Failure { value = 7 });
                }
                destruct() {}
            }

            static RESOURCE: Resource = { value = 42 };
            static SECOND: Resource = { value = 43 };
            func main() {}
        "#
        .replace("DESTRUCTOR", destructor);

        let (backend, plan) = runtime_native_plan(&source);

        let artifacts = generated_artifacts_of_kind(
            &backend,
            &plan,
            bray_codegen::BackendArtifactKind::BackendIr,
        );

        for artifact in &artifacts {
            let ir = String::from_utf8_lossy(artifact);

            for name in [".incident.destroy", "__bray_incident_report_identity"] {
                let definitions = ir
                    .lines()
                    .filter(|line| line.starts_with("define ") && line.contains(name))
                    .count();

                assert!(
                    definitions <= 1,
                    "owners with the same cleanup share {name}: {ir}"
                );
            }
        }

        let ir = artifacts
            .iter()
            .map(|artifact| String::from_utf8_lossy(artifact))
            .collect::<String>();

        let callback = ir
            .split("\ndefine ")
            .find(|body| {
                body.lines()
                    .next()
                    .is_some_and(|header| header.contains(".incident.destroy"))
            })
            .expect("fallible static cleanup emits its incident destroyer")
            .split_once("\n}")
            .expect("LLVM function has a closing brace")
            .0;

        let (_, body) = callback
            .split_once("{\n")
            .expect("LLVM callback has a body");

        let (before_cleanup, _) = body
            .split_once("@bray_lifecycle_")
            .expect("incident destructor invokes the selected cleanup");

        let outcome_size = std::mem::size_of::<bray_runtime_abi::NativeRunOutcome>();

        for destination in ["%1", "%2"] {
            assert!(
                before_cleanup.lines().any(|line| {
                    (line.trim_start().starts_with("store ")
                        && line.contains("zeroinitializer")
                        && line.contains(&format!("ptr {destination},")))
                        || (line.contains("@llvm.memset.")
                            && line.contains(&format!("{destination}, i8 0, i64 {outcome_size},")))
                }),
                "{destructor:?}, {destination}: {callback}"
            );
        }
    }
}
