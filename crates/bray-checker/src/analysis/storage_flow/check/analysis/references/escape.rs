use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::{BorrowKind, DependencyReferenceKind};

use crate::analysis::storage_flow::memory::Target;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ReferenceAccess {
    SharedBorrow,
    MutableBorrow,
    Address,
}

/// Actual regions reachable from one evaluated value, retaining whether each
/// descriptor is a language borrow or a publishable raw address.
#[derive(Clone, Debug, Default)]
pub(in crate::analysis::storage_flow::check::analysis) struct ReachedReferenceRegions {
    regions: Vec<Target>,
    borrowed: BTreeSet<Target>,
    shared: BTreeSet<Target>,
}

impl ReachedReferenceRegions {
    pub(in crate::analysis::storage_flow::check::analysis) fn new(
        regions: impl IntoIterator<Item = (Target, DependencyReferenceKind)>,
    ) -> Self {
        Self::with_borrow_kinds(
            regions
                .into_iter()
                .map(|(target, kind)| (target, kind, None)),
        )
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn with_borrow_kinds(
        regions: impl IntoIterator<
            Item = (Target, DependencyReferenceKind, Option<BorrowKind>),
        >,
    ) -> Self {
        let mut classified = BTreeMap::new();

        for (target, kind, borrow_kind) in regions {
            let access = match (kind, borrow_kind) {
                (DependencyReferenceKind::Address, _) => ReferenceAccess::Address,
                (DependencyReferenceKind::Borrow, Some(BorrowKind::Shared)) => {
                    ReferenceAccess::SharedBorrow
                }
                (DependencyReferenceKind::Borrow, _) => ReferenceAccess::MutableBorrow,
            };

            classified
                .entry(target)
                .and_modify(|current| {
                    if access > *current {
                        *current = access;
                    }
                })
                .or_insert(access);
        }

        let borrowed = classified
            .iter()
            .filter_map(|(target, access)| {
                (*access != ReferenceAccess::Address).then_some(target.clone())
            })
            .collect();

        let shared = classified
            .iter()
            .filter_map(|(target, access)| {
                (*access == ReferenceAccess::SharedBorrow).then_some(target.clone())
            })
            .collect();

        Self {
            regions: classified.into_keys().collect(),
            borrowed,
            shared,
        }
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn borrowed(
        regions: impl IntoIterator<Item = Target>,
    ) -> Self {
        Self::new(
            regions
                .into_iter()
                .map(|target| (target, DependencyReferenceKind::Borrow)),
        )
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn regions(&self) -> &[Target] {
        &self.regions
    }

    /// Regions that an invocation can mutate through its evaluated operands.
    pub(in crate::analysis::storage_flow::check::analysis) fn mutation_regions(
        &self,
        memory: &crate::analysis::storage_flow::memory::State,
    ) -> Vec<Target> {
        let mut regions = BTreeSet::new();

        for target in &self.regions {
            if !self.borrowed.contains(target) {
                regions.insert(target.exposure_region());
            } else if !self.shared.contains(target) {
                regions.insert(target.clone());
            } else {
                regions.extend(
                    memory
                        .contained_reference_regions(target)
                        .filter_map(|(target, kind)| {
                            (kind == DependencyReferenceKind::Address)
                                .then(|| target.exposure_region())
                        }),
                );
            }
        }

        regions.into_iter().collect()
    }

    pub(in crate::analysis::storage_flow::check::analysis) fn classified(
        &self,
    ) -> impl Iterator<Item = (Target, DependencyReferenceKind)> + '_ {
        self.regions.iter().cloned().map(|target| {
            let kind = if self.borrowed.contains(&target) {
                DependencyReferenceKind::Borrow
            } else {
                DependencyReferenceKind::Address
            };

            (target, kind)
        })
    }
}

impl std::ops::Deref for ReachedReferenceRegions {
    type Target = [Target];

    fn deref(&self) -> &Self::Target {
        self.regions()
    }
}

#[derive(Debug, Default)]
pub(in crate::analysis::storage_flow::check::analysis) struct ResolvedEscapes {
    pub(in crate::analysis::storage_flow::check::analysis) reached: BTreeSet<Target>,
    pub(in crate::analysis::storage_flow::check::analysis) published: BTreeSet<Target>,
}

#[cfg(test)]
mod tests {
    use super::ReachedReferenceRegions;
    use crate::analysis::storage_flow::memory::{Place, State, Target};
    use crate::test_support::{error_type, integer_expression_unit};
    use bray_bound_tree::{
        BoundUnitId, BoundUnitKind, StorageIdentity, StoragePlanBuilder, StorageProjection,
    };
    use bray_symbols::{BorrowKind, DependencyReferenceKind, SymbolOrdinal};

    #[test]
    fn mutation_roots_respect_reference_authority() {
        let unit = BoundUnitId::new(80);
        let (_, expressions) = integer_expression_unit(unit, 4);
        let mut storage = StoragePlanBuilder::new(unit, BoundUnitKind::CallableBody);
        let places = expressions
            .iter()
            .map(|expression| {
                Place::root(
                    storage
                        .push_identity(StorageIdentity::Temporary(*expression))
                        .expect("test storage identity"),
                )
            })
            .collect::<Vec<_>>();

        let shared = Target::storage(places[0].clone());
        let raw_referent = Target::storage(places[1].clone());
        let mutable = Target::storage(places[2].clone());
        let raw = Target::storage(places[3].clone()).project(&[
            StorageProjection::TupleElement(SymbolOrdinal::new(0)),
        ]);
        let pointer = Place {
            root: places[0].root,
            projections: [StorageProjection::TupleElement(SymbolOrdinal::new(1))].into(),
        };
        let mut memory = State::default();

        memory.address(
            places[1].clone(),
            pointer,
            error_type(),
            expressions[0],
        );

        let roots = ReachedReferenceRegions::with_borrow_kinds([
            (
                shared.clone(),
                DependencyReferenceKind::Borrow,
                Some(BorrowKind::Shared),
            ),
            (
                mutable.clone(),
                DependencyReferenceKind::Borrow,
                Some(BorrowKind::Mutable),
            ),
            (raw.clone(), DependencyReferenceKind::Address, None),
        ]);
        let mutations = roots.mutation_regions(&memory);

        assert!(!mutations.contains(&shared));
        assert!(mutations.contains(&raw_referent.exposure_region()));
        assert!(mutations.contains(&mutable));
        assert!(mutations.contains(&raw.exposure_region()));
    }
}
