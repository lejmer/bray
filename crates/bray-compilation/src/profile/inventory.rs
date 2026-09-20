use std::collections::{BTreeMap, BTreeSet};

use bray_profile::{
    CompilationProfileCodegenDependency, CompilationProfileCodegenInstance,
    CompilationProfileCodegenUnit, CompilationProfileNativeCodegen,
};

pub(super) fn set_native_codegen_plan(
    inventory: &mut CompilationProfileNativeCodegen,
    reachability: &bray_codegen::CodegenReachability,
    units: &[bray_codegen::CodegenUnit],
    mappings: &[bray_codegen::CodegenMappings],
) {
    // The profile owns one canonical key list across generated and external instances.
    let mut keys = reachability
        .instances()
        .iter()
        .map(|instance| instance.key().clone())
        .chain(reachability.external_instances().iter().cloned())
        .collect::<Vec<_>>();

    keys.sort_unstable();
    keys.dedup();

    let identities = keys
        .iter()
        .enumerate()
        .map(|(index, key)| {
            let id = u32::try_from(index)
                .unwrap_or_else(|_| panic!("codegen inventory identity must fit u32"));

            (key.clone(), id)
        })
        .collect::<BTreeMap<_, _>>();

    let symbols = mappings
        .iter()
        .flat_map(bray_codegen::CodegenMappings::symbols)
        .filter_map(|mapping| match mapping.key() {
            bray_codegen::CodegenSymbolKey::Instance(instance) => {
                Some((instance.clone(), mapping.name().as_str().to_owned()))
            }
            bray_codegen::CodegenSymbolKey::Runtime(_)
            | bray_codegen::CodegenSymbolKey::ProtectedFrame { .. } => None,
        })
        .collect::<BTreeMap<_, _>>();

    inventory.instances = keys
        .iter()
        .map(|key| CompilationProfileCodegenInstance {
            id: identities[key],
            symbol: symbols.get(key).cloned(),
            root: reachability.roots().binary_search(key).is_ok(),
            external: reachability.is_external(key),
        })
        .collect();

    inventory.dependencies = reachability
        .instances()
        .iter()
        .flat_map(|instance| {
            instance.dependencies().iter().map(|dependency| {
                CompilationProfileCodegenDependency {
                    source: identities[instance.key()],
                    target: identities[dependency.instance()],
                    kind: dependency_kind(dependency.kind()).to_owned(),
                }
            })
        })
        .collect();

    inventory.units = units
        .iter()
        .enumerate()
        .map(|(index, unit)| {
            let mut packages = BTreeSet::new();
            let mut linkages = BTreeSet::new();
            let mut visibilities = BTreeSet::new();

            for instance in unit.instances() {
                let compatibility = unit.compatibility(instance.key()).unwrap_or_else(|| {
                    panic!("validated codegen unit must retain every compatibility class")
                });

                packages.insert(compatibility.package().as_str().to_owned());
                linkages.insert(linkage_name(compatibility.linkage()).to_owned());
                visibilities.insert(visibility_name(compatibility.visibility()).to_owned());
            }

            CompilationProfileCodegenUnit {
                id: u32::try_from(index)
                    .unwrap_or_else(|_| panic!("codegen unit identity must fit u32")),
                work: unit.estimated_work().units(),
                instances: unit
                    .instances()
                    .iter()
                    .map(|instance| identities[instance.key()])
                    .collect(),
                packages: packages.into_iter().collect(),
                linkages: linkages.into_iter().collect(),
                visibilities: visibilities.into_iter().collect(),
            }
        })
        .collect();
}

const fn dependency_kind(kind: bray_codegen::CodegenInstanceDependencyKind) -> &'static str {
    match kind {
        bray_codegen::CodegenInstanceDependencyKind::Definition => "definition",
        bray_codegen::CodegenInstanceDependencyKind::DirectAwaitedFrame => "direct_awaited_frame",
        bray_codegen::CodegenInstanceDependencyKind::StartedTask => "started_task",
    }
}

const fn linkage_name(linkage: bray_codegen::CodegenLinkage) -> &'static str {
    match linkage {
        bray_codegen::CodegenLinkage::Private => "private",
        bray_codegen::CodegenLinkage::Internal => "internal",
        bray_codegen::CodegenLinkage::External => "external",
        bray_codegen::CodegenLinkage::Weak => "weak",
        bray_codegen::CodegenLinkage::Fallback => "fallback",
        bray_codegen::CodegenLinkage::LinkOnce => "link_once",
        bray_codegen::CodegenLinkage::Common => "common",
        bray_codegen::CodegenLinkage::Import => "import",
        bray_codegen::CodegenLinkage::Export => "export",
    }
}

const fn visibility_name(visibility: bray_codegen::CodegenDefinitionVisibility) -> &'static str {
    match visibility {
        bray_codegen::CodegenDefinitionVisibility::Unit => "unit",
        bray_codegen::CodegenDefinitionVisibility::Product => "product",
        bray_codegen::CodegenDefinitionVisibility::Public => "public",
    }
}
