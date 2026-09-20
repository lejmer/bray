use std::collections::{BTreeMap, BTreeSet};

use bray_profile::{
    CompilationProfileCodegenDependency, CompilationProfileCodegenInstance,
    CompilationProfileCodegenUnit, CompilationProfileNativeCodegen,
    CompilationProfileNativeDemand, CompilationProfileNativeDemandKind,
};

pub(super) fn set_native_codegen_plan(
    inventory: &mut CompilationProfileNativeCodegen,
    reachability: &bray_codegen::CodegenReachability,
    demands: &[crate::compilation::NativeDemand],
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

    inventory.demands = demands
        .iter()
        .map(|demand| CompilationProfileNativeDemand {
            predecessor: demand.predecessor().map(|key| identities[key]),
            target: identities[demand.target()],
            kind: demand_kind(demand.reason()),
        })
        .collect();

    let inclusion_paths = canonical_inclusion_paths(keys.len(), &inventory.demands);

    inventory.instances = keys
        .iter()
        .map(|key| CompilationProfileCodegenInstance {
            id: identities[key],
            symbol: symbols.get(key).cloned(),
            external: reachability.is_external(key),
            inclusion_path: inclusion_paths[usize::try_from(identities[key])
                .unwrap_or_else(|_| panic!("profile instance identity must fit usize"))]
            .clone(),
            pre_optimization_blocks: mir_blocks(reachability, key),
            pre_optimization_operations: mir_operations(reachability, key),
            post_optimization_blocks: mir_blocks(reachability, key),
            post_optimization_operations: mir_operations(reachability, key),
        })
        .collect();

    let dependencies = reachability
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
        .collect::<Vec<_>>();

    inventory.dependencies = canonical_dependencies(dependencies);

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

            let instances = unit
                .instances()
                .iter()
                .map(|instance| identities[instance.key()])
                .collect::<Vec<_>>();

            let inclusion_path = instances
                .iter()
                .map(|id| {
                    &inclusion_paths[usize::try_from(*id)
                        .unwrap_or_else(|_| panic!("profile instance identity must fit usize"))]
                })
                .min_by(|left, right| (left.len(), left).cmp(&(right.len(), right)))
                .cloned()
                .unwrap_or_else(|| panic!("codegen unit must contain an instance"));

            CompilationProfileCodegenUnit {
                id: u32::try_from(index)
                    .unwrap_or_else(|_| panic!("codegen unit identity must fit u32")),
                work: unit.estimated_work().units(),
                instances,
                inclusion_path,
                packages: packages.into_iter().collect(),
                linkages: linkages.into_iter().collect(),
                visibilities: visibilities.into_iter().collect(),
            }
        })
        .collect();
}

fn mir_blocks(
    reachability: &bray_codegen::CodegenReachability,
    key: &bray_codegen::CodegenInstanceKey,
) -> u64 {
    reachability.instance(key).map_or(0, |instance| {
        u64::try_from(instance.mir().blocks().len()).unwrap_or(u64::MAX)
    })
}

fn mir_operations(
    reachability: &bray_codegen::CodegenReachability,
    key: &bray_codegen::CodegenInstanceKey,
) -> u64 {
    reachability.instance(key).map_or(0, |instance| {
        u64::try_from(instance.mir().operations().len()).unwrap_or(u64::MAX)
    })
}

fn canonical_inclusion_paths(
    instance_count: usize,
    demands: &[CompilationProfileNativeDemand],
) -> Vec<Vec<u32>> {
    let mut paths = vec![None; instance_count];

    for (index, demand) in demands.iter().enumerate() {
        if demand.predecessor.is_none() {
            let index = u32::try_from(index)
                .unwrap_or_else(|_| panic!("native demand identity must fit u32"));

            let target = usize::try_from(demand.target)
                .unwrap_or_else(|_| panic!("profile instance identity must fit usize"));

            retain_shorter_path(&mut paths[target], vec![index]);
        }
    }

    for _ in 0..instance_count {
        let mut changed = false;

        for (index, demand) in demands.iter().enumerate() {
            let Some(predecessor) = demand.predecessor else {
                continue;
            };

            let predecessor = usize::try_from(predecessor)
                .unwrap_or_else(|_| panic!("profile instance identity must fit usize"));

            let Some(mut path) = paths[predecessor].clone() else {
                continue;
            };

            path.push(
                u32::try_from(index)
                    .unwrap_or_else(|_| panic!("native demand identity must fit u32")),
            );

            let target = usize::try_from(demand.target)
                .unwrap_or_else(|_| panic!("profile instance identity must fit usize"));

            changed |= retain_shorter_path(&mut paths[target], path);
        }

        if !changed {
            break;
        }
    }

    paths
        .into_iter()
        .map(|path| path.unwrap_or_else(|| panic!("reachable native instance must have a demand path")))
        .collect()
}

fn retain_shorter_path(current: &mut Option<Vec<u32>>, candidate: Vec<u32>) -> bool {
    if current
        .as_ref()
        .is_some_and(|path| (path.len(), path) <= (candidate.len(), &candidate))
    {
        return false;
    }

    *current = Some(candidate);

    true
}

const fn demand_kind(
    reason: crate::compilation::NativeDemandReason,
) -> CompilationProfileNativeDemandKind {
    use crate::compilation::NativeDemandReason as Reason;

    match reason {
        Reason::ExecutableEntry => CompilationProfileNativeDemandKind::ExecutableEntry,
        Reason::TestEntry => CompilationProfileNativeDemandKind::TestEntry,
        Reason::LibraryExport => CompilationProfileNativeDemandKind::LibraryExport,
        Reason::ImplementationFulfillment => {
            CompilationProfileNativeDemandKind::ImplementationFulfillment
        }
        Reason::RuntimeRole => CompilationProfileNativeDemandKind::RuntimeRole,
        Reason::NativeExport => CompilationProfileNativeDemandKind::NativeExport,
        Reason::StaticLifecycle => CompilationProfileNativeDemandKind::StaticLifecycle,
        Reason::CallableDefault => CompilationProfileNativeDemandKind::CallableDefault,
        Reason::DirectCall => CompilationProfileNativeDemandKind::DirectCall,
        Reason::AddressedFunction => CompilationProfileNativeDemandKind::AddressedFunction,
        Reason::GeneratedHelper => CompilationProfileNativeDemandKind::GeneratedHelper,
        Reason::NativeReference => CompilationProfileNativeDemandKind::NativeReference,
        Reason::HostedRoot => CompilationProfileNativeDemandKind::HostedRoot,
    }
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

fn canonical_dependencies(
    mut dependencies: Vec<CompilationProfileCodegenDependency>,
) -> Vec<CompilationProfileCodegenDependency> {
    dependencies.sort_unstable_by(|left, right| {
        (&left.source, &left.target, &left.kind).cmp(&(&right.source, &right.target, &right.kind))
    });

    dependencies
}

#[cfg(test)]
mod tests {
    use bray_profile::{
        CompilationProfileCodegenDependency, CompilationProfileNativeDemand,
        CompilationProfileNativeDemandKind,
    };

    use super::{canonical_dependencies, canonical_inclusion_paths};

    #[test]
    fn mixed_dependency_kinds_are_ordered_by_source_target_then_kind() {
        let dependency = |source, target, kind: &str| CompilationProfileCodegenDependency {
            source,
            target,
            kind: kind.to_owned(),
        };

        let dependencies = canonical_dependencies(vec![
            dependency(0, 2, "direct_awaited_frame"),
            dependency(0, 1, "started_task"),
            dependency(0, 1, "definition"),
        ]);

        assert_eq!(
            dependencies,
            [
                dependency(0, 1, "definition"),
                dependency(0, 1, "started_task"),
                dependency(0, 2, "direct_awaited_frame"),
            ]
        );
    }

    #[test]
    fn inclusion_paths_choose_the_shortest_canonical_explanation() {
        let demand = |predecessor, target, kind| CompilationProfileNativeDemand {
            predecessor,
            target,
            kind,
        };

        let demands = [
            demand(None, 0, CompilationProfileNativeDemandKind::ExecutableEntry),
            demand(None, 1, CompilationProfileNativeDemandKind::RuntimeRole),
            demand(Some(0), 2, CompilationProfileNativeDemandKind::DirectCall),
            demand(Some(1), 2, CompilationProfileNativeDemandKind::DirectCall),
            demand(Some(2), 3, CompilationProfileNativeDemandKind::GeneratedHelper),
        ];

        assert_eq!(
            canonical_inclusion_paths(4, &demands),
            [vec![0], vec![1], vec![0, 2], vec![0, 2, 4]]
        );
    }
}
