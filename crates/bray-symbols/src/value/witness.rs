use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;

use super::{ConstantTermId, DependencyProjection, DependencySubject};

/// The complete external support of a witness on one established outcome.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyWitnessSupport<Subject = DependencySubject, Condition = ConstantTermId> {
    owners: BTreeSet<Subject>,
    addresses: BTreeMap<Condition, BTreeSet<Subject>>,
}

impl<Subject: Ord, Condition: Ord> DependencyWitnessSupport<Subject, Condition> {
    /// Retains actual owner routes and the lifetimes required by each address observation.
    pub fn new(
        owners: impl IntoIterator<Item = Subject>,
        addresses: impl IntoIterator<Item = (Condition, BTreeSet<Subject>)>,
    ) -> Self {
        let mut support = Self { owners: owners.into_iter().collect(), addresses: BTreeMap::new() };

        for (address, owners) in addresses {
            support.addresses.entry(address).or_default().extend(owners);
        }

        support
    }

    /// The storage owners required by the complete carrier.
    pub const fn owners(&self) -> &BTreeSet<Subject> { &self.owners }

    /// The retained address observations and the storage owners supporting each.
    pub const fn addresses(&self) -> &BTreeMap<Condition, BTreeSet<Subject>> { &self.addresses }

    /// Every storage owner required to instantiate this support.
    pub fn owner_subjects(&self) -> impl Iterator<Item = &Subject> {
        self.owners.iter().chain(self.addresses.values().flat_map(|owners| owners.iter()))
    }

    /// Whether this carrier has no external supports.
    pub fn is_independent(&self) -> bool { self.owners.is_empty() && self.addresses.is_empty() }

    /// Remaps both sides of each support relation together.
    pub fn try_map<C, S: Ord, T: Ord, E>(
        &self,
        context: &mut C,
        mut owner: impl FnMut(&mut C, &Subject) -> Result<S, E>,
        mut address: impl FnMut(&mut C, &Condition) -> Result<T, E>,
    ) -> Result<DependencyWitnessSupport<S, T>, E> {
        let owners = self.owners.iter().map(|source| owner(context, source)).collect::<Result<Vec<_>, _>>()?;
        let addresses = self.addresses.iter().map(|(descriptor, owners)| {
            let descriptor = address(context, descriptor)?;
            let owners = owners.iter().map(|source| owner(context, source)).collect::<Result<_, _>>()?;

            Ok((descriptor, owners))
        }).collect::<Result<Vec<_>, _>>()?;

        Ok(DependencyWitnessSupport::new(owners, addresses))
    }
}

/// A checked trusted condition carried by a complete returned owner.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyWitnessCarrier<Projection = DependencyProjection, Condition = ConstantTermId, Subject = DependencySubject> {
    guarantee: Condition,
    path: Arc<[Projection]>,
    support: Option<Arc<DependencyWitnessSupport<Subject, Condition>>>,
}

impl<Projection, Condition: Copy, Subject> DependencyWitnessCarrier<Projection, Condition, Subject> {
    /// Records a checked carrier path relative to the complete returned value.
    pub fn new(guarantee: Condition, path: impl IntoIterator<Item = Projection>) -> Self {
        Self { guarantee, path: path.into_iter().collect(), support: None }
    }

    /// Records the complete external support. Absence means support could not be exported.
    pub fn with_support(mut self, support: Option<DependencyWitnessSupport<Subject, Condition>>) -> Self {
        self.support = support.map(Arc::new);

        self
    }

    /// Changes the path or condition in the same semantic store, retaining support identities.
    pub fn remap<P>(&self, guarantee: Condition, path: impl IntoIterator<Item = P>) -> DependencyWitnessCarrier<P, Condition, Subject> {
        DependencyWitnessCarrier { guarantee, path: path.into_iter().collect(), support: self.support.as_ref().map(Arc::clone) }
    }

    /// The retained external support of the clause, when complete.
    pub fn support(&self) -> Option<&DependencyWitnessSupport<Subject, Condition>> { self.support.as_deref() }

    /// Returns the checked condition supported by this owner.
    pub const fn guarantee(&self) -> Condition { self.guarantee }

    /// Returns the complete carrier's owned path within the result.
    pub fn path(&self) -> &[Projection] { &self.path }
}

impl<Projection: Ord + Clone, Condition: Ord + Copy, Subject: Ord + Clone> DependencyWitnessCarrier<Projection, Condition, Subject> {
    /// Keeps clauses supported on every exit and retains their possible external supports.
    pub fn common<'row>(
        mut rows: impl Clone + Iterator<Item = &'row Arc<[Self]>>,
    ) -> Arc<[Self]>
    where Self: 'row {
        let Some(first) = rows.next() else { return Arc::default(); };

        if rows.clone().next().is_none() { return Arc::clone(first); }

        sorted_unique_shared_slice(first.iter().filter_map(|carrier| {
            let mut supports = Vec::new();
            let mut complete = true;

            for row in std::iter::once(first).chain(rows.clone()) {
                let matching = row.iter().filter(|candidate| {
                    candidate.guarantee == carrier.guarantee && candidate.path == carrier.path
                });
                let supported = matching.clone().filter_map(Self::support)
                    .min_by_key(|support| !support.is_independent());

                match supported {
                    Some(support) => supports.push(support),
                    None if matching.count() != 0 => complete = false,
                    None => return None,
                }
            }

            let support = complete.then(|| DependencyWitnessSupport::new(
                supports.iter().flat_map(|support| support.owners().iter().cloned()),
                supports.iter().flat_map(|support| support.addresses().iter()
                    .map(|(address, owners)| (*address, owners.clone()))),
            ));

            Some(carrier.clone().with_support(support))
        }))
    }
}
