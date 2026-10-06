use super::compilation::test_product_identity;
use crate::CancellationToken;
use crate::compilation::product::specialization::ConcreteCodegenReachability;
use bray_codegen::{CodegenLinkage, CodegenPartitionPolicy, partition_codegen_units};
use bray_runtime_interface::RuntimeAbiRole;
use std::collections::{BTreeMap, BTreeSet};

pub(in super::super) fn assert_native_callback_entry(
    plan: &crate::compilation::product::codegen::NativeProductPlan,
    entry_name: &str,
    entry_linkage: CodegenLinkage,
) -> String {
    let callback = plan
        .mappings()
        .iter()
        .flat_map(bray_codegen::CodegenMappings::symbols)
        .find(|mapping| {
            mapping.native_entry().is_some_and(|entry| {
                entry.name().as_str() == entry_name && entry.linkage() == entry_linkage
            })
        })
        .unwrap_or_else(|| panic!("native callback entry must be mapped"));

    assert_eq!(callback.linkage(), CodegenLinkage::LinkOnce);
    assert_ne!(callback.name().as_str(), entry_name);

    let bray_codegen::CodegenSymbolKey::Instance(instance) = callback.key() else {
        panic!("native callback entry must map a concrete instance");
    };

    let compatibility = plan
        .units()
        .iter()
        .find_map(|unit| unit.key().compatibility(instance))
        .unwrap_or_else(|| panic!("callback instance must retain partition compatibility"));

    assert_eq!(compatibility.linkage(), CodegenLinkage::LinkOnce);

    assert_eq!(
        compatibility.visibility(),
        bray_codegen::CodegenDefinitionVisibility::Product
    );

    let host = plan
        .executable_host()
        .unwrap_or_else(|| panic!("callback plan must retain an executable host"));

    assert!(
        host.requirements()
            .requires_role(RuntimeAbiRole::ForeignCallbackExecution)
    );

    callback.name().as_str().to_owned()
}

pub(in super::super) fn realize_codegen_mappings(
    compilation: &crate::Compilation,
    target: &bray_codegen::CodegenTarget,
    reachability: &ConcreteCodegenReachability,
    cancellation: &CancellationToken,
) {
    let roots = reachability
        .graph()
        .roots()
        .iter()
        .cloned()
        .collect::<BTreeSet<_>>();

    let compatibility = reachability
        .graph()
        .instances()
        .iter()
        .map(|instance| {
            compilation
                .codegen_partition_compatibility(
                    instance,
                    reachability
                        .instance(instance.key())
                        .expect("retained instance must be concrete"),
                    &test_product_identity(),
                    &roots,
                    cancellation,
                )
                .map(|compatibility| (instance.key().clone(), compatibility))
        })
        .collect::<Result<BTreeMap<_, _>, _>>()
        .unwrap_or_else(|error| panic!("consumer partition plan must resolve: {error:?}"));

    let units = partition_codegen_units(
        CodegenPartitionPolicy::NATIVE_BALANCED,
        reachability.graph(),
        |instance| compatibility.get(instance.key()).cloned(),
        |mir| {
            crate::compilation::product::mir_content_identity(&compilation, mir)
                .unwrap_or_else(|error| panic!("test MIR identity must resolve: {error:?}"))
        },
    )
    .unwrap_or_else(|error| panic!("consumer units must partition: {error:?}"));

    for unit in units.iter() {
        compilation
            .codegen_mappings_for_product(
                &test_product_identity(),
                unit,
                None,
                &BTreeSet::new(),
                target,
                &roots,
                reachability,
                false,
                cancellation,
            )
            .unwrap_or_else(|error| panic!("consumer mappings must realize: {error:?}"));
    }
}
