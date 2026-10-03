use std::collections::{BTreeMap, BTreeSet};
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use bray_base::{StableDigestHasher, shared_slice};

use super::{
    CodegenInstance, CodegenInstanceDependencyKind, CodegenInstanceKey,
    CodegenPartitionCompatibility, CodegenPartitionPolicy, CodegenReachability, CodegenUnit,
    CodegenUnitBuildError, CodegenWork,
};

/// Partitions a closed reachable graph using deterministic dependency and MIR structure.
pub fn partition_codegen_units(
    policy: CodegenPartitionPolicy,
    reachability: &CodegenReachability,
    compatibility: impl Fn(&CodegenInstance) -> Option<CodegenPartitionCompatibility>,
    mir_content_identity: impl Fn(&bray_ir::MirUnit) -> [u8; 32],
) -> Result<Arc<[CodegenUnit]>, CodegenPartitionError> {
    let instances = reachability.instances();

    let indices: BTreeMap<_, _> = instances
        .iter()
        .enumerate()
        .map(|(index, instance)| (instance.key(), index))
        .collect();

    let classes = instances
        .iter()
        .map(|instance| {
            compatibility(instance).ok_or_else(|| {
                // The error retains the omitted identity beyond the reachability borrow.
                CodegenPartitionError::MissingCompatibility(instance.key().clone())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut groups = required_groups(instances, &indices, &classes)
        .into_values()
        .map(|members| PartitionGroup::new(policy, instances, &members, &classes))
        .collect::<Vec<_>>();

    groups.sort_unstable_by(|left, right| {
        left.target
            .cmp(right.target)
            .then_with(|| left.dependencies.cmp(&right.dependencies))
            .then_with(|| left.anchor.cmp(&right.anchor))
            .then_with(|| left.instances[0].key().cmp(right.instances[0].key()))
    });

    // Each recipe owns its per-definition metadata beyond the retained graph borrow.
    let compatibility = |instance: &CodegenInstance| Some(classes[indices[instance.key()]].clone());

    let mut units = Vec::new();
    let mut current = Vec::new();
    let mut current_work = CodegenWork::new(0);
    let mut current_dependencies = PublicationDependencies::default();

    for group in groups {
        if current
            .first()
            .is_some_and(|instance: &CodegenInstance| instance.key().target() != group.target)
            || !current.is_empty() && current_dependencies != group.dependencies
            || group.independent_publication
            || current_work.saturating_add(group.work) > policy.upper_bound()
        {
            finish_unit(
                policy,
                &mut current,
                &mut current_work,
                &mut units,
                &compatibility,
                &mir_content_identity,
            )?;
        }

        if group.independent_publication || group.work > policy.upper_bound() {
            units.push(
                CodegenUnit::try_from_partition(
                    policy,
                    // The unit owns Arc-backed payloads after the reachability graph is released.
                    group.instances.into_iter().cloned(),
                    &compatibility,
                    true,
                    &mir_content_identity,
                )
                .map_err(CodegenPartitionError::InvalidUnit)?,
            );

            continue;
        }

        current_dependencies = group.dependencies;
        current_work = current_work.saturating_add(group.work);

        // Pending units retain Arc-backed instance payloads across group iterations.
        current.extend(group.instances.into_iter().cloned());

        if current_work >= policy.lower_bound()
            && is_content_boundary(policy, group.anchor, group.work)
        {
            finish_unit(
                policy,
                &mut current,
                &mut current_work,
                &mut units,
                &compatibility,
                &mir_content_identity,
            )?;
        }
    }

    finish_unit(
        policy,
        &mut current,
        &mut current_work,
        &mut units,
        &compatibility,
        &mir_content_identity,
    )?;

    Ok(shared_slice(units))
}

fn required_groups<'a>(
    instances: &'a [CodegenInstance],
    indices: &BTreeMap<&'a CodegenInstanceKey, usize>,
    classes: &[CodegenPartitionCompatibility],
) -> BTreeMap<usize, Vec<usize>> {
    let mut edges = vec![Vec::new(); instances.len()];
    let mut disjoint = DisjointSet::new(instances.len());

    for (source, instance) in instances.iter().enumerate() {
        for dependency in instance.dependencies() {
            let Some(&target) = indices.get(dependency.instance()) else {
                continue;
            };

            edges[source].push(target);

            // A private definition cannot be resolved by an external declaration.
            // Independent started tasks can import frame helpers across units.
            // direct-awaited frames and cycles remain indivisible.
            if dependency.kind() == CodegenInstanceDependencyKind::DirectAwaitedFrame
                || classes[target].visibility() == super::CodegenDefinitionVisibility::Unit
            {
                disjoint.union(source, target);
            }
        }
    }

    for component in
        bray_base::strongly_connected_components(0..edges.len(), |node| edges[node].iter().copied())
    {
        let Some((&first, rest)) = component.split_first() else {
            continue;
        };

        for &member in rest {
            disjoint.union(first, member);
        }
    }

    let mut groups = BTreeMap::new();

    for index in 0..instances.len() {
        groups
            .entry(disjoint.find(index))
            .or_insert_with(Vec::new)
            .push(index);
    }

    groups
}

fn finish_unit(
    policy: CodegenPartitionPolicy,
    current: &mut Vec<CodegenInstance>,
    current_work: &mut CodegenWork,
    units: &mut Vec<CodegenUnit>,
    compatibility: &impl Fn(&CodegenInstance) -> Option<CodegenPartitionCompatibility>,
    mir_content_identity: &impl Fn(&bray_ir::MirUnit) -> [u8; 32],
) -> Result<(), CodegenPartitionError> {
    if current.is_empty() {
        return Ok(());
    }

    let instances = std::mem::take(current);

    *current_work = CodegenWork::new(0);

    units.push(
        CodegenUnit::try_from_partition(
            policy,
            instances,
            compatibility,
            false,
            mir_content_identity,
        )
        .map_err(CodegenPartitionError::InvalidUnit)?,
    );

    Ok(())
}

fn is_content_boundary(
    policy: CodegenPartitionPolicy,
    anchor: [u8; 32],
    group_work: CodegenWork,
) -> bool {
    // Partition ordering consumes the digest prefix. Use a disjoint suffix so the
    // cut marker remains independent of an insertion's sorted position.
    let marker = u64::from_le_bytes([
        anchor[24], anchor[25], anchor[26], anchor[27], anchor[28], anchor[29], anchor[30],
        anchor[31],
    ]);

    marker % policy.target_work().units() < group_work.units().min(policy.target_work().units())
}

struct PartitionGroup<'a> {
    target: &'a bray_ir::MirTargetContract,
    instances: Vec<&'a CodegenInstance>,
    work: CodegenWork,
    anchor: [u8; 32],
    independent_publication: bool,
    dependencies: PublicationDependencies<'a>,
}

impl<'a> PartitionGroup<'a> {
    fn new(
        policy: CodegenPartitionPolicy,
        instances: &'a [CodegenInstance],
        members: &[usize],
        classes: &[CodegenPartitionCompatibility],
    ) -> Self {
        let first = *members.first().expect("required groups must be nonempty");
        let mut group_instances = Vec::with_capacity(members.len());
        let mut work = CodegenWork::new(0);
        let mut hasher = StableDigestHasher::new();

        hasher.write(b"bray.codegen-partition-group");
        policy.hash(&mut hasher);

        for &member in members {
            let instance = &instances[member];

            // Stable semantic identities order groups independently of body edits
            // and per-definition package, source, linkage or visibility metadata.
            instance.key().hash(&mut hasher);
            work = work.saturating_add(policy.estimate(instance));
            group_instances.push(instance);
        }

        // A library unit must not introduce another member's unrelated semantic
        // dependency closure. Independent leaves and callers with the same providers
        // can still share bounded publication work. Product batching has closed demand.
        let dependencies = if policy.identity()
            == CodegenPartitionPolicy::NATIVE_LIBRARY_PUBLICATION.identity()
        {
            let owned = group_instances.iter().map(|instance| instance.key()).collect::<BTreeSet<_>>();

            PublicationDependencies {
                instances: group_instances
                    .iter()
                    .flat_map(|instance| instance.dependencies())
                    .map(|dependency| dependency.instance())
                    .filter(|key| !owned.contains(key))
                    .collect(),
                statics: members.iter()
                    .map(|&member| classes[member].native_storage_dependencies_identity())
                    .filter(|identity| *identity != [0; 32])
                    .collect(),
                runtime: group_instances
                    .iter()
                    .flat_map(|instance| crate::demanded_runtime_references_for_mir(instance.mir()))
                    .collect(),
            }
        } else {
            PublicationDependencies::default()
        };

        Self {
            target: instances[first].key().target(),
            instances: group_instances,
            work,
            anchor: hasher.finalize(),
            dependencies,
            // Overridable definitions and optional native references can make bitcode
            // summaries opaque. Do not hide unrelated exports in their publication unit.
            independent_publication: policy.identity()
                == CodegenPartitionPolicy::NATIVE_LIBRARY_PUBLICATION.identity()
                && members.iter().any(|&member| {
                    let class = &classes[member];

                    class.native_selection_boundary()
                        || matches!(
                            class.linkage(),
                            crate::CodegenLinkage::Weak | crate::CodegenLinkage::Fallback
                        )
                        || class.linkage() == crate::CodegenLinkage::LinkOnce
                            && instances[member].key().target().machine().object_format()
                                != bray_target::ObjectFormat::Coff
                }),
        }
    }
}

// All directly named providers must match before library publication combines
// definitions. Foreign callable keys remain present even when resolved externally.
#[derive(Default, Eq, Ord, PartialEq, PartialOrd)]
struct PublicationDependencies<'a> {
    instances: BTreeSet<&'a CodegenInstanceKey>,
    statics: BTreeSet<[u8; 32]>,
    runtime: BTreeSet<bray_ir::MirRuntimeReference>,
}

struct DisjointSet {
    parents: Vec<usize>,
    ranks: Vec<u8>,
}

impl DisjointSet {
    fn new(length: usize) -> Self {
        Self {
            parents: (0..length).collect(),
            ranks: vec![0; length],
        }
    }

    fn find(&mut self, mut member: usize) -> usize {
        let mut root = member;

        while self.parents[root] != root {
            root = self.parents[root];
        }

        while self.parents[member] != member {
            let parent = self.parents[member];

            self.parents[member] = root;
            member = parent;
        }

        root
    }

    fn union(&mut self, left: usize, right: usize) {
        let mut left = self.find(left);
        let mut right = self.find(right);

        if left == right {
            return;
        }

        if self.ranks[left] < self.ranks[right] {
            std::mem::swap(&mut left, &mut right);
        }

        self.parents[right] = left;

        if self.ranks[left] == self.ranks[right] {
            self.ranks[left] = self.ranks[left].saturating_add(1);
        }
    }
}

/// A deterministic partitioning contract violation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum CodegenPartitionError {
    /// The caller omitted package, linkage, or visibility identity for one definition.
    MissingCompatibility(CodegenInstanceKey),
    /// The partitioner produced an invalid generated unit.
    InvalidUnit(CodegenUnitBuildError),
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_ir::MirTargetContract;
    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_symbols::PackageIdentity;
    use bray_testing::{
        test_mir_content_identity, test_mir_target, test_mir_unit, test_mir_unit_for_target,
        test_mir_unit_with_declaration, test_mir_unit_with_declaration_for_target,
    };

    use super::partition_codegen_units;
    use crate::{
        CodegenDefinitionVisibility, CodegenInstance, CodegenInstanceDependency,
        CodegenInstanceKey, CodegenLinkage, CodegenOversizedUnitReason,
        CodegenPartitionCompatibility, CodegenPartitionPolicy, CodegenReachabilityBuilder,
        CodegenWork,
    };

    #[test]
    fn partition_membership_is_stable_for_reversed_root_order() {
        let first = CodegenInstance::non_generic(test_mir_unit(4));
        let second = CodegenInstance::non_generic(test_mir_unit_with_declaration(8, 1));

        let forward = partitions(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            [first.clone(), second.clone()],
            |_| compatibility(1, CodegenLinkage::Internal),
        );

        let reversed = partitions(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            [second, first],
            |_| compatibility(1, CodegenLinkage::Internal),
        );

        assert_eq!(forward, reversed);
        assert_eq!(forward.len(), 1);
    }

    #[test]
    fn optional_content_cuts_wait_for_the_lower_bound() {
        let instances = (0..16).map(partition_test_instance).collect::<Vec<_>>();

        let units = partitions(locality_policy(), instances, |_| {
            compatibility(1, CodegenLinkage::Internal)
        });

        assert!(
            units
                .iter()
                .take(units.len().saturating_sub(1))
                .all(|unit| unit.estimated_work() >= locality_policy().lower_bound()),
            "only the final remainder may be below the lower bound: {units:?}",
        );
    }

    #[test]
    fn unrelated_additions_preserve_distant_unit_membership() {
        let instances: Vec<_> = (0..1024).map(partition_test_instance).collect();
        let added = partition_test_instance(10_000);

        let baseline = partitions(locality_policy(), instances.iter().cloned(), |_| {
            compatibility(1, CodegenLinkage::Internal)
        });

        let updated = partitions(
            locality_policy(),
            instances.into_iter().chain([added.clone()]),
            |_| compatibility(1, CodegenLinkage::Internal),
        );

        let baseline_memberships: Vec<_> = baseline
            .iter()
            .map(|unit| unit.key().instances().to_vec())
            .collect();

        let updated_memberships: Vec<_> = updated
            .iter()
            .map(|unit| {
                unit.key()
                    .instances()
                    .iter()
                    .filter(|instance| *instance != added.key())
                    .cloned()
                    .collect::<Vec<_>>()
            })
            .collect();

        let unchanged = baseline_memberships
            .iter()
            .filter(|membership| updated_memberships.contains(membership))
            .count();

        assert!(
            unchanged.saturating_mul(2) >= baseline_memberships.len(),
            "{unchanged} of {} memberships were unchanged",
            baseline_memberships.len(),
        );
    }

    #[test]
    fn library_publication_packs_independent_groups_and_keeps_required_frames() {
        let first_mir = test_mir_unit(4);
        let second_mir = test_mir_unit_with_declaration(8, 1);
        let second_key = CodegenInstanceKey::non_generic(&second_mir);

        let first = CodegenInstance::try_new(
            CodegenInstanceKey::non_generic(&first_mir),
            first_mir,
            [CodegenInstanceDependency::new(
                crate::CodegenInstanceDependencyKind::DirectAwaitedFrame,
                second_key,
            )],
        )
        .unwrap_or_else(|error| panic!("first instance must validate: {error:?}"));

        let second = CodegenInstance::non_generic(second_mir);
        let independent = CodegenInstance::non_generic(test_mir_unit_with_declaration(12, 2));

        let units = partitions(
            CodegenPartitionPolicy::NATIVE_LIBRARY_PUBLICATION,
            [first, second, independent],
            |_| compatibility(1, CodegenLinkage::Internal),
        );

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].instances().len(), 3);
        assert!(units[0].estimated_work() <= units[0].key().partition_policy().upper_bound());
    }

    #[test]
    fn direct_awaited_frames_and_definition_cycles_are_co_located() {
        let first_mir = test_mir_unit(4);
        let second_mir = test_mir_unit_with_declaration(8, 1);
        let first_key = CodegenInstanceKey::non_generic(&first_mir);
        let second_key = CodegenInstanceKey::non_generic(&second_mir);

        let first = CodegenInstance::try_new(
            first_key.clone(),
            first_mir,
            [CodegenInstanceDependency::new(
                crate::CodegenInstanceDependencyKind::DirectAwaitedFrame,
                second_key.clone(),
            )],
        )
        .unwrap_or_else(|error| panic!("first instance must validate: {error:?}"));

        let second = CodegenInstance::try_new(
            second_key,
            second_mir,
            [CodegenInstanceDependency::definition(first_key)],
        )
        .unwrap_or_else(|error| panic!("second instance must validate: {error:?}"));

        let units = partitions(tiny_policy(), [first, second], |_| {
            compatibility(1, CodegenLinkage::Internal)
        });

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].instances().len(), 2);
    }

    #[test]
    fn required_groups_retain_per_definition_compatibility() {
        let first_mir = test_mir_unit(4);
        let second_mir = test_mir_unit_with_declaration(8, 1);
        let second_key = CodegenInstanceKey::non_generic(&second_mir);

        let first = CodegenInstance::try_new(
            CodegenInstanceKey::non_generic(&first_mir),
            first_mir,
            [CodegenInstanceDependency::new(
                crate::CodegenInstanceDependencyKind::DirectAwaitedFrame,
                second_key,
            )],
        )
        .unwrap_or_else(|error| panic!("first instance must validate: {error:?}"));

        let second = CodegenInstance::non_generic(second_mir);
        let graph = graph([first, second]);

        let units = partition_codegen_units(
            tiny_policy(),
            &graph,
            |instance| {
                let package = if instance.mir().unit().raw() == 4 {
                    1
                } else {
                    2
                };

                Some(compatibility(package, CodegenLinkage::Internal))
            },
            test_mir_content_identity,
        )
        .unwrap_or_else(|error| panic!("required group must partition: {error:?}"));

        assert_eq!(units.len(), 1);

        let first = &units[0].instances()[0];
        let second = &units[0].instances()[1];

        assert_ne!(
            units[0].compatibility(first.key()),
            units[0].compatibility(second.key())
        );
    }

    #[test]
    fn independent_definitions_share_units_without_losing_symbol_metadata() {
        let first = CodegenInstance::non_generic(test_mir_unit(4));
        let second = CodegenInstance::non_generic(test_mir_unit_with_declaration(8, 1));
        let first_class = compatibility(1, CodegenLinkage::Internal);

        let second_class = CodegenPartitionCompatibility::new(
            compatibility(2, CodegenLinkage::Export).package().clone(),
            [2; 32],
            CodegenLinkage::Export,
            CodegenDefinitionVisibility::Public,
        );

        let units = partitions(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            [first.clone(), second.clone()],
            |instance| {
                // Each recipe owns immutable metadata independently of the test's classes.
                if instance.key() == first.key() {
                    first_class.clone()
                } else {
                    second_class.clone()
                }
            },
        );

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].compatibility(first.key()), Some(&first_class));
        assert_eq!(units[0].compatibility(second.key()), Some(&second_class));

        let reconstructed = crate::CodegenUnit::try_from_key(
            units[0].key(),
            units[0].instances().iter().cloned(),
            test_mir_content_identity,
        )
        .unwrap_or_else(|error| panic!("mixed metadata must reconstruct: {error:?}"));

        assert_eq!(reconstructed, units[0]);
    }

    #[test]
    fn library_publication_is_bounded_without_singletons_or_one_mega_unit() {
        let units = partitions(
            CodegenPartitionPolicy::NATIVE_LIBRARY_PUBLICATION,
            (0..256).map(partition_test_instance),
            |instance| {
                compatibility(
                    u64::from(instance.mir().unit().raw() % 4),
                    CodegenLinkage::Export,
                )
            },
        );

        assert!(
            units.len() > 1 && units.len() < 64,
            "bounded publication: {units:?}"
        );

        assert_eq!(
            units
                .iter()
                .map(|unit| unit.instances().len())
                .sum::<usize>(),
            256
        );

        assert!(units.iter().all(|unit| {
            unit.estimated_work()
                <= CodegenPartitionPolicy::NATIVE_LIBRARY_PUBLICATION.upper_bound()
                && unit.oversized().is_none()
        }));
    }

    #[test]
    fn library_publication_does_not_mix_unrelated_dependency_closures() {
        let instances = (0..4).map(partition_test_instance).collect::<Vec<_>>();
        let provider = instances[0].key().clone();
        let other_provider = instances[1].key().clone();

        let callers = (4..20).map(|index| {
            let instance = partition_test_instance(index);

            CodegenInstance::try_new(
                instance.key().clone(),
                instance.mir().clone(),
                [CodegenInstanceDependency::definition(if index < 12 {
                    provider.clone()
                } else {
                    other_provider.clone()
                })],
            )
            .unwrap_or_else(|error| panic!("test caller must validate: {error:?}"))
        });

        let units = partitions(
            CodegenPartitionPolicy::NATIVE_LIBRARY_PUBLICATION,
            instances.into_iter().chain(callers),
            |_| compatibility(1, CodegenLinkage::Export),
        );

        assert!(units.iter().any(|unit| unit.instances().len() > 1));

        for unit in units.iter() {
            let dependencies = unit.instances().iter().map(|instance| {
                instance.dependencies().iter().map(|dependency| dependency.instance()).collect::<Vec<_>>()
            }).collect::<BTreeSet<_>>();

            assert_eq!(dependencies.len(), 1);
        }
    }

    #[test]
    fn runtime_provider_dependencies_bound_publication_but_not_required_groups() {
        let instances = (0..16).map(|index| {
            let bound = bray_testing::test_bound_unit_with_declaration(index, index);
            let source = bray_ir::MirSourceAnchor::from(bound.key().source());
            let mut builder = bray_ir::MirUnitBuilder::for_bound(bound.identity(), bray_ir::MirUnitKind::Synchronous, test_mir_target());
            let block = builder.push_block(source.clone(), bray_ir::MirBlockKind::Ordinary).unwrap();
            let ty = bray_testing::test_mir_type();

            let runtime = bray_ir::MirRuntimeReference::new(
                if index % 2 == 0 { bray_runtime_interface::RuntimeAbiRole::OutgoingAdmission }
                else { bray_runtime_interface::RuntimeAbiRole::OutgoingDischarge },
                RuntimeAbiVersion::new(1, 0),
            );

            let operation = if index % 2 == 0 { bray_ir::MirOperationKind::AdmitOutgoing { ty, runtime } }
                else { bray_ir::MirOperationKind::DischargeOutgoing { ty, runtime } };

            builder.push_operation(block, source.clone(), operation, None).unwrap();
            builder.set_terminator(block, source, bray_ir::MirTerminatorKind::Return(None));

            CodegenInstance::non_generic(builder.finish(block))
        }).collect::<Vec<_>>();

        let units = partitions(CodegenPartitionPolicy::NATIVE_LIBRARY_PUBLICATION, instances.iter().cloned(), |_| compatibility(1, CodegenLinkage::Export));

        assert!(units.iter().any(|unit| unit.instances().len() > 1));
        assert!(units.iter().all(|unit| crate::demanded_runtime_references(unit).len() == 1));

        let cycle = (0..2).map(|index| CodegenInstance::try_new(
            instances[index].key().clone(),
            instances[index].mir().clone(),
            [CodegenInstanceDependency::definition(instances[1-index].key().clone())],
        ).unwrap());

        let units = partitions(CodegenPartitionPolicy::NATIVE_LIBRARY_PUBLICATION, cycle, |_| compatibility(1, CodegenLinkage::Export));

        assert_eq!(units.len(), 1);
        assert_eq!(crate::demanded_runtime_references(&units[0]).len(), 2);
    }

    #[test]
    fn native_selection_boundaries_do_not_hide_unrelated_library_exports() {
        for linkage in [
            CodegenLinkage::Weak,
            CodegenLinkage::Fallback,
            CodegenLinkage::LinkOnce,
        ] {
            let instances = (0..32).map(partition_test_instance).collect::<Vec<_>>();
            let selected = instances[0].key().clone();

            let units = partitions(
                CodegenPartitionPolicy::NATIVE_LIBRARY_PUBLICATION,
                instances,
                |instance| {
                    compatibility(
                        1,
                        if instance.key() == &selected {
                            linkage
                        } else {
                            CodegenLinkage::Export
                        },
                    )
                },
            );

            let boundary = units
                .iter()
                .find(|unit| {
                    unit.instances()
                        .iter()
                        .any(|instance| instance.key() == &selected)
                })
                .unwrap_or_else(|| panic!("native selection boundary must publish"));

            assert_eq!(boundary.instances().len(), 1);
            assert!(units.iter().any(|unit| unit.instances().len() > 1));
        }

        let instances = (0..32).map(partition_test_instance).collect::<Vec<_>>();
        let selected = instances[0].key().clone();

        let units = partitions(
            CodegenPartitionPolicy::NATIVE_LIBRARY_PUBLICATION,
            instances,
            |instance| {
                let class = compatibility(1, CodegenLinkage::Export);

                if instance.key() == &selected {
                    class.with_native_selection_boundary()
                } else {
                    class
                }
            },
        );

        let boundary = units
            .iter()
            .find(|unit| {
                unit.instances()
                    .iter()
                    .any(|instance| instance.key() == &selected)
            })
            .unwrap_or_else(|| panic!("native selection boundary must publish"));

        assert_eq!(boundary.instances().len(), 1);
        assert!(units.iter().any(|unit| unit.instances().len() > 1));

        let target = MirTargetContract::new(
            bray_target::NativeTarget::X86_64WindowsMsvc.profile(),
            RuntimeAbiVersion::new(1, 0),
        );

        let units = partitions(
            CodegenPartitionPolicy::NATIVE_LIBRARY_PUBLICATION,
            [
                CodegenInstance::non_generic(test_mir_unit_for_target(4, target.clone())),
                CodegenInstance::non_generic(test_mir_unit_with_declaration_for_target(
                    8, 1, target,
                )),
            ],
            |_| compatibility(1, CodegenLinkage::LinkOnce),
        );

        assert_eq!(
            units.len(),
            1,
            "COFF link-once definitions have exact COMDAT selection"
        );

        let units = partitions(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            [
                CodegenInstance::non_generic(test_mir_unit(4)),
                CodegenInstance::non_generic(test_mir_unit_with_declaration(8, 1)),
            ],
            |_| compatibility(1, CodegenLinkage::Fallback),
        );

        assert_eq!(
            units.len(),
            1,
            "product batching does not impose library extraction boundaries"
        );
    }

    #[test]
    fn metadata_changes_preserve_unrelated_partition_membership() {
        let instances = (0..256).map(partition_test_instance).collect::<Vec<_>>();

        let baseline = partitions(locality_policy(), instances.iter().cloned(), |_| {
            compatibility(1, CodegenLinkage::Internal)
        });

        let updated = partitions(locality_policy(), instances, |instance| {
            if instance.mir().unit().raw() == 128 {
                compatibility(2, CodegenLinkage::Export)
            } else {
                compatibility(1, CodegenLinkage::Internal)
            }
        });

        assert_eq!(
            baseline
                .iter()
                .map(|unit| unit.key().instances())
                .collect::<Vec<_>>(),
            updated
                .iter()
                .map(|unit| unit.key().instances())
                .collect::<Vec<_>>(),
        );

        assert_eq!(
            baseline
                .iter()
                .filter(|unit| updated.contains(unit))
                .count(),
            baseline.len() - 1
        );
    }

    #[test]
    fn independent_started_tasks_can_use_external_frame_helpers() {
        let first_mir = test_mir_unit(4);
        let second_mir = test_mir_unit_with_declaration(8, 1);
        let second_key = CodegenInstanceKey::non_generic(&second_mir);

        let first = CodegenInstance::try_new(
            CodegenInstanceKey::non_generic(&first_mir),
            first_mir,
            [CodegenInstanceDependency::new(
                crate::CodegenInstanceDependencyKind::StartedTask,
                second_key.clone(),
            )],
        )
        .unwrap_or_else(|error| panic!("started task must validate: {error:?}"));

        let units = partitions(
            tiny_policy(),
            [first, CodegenInstance::non_generic(second_mir)],
            |_| compatibility(1, CodegenLinkage::Internal),
        );

        assert_eq!(units.len(), 2);

        assert!(
            units
                .iter()
                .any(|unit| unit.external_instances().contains(&second_key))
        );
    }

    #[test]
    fn started_task_dependency_cycles_remain_indivisible() {
        let first_mir = test_mir_unit(4);
        let second_mir = test_mir_unit_with_declaration(8, 1);
        let first_key = CodegenInstanceKey::non_generic(&first_mir);
        let second_key = CodegenInstanceKey::non_generic(&second_mir);

        let first = CodegenInstance::try_new(
            first_key.clone(),
            first_mir,
            [CodegenInstanceDependency::new(
                crate::CodegenInstanceDependencyKind::StartedTask,
                second_key.clone(),
            )],
        )
        .unwrap_or_else(|error| panic!("started task must validate: {error:?}"));

        let second = CodegenInstance::try_new(
            second_key,
            second_mir,
            [CodegenInstanceDependency::definition(first_key)],
        )
        .unwrap_or_else(|error| panic!("cycle must validate: {error:?}"));

        let units = partitions(tiny_policy(), [first, second], |_| {
            compatibility(1, CodegenLinkage::Internal)
        });

        assert_eq!(units.len(), 1);

        assert_eq!(
            units[0].oversized().map(|oversized| oversized.reason()),
            Some(CodegenOversizedUnitReason::IndivisibleDependencyGroup)
        );
    }

    #[test]
    fn unit_visible_definitions_are_co_located_with_every_caller() {
        let private_mir = test_mir_unit_with_declaration(12, 2);
        let private_key = CodegenInstanceKey::non_generic(&private_mir);

        let callers = [test_mir_unit(4), test_mir_unit_with_declaration(8, 1)].map(|mir| {
            CodegenInstance::try_new(
                CodegenInstanceKey::non_generic(&mir),
                mir,
                [CodegenInstanceDependency::definition(private_key.clone())],
            )
            .unwrap_or_else(|error| panic!("private call must validate: {error:?}"))
        });

        let units = partitions(
            tiny_policy(),
            callers
                .into_iter()
                .chain([CodegenInstance::non_generic(private_mir)]),
            |instance| {
                if instance.key() == &private_key {
                    CodegenPartitionCompatibility::new(
                        compatibility(2, CodegenLinkage::Private).package().clone(),
                        [2; 32],
                        CodegenLinkage::Private,
                        CodegenDefinitionVisibility::Unit,
                    )
                } else {
                    compatibility(1, CodegenLinkage::Export)
                }
            },
        );

        assert_eq!(units.len(), 1);
        assert_eq!(units[0].instances().len(), 3);
        assert!(units[0].external_instances().is_empty());

        assert_eq!(
            units[0].oversized().map(|oversized| oversized.reason()),
            Some(CodegenOversizedUnitReason::IndivisibleDependencyGroup)
        );
    }

    #[test]
    fn resolved_external_dependencies_do_not_enter_required_groups() {
        let mir = test_mir_unit(4);
        let key = CodegenInstanceKey::non_generic(&mir);
        let external = CodegenInstanceKey::non_generic(&test_mir_unit_with_declaration(8, 1));

        let instance = CodegenInstance::try_new(
            key.clone(),
            mir,
            [CodegenInstanceDependency::definition(external.clone())],
        )
        .unwrap_or_else(|error| panic!("test instance must validate: {error:?}"));

        let mut builder = CodegenReachabilityBuilder::try_new([key])
            .unwrap_or_else(|error| panic!("test roots must validate: {error:?}"));

        let _ = builder.take_frontier();

        builder
            .push_instance(instance)
            .unwrap_or_else(|error| panic!("test instance must publish: {error:?}"));

        assert_eq!(
            builder.take_frontier().as_ref(),
            std::slice::from_ref(&external)
        );

        builder
            .push_external(external.clone())
            .unwrap_or_else(|error| panic!("external dependency must resolve: {error:?}"));

        let graph = builder
            .finish()
            .unwrap_or_else(|error| panic!("test graph must close: {error:?}"));

        let units = partition_codegen_units(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            &graph,
            |_| Some(compatibility(1, CodegenLinkage::Internal)),
            test_mir_content_identity,
        )
        .unwrap_or_else(|error| panic!("external dependency must partition: {error:?}"));

        assert_eq!(units.len(), 1);

        assert_eq!(
            units[0].external_instances(),
            std::slice::from_ref(&external)
        );
    }

    #[test]
    fn target_identity_is_a_partition_boundary() {
        let first_target = test_mir_target();

        let second_target =
            MirTargetContract::new(first_target.profile().clone(), RuntimeAbiVersion::new(2, 0));

        let first = CodegenInstance::non_generic(test_mir_unit_for_target(4, first_target));
        let second = CodegenInstance::non_generic(test_mir_unit_for_target(8, second_target));

        let units = partitions(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            [first, second],
            |_| compatibility(1, CodegenLinkage::Internal),
        );

        assert_eq!(units.len(), 2);
        assert_ne!(units[0].target(), units[1].target());
    }

    #[test]
    fn indivisible_groups_publish_oversized_work_metadata() {
        let first_mir = test_mir_unit(4);
        let second_mir = test_mir_unit_with_declaration(8, 1);
        let first_key = CodegenInstanceKey::non_generic(&first_mir);
        let second_key = CodegenInstanceKey::non_generic(&second_mir);

        let first = CodegenInstance::try_new(
            first_key.clone(),
            first_mir,
            [CodegenInstanceDependency::definition(second_key.clone())],
        )
        .unwrap_or_else(|error| panic!("first instance must validate: {error:?}"));

        let second = CodegenInstance::try_new(
            second_key,
            second_mir,
            [CodegenInstanceDependency::definition(first_key)],
        )
        .unwrap_or_else(|error| panic!("second instance must validate: {error:?}"));

        let units = partitions(tiny_policy(), [first, second], |_| {
            compatibility(1, CodegenLinkage::Internal)
        });

        let oversized = units[0]
            .oversized()
            .unwrap_or_else(|| panic!("the dependency cycle must exceed the tiny bound"));

        assert_eq!(oversized.work(), units[0].estimated_work());
        assert_eq!(oversized.upper_bound(), tiny_policy().upper_bound());

        assert_eq!(
            oversized.reason(),
            CodegenOversizedUnitReason::IndivisibleDependencyGroup
        );
    }

    #[test]
    fn oversized_single_definitions_publish_an_exact_reason_and_reconstruct() {
        let units = partitions(singleton_policy(), [partition_test_instance(1)], |_| {
            compatibility(1, CodegenLinkage::Internal)
        });

        let oversized = units[0]
            .oversized()
            .unwrap_or_else(|| panic!("the single definition must exceed the tiny bound"));

        assert_eq!(
            oversized.reason(),
            CodegenOversizedUnitReason::IndivisibleDefinition
        );

        let reconstructed = crate::CodegenUnit::try_from_key(
            units[0].key(),
            units[0].instances().iter().cloned(),
            test_mir_content_identity,
        )
        .unwrap_or_else(|error| panic!("oversized unit must reconstruct: {error:?}"));

        assert_eq!(reconstructed, units[0]);
    }

    fn partitions(
        policy: CodegenPartitionPolicy,
        instances: impl IntoIterator<Item = CodegenInstance>,
        compatibility: impl Fn(&CodegenInstance) -> CodegenPartitionCompatibility,
    ) -> Arc<[crate::CodegenUnit]> {
        let graph = graph(instances);

        partition_codegen_units(
            policy,
            &graph,
            |instance| Some(compatibility(instance)),
            test_mir_content_identity,
        )
        .unwrap_or_else(|error| panic!("test partitions must validate: {error:?}"))
    }

    fn graph(instances: impl IntoIterator<Item = CodegenInstance>) -> crate::CodegenReachability {
        let instances: Vec<_> = instances.into_iter().collect();

        let mut builder = CodegenReachabilityBuilder::try_new(
            instances.iter().map(|instance| instance.key().clone()),
        )
        .unwrap_or_else(|error| panic!("test roots must validate: {error:?}"));

        let _ = builder.take_frontier();

        for instance in instances {
            if let Err(error) = builder.push_instance(instance) {
                panic!("test instance must publish: {error:?}");
            }
        }

        builder
            .finish()
            .unwrap_or_else(|error| panic!("test graph must close: {error:?}"))
    }

    fn compatibility(package: u64, linkage: CodegenLinkage) -> CodegenPartitionCompatibility {
        let package = PackageIdentity::try_new(format!("test.package.{package}"))
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        CodegenPartitionCompatibility::new(
            package,
            [0; 32],
            linkage,
            CodegenDefinitionVisibility::Product,
        )
    }

    fn tiny_policy() -> CodegenPartitionPolicy {
        CodegenPartitionPolicy::try_new(
            7,
            1,
            1,
            CodegenWork::new(1),
            CodegenWork::new(16),
            CodegenWork::new(32),
        )
        .unwrap_or_else(|error| panic!("test policy must validate: {error:?}"))
    }

    fn locality_policy() -> CodegenPartitionPolicy {
        CodegenPartitionPolicy::try_new(
            8,
            1,
            1,
            CodegenWork::new(128),
            CodegenWork::new(256),
            CodegenWork::new(512),
        )
        .unwrap_or_else(|error| panic!("locality policy must validate: {error:?}"))
    }

    fn singleton_policy() -> CodegenPartitionPolicy {
        CodegenPartitionPolicy::try_new(
            9,
            1,
            1,
            CodegenWork::new(1),
            CodegenWork::new(8),
            CodegenWork::new(16),
        )
        .unwrap_or_else(|error| panic!("singleton policy must validate: {error:?}"))
    }

    fn partition_test_instance(ordinal: u32) -> CodegenInstance {
        CodegenInstance::non_generic(test_mir_unit_with_declaration(
            ordinal.saturating_add(1),
            ordinal,
        ))
    }
}
