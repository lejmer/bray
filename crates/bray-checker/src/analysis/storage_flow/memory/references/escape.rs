use std::collections::{BTreeMap, BTreeSet};

use bray_bound_tree::{BoundExecutionSite, StorageProjection};
use bray_symbols::DependencyReferenceKind;

use super::super::model::{Contents, Origin, State, Target, Transfer};
use super::storage_subtree;

impl State {
    /// Returns published raw operands and their stored references before caller-side alias resolution.
    pub(in crate::analysis::storage_flow) fn escape_roots(
        &self,
        roots: impl IntoIterator<Item = (Target, DependencyReferenceKind)>,
        occurrence: Option<BoundExecutionSite>,
    ) -> BTreeSet<Target> {
        let (reached, published, roots, _) = self.walk_escape_regions(
            roots,
            occurrence.and_then(|site| self.pending.get(&site)),
            true,
        );

        published
            .into_iter()
            .filter(|target| {
                !matches!(target.origin, Origin::Returned { staged: true, .. })
                    && (roots.contains(target)
                        || !reached
                            .iter()
                            .any(|region| self.entry_field_is_reachable(region, target)))
            })
            .collect()
    }

    fn entry_field_is_reachable(&self, region: &Target, target: &Target) -> bool {
        let Origin::Entry {
            reference: parent,
            projections,
            kind: DependencyReferenceKind::Address,
            ..
        } = &region.origin
        else {
            return false;
        };

        let Origin::Entry {
            reference: field, ..
        } = &target.origin
        else {
            return false;
        };

        let prefix = parent
            .projections
            .iter()
            .copied()
            .chain([StorageProjection::AddressTarget])
            .chain(projections.iter().copied());
        let prefix_len = parent.projections.len() + 1 + projections.len();

        if region.previous != target.previous
            || parent.root != field.root
            || field.projections.len() < prefix_len
            || !field
                .projections
                .iter()
                .copied()
                .zip(prefix)
                .all(|(field, parent)| field == parent)
        {
            return false;
        }

        // The caller expands the escaped raw address. Only a field still holding its
        // original incoming pointer is redundant. Writes and captured old copies differ.
        self.values
            .get(&region.project(&field.projections[prefix_len..]))
            .and_then(|value| value.address.as_ref())
            .is_some_and(|references| {
                references.is_exact() && references.targets.contains(target)
            })
    }

    /// Resolves call-time reachability separately from addresses published after the call.
    pub(in crate::analysis::storage_flow) fn escaped_regions(
        &self,
        roots: impl IntoIterator<Item = (Target, DependencyReferenceKind)>,
        occurrence: Option<BoundExecutionSite>,
    ) -> (BTreeSet<Target>, BTreeSet<Target>) {
        let staged = occurrence.and_then(|site| self.pending.get(&site));
        let (mut reached, mut published, _, unknown) =
            self.walk_escape_regions(roots, staged, false);
        let mut implicit = BTreeSet::new();

        self.include_unknown_targets(&mut implicit, unknown);

        for target in implicit
            .iter()
            .flat_map(|target| self.effect_origins(target).0)
            .filter(|target| self.unknown_reference_may_reach(target))
        {
            let target = target.exposure_region();

            reached.insert(target.clone());
            published.insert(target);
        }

        (reached, published)
    }

    fn walk_escape_regions(
        &self,
        roots: impl IntoIterator<Item = (Target, DependencyReferenceKind)>,
        staged: Option<&Transfer>,
        contained_only: bool,
    ) -> (BTreeSet<Target>, BTreeSet<Target>, BTreeSet<Target>, bool) {
        use DependencyReferenceKind::{Address, Borrow};

        let mut pending = roots.into_iter().collect::<Vec<_>>();
        let mut reached = BTreeSet::new();
        let mut published = BTreeSet::new();
        let mut input_addresses = BTreeSet::new();
        let mut modes = BTreeMap::new();
        let mut unknown = false;

        for (target, kind) in &pending {
            if *kind == Address {
                input_addresses.insert(target.exposure_region());
            }
        }

        while let Some((target, kind)) = pending.pop() {
            let target = if kind == Address {
                target.exposure_region()
            } else {
                target
            };

            match modes.get(&target) {
                Some(Address) => continue,
                Some(Borrow) if kind == Borrow => continue,
                _ => {}
            }

            modes.insert(target.clone(), kind);
            reached.insert(target.clone());

            if kind == Address {
                published.insert(target.clone());
            }

            let private = target.is_staged();
            let (values, contents) = if private {
                let Some(staged) = staged else { continue; };

                if let Origin::Returned { reference, .. } = &target.origin {
                    let bounds = &staged.returned_inputs[reference];
                    let (origins, unbounded) =
                        Self::effect_origins_from(&target, Some(bounds));

                    pending.extend(
                        origins
                            .filter(|origin| *origin != &target)
                            .cloned()
                            .map(|origin| (origin, kind)),
                    );
                    unknown |= unbounded || bounds.is_displaced();
                }

                (&staged.values, &staged.payloads)
            } else {
                if matches!(target.origin, Origin::Returned { .. }) {
                    let (origins, unbounded) = self.effect_origins(&target);

                    pending.extend(
                        origins
                            .filter(|origin| *origin != &target)
                            .cloned()
                            .map(|origin| (origin, kind)),
                    );
                    unknown |= unbounded;
                }

                (self.values.as_ref(), &self.contents)
            };

            let reference_free = contents.get(&target).is_some_and(|contents| {
                !contents.exposed
                    && contents
                        .storage_type
                        .is_some_and(|ty| self.complete_reference_types.get(&ty) == Some(&true))
            });
            let ranged = reference_free
                || contained_only
                    && matches!(
                        target.origin,
                        Origin::Storage(_) | Origin::Allocation { .. } | Origin::Returned { .. }
                    );
            let contained = ranged.then(|| storage_subtree(values, &target));
            let all = (!ranged).then_some(values);
            let producer = private.then(|| target.prefix(0));

            for (slot, value) in contained
                .into_iter()
                .flatten()
                .chain(all.into_iter().flatten())
            {
                if producer
                    .as_ref()
                    .is_some_and(|producer| !producer.contains(slot))
                    || contained_only && !target.contains_storage(slot)
                    || reference_free && !target.contains_storage(slot)
                    || self.target_relationship(&target, slot)
                        == bray_bound_tree::StorageRelationship::Disjoint
                {
                    continue;
                }

                unknown |= !reference_free && value.unknown_dependencies();

                let reference_kind = value.reference_kind();

                for references in value.possible_references() {
                    pending.extend(
                        references
                            .targets
                            .iter()
                            .cloned()
                            .map(|target| (target, reference_kind)),
                    );
                    unknown |= references.is_unknown() || references.is_displaced();
                }

                if let Some(backing) = value.owned_backing() {
                    pending.extend(
                        backing
                            .targets
                            .iter()
                            .cloned()
                            .map(|target| (target, Address)),
                    );
                    unknown |= backing.is_unknown() || backing.is_displaced();
                }
            }
        }

        (reached, published, input_addresses, unknown)
    }

    #[cfg(test)]
    pub(in crate::analysis::storage_flow) fn escape_targets(
        &mut self,
        roots: impl IntoIterator<Item = Target>,
    ) {
        let (_, published) = self.escaped_regions(
            roots
                .into_iter()
                .map(|target| (target, DependencyReferenceKind::Address)),
            None,
        );

        self.escape_regions(published, None);
    }

    pub(in crate::analysis::storage_flow) fn escape_regions(
        &mut self,
        regions: impl IntoIterator<Item = Target>,
        occurrence: Option<BoundExecutionSite>,
    ) {
        for target in regions {
            self.effect_payloads(&target, occurrence)
                .entry(target)
                .or_default()
                .escaped = true;
        }
    }

    pub(in crate::analysis::storage_flow::memory) fn effect_payloads(
        &mut self,
        target: &Target,
        occurrence: Option<BoundExecutionSite>,
    ) -> &mut BTreeMap<Target, Contents> {
        if target.is_staged() {
            let occurrence =
                occurrence.expect("a private effect retains its producing operation");

            &mut self
                .pending
                .get_mut(&occurrence)
                .expect("a private effect reaches the staged output")
                .payloads
        } else {
            &mut self.contents
        }
    }
}
