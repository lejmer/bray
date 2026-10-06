use super::support::artifacts::generated_artifacts;
use super::support::compilation::{
    codegen_compilation_for_product, codegen_compilation_for_sources_target,
    codegen_compilation_for_sources_target_with_worker_budget_and_platform_services,
    test_product_identity,
};
use super::support::dependencies::{GenericDependencyFixture, generic_dependency_from_fixture};
use super::support::linker::{test_linked_product, test_linker};
use super::support::runtime::runtime_artifact;
use super::support::specialization::CONCRETE_GENERIC_SOURCE;
use crate::{
    CancellationToken, CompilationOptions, CompilationProfileConfiguration, CompilationProfileMode,
    CompilationRequest, DependencyInterfaceInput, SelectedTarget, WorkerBudget,
};
use bray_base::NonEmptySharedStr;
use bray_codegen::{CodeGenerator, CodeGeneratorRegistry, CodegenConfiguration};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeCapability};
use bray_symbols::{NativeLinkKind, NativeLinkRequirement, ProductKind, TypeData};
use bray_testing::TemporaryFile;
use std::collections::BTreeSet;

use std::sync::Arc;

#[test]
fn native_demand_inventory_is_stable_for_imports_exports_and_static_lifecycle() {
    let source = concat!(
        "module application;\n",
        "using example.dependency.templates.identity;\n",
        "internal struct Resource {}\n",
        "impl Resource\n",
        "{\n",
        "    finalize() {}\n",
        "}\n",
        "internal static RESOURCE: Resource = Resource {};\n",
        "public func exported() -> i32\n",
        "{\n",
        "    return example.dependency.templates.identity<i32>(1);\n",
        "}\n",
    );

    let serial = profiled_dependency_library(source, WorkerBudget::serial(), PROFILE_DEPENDENCY);

    let parallel = profiled_dependency_library(
        source,
        WorkerBudget::new(4)
            .unwrap_or_else(|error| panic!("parallel test worker budget must validate: {error:?}")),
        PROFILE_DEPENDENCY,
    );

    let reordered =
        profiled_dependency_library(source, WorkerBudget::serial(), REORDERED_PROFILE_DEPENDENCY);

    let serial = native_profile(&serial);
    let parallel = native_profile(&parallel);
    let reordered = native_profile(&reordered);

    assert_eq!(serial, parallel);
    assert_eq!(serial, reordered);
    assert_eq!(serial.instances.len(), 8);

    assert!(serial.instances.iter().all(|instance| {
        !instance.inclusion_path.is_empty()
            && instance.pre_optimization_blocks == instance.post_optimization_blocks
            && instance.pre_optimization_operations == instance.post_optimization_operations
    }));

    use bray_profile::CompilationProfileNativeDemandKind as Kind;

    for expected in [Kind::LibraryExport, Kind::StaticLifecycle, Kind::DirectCall] {
        assert!(
            serial.demands.iter().any(|demand| demand.kind == expected),
            "missing {expected:?} from {:?}",
            serial
                .demands
                .iter()
                .map(|demand| demand.kind)
                .collect::<Vec<_>>()
        );
    }

    assert!(serial.units.iter().any(|unit| {
        unit.packages
            .iter()
            .any(|package| package == "example.dependency")
    }));

    let hosted = profiled_product(
        concat!(
            "module application;\n",
            "internal struct HostedResource {}\n",
            "impl HostedResource\n",
            "{\n",
            "    finalize() {}\n",
            "}\n",
            "internal static HOSTED_RESOURCE: HostedResource = HostedResource {};\n",
            "func main() {}\n",
        ),
        ProductKind::Executable,
        WorkerBudget::serial(),
        [],
    );

    let hosted = native_profile(&hosted);

    assert!(
        hosted
            .demands
            .iter()
            .any(|demand| demand.kind == Kind::ExecutableEntry)
    );

    assert!(
        hosted
            .demands
            .iter()
            .any(|demand| demand.kind == Kind::StaticLifecycle)
    );

    let host_root = hosted
        .demands
        .iter()
        .find(|demand| demand.kind == Kind::HostedRoot)
        .expect("hosted product must retain a generated root")
        .target;

    assert!(hosted.runtime_demands.iter().any(|demand| {
        demand.predecessor == Some(host_root)
            && demand.role == RuntimeAbiRole::ProductHostControl.as_str()
            && !demand.provider.is_empty()
    }));
}

#[test]
fn native_preparation_is_deterministic_across_worker_budgets() {
    let (serial_backend, serial_compilation) = codegen_compilation_for_product_with_worker_budget(
        CONCRETE_GENERIC_SOURCE,
        ProductKind::Library,
        WorkerBudget::serial(),
    );

    let parallel_budget = WorkerBudget::new(4)
        .unwrap_or_else(|error| panic!("parallel worker budget must validate: {error:?}"));

    let (parallel_backend, parallel_compilation) =
        codegen_compilation_for_product_with_worker_budget(
            CONCRETE_GENERIC_SOURCE,
            ProductKind::Library,
            parallel_budget,
        );

    let serial = serial_compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .unwrap_or_else(|error| panic!("serial native plan must prepare: {error:?}"));

    let parallel = parallel_compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            None,
            [],
            None,
        )
        .unwrap_or_else(|error| panic!("parallel native plan must prepare: {error:?}"));

    assert_eq!(
        native_partition_recipe(&serial),
        native_partition_recipe(&parallel)
    );

    assert_eq!(serial.executable_host(), parallel.executable_host());
    assert_eq!(serial.product_host(), parallel.product_host());
    assert_eq!(serial.static_instances(), parallel.static_instances());

    assert_eq!(
        generated_artifacts(&serial_backend, &serial),
        generated_artifacts(&parallel_backend, &parallel)
    );
}

#[test]
fn fresh_native_plans_ignore_unrelated_semantic_interning() {
    let storage_source = r#"
            trusted module app;

            static GENERIC_VALUE<const N: i32>: i32 = N;

            func first() -> i32
            {
                return GENERIC_VALUE<1>;
            }

            func second() -> i32
            {
                return GENERIC_VALUE<2>;
            }

            @link(name = "native")
            @symbol(name = "native_value")
            extern trusted static NATIVE: i32;

            trusted func repeated_native_pointer() -> RawPointer<i32>
            {
                return NATIVE;
            }

            trusted func native_pointer() -> RawPointer<i32>
            {
                return NATIVE;
            }
        "#;

    for source in [CONCRETE_GENERIC_SOURCE, storage_source] {
        let native_links = if source == CONCRETE_GENERIC_SOURCE {
            Vec::new()
        } else {
            vec![NativeLinkRequirement::new(
                NonEmptySharedStr::try_new("native").unwrap(),
                NativeLinkKind::Dynamic,
            )]
        };

        let (first_backend, first_compilation) = codegen_compilation_for_sources_target(
            &[source],
            ProductKind::Library,
            SelectedTarget::baseline(),
            &native_links,
        );

        let (second_backend, second_compilation) = codegen_compilation_for_sources_target(
            &[source],
            ProductKind::Library,
            SelectedTarget::baseline(),
            &native_links,
        );

        second_compilation
            .semantic_value_store()
            .expect("second semantic store must exist")
            .intern_type(TypeData::tuple([]))
            .expect("unrelated type must intern");

        let plan = |compilation: &crate::Compilation| {
            compilation
                .native_product_plan(
                    test_product_identity(),
                    crate::BuildConfiguration::Development,
                    None,
                    [],
                    None,
                )
                .unwrap_or_else(|error| panic!("native plan must prepare: {error:?}"))
        };

        let first = plan(&first_compilation);
        let second = plan(&second_compilation);

        let identities = |plan: &crate::compilation::product::codegen::NativeProductPlan| {
            plan.units()
                .iter()
                .map(|unit| unit.key().content_identity())
                .collect::<Vec<_>>()
        };

        assert_eq!(
            native_partition_recipe(&first),
            native_partition_recipe(&second)
        );

        assert_eq!(identities(&first), identities(&second));

        if source != CONCRETE_GENERIC_SOURCE {
            let storage_identities = first
                .units()
                .iter()
                .flat_map(|unit| {
                    unit.instances().iter().map(|instance| {
                        unit.compatibility(instance.key())
                            .unwrap()
                            .native_storage_dependencies_identity()
                    })
                })
                .filter(|identity| *identity != [0; 32])
                .collect::<BTreeSet<_>>();

            assert_eq!(
                storage_identities.len(),
                3,
                "owned specializations and imported storage must have distinct stable identities",
            );
        }

        assert_eq!(
            generated_artifacts(&first_backend, &first),
            generated_artifacts(&second_backend, &second),
        );

        let profile = first_compilation
            .selected_target()
            .target()
            .profile()
            .clone();

        let name = bray_target::TargetOutputName::for_native(
            profile.machine().object_format(),
            bray_target::TargetOutputKind::RelocatableObject,
        );

        let outputs = bray_target::TargetOutputDescription::try_new(profile, [name])
            .expect("native output names must validate");

        let publish =
            |compilation: &crate::Compilation,
             native: &crate::compilation::product::codegen::NativeProductPlan| {
                let output = tempfile::tempdir().expect("managed output directory must exist");

                let request = bray_emitter::EmissionRequest::try_new(
                    test_product_identity(),
                    ProductKind::Library,
                    None,
                    compilation
                        .selected_target()
                        .target()
                        .profile()
                        .identity()
                        .clone(),
                    bray_emitter::RequestedArtifactDestination::FilesystemDirectory(
                        output.path().to_path_buf().into(),
                    ),
                    [bray_emitter::RequestedArtifact::new(
                        bray_emitter::ArtifactKind::RelocatableObject,
                        bray_emitter::ArtifactRequirement::Required,
                    )],
                    bray_emitter::ReplacementPolicy::ReplaceExisting,
                )
                .expect("native emission request must validate");

                let inputs =
                    crate::ProductEmissionInputs::new(&outputs).with_native_codegen(native);

                let result = compilation
                    .emit_product(request, inputs)
                    .unwrap_or_else(|error| panic!("native emission must complete: {error:?}"));

                let generation = result
                    .generation()
                    .expect("managed generation must publish");

                let artifact = &result.artifacts().artifacts()[0];

                let path = generation
                    .artifact_path(artifact.id())
                    .expect("artifact path must resolve");

                std::fs::read(
                    path.parent()
                        .expect("artifact must have a generation directory")
                        .join("manifest.json"),
                )
                .expect("generation manifest must be readable")
            };

        assert_eq!(
            publish(&first_compilation, &first),
            publish(&second_compilation, &second)
        );

        if source == CONCRETE_GENERIC_SOURCE {
            let changed_source = CONCRETE_GENERIC_SOURCE.replace("return count;", "return 12345;");

            let (changed_backend, changed_compilation) =
                codegen_compilation_for_product(&changed_source, ProductKind::Library);

            let changed = plan(&changed_compilation);

            assert_eq!(
                native_partition_recipe(&first),
                native_partition_recipe(&changed)
            );

            assert_ne!(identities(&first), identities(&changed));

            assert_ne!(
                generated_artifacts(&first_backend, &first),
                generated_artifacts(&changed_backend, &changed),
            );
        }
    }
}

#[test]
fn cancelled_native_preparation_publishes_no_partial_plan() {
    let (_, compilation) = codegen_compilation_for_product_with_worker_budget(
        CONCRETE_GENERIC_SOURCE,
        ProductKind::Library,
        WorkerBudget::new(4)
            .unwrap_or_else(|error| panic!("parallel worker budget must validate: {error:?}")),
    );

    let cancellation = CancellationToken::new();

    cancellation.cancel();

    let cancelled = compilation.native_product_plan_with_cancellation(
        test_product_identity(),
        crate::BuildConfiguration::Development,
        None,
        [],
        None,
        &cancellation,
    );

    assert!(cancelled.is_err_and(|error| error.is_cancelled()));

    assert!(
        compilation
            .native_product_plan(
                test_product_identity(),
                crate::BuildConfiguration::Development,
                None,
                [],
                None,
            )
            .is_ok()
    );
}

fn codegen_compilation_for_product_with_worker_budget(
    source: &str,
    product_kind: ProductKind,
    worker_budget: WorkerBudget,
) -> (
    Arc<bray_codegen_llvm::LlvmCodeGenerator>,
    crate::Compilation,
) {
    codegen_compilation_for_sources_target_with_worker_budget_and_platform_services(
        &[source],
        product_kind,
        SelectedTarget::baseline(),
        &[],
        [],
        worker_budget,
    )
}

fn native_partition_recipe(
    plan: &crate::compilation::product::codegen::NativeProductPlan,
) -> Vec<(
    Vec<bray_codegen::CodegenInstanceKey>,
    bray_codegen::CodegenWork,
    Option<bray_codegen::CodegenOversizedUnit>,
)> {
    plan.units()
        .iter()
        .map(|unit| {
            let key = unit.key();

            (
                key.instances().to_vec(),
                key.estimated_work(),
                key.oversized(),
            )
        })
        .collect()
}

const PROFILE_DEPENDENCY: GenericDependencyFixture = GenericDependencyFixture {
    source: concat!(
        "module templates;\n",
        "func helper<T>(pos value: T) -> T { return value; }\n",
        "public func identity<T>(pos value: T) -> T\n",
        "{\n",
        "    let invoke = lambda(pos item: T) -> T { return helper<T>(item); };\n",
        "    return invoke(value);\n",
        "}\n",
        "public func disconnected(pos value: i32) -> i32 { return value; }\n",
    ),
    runtime_frames: None,
    executable_templates: 4,
    platform_service: None,
};

const REORDERED_PROFILE_DEPENDENCY: GenericDependencyFixture = GenericDependencyFixture {
    source: concat!(
        "module templates;\n",
        "public func disconnected(pos value: i32) -> i32 { return value; }\n",
        "public func identity<T>(pos value: T) -> T\n",
        "{\n",
        "    let invoke = lambda(pos item: T) -> T { return helper<T>(item); };\n",
        "    return invoke(value);\n",
        "}\n",
        "func helper<T>(pos value: T) -> T { return value; }\n",
    ),
    runtime_frames: None,
    executable_templates: 4,
    platform_service: None,
};

fn profiled_dependency_library(
    source: &str,
    worker_budget: WorkerBudget,
    dependency: GenericDependencyFixture,
) -> crate::Compilation {
    profiled_product(
        source,
        ProductKind::Library,
        worker_budget,
        [generic_dependency_from_fixture(true, false, dependency)],
    )
}

fn profiled_product<const N: usize>(
    source: &str,
    kind: ProductKind,
    worker_budget: WorkerBudget,
    dependencies: [DependencyInterfaceInput; N],
) -> crate::Compilation {
    let backend = Arc::new(
        bray_codegen_llvm::LlvmCodeGenerator::try_new()
            .unwrap_or_else(|error| panic!("LLVM backend must initialize: {error:?}")),
    );

    let registry = CodeGeneratorRegistry::try_new([Arc::clone(&backend) as Arc<dyn CodeGenerator>])
        .unwrap_or_else(|error| panic!("LLVM backend must register: {error:?}"));

    let codegen = CodegenConfiguration::try_new(registry, backend.identity().clone())
        .unwrap_or_else(|error| panic!("LLVM backend must select: {error:?}"));

    let target = SelectedTarget::baseline();
    let runtime = crate::test_support::runtime_standard_library_dependency(&target);

    let request = CompilationRequest::with_options(
        crate::test_support::package_identity(),
        vec![crate::test_support::source_input(source, 0)],
        CompilationOptions::new(worker_budget, kind, target),
    )
    .with_dependency_interfaces(std::iter::once(runtime).chain(dependencies))
    .with_profile(CompilationProfileConfiguration::new(
        CompilationProfileMode::Summary,
    ));

    let compilation = crate::Compilation::load_with_codegen(request, codegen)
        .unwrap_or_else(|error| panic!("profiled library must load: {error:?}"));

    assert!(
        compilation.check_diagnostics().is_empty(),
        "{:#?}",
        compilation.check_diagnostics()
    );

    let archive = TemporaryFile::write("libbray_runtime.a", b"!<arch>\n");

    let runtime = match kind {
        ProductKind::Executable | ProductKind::Test => {
            Some(runtime_artifact(&compilation, archive.path()))
        }
        ProductKind::Library => None,
    };

    let required_capabilities = runtime.as_ref().map_or_else(Vec::new, |_| {
        vec![
            RuntimeCapability::CooperativeExecution,
            RuntimeCapability::MainThreadLane,
        ]
    });

    compilation
        .native_product_plan(
            test_product_identity(),
            crate::BuildConfiguration::Development,
            runtime,
            required_capabilities,
            Some((&test_linker(), test_linked_product(kind))),
        )
        .unwrap_or_else(|error| panic!("profiled native plan must resolve: {error:?}"));

    compilation
}

fn native_profile(
    compilation: &crate::Compilation,
) -> bray_profile::CompilationProfileNativeCodegen {
    let report = compilation
        .profile_report()
        .unwrap_or_else(|| panic!("native compilation profile must exist"));

    report
        .validate()
        .unwrap_or_else(|error| panic!("native compilation profile must validate: {error:?}"));

    report
        .native_codegen
        .unwrap_or_else(|| panic!("native compilation profile must retain its inventory"))
}
