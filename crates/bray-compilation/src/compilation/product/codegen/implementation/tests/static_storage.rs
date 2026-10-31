use super::support::artifacts::generated_artifacts;
use super::support::compilation::{codegen_compilation, source_request_with_runtime};
use super::support::dependencies::{
    GenericDependencyFixture, generic_consumer_for_target_with_source,
    generic_dependency_from_fixture,
};
use super::support::mappings::realize_codegen_mappings;
use super::support::runtime::{
    runtime_native_plan, runtime_native_plan_for_product, runtime_native_plan_for_sources_target,
    runtime_native_plan_for_target,
};
use crate::{CancellationToken, CompilationOptions, SelectedTarget, WorkerBudget};

use bray_codegen::{CodegenOptions, OptimizationLevel};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{ProductKind, StaticStorageDuration};
use bray_target::NativeTarget;
use std::collections::BTreeSet;

const CONTEXTUAL_SELF_STATIC_SOURCE: &str = r#"
    module templates;

    static VALUE<T>: u32 = 7;

    struct Cell<T>
    {
        value: T;
    }

    trait Access
    {
        func read() -> u32
        {
            return VALUE<Self> + VALUE<(Self, Self)>;
        }
    }

    impl CellAccess = Cell<T>(Access)
    {
    }

    impl Cell<T>
    {
        func inherent() -> u32
        {
            return VALUE<Self> + VALUE<(Self, Self)>;
        }
    }

    func forward<T>(pos cell: &Cell<T>) -> u32
    {
        return cell.read() + cell.inherent() + VALUE<Cell<T>> + VALUE<(Cell<T>, Cell<T>)>;
    }
"#;

#[test]
#[ignore = "tier:native"]
fn contextual_self_generic_statics_share_the_concrete_instance() {
    let source = r#"
        module application;

        using templates;

        func main() -> i32
        {
            let first: templates.Cell<i32> = { value = 1 };
            let second: templates.Cell<u64> = { value = 2 };

            if templates.forward<i32>(&first) != 42 || templates.forward<u64>(&second) != 42
            {
                return 1;
            }

            return 0;
        }
    "#;

    let (_, source_compilation) = codegen_compilation(source_request_with_runtime(
        &[CONTEXTUAL_SELF_STATIC_SOURCE, source],
        CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Executable,
            SelectedTarget::baseline(),
        ),
    ));

    let imported_compilation = generic_consumer_for_target_with_source(
        generic_dependency_from_fixture(
            true,
            false,
            GenericDependencyFixture {
                source: CONTEXTUAL_SELF_STATIC_SOURCE,
                runtime_frames: &[],
                executable_templates: 4,
                platform_service: None,
            },
        ),
        SelectedTarget::baseline(),
        &source.replace("templates", "example.dependency.templates"),
    );

    for compilation in [&source_compilation, &imported_compilation] {
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
            .expect("test target must support codegen");

        let semantics = compilation
            .product_semantics()
            .expect("static consumer semantics must resolve");

        for optimization in [
            OptimizationLevel::None,
            OptimizationLevel::Basic,
            OptimizationLevel::Full,
        ] {
            let roots = compilation
                .product_root_instances(semantics.value(), None, &target, &cancellation)
                .expect("static consumer roots must resolve");

            let reachability = compilation
                .codegen_reachability(
                    roots,
                    None,
                    &target,
                    CodegenOptions::default().with_optimization(optimization),
                    false,
                    &cancellation,
                )
                .expect("contextual Self static instances must close");

            let mappings =
                realize_codegen_mappings(compilation, &target, &reachability, &cancellation);

            let instances = mappings
                .iter()
                .flat_map(bray_codegen::CodegenMappings::static_storages)
                .map(|mapping| mapping.instance())
                .collect::<BTreeSet<_>>();

            assert_eq!(
                instances.len(),
                4,
                "Self and explicit types must share storage, while distinct subjects and nested types must not"
            );
        }
    }
}

#[test]
#[ignore = "tier:native"]
fn library_static_definition_survives_initializer_pruning() {
    let (backend, plan) = runtime_native_plan_for_product(
        "module app; static ANSWER: i32 = 42;",
        ProductKind::Library,
    );

    let statics = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::static_storages)
        .map(|mapping| mapping.instance())
        .collect::<BTreeSet<_>>();

    assert_eq!(
        statics.len(),
        1,
        "the exported static keeps its semantic instance"
    );

    assert!(
        plan.mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::static_storages)
            .any(|mapping| mapping.defines_storage() && mapping.storage().is_none())
    );

    assert!(!generated_artifacts(&backend, &plan).is_empty());
}

#[test]
#[ignore = "tier:native"]
fn initialized_protected_statics_preserve_scalar_and_product_representations() {
    let source = r#"
        trusted module app;

        using core.atomic;
        using core.memory;

        struct State
        {
            count: u64;
            ready: bool;
        }

        static COUNTER: Uninit<core.atomic.Atomic<u64>> =
            core.memory.uninit_from<core.atomic.Atomic<u64>>(core.atomic.initialize<u64>(17));
        static STATE: Uninit<State> = core.memory.uninit_from<State>({ count = 23, ready = true });

        func main() -> i32
        {
            let counter = core.memory.uninit_pointer<core.atomic.Atomic<u64>>(&COUNTER);
            let state = core.memory.uninit_pointer<State>(&STATE);

            if core.memory.is_null<core.atomic.Atomic<u64>>(counter) || core.memory.is_null<State>(state)
            {
                return 1;
            }

            return 0;
        }
    "#;

    let (backend, plan) = runtime_native_plan(source);

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
#[ignore = "tier:native"]
fn trivial_generic_statics_use_direct_native_storage() {
    let source = concat!(
        "module app;\n",
        "\n",
        "static GENERIC_VALUE<const N: i32>: i32 = N;\n",
        "\n",
        "func main() -> i32\n",
        "{\n",
        "    return GENERIC_VALUE<1> + GENERIC_VALUE<2> - 3;\n",
        "}\n",
    );

    let (backend, plan) = runtime_native_plan(source);

    let static_mappings = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::static_storages)
        .collect::<Vec<_>>();

    assert!(static_mappings.len() >= 4);

    let instances = static_mappings
        .iter()
        .map(|mapping| mapping.instance())
        .collect::<BTreeSet<_>>();

    let symbols = static_mappings
        .iter()
        .map(|mapping| mapping.symbol())
        .collect::<BTreeSet<_>>();

    assert_eq!(instances.len(), 2);
    assert_eq!(symbols.len(), instances.len());

    let product_instances = instances
        .iter()
        .filter(|instance| instance.duration() == StaticStorageDuration::Product)
        .count();

    let thread_instances = instances
        .iter()
        .filter(|instance| instance.duration() == StaticStorageDuration::ExactThread)
        .count();

    assert_eq!(product_instances, 2);
    assert_eq!(thread_instances, 0);

    assert!(
        static_mappings
            .iter()
            .all(|mapping| mapping.finalization().is_none()
                && mapping.destroy().is_none()
                && mapping.outgoing_requirements().is_empty())
    );

    // Reusable report code resolves the resident consumer descriptor. Trivial
    // statics still need neither managed entries nor an independent provider.
    let host = plan
        .product_host()
        .expect("report construction retains its resident consumer descriptor");

    assert!(host.is_final_image());
    assert!(!host.is_independent());
    assert!(host.statics().is_empty());
    assert!(host.execution().is_none());

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
#[ignore = "tier:native"]
fn exact_thread_static_emits_attachment_owned_cleanup() {
    let source = concat!(
        "module app;\n",
        "@thread_local static THREAD_ANSWER: i32 = 42;\n",
        "func main() -> i32\n",
        "{\n",
        "    return THREAD_ANSWER;\n",
        "}\n",
    );

    let (backend, plan) = runtime_native_plan(source);

    let mappings = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::static_storages)
        .filter(|mapping| mapping.instance().duration() == StaticStorageDuration::ExactThread)
        .collect::<Vec<_>>();

    assert!(!mappings.is_empty());

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
#[ignore = "tier:native"]
fn repeated_static_references_emit_one_native_instance() {
    let source = r#"module app;

using core.memory;
static ANSWER: i32 = 42;
func answer_address() -> DataPointer<i32>
{
    return core.memory.address_of<i32>(&ANSWER);
}
public func main() -> i32
{
    let first = core.memory.address_of<i32>(&ANSWER);
    let second = answer_address();
    return ANSWER;
}
"#;

    let (backend, plan) = runtime_native_plan_for_target(
        source,
        ProductKind::Library,
        SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
    );

    assert_eq!(
        plan.mappings()
            .iter()
            .flat_map(bray_codegen::CodegenMappings::static_storages)
            .map(|mapping| mapping.instance().clone())
            .collect::<BTreeSet<_>>()
            .len(),
        1
    );

    let units = generated_artifacts(&backend, &plan);

    let definitions = units
        .iter()
        .flat_map(|artifact| {
            match bray_native_artifact::scan_object_unit_summary(artifact)
                .expect("fixture object unit must have a native summary")
            {
                bray_native_artifact::NativeUnitSummary::Exact { definitions, .. } => {
                    definitions.to_vec()
                }
                summary => {
                    panic!("ordinary static units must have exact summaries: {summary:?}")
                }
            }
        })
        .collect::<Vec<_>>();

    let storage = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::static_storages)
        .next()
        .expect("fixture iterator must provide its expected item");

    for name in [
        storage.symbol().as_str().to_owned(),
        storage.accessor_name(),
        storage.host_name(),
    ] {
        assert_eq!(
            definitions
                .iter()
                .filter(|definition| definition.symbol().identity().name() == Some(name.as_str()))
                .count(),
            1,
            "static definition {name} must have one owning unit"
        );
    }

    assert!(units.iter().all(|artifact| !artifact.is_empty()));
}

#[test]
#[ignore = "tier:native"]
fn static_cleanup_orders_declaration_names_independently_of_source_order() {
    let (_, plan) = runtime_native_plan_for_target(
        concat!(
            "module app;\n",
            "@thread_local static ZETA: u64 = 1;\n",
            "@thread_local static ALPHA: u64 = 2;\n",
            "func main() -> i32 { _ = ZETA + ALPHA; return 0; }\n"
        ),
        ProductKind::Executable,
        SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
    );

    let host = plan.product_host().expect("thread statics need a host");

    let names = host
        .statics()
        .iter()
        .map(|entry| {
            let mapping = plan
                .mappings()
                .iter()
                .flat_map(bray_codegen::CodegenMappings::static_storages)
                .find(|mapping| mapping.host_name() == entry.host_symbol().as_str())
                .expect("fixture lookup must find its expected value");

            if mapping
                .instance()
                .order_key()
                .windows(5)
                .any(|text| text == b"ALPHA")
            {
                "ALPHA"
            } else {
                assert!(
                    mapping
                        .instance()
                        .order_key()
                        .windows(4)
                        .any(|text| text == b"ZETA")
                );

                "ZETA"
            }
        })
        .collect::<Vec<_>>();

    assert_eq!(names, ["ALPHA", "ZETA"]);
}

#[test]
#[ignore = "tier:native"]
fn const_generic_static_specializations_emit_distinct_native_instances() {
    let source = concat!(
        "module app;\n",
        "@thread_local static VALUE<const N: u64>: u64 = N;\n",
        "func values() -> (u64, u64)\n",
        "{\n",
        "    return(VALUE<7>, VALUE<11>);\n",
        "}\n",
        "func main() -> i32\n",
        "{\n",
        "    _ = values();\n",
        "    return 0;\n",
        "}\n",
    );

    let (_, plan) = runtime_native_plan_for_target(
        source,
        ProductKind::Executable,
        SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
    );

    let mappings = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::static_storages)
        .collect::<Vec<_>>();

    assert_eq!(
        mappings
            .iter()
            .map(|mapping| mapping.instance())
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );

    assert_eq!(
        mappings
            .iter()
            .map(|mapping| mapping.symbol())
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );

    assert_eq!(
        mappings
            .iter()
            .map(|mapping| mapping.initial_value())
            .collect::<BTreeSet<_>>()
            .len(),
        2
    );

    let host = plan
        .product_host()
        .expect("thread static instances need a host");

    let values = host
        .statics()
        .iter()
        .map(|entry| {
            let storage = mappings
                .iter()
                .find(|mapping| mapping.host_name() == entry.host_symbol().as_str())
                .expect("fixture lookup must find its expected value");

            let value = plan
                .mappings()
                .iter()
                .flat_map(bray_codegen::CodegenMappings::constants)
                .find(|constant| constant.value() == storage.initial_value())
                .expect("fixture lookup must find its expected value");

            let bray_symbols::ConstantValueKind::Integer(integer) = value.data().kind() else {
                panic!("generic integer static must retain its integer initializer");
            };

            integer.magnitude().to_vec()
        })
        .collect::<Vec<_>>();

    assert_eq!(values, [vec![7], vec![11]]);
}

#[test]
#[ignore = "tier:native"]
fn native_static_storage_fixture_prepares_for_windows() {
    let sources = [
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../xtask/fixtures/native-execution/static-storage.bray"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../xtask/fixtures/native-execution/static-storage-contribution.bray"
        )),
    ];

    let (backend, plan) = runtime_native_plan_for_sources_target(
        &sources,
        ProductKind::Executable,
        SelectedTarget::for_native(NativeTarget::X86_64WindowsMsvc),
        &[],
    );

    let requirements = plan
        .executable_host()
        .unwrap_or_else(|| panic!("fixture must retain an executable host"))
        .requirements();

    assert!(requirements.requires_role(RuntimeAbiRole::AwaitedFrameComposition));
    assert!(requirements.requires_role(RuntimeAbiRole::FrameCompletionMove));

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}
