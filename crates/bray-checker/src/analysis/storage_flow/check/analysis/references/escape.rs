use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::{BorrowKind, DependencyReferenceKind};

use crate::analysis::storage_flow::memory::Target;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
enum ReferenceAccess {
    Borrow,
    Address,
}

/// Actual regions reachable from one evaluated value, retaining whether each
/// descriptor is a language borrow or a publishable raw address.
#[derive(Clone, Debug, Default)]
pub(in crate::analysis::storage_flow::check::analysis) struct ReachedReferenceRegions {
    regions: Vec<Target>,
    borrowed: BTreeSet<Target>,
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
        regions: impl IntoIterator<Item = (Target, DependencyReferenceKind, Option<BorrowKind>)>,
    ) -> Self {
        let mut classified = BTreeMap::new();

        for (target, kind, _) in regions {
            let access = match kind {
                DependencyReferenceKind::Address => ReferenceAccess::Address,
                DependencyReferenceKind::Borrow => ReferenceAccess::Borrow,
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

        Self {
            regions: classified.into_keys().collect(),
            borrowed,
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
    pub(in crate::analysis::storage_flow::check::analysis) borrowed: BTreeSet<Target>,
}
