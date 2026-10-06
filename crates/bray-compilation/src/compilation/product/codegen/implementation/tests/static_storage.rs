use super::support::artifacts::generated_artifacts;
use super::support::runtime::{
    runtime_native_plan, runtime_native_plan_for_sources_target, runtime_native_plan_for_target,
};
use crate::SelectedTarget;

use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{ProductKind, StaticStorageDuration};
use bray_target::NativeTarget;
use std::collections::BTreeSet;

#[test]
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

    assert!(plan.product_host().is_none());

    assert!(
        generated_artifacts(&backend, &plan)
            .iter()
            .all(|artifact| !artifact.is_empty())
    );
}

#[test]
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
fn repeated_static_references_emit_one_native_instance() {
    let source = concat!(
        "module app;\n",
        "static ANSWER: i32 = 42;\n",
        "func answer_address() -> RawPointer<i32>\n",
        "{\n",
        "    return core.memory.address_of<i32>(&ANSWER);\n",
        "}\n",
        "public func main() -> i32\n",
        "{\n",
        "    let first = core.memory.address_of<i32>(&ANSWER);\n",
        "    let second = answer_address();\n",
        "    return ANSWER;\n",
        "}\n",
    );

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
            match bray_native_artifact::scan_object_unit_summary(artifact).unwrap() {
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
        .unwrap();

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
fn static_cleanup_orders_declaration_names_independently_of_source_order() {
    let (_, plan) = runtime_native_plan_for_target(
        concat!(
            "module app;\n",
            "@thread_local static ZETA: u64 = 1;\n",
            "@thread_local static ALPHA: u64 = 2;\n",
            "func main() -> i32 { let _: u64 = ZETA + ALPHA; return 0; }\n"
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
                .unwrap();

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
        "    let _: (u64, u64) = values();\n",
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
                .unwrap();

            let value = plan
                .mappings()
                .iter()
                .flat_map(bray_codegen::CodegenMappings::constants)
                .find(|constant| constant.value() == storage.initial_value())
                .unwrap();

            let bray_symbols::ConstantValueKind::Integer(integer) = value.data().kind() else {
                panic!("generic integer static must retain its integer initializer");
            };

            integer.magnitude().to_vec()
        })
        .collect::<Vec<_>>();

    assert_eq!(values, [vec![7], vec![11]]);
}

#[test]
fn native_static_storage_fixture_prepares_for_windows() {
    let sources = [
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../xtask/fixtures/native-execution/static_storage.bray"
        )),
        include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../xtask/fixtures/native-execution/static_storage_contribution.bray"
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
