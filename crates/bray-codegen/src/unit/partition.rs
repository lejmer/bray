use std::collections::BTreeMap;
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

    let mut groups = required_groups(instances, &indices)?;
    let mut partition_groups = Vec::with_capacity(groups.len());

    for members in groups.values_mut() {
        members.sort_unstable();

        partition_groups.push(PartitionGroup::try_new(
            policy,
            instances,
            members,
            &compatibility,
        )?);
    }

    partition_groups.sort_unstable_by(|left, right| {
        left.compatibility
            .cmp(&right.compatibility)
            .then_with(|| left.target.cmp(right.target))
            .then_with(|| left.anchor.cmp(&right.anchor))
            .then_with(|| left.instances[0].key().cmp(right.instances[0].key()))
    });

    let mut units = Vec::new();
    let mut current = Vec::new();
    let mut current_work = CodegenWork::new(0);
    let mut current_compatibility = None;
    let mut current_target: Option<&bray_ir::MirTargetContract> = None;

    for group in partition_groups {
        // The pending unit owns its Arc-backed class beyond this group iteration.
        let Some(group_compatibility) = group.compatibility.clone() else {
            finish_unit(
                policy,
                &mut current,
                &mut current_work,
                &mut current_compatibility,
                &mut current_target,
                &mut units,
                &mir_content_identity,
            )?;

            units.push(group.into_indivisible_unit(policy, &mir_content_identity)?);
            continue;
        };

        if current_compatibility.as_ref() != Some(&group_compatibility)
            || current_target != Some(group.target)
            || current_work
                .saturating_add(group.work)
                .gt(&policy.upper_bound())
        {
            finish_unit(
                policy,
                &mut current,
                &mut current_work,
                &mut current_compatibility,
                &mut current_target,
                &mut units,
                &mir_content_identity,
            )?;
        }

        if group.work > policy.upper_bound() {
            let unit = CodegenUnit::try_from_indivisible_group(
                policy,
                group.instances.into_iter().cloned(),
                |_| Some(group_compatibility.clone()),
                &mir_content_identity,
            )
            .map_err(CodegenPartitionError::InvalidUnit)?;

            units.push(unit);
            continue;
        }

        current_compatibility = Some(group_compatibility);
        current_target = Some(group.target);
        current_work = current_work.saturating_add(group.work);
        current.extend(group.instances.into_iter().cloned());

        if current_work >= policy.lower_bound()
            && is_content_boundary(policy, group.anchor, group.work)
        {
            finish_unit(
                policy,
                &mut current,
                &mut current_work,
                &mut current_compatibility,
                &mut current_target,
                &mut units,
                &mir_content_identity,
            )?;
        }
    }

    finish_unit(
        policy,
        &mut current,
        &mut current_work,
        &mut current_compatibility,
        &mut current_target,
        &mut units,
        &mir_content_identity,
    )?;

    Ok(shared_slice(units))
}

fn required_groups<'a>(
    instances: &'a [CodegenInstance],
    indices: &BTreeMap<&'a CodegenInstanceKey, usize>,
) -> Result<BTreeMap<usize, Vec<usize>>, CodegenPartitionError> {
    let mut definition_edges = vec![Vec::new(); instances.len()];
    let mut co_location_edges = Vec::new();

    for (source, instance) in instances.iter().enumerate() {
        for dependency in instance.dependencies() {
            let Some(&target) = indices.get(dependency.instance()) else {
                continue;
            };

            match dependency.kind() {
                CodegenInstanceDependencyKind::Definition => {
                    definition_edges[source].push(target);
                }
                CodegenInstanceDependencyKind::DirectAwaitedFrame => {
                    co_location_edges.push((source, target));
                }
                CodegenInstanceDependencyKind::StartedTask => {}
            }
        }
    }

    let mut disjoint = DisjointSet::new(instances.len());

    for component in bray_base::strongly_connected_components(0..definition_edges.len(), |node| {
        definition_edges[node].iter().copied()
    }) {
        let Some((&first, rest)) = component.split_first() else {
            continue;
        };

        for &member in rest {
            disjoint.union(first, member);
        }
    }

    for (source, target) in co_location_edges {
        disjoint.union(source, target);
    }

    let mut groups = BTreeMap::new();

    for index in 0..instances.len() {
        groups
            .entry(disjoint.find(index))
            .or_insert_with(Vec::new)
            .push(index);
    }

    Ok(groups)
}

fn finish_unit(
    policy: CodegenPartitionPolicy,
    current: &mut Vec<CodegenInstance>,
    current_work: &mut CodegenWork,
    current_compatibility: &mut Option<CodegenPartitionCompatibility>,
    current_target: &mut Option<&bray_ir::MirTargetContract>,
    units: &mut Vec<CodegenUnit>,
    mir_content_identity: &impl Fn(&bray_ir::MirUnit) -> [u8; 32],
) -> Result<(), CodegenPartitionError> {
    if current.is_empty() {
        return Ok(());
    }

    let Some(compatibility) = current_compatibility.take() else {
        // The error owns the unit identity after the pending partition is discarded.
        return Err(CodegenPartitionError::MissingCompatibility(
            current[0].key().clone(),
        ));
    };

    let instances = std::mem::take(current);

    *current_work = CodegenWork::new(0);
    *current_target = None;

    units.push(
        CodegenUnit::try_from_instances(policy, compatibility, instances, mir_content_identity)
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
    compatibility: Option<CodegenPartitionCompatibility>,
    compatibilities: Vec<CodegenPartitionCompatibility>,
    target: &'a bray_ir::MirTargetContract,
    instances: Vec<&'a CodegenInstance>,
    work: CodegenWork,
    anchor: [u8; 32],
}

impl<'a> PartitionGroup<'a> {
    fn try_new(
        policy: CodegenPartitionPolicy,
        instances: &'a [CodegenInstance],
        members: &[usize],
        compatibility: &impl Fn(&CodegenInstance) -> Option<CodegenPartitionCompatibility>,
    ) -> Result<Self, CodegenPartitionError> {
        let Some(&first) = members.first() else {
            return Err(CodegenPartitionError::InvalidUnit(
                CodegenUnitBuildError::Empty,
            ));
        };

        let first_instance = &instances[first];

        let Some(class) = compatibility(first_instance) else {
            // The error owns the first identity after this group borrow ends.
            return Err(CodegenPartitionError::MissingCompatibility(
                first_instance.key().clone(),
            ));
        };

        let mut group_instances = Vec::with_capacity(members.len());
        let mut compatibilities = Vec::with_capacity(members.len());
        let mut work = CodegenWork::new(0);
        let mut hasher = StableDigestHasher::new();

        hasher.write(b"bray.codegen-partition-group");
        policy.hash(&mut hasher);
        class.hash(&mut hasher);

        for &member in members {
            let instance = &instances[member];

            let Some(member_class) = compatibility(instance) else {
                // The error owns the member identity after this group borrow ends.
                return Err(CodegenPartitionError::MissingCompatibility(
                    instance.key().clone(),
                ));
            };

            instance.key().hash(&mut hasher);
            member_class.hash(&mut hasher);
            work = work.saturating_add(policy.estimate(instance));
            group_instances.push(instance);
            compatibilities.push(member_class);
        }

        let homogeneous = compatibilities
            .iter()
            .all(|compatibility| compatibility == &class);

        Ok(Self {
            compatibility: homogeneous.then_some(class),
            compatibilities,
            target: first_instance.key().target(),
            instances: group_instances,
            work,
            anchor: hasher.finalize(),
        })
    }

    fn into_indivisible_unit(
        self,
        policy: CodegenPartitionPolicy,
        mir_content_identity: &impl Fn(&bray_ir::MirUnit) -> [u8; 32],
    ) -> Result<CodegenUnit, CodegenPartitionError> {
        let compatibility: BTreeMap<_, _> = self
            .instances
            .iter()
            .zip(&self.compatibilities)
            .map(|(instance, compatibility)| (instance.key(), compatibility))
            .collect();

        CodegenUnit::try_from_indivisible_group(
            policy,
            // The completed unit owns instance payloads beyond the borrowed reachability graph.
            self.instances.iter().map(|instance| (*instance).clone()),
            // Every recipe entry owns its Arc-backed compatibility identity.
            |instance| {
                compatibility
                    .get(instance.key())
                    .map(|value| (*value).clone())
            },
            mir_content_identity,
        )
        .map_err(CodegenPartitionError::InvalidUnit)
    }
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
    use std::sync::Arc;

    use bray_ir::MirTargetContract;
    use bray_runtime_interface::RuntimeAbiVersion;
    use bray_symbols::PackageIdentity;
    use bray_testing::{
        test_mir_content_identity, test_mir_target, test_mir_unit, test_mir_unit_for_target, test_mir_unit_with_declaration,
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
    fn library_publication_separates_independent_groups_and_keeps_required_frames() {
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

        assert_eq!(units.len(), 2);
        assert!(units.iter().any(|unit| unit.instances().len() == 2));
        assert!(units.iter().any(|unit| unit.instances().len() == 1));
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

        let units = partition_codegen_units(tiny_policy(), &graph, |instance| {
            let package = if instance.mir().unit().raw() == 4 {
                1
            } else {
                2
            };

            Some(compatibility(package, CodegenLinkage::Internal))
        }, test_mir_content_identity)
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
    fn independent_incompatible_definitions_stay_in_separate_units() {
        let first = CodegenInstance::non_generic(test_mir_unit(4));
        let second = CodegenInstance::non_generic(test_mir_unit_with_declaration(8, 1));

        let units = partitions(
            CodegenPartitionPolicy::NATIVE_BALANCED,
            [first, second],
            |instance| {
                if instance.mir().unit().raw() == 4 {
                    compatibility(1, CodegenLinkage::Internal)
                } else {
                    compatibility(2, CodegenLinkage::Export)
                }
            },
        );

        assert_eq!(units.len(), 2);

        assert_ne!(
            units[0].compatibility(units[0].instances()[0].key()),
            units[1].compatibility(units[1].instances()[0].key())
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

        let units =
            partition_codegen_units(CodegenPartitionPolicy::NATIVE_BALANCED, &graph, |_| {
                Some(compatibility(1, CodegenLinkage::Internal))
            }, test_mir_content_identity)
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

        let reconstructed =
            crate::CodegenUnit::try_from_key(units[0].key(), units[0].instances().iter().cloned(), test_mir_content_identity)
                .unwrap_or_else(|error| panic!("oversized unit must reconstruct: {error:?}"));

        assert_eq!(reconstructed, units[0]);
    }

    fn partitions(
        policy: CodegenPartitionPolicy,
        instances: impl IntoIterator<Item = CodegenInstance>,
        compatibility: impl Fn(&CodegenInstance) -> CodegenPartitionCompatibility,
    ) -> Arc<[crate::CodegenUnit]> {
        let graph = graph(instances);

        partition_codegen_units(policy, &graph, |instance| Some(compatibility(instance)), test_mir_content_identity)
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

        CodegenPartitionCompatibility::new(package, [0; 32], linkage, CodegenDefinitionVisibility::Product)
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
