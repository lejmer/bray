//! Dependency closure for validated native units.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Mutex};

use bray_symbols::{
    NativeSymbolBinding, NativeSymbolContract, NativeSymbolIdentity, NativeSymbolPresence,
};

use crate::{
    NativeArtifactIndex, NativeContentDigest, NativeDefinitionSelection, NativeRoot,
    NativeUnitSummary,
};

type SymbolKey = (NativeSymbolIdentity, Option<String>);

fn symbol_key(symbol: &NativeSymbolContract) -> SymbolKey {
    (
        symbol.identity().clone(),
        symbol.version().map(str::to_owned),
    )
}

/// Identifies a unit within one artifact in a resolver's ordered dependency set.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeUnitLocation {
    /// Ordinal of the artifact supplied to the resolver.
    pub artifact: usize,
    /// Authenticated content identity within that artifact.
    pub digest: NativeContentDigest,
}

/// A reason that one indivisible native unit belongs to the selected product.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum NativeUnitInclusion {
    /// The publisher identified this unit as an owned entry to close transitively.
    Seed,
    /// A concrete program demand names a definition in the unit.
    Demand(NativeSymbolContract),
    /// A selected unit refers to a definition in this unit.
    Reference {
        /// Referring unit.
        from: NativeUnitLocation,
        /// Required native symbol.
        symbol: NativeSymbolContract,
    },
    /// Native initialization or finalization requires the unit.
    Lifecycle(NativeRoot),
    /// Another selected unit requires this unit to be retained with it.
    CoRetention(NativeUnitLocation),
    /// The unit has no exact symbol summary and remains a conservative link input.
    Opaque,
}

/// Closed native units in deterministic dependency-compatible order, with inclusion reasons.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeUnitSelection {
    units: Vec<NativeUnitLocation>,
    reasons: BTreeMap<NativeUnitLocation, BTreeSet<NativeUnitInclusion>>,
    statics: Vec<crate::NativeStatic>,
    static_accesses: BTreeMap<SymbolKey, Arc<[[u8; 32]]>>,
}

impl NativeUnitSelection {
    /// Referencing units precede their providers, except within unavoidable cycles.
    pub fn units(&self) -> &[NativeUnitLocation] {
        &self.units
    }

    /// Returns the complete lifecycle metadata, including statics whose storage the product supplies.
    pub fn statics(&self) -> &[crate::NativeStatic] {
        &self.statics
    }

    /// Returns the resolved static dependencies of one requested native symbol.
    pub fn static_accesses(&self, symbol: &NativeSymbolContract) -> Option<&[[u8; 32]]> {
        self.static_accesses
            .get(&symbol_key(symbol))
            .map(AsRef::as_ref)
    }

    /// Returns all distinct reasons for retaining one unit.
    pub fn reasons(&self, unit: NativeUnitLocation) -> Option<&BTreeSet<NativeUnitInclusion>> {
        self.reasons.get(&unit)
    }
}

/// A required symbol cannot be resolved by the validated candidate set.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum NativeResolutionError {
    /// No exact provider exists for a required symbol.
    Unresolved(NativeSymbolContract),
    /// Distinct ordinary strong providers would make the native link invalid.
    DuplicateStrong(NativeSymbolContract),
    /// Two contributions disagree about the same closed static instance.
    ConflictingStatic([u8; 32]),
    /// Distinct static identities claim the same structural cleanup key.
    AmbiguousStaticOrder { first: [u8; 32], second: [u8; 32] },
    /// A required static has no lifecycle contribution in the dependency set.
    MissingStatic([u8; 32]),
    /// Retained lifecycle requirements contain a cycle.
    StaticLifecycleCycle(Vec<[u8; 32]>),
}

/// Closes native demand across validated artifacts without inspecting their payloads.
#[derive(Debug)]
pub struct NativeUnitResolver {
    artifacts: Vec<NativeArtifactIndex>,
    locations: Vec<(usize, usize)>,
    providers: BTreeMap<SymbolKey, Vec<(usize, usize)>>,
    statics: BTreeMap<[u8; 32], (usize, usize)>,
    static_symbols: BTreeMap<NativeSymbolIdentity, BTreeSet<[u8; 32]>>,
    groups: BTreeMap<usize, BTreeSet<usize>>,
    selections: Mutex<
        BTreeMap<
            Vec<NativeSymbolContract>,
            Result<Arc<NativeUnitSelection>, NativeResolutionError>,
        >,
    >,
}

impl NativeUnitResolver {
    /// Indexes an ordered dependency set while retaining each artifact's identity.
    pub fn new(artifacts: impl IntoIterator<Item = NativeArtifactIndex>) -> Self {
        let artifacts = artifacts.into_iter().collect::<Vec<_>>();
        let mut locations = Vec::new();
        let mut statics = BTreeMap::new();
        let mut static_symbols = BTreeMap::<_, BTreeSet<_>>::new();
        let mut providers: BTreeMap<_, Vec<_>> = BTreeMap::new();
        let mut groups: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();

        for (artifact, index) in artifacts.iter().enumerate() {
            let offset = locations.len();

            for (local, unit) in index.units().iter().enumerate() {
                let unit_index = locations.len();

                locations.push((artifact, local));

                for (entry_index, entry) in unit.statics().iter().enumerate() {
                    statics
                        .entry(entry.identity())
                        .or_insert((unit_index, entry_index));

                    let symbol = NativeSymbolIdentity::Name(
                        bray_base::NonEmptySharedStr::try_new(
                            index.target().object_symbol_name(entry.symbol()).as_ref(),
                        )
                        .expect("validated native static must have a nonempty host symbol"),
                    );

                    static_symbols
                        .entry(symbol)
                        .or_default()
                        .insert(entry.identity());
                }

                if let NativeUnitSummary::Exact { definitions, .. } = unit.summary() {
                    for (definition_index, definition) in definitions.iter().enumerate() {
                        providers
                            .entry(symbol_key(definition.symbol()))
                            .or_default()
                            .push((unit_index, definition_index));
                    }
                }
            }

            for group in index.co_retention_groups() {
                let members = group
                    .members()
                    .iter()
                    .map(|digest| {
                        offset
                            + index
                                .units()
                                .binary_search_by_key(digest, |unit| unit.digest())
                                .expect("validated co-retention member must be indexed")
                    })
                    .collect::<BTreeSet<_>>();

                for &member in &members {
                    groups
                        .entry(member)
                        .or_default()
                        .extend(members.iter().copied());
                }
            }
        }

        Self {
            artifacts,
            locations,
            providers,
            statics,
            static_symbols,
            groups,
            selections: Mutex::new(BTreeMap::new()),
        }
    }

    /// Returns the separate validated artifacts in dependency order.
    pub fn artifacts(&self) -> &[NativeArtifactIndex] {
        &self.artifacts
    }

    /// Returns the indexed lifecycle contribution for one closed static instance.
    pub fn static_entry(&self, identity: &[u8; 32]) -> Option<&crate::NativeStatic> {
        let &(unit, entry) = self.statics.get(identity)?;

        Some(&self.unit(unit).statics()[entry])
    }

    fn unit(&self, position: usize) -> &crate::NativeUnit {
        let (artifact, unit) = self.locations[position];

        &self.artifacts[artifact].units()[unit]
    }

    fn location(&self, position: usize) -> NativeUnitLocation {
        NativeUnitLocation {
            artifact: self.locations[position].0,
            digest: self.unit(position).digest(),
        }
    }

    /// Closes required symbols, lifecycle roots, opaque inputs and co-retention groups.
    pub fn select(
        &self,
        demands: impl IntoIterator<Item = NativeSymbolContract>,
    ) -> Result<Arc<NativeUnitSelection>, NativeResolutionError> {
        let demands = demands
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        if let Some(selection) = self
            .selections
            .lock()
            .expect("native selection cache mutex poisoned")
            .get(&demands)
        {
            return selection.clone();
        }

        let selection = self
            .select_uncached(&demands, &BTreeMap::new(), &[], [], false)
            .map(Arc::new);

        self.selections
            .lock()
            .expect("native selection cache mutex poisoned")
            .insert(demands, selection.clone());

        selection
    }

    /// Closes demand with product definitions, allowing ordinary strong units to displace weak ones.
    /// Declared native libraries resolve unknown definitions at the final linker.
    pub fn select_with_provided(
        &self,
        demands: impl IntoIterator<Item = NativeSymbolContract>,
        provided: impl IntoIterator<Item = (NativeSymbolIdentity, NativeSymbolBinding)>,
        static_demands: impl IntoIterator<Item = [u8; 32]>,
        native_links: &[bray_symbols::NativeLinkRequirement],
    ) -> Result<NativeUnitSelection, NativeResolutionError> {
        let demands = demands
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        let mut definitions = BTreeMap::new();

        for (identity, binding) in provided {
            let existing = definitions.entry(identity).or_insert(binding);

            *existing = existing.strongest(binding);
        }

        self.select_uncached(
            &demands,
            &definitions,
            &[],
            static_demands,
            !native_links.is_empty(),
        )
    }

    /// Closes from known owned units when native publication starts from physical entries.
    pub fn select_seeded(
        &self,
        seeds: impl IntoIterator<Item = NativeUnitLocation>,
    ) -> Result<NativeUnitSelection, NativeResolutionError> {
        let seeds = seeds
            .into_iter()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();

        self.select_uncached(&[], &BTreeMap::new(), &seeds, [], false)
    }

    fn select_uncached(
        &self,
        demands: &[NativeSymbolContract],
        provided: &BTreeMap<NativeSymbolIdentity, NativeSymbolBinding>,
        seeds: &[NativeUnitLocation],
        static_demands: impl IntoIterator<Item = [u8; 32]>,
        external_links: bool,
    ) -> Result<NativeUnitSelection, NativeResolutionError> {
        let mut state = SelectionState {
            external_links,
            ..SelectionState::default()
        };

        for seed in seeds {
            let index = self
                .locations
                .binary_search_by_key(seed, |&(artifact, unit)| NativeUnitLocation {
                    artifact,
                    digest: self.artifacts[artifact].units()[unit].digest(),
                })
                .expect("seeded native unit must belong to the resolver index");

            state.include(index, NativeUnitInclusion::Seed, None);
        }

        for identity in static_demands {
            let entry = self
                .static_entry(&identity)
                .ok_or(NativeResolutionError::MissingStatic(identity))?;

            self.retain_static(entry, None, &mut state)?;
        }

        for demand in demands {
            self.require(
                &demand,
                NativeUnitInclusion::Demand(demand.clone()),
                None,
                provided,
                &mut state,
            )?;
        }

        for unit_index in 0..self.locations.len() {
            let unit = self.unit(unit_index);

            match unit.summary() {
                NativeUnitSummary::Exact { roots, .. } => {
                    for &root in roots.iter() {
                        state.include(unit_index, NativeUnitInclusion::Lifecycle(root), None);
                    }
                }
                NativeUnitSummary::Opaque { .. } => {
                    state.include(unit_index, NativeUnitInclusion::Opaque, None);
                }
            }
        }

        while !state.pending.is_empty() || !state.pending_statics.is_empty() {
            if let Some(unit_index) = state.pending.pop_first() {
                let unit = self.unit(unit_index);

                if let Some(group) = self.groups.get(&unit_index) {
                    for &member in group {
                        if member != unit_index {
                            state.include(
                                member,
                                NativeUnitInclusion::CoRetention(self.location(unit_index)),
                                Some(SelectionDependency::Unit(unit_index)),
                            );
                        }
                    }
                }

                for entry in unit.statics() {
                    self.retain_static(
                        entry,
                        Some(SelectionDependency::Unit(unit_index)),
                        &mut state,
                    )?;
                }

                for reference in unit.summary().references() {
                    self.require(
                        reference,
                        NativeUnitInclusion::Reference {
                            from: self.location(unit_index),
                            symbol: reference.clone(),
                        },
                        Some(SelectionDependency::Unit(unit_index)),
                        provided,
                        &mut state,
                    )?;
                }

                continue;
            }

            let identity = state
                .pending_statics
                .pop_first()
                .expect("pending native work must have a unit or static");

            let from = Some(SelectionDependency::Static(identity));

            let entry = self
                .static_entry(&identity)
                .expect("scheduled native static must have indexed metadata");

            if state.statics.insert(identity, entry.clone()).is_some() {
                continue;
            }

            for provider in std::iter::once(entry).chain(
                entry
                    .dependencies()
                    .iter()
                    .map(|identity| {
                        self.static_entry(identity)
                            .ok_or(NativeResolutionError::MissingStatic(*identity))
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            ) {
                self.retain_static(provider, from, &mut state)?;

                let (provider_unit, _) = self.statics[&provider.identity()];

                let symbol = NativeSymbolContract::required_name(
                    bray_base::NonEmptySharedStr::try_new(
                        self.artifacts[self.locations[provider_unit].0]
                            .target()
                            .object_symbol_name(provider.symbol())
                            .as_ref(),
                    )
                    .expect("native static symbol must be nonempty"),
                );

                let reason = NativeUnitInclusion::Demand(symbol.clone());

                self.require(&symbol, reason, from, provided, &mut state)?;
            }
        }

        for providers in self.providers.values() {
            let selected = providers
                .iter()
                .filter(|(unit_index, _)| state.reasons.contains_key(unit_index))
                .map(|&(unit_index, definition_index)| {
                    let NativeUnitSummary::Exact { definitions, .. } =
                        self.unit(unit_index).summary()
                    else {
                        unreachable!("only exact definitions enter the provider index");
                    };

                    &definitions[definition_index]
                })
                .collect::<Vec<_>>();

            if duplicate_strong(selected.iter().copied()) {
                return Err(NativeResolutionError::DuplicateStrong(
                    selected[0].symbol().clone(),
                ));
            }
        }

        let (order, static_accesses) = state.close_dependencies();

        let units = order
            .into_iter()
            .map(|index| self.location(index))
            .collect();

        let reasons = state
            .reasons
            .into_iter()
            .map(|(index, reasons)| (self.location(index), reasons))
            .collect();

        Ok(NativeUnitSelection {
            units,
            reasons,
            statics: state.statics.into_values().collect(),
            static_accesses,
        })
    }

    fn retain_static(
        &self,
        entry: &crate::NativeStatic,
        from: Option<SelectionDependency>,
        state: &mut SelectionState,
    ) -> Result<(), NativeResolutionError> {
        if self.static_entry(&entry.identity()) != Some(entry) {
            return Err(NativeResolutionError::ConflictingStatic(entry.identity()));
        }

        if let Some(from) = from {
            state
                .edges
                .entry(from)
                .or_default()
                .insert(SelectionDependency::Static(entry.identity()));
        }

        if !state.statics.contains_key(&entry.identity()) {
            state.pending_statics.insert(entry.identity());
        }

        Ok(())
    }

    fn require(
        &self,
        symbol: &NativeSymbolContract,
        reason: NativeUnitInclusion,
        from: Option<SelectionDependency>,
        provided: &BTreeMap<NativeSymbolIdentity, NativeSymbolBinding>,
        state: &mut SelectionState,
    ) -> Result<(), NativeResolutionError> {
        if from.is_none() {
            state.roots.entry(symbol_key(symbol)).or_default();
        }

        if symbol.presence() == NativeSymbolPresence::Optional {
            return Ok(());
        }

        if symbol.version().is_none()
            && let Some(identities) = self.static_symbols.get(symbol.identity())
        {
            for identity in identities {
                let entry = self
                    .static_entry(identity)
                    .expect("indexed static host must have lifecycle metadata");

                self.retain_static(entry, from, state)?;
                state.connect(symbol, from, SelectionDependency::Static(*identity));
            }
        }

        let provided_binding = symbol
            .version()
            .is_none()
            .then(|| provided.get(symbol.identity()).copied())
            .flatten();

        if provided_binding == Some(NativeSymbolBinding::Strong) {
            return Ok(());
        }

        let Some(candidates) = self.providers.get(&symbol_key(symbol)) else {
            if provided_binding.is_some() {
                return Ok(());
            }

            let mut opaque_terminal = false;

            for unit_index in 0..self.locations.len() {
                let unit = self.unit(unit_index);

                if matches!(unit.summary(), NativeUnitSummary::Opaque { .. }) {
                    opaque_terminal = true;

                    state.connect(symbol, from, SelectionDependency::Unit(unit_index));
                }
            }

            if opaque_terminal
                || state.external_links
                || from.is_some_and(|node| {
                    matches!(node,
                    SelectionDependency::Unit(unit) if !self.unit(unit).native_links().is_empty())
                })
            {
                // Unknown definitions in opaque code or declared native libraries resolve at the linker.
                return Ok(());
            }

            return Err(NativeResolutionError::Unresolved(symbol.clone()));
        };

        let mut ordinary = Vec::new();
        let mut fallback = Vec::new();

        for &(unit_index, definition_index) in candidates {
            let NativeUnitSummary::Exact { definitions, .. } = self.unit(unit_index).summary()
            else {
                unreachable!("only exact definitions enter the provider index");
            };

            let definition = &definitions[definition_index];
            let provider = (unit_index, definition);

            if matches!(definition.selection(), NativeDefinitionSelection::Fallback) {
                fallback.push(provider);
            } else {
                ordinary.push(provider);
            }
        }

        if provided_binding == Some(NativeSymbolBinding::Weak) {
            ordinary.retain(|(_, definition)| {
                definition.symbol().binding() == NativeSymbolBinding::Strong
            });

            if ordinary.is_empty() {
                return Ok(());
            }
        }

        let candidates = if ordinary.is_empty() {
            &fallback
        } else {
            &ordinary
        };

        let strong = candidates
            .iter()
            .filter(|(_, definition)| definition.symbol().binding() == NativeSymbolBinding::Strong)
            .collect::<Vec<_>>();

        let chosen = if strong.is_empty() {
            candidates.iter().collect::<Vec<_>>()
        } else {
            strong
        };

        if duplicate_strong(chosen.iter().map(|(_, definition)| *definition)) {
            return Err(NativeResolutionError::DuplicateStrong(symbol.clone()));
        }

        for &&(unit_index, _) in &chosen {
            state.include(unit_index, reason.clone(), from);

            if from.is_none() {
                state.connect(symbol, None, SelectionDependency::Unit(unit_index));
            }
        }

        Ok(())
    }
}

fn duplicate_strong<'a>(
    definitions: impl IntoIterator<Item = &'a crate::NativeDefinition>,
) -> bool {
    let definitions = definitions.into_iter().collect::<Vec<_>>();

    let ordinary = definitions
        .iter()
        .copied()
        .filter(|definition| !matches!(definition.selection(), NativeDefinitionSelection::Fallback))
        .collect::<Vec<_>>();

    let relevant = if ordinary.is_empty() {
        &definitions
    } else {
        &ordinary
    };

    let strong = relevant
        .iter()
        .copied()
        .filter(|definition| definition.symbol().binding() == NativeSymbolBinding::Strong)
        .collect::<Vec<_>>();

    strong.len() > 1
        && strong.iter().any(|definition| {
            matches!(definition.selection(), NativeDefinitionSelection::Ordinary)
                || matches!(
                    definition.selection(),
                    NativeDefinitionSelection::Comdat {
                        rule: crate::NativeComdatSelection::NoDuplicates,
                        ..
                    }
                )
        })
}

#[derive(Clone, Copy, Eq, Ord, PartialEq, PartialOrd)]
enum SelectionDependency {
    Unit(usize),
    Static([u8; 32]),
}

#[derive(Default)]
struct SelectionState {
    pending_statics: BTreeSet<[u8; 32]>,
    statics: BTreeMap<[u8; 32], crate::NativeStatic>,
    external_links: bool,
    reasons: BTreeMap<usize, BTreeSet<NativeUnitInclusion>>,
    pending: BTreeSet<usize>,
    edges: BTreeMap<SelectionDependency, BTreeSet<SelectionDependency>>,
    roots: BTreeMap<SymbolKey, BTreeSet<SelectionDependency>>,
}

impl SelectionState {
    fn include(
        &mut self,
        unit: usize,
        reason: NativeUnitInclusion,
        from: Option<SelectionDependency>,
    ) {
        match self.reasons.entry(unit) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert([reason].into());
                self.pending.insert(unit);
            }
            std::collections::btree_map::Entry::Occupied(mut entry) => {
                entry.get_mut().insert(reason);
            }
        }

        if let Some(from) = from {
            self.edges
                .entry(from)
                .or_default()
                .insert(SelectionDependency::Unit(unit));
        }
    }

    fn connect(
        &mut self,
        symbol: &NativeSymbolContract,
        from: Option<SelectionDependency>,
        to: SelectionDependency,
    ) {
        match from {
            Some(from) => self.edges.entry(from).or_default().insert(to),
            None => self.roots.entry(symbol_key(symbol)).or_default().insert(to),
        };
    }

    fn close_dependencies(&self) -> (Vec<usize>, BTreeMap<SymbolKey, Arc<[[u8; 32]]>>) {
        let components = bray_base::strongly_connected_components(
            self.reasons
                .keys()
                .copied()
                .map(SelectionDependency::Unit)
                .chain(
                    self.statics
                        .keys()
                        .copied()
                        .map(SelectionDependency::Static),
                ),
            |node| self.edges.get(&node).into_iter().flatten().copied(),
        );

        let component_of = components
            .iter()
            .enumerate()
            .flat_map(|(component, members)| members.iter().map(move |&node| (node, component)))
            .collect::<BTreeMap<_, _>>();

        let mut summaries = vec![Arc::<[[u8; 32]]>::from([]); components.len()];

        for (component, members) in components.iter().enumerate().rev() {
            let mut accesses = BTreeSet::new();
            let mut dependencies = BTreeSet::new();

            for node in members {
                if let SelectionDependency::Static(identity) = node {
                    accesses.insert(*identity);
                }

                dependencies.extend(
                    self.edges
                        .get(node)
                        .into_iter()
                        .flatten()
                        .map(|successor| component_of[successor])
                        .filter(|&next| next != component),
                );
            }

            for dependency in dependencies {
                accesses.extend(summaries[dependency].iter().copied());
            }

            summaries[component] = accesses.into_iter().collect::<Vec<_>>().into();
        }

        let accesses = self
            .roots
            .iter()
            .map(|(symbol, nodes)| {
                let identities = nodes
                    .iter()
                    .flat_map(|node| summaries[component_of[node]].iter().copied())
                    .collect::<BTreeSet<_>>();

                (
                    symbol.clone(),
                    identities.into_iter().collect::<Vec<_>>().into(),
                )
            })
            .collect();

        let order = components
            .into_iter()
            .flatten()
            .filter_map(|node| match node {
                SelectionDependency::Unit(unit) => Some(unit),
                SelectionDependency::Static(_) => None,
            })
            .collect();

        (order, accesses)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_base::NonEmptySharedStr;
    use bray_symbols::{
        NativeLinkKind, NativeLinkRequirement, NativeSymbolBinding, NativeSymbolContract,
        NativeSymbolIdentity, NativeSymbolPresence,
    };
    use bray_target::NativeTarget;

    use super::{
        NativeResolutionError, NativeUnitInclusion, NativeUnitLocation, NativeUnitResolver,
    };
    use crate::{
        NativeArtifactIndex, NativeCoRetentionGroup, NativeComdatSelection, NativeContentDigest,
        NativeDefinition, NativeDefinitionSelection, NativeRoot, NativeUnit, NativeUnitKind,
        NativeUnitSummary,
    };

    fn digest(value: u8) -> NativeContentDigest {
        NativeContentDigest::new([value; 32])
    }

    fn location(value: u8) -> NativeUnitLocation {
        NativeUnitLocation {
            artifact: 0,
            digest: digest(value),
        }
    }

    fn symbol(
        name: &str,
        binding: NativeSymbolBinding,
        presence: NativeSymbolPresence,
    ) -> NativeSymbolContract {
        NativeSymbolContract::new(
            NativeSymbolIdentity::Name(NonEmptySharedStr::try_new(name).expect("test name")),
            None,
            binding,
            presence,
        )
    }

    fn required(name: &str) -> NativeSymbolContract {
        symbol(
            name,
            NativeSymbolBinding::Strong,
            NativeSymbolPresence::Required,
        )
    }

    fn unit(
        id: u8,
        definitions: &[(&str, NativeSymbolBinding, NativeDefinitionSelection)],
        references: &[NativeSymbolContract],
        roots: &[NativeRoot],
    ) -> NativeUnit {
        NativeUnit::new(
            digest(id),
            NativeUnitKind::Bitcode,
            NativeUnitSummary::Exact {
                definitions: definitions
                    .iter()
                    .map(|(name, binding, selection)| {
                        NativeDefinition::new(
                            symbol(name, *binding, NativeSymbolPresence::Required),
                            selection.clone(),
                        )
                    })
                    .collect::<Vec<_>>()
                    .into(),
                references: Arc::from(references),
                roots: Arc::from(roots),
            },
            [],
        )
    }

    fn index(
        units: impl IntoIterator<Item = NativeUnit>,
        groups: impl IntoIterator<Item = NativeCoRetentionGroup>,
    ) -> NativeArtifactIndex {
        NativeArtifactIndex::try_new(NativeTarget::X86_64WindowsMsvc, digest(99), units, groups)
            .expect("test native index")
    }

    #[test]
    fn provided_storage_keeps_transitive_static_metadata_without_its_native_payload() {
        let contribution = |id, name, dependencies: Vec<[u8; 32]>| {
            crate::NativeStatic::new(
                NonEmptySharedStr::try_new(name).unwrap(),
                [id; 32],
                vec![id].into(),
                bray_symbols::StaticStorageDuration::Product,
                dependencies,
                true,
                id == 3,
            )
        };

        let first = unit(
            1,
            &[
                (
                    "caller",
                    NativeSymbolBinding::Strong,
                    NativeDefinitionSelection::Ordinary,
                ),
                (
                    "first.host",
                    NativeSymbolBinding::Strong,
                    NativeDefinitionSelection::Ordinary,
                ),
            ],
            &[],
            &[],
        )
        .with_statics([contribution(1, "first.host", vec![[2; 32]])]);

        let second = unit(
            2,
            &[(
                "second.host",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        )
        .with_statics([contribution(2, "second.host", vec![[3; 32]])]);

        let third = unit(
            3,
            &[(
                "third.host",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        )
        .with_statics([contribution(3, "third.host", vec![])]);

        let resolver = NativeUnitResolver::new([index([first, second, third], [])]);

        let provided = |name| {
            (
                NativeSymbolIdentity::Name(NonEmptySharedStr::try_new(name).unwrap()),
                NativeSymbolBinding::Strong,
            )
        };

        let selected = resolver
            .select_with_provided([required("caller")], [provided("second.host")], [], &[])
            .unwrap();

        assert_eq!(selected.units(), [location(1), location(3)]);

        assert_eq!(
            selected
                .statics()
                .iter()
                .map(crate::NativeStatic::identity)
                .collect::<Vec<_>>(),
            [[1; 32], [2; 32], [3; 32]]
        );

        assert!(selected.statics()[2].requires_main_thread());

        assert_eq!(
            selected.static_accesses(&required("caller")),
            Some([[1; 32], [2; 32], [3; 32]].as_slice())
        );

        let selected = resolver
            .select_with_provided([], [provided("second.host")], [[2; 32]], &[])
            .unwrap();

        assert_eq!(selected.units(), [location(3)]);

        assert_eq!(
            selected
                .statics()
                .iter()
                .map(crate::NativeStatic::identity)
                .collect::<Vec<_>>(),
            [[2; 32], [3; 32]]
        );

        let selected = resolver
            .select_with_provided(
                [],
                [provided("second.host"), provided("third.host")],
                [[2; 32]],
                &[],
            )
            .unwrap();

        assert!(selected.units().is_empty());
        assert_eq!(selected.statics().len(), 2);
    }

    #[test]
    fn callable_static_dependencies_follow_resolved_providers_and_provided_host_entries() {
        let entry = crate::NativeStatic::new(
            NonEmptySharedStr::try_new("state.host").unwrap(),
            [3; 32],
            vec![3].into(),
            bray_symbols::StaticStorageDuration::Product,
            [],
            true,
            false,
        );

        let caller = unit(
            1,
            &[(
                "caller",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[required("fallback")],
            &[],
        );

        let fallback = unit(
            2,
            &[(
                "fallback",
                NativeSymbolBinding::Weak,
                NativeDefinitionSelection::Ordinary,
            )],
            &[required("state.host")],
            &[],
        );

        let state = unit(
            3,
            &[(
                "state.host",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        )
        .with_statics([entry]);

        let observer = unit(
            4,
            &[(
                "observer",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[required("state.host")],
            &[],
        );

        let resolver = NativeUnitResolver::new([index([caller, fallback, state, observer], [])]);

        let provided = |name| {
            (
                NativeSymbolIdentity::Name(NonEmptySharedStr::try_new(name).unwrap()),
                NativeSymbolBinding::Strong,
            )
        };

        let selected = resolver
            .select([required("caller"), required("observer")])
            .unwrap();

        assert_eq!(
            selected.static_accesses(&required("caller")),
            Some([[3; 32]].as_slice())
        );

        assert_eq!(
            selected.static_accesses(&required("observer")),
            Some([[3; 32]].as_slice())
        );

        let selected = resolver
            .select_with_provided(
                [required("caller"), required("observer")],
                [provided("fallback"), provided("state.host")],
                [],
                &[],
            )
            .unwrap();

        assert_eq!(
            selected.static_accesses(&required("caller")),
            Some([].as_slice())
        );

        assert_eq!(
            selected.static_accesses(&required("observer")),
            Some([[3; 32]].as_slice())
        );

        assert_eq!(selected.statics().len(), 1);

        assert_eq!(
            selected
                .units()
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            [location(1), location(4)].into()
        );

        let selected = resolver
            .select_with_provided([required("caller")], [provided("fallback")], [], &[])
            .unwrap();

        assert_eq!(selected.units(), [location(1)]);
        assert!(selected.statics().is_empty());

        assert_eq!(
            selected.static_accesses(&required("caller")),
            Some([].as_slice())
        );
    }

    #[test]
    fn lifecycle_metadata_closes_providers_and_rejects_missing_or_conflicting_instances() {
        let contribution = |id, name, dependencies: Vec<[u8; 32]>| {
            crate::NativeStatic::new(
                NonEmptySharedStr::try_new(name).unwrap(),
                [id; 32],
                vec![id].into(),
                bray_symbols::StaticStorageDuration::Product,
                dependencies,
                true,
                false,
            )
        };

        let first = unit(
            1,
            &[(
                "first",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        )
        .with_statics([contribution(1, "first", vec![[2; 32]])]);

        let second = unit(
            2,
            &[(
                "second",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        )
        .with_statics([contribution(2, "second", vec![])]);

        let resolver = NativeUnitResolver::new([index([first.clone()], []), index([second], [])]);
        let selected = resolver.select([required("first")]).unwrap();

        assert_eq!(
            selected.units(),
            [
                location(1),
                NativeUnitLocation {
                    artifact: 1,
                    digest: digest(2)
                }
            ]
        );

        assert_eq!(
            NativeUnitResolver::new([index([first], [])]).select([required("first")]),
            Err(NativeResolutionError::MissingStatic([2; 32]))
        );

        let first = unit(
            1,
            &[(
                "first",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        )
        .with_statics([contribution(1, "first", vec![])]);

        let conflicting = unit(
            2,
            &[(
                "second",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        )
        .with_statics([contribution(1, "second", vec![])]);

        assert_eq!(
            NativeUnitResolver::new([index([first], []), index([conflicting], [])])
                .select([required("first"), required("second")]),
            Err(NativeResolutionError::ConflictingStatic([1; 32]))
        );
    }

    #[test]
    fn closes_cyclic_code_and_addressed_data_without_selecting_disconnected_units() {
        let first = unit(
            1,
            &[(
                "entry",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[required("table")],
            &[],
        );

        let data = unit(
            2,
            &[(
                "table",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[required("callback")],
            &[],
        );

        let callback = unit(
            3,
            &[(
                "callback",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[required("entry")],
            &[],
        );

        let unused = unit(
            4,
            &[(
                "unused",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let index = index([unused, callback, first, data], []);

        let selected = NativeUnitResolver::new([index])
            .select([required("entry")])
            .expect("cyclic closure");

        assert_eq!(
            selected
                .units()
                .iter()
                .copied()
                .collect::<std::collections::BTreeSet<_>>(),
            [location(1), location(2), location(3)].into()
        );

        assert!(
            selected
                .reasons(location(2))
                .expect("data reason")
                .contains(&NativeUnitInclusion::Reference {
                    from: location(1),
                    symbol: required("table")
                })
        );
    }

    #[test]
    fn closes_across_artifacts_and_preserves_producer_and_unit_provenance() {
        let caller = unit(
            1,
            &[(
                "caller",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[required("callee")],
            &[],
        );

        let callee = unit(
            2,
            &[(
                "callee",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let unused = unit(
            3,
            &[(
                "unused",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let dependency = NativeArtifactIndex::try_new(
            NativeTarget::X86_64WindowsMsvc,
            digest(98),
            [callee, unused],
            [],
        )
        .expect("dependency index");

        let resolver = NativeUnitResolver::new([index([caller], []), dependency]);

        let selected = resolver
            .select([required("caller")])
            .expect("cross-package closure");

        assert_eq!(
            selected.units(),
            [
                location(1),
                NativeUnitLocation {
                    artifact: 1,
                    digest: digest(2)
                }
            ]
        );

        assert_eq!(
            resolver
                .artifacts()
                .iter()
                .map(NativeArtifactIndex::producer)
                .collect::<Vec<_>>(),
            [digest(99), digest(98)]
        );

        assert!(
            selected
                .reasons(NativeUnitLocation {
                    artifact: 1,
                    digest: digest(2)
                })
                .expect("provider reason")
                .contains(&NativeUnitInclusion::Reference {
                    from: location(1),
                    symbol: required("callee")
                })
        );
    }

    #[test]
    fn identical_payload_identity_does_not_erase_artifact_ownership() {
        let first = unit(
            1,
            &[(
                "value",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let resolver = NativeUnitResolver::new([index([first.clone()], []), index([first], [])]);

        assert_eq!(
            resolver.select([required("value")]),
            Err(NativeResolutionError::DuplicateStrong(required("value")))
        );
    }

    #[test]
    fn referencing_unit_precedes_its_native_provider() {
        let caller = unit(
            1,
            &[(
                "caller",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[required("callee")],
            &[],
        );

        let callee = unit(
            2,
            &[(
                "callee",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let selection = NativeUnitResolver::new([index([callee, caller], [])])
            .select([required("caller")])
            .expect("linked pair");

        assert_eq!(selection.units(), [location(1), location(2)]);
    }

    #[test]
    fn owned_unit_seeds_close_through_the_same_reference_graph() {
        let owner = unit(
            1,
            &[(
                "entry",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[required("support")],
            &[],
        );

        let support = unit(
            2,
            &[(
                "support",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let unused = unit(
            3,
            &[(
                "unused",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let selected = NativeUnitResolver::new([index([unused, support, owner], [])])
            .select_seeded([location(1)])
            .expect("owned-unit closure");

        assert_eq!(selected.units(), [location(1), location(2)]);

        assert!(
            selected
                .reasons(location(1))
                .unwrap()
                .contains(&NativeUnitInclusion::Seed)
        );
    }

    #[test]
    fn product_definition_satisfies_selected_unit_reference() {
        let caller = unit(
            1,
            &[(
                "caller",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[required("callee")],
            &[],
        );

        let callee = unit(
            2,
            &[(
                "callee",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let provided =
            NativeSymbolIdentity::Name(NonEmptySharedStr::try_new("callee").expect("test name"));

        let selection = NativeUnitResolver::new([index([callee, caller], [])])
            .select_with_provided(
                [required("caller")],
                [(provided, NativeSymbolBinding::Strong)],
                [],
                &[],
            )
            .expect("product-satisfied closure");

        assert_eq!(selection.units(), [location(1)]);
    }

    #[test]
    fn weak_product_definition_only_yields_to_an_ordinary_strong_provider() {
        let caller = unit(
            1,
            &[(
                "caller",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[required("callee")],
            &[],
        );

        let strong = unit(
            2,
            &[(
                "callee",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let weak = unit(
            3,
            &[(
                "callee",
                NativeSymbolBinding::Weak,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let fallback = unit(
            4,
            &[(
                "callee",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Fallback,
            )],
            &[],
            &[],
        );

        let provided = || {
            (
                NativeSymbolIdentity::Name(
                    NonEmptySharedStr::try_new("callee").expect("test name"),
                ),
                NativeSymbolBinding::Weak,
            )
        };

        let selected = NativeUnitResolver::new([index(
            [caller.clone(), strong, weak.clone(), fallback.clone()],
            [],
        )])
        .select_with_provided([required("caller")], [provided()], [], &[])
        .expect("strong provider overrides weak product definition");

        assert_eq!(selected.units(), [location(1), location(2)]);

        for provider in [weak, fallback] {
            let selected = NativeUnitResolver::new([index([caller.clone(), provider], [])])
                .select_with_provided([required("caller")], [provided()], [], &[])
                .expect("weak product definition satisfies reference");

            assert_eq!(selected.units(), [location(1)]);
        }
    }

    #[test]
    fn declared_native_links_terminate_unknown_roots_but_keep_indexed_providers() {
        let provider = unit(
            1,
            &[(
                "known",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let resolver = NativeUnitResolver::new([index([provider], [])]);

        let links = [NativeLinkRequirement::new(
            NonEmptySharedStr::try_new("external").unwrap(),
            NativeLinkKind::Dynamic,
        )];

        assert_eq!(
            resolver.select_with_provided([required("unknown")], [], [], &[]),
            Err(NativeResolutionError::Unresolved(required("unknown")))
        );

        assert_eq!(
            resolver
                .select_with_provided([required("unknown"), required("known")], [], [], &links)
                .expect("unknown native-library definitions resolve at the linker")
                .units(),
            [location(1)]
        );
    }

    #[test]
    fn retains_initializers_groups_and_opaque_units_in_stable_order() {
        let entry = unit(
            1,
            &[(
                "entry",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let grouped = unit(
            2,
            &[(
                "grouped",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let initializer = unit(
            3,
            &[(
                "init",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[NativeRoot::Initialization],
        );

        let opaque = NativeUnit::new(
            digest(4),
            NativeUnitKind::OpaqueArchive,
            NativeUnitSummary::opaque([]),
            [],
        );

        let group = NativeCoRetentionGroup::try_new([digest(1), digest(2)]).expect("group");

        let first = index(
            [
                entry.clone(),
                grouped.clone(),
                initializer.clone(),
                opaque.clone(),
            ],
            [group.clone()],
        );

        let reversed = index([opaque, initializer, grouped, entry], [group]);

        let selected = NativeUnitResolver::new([first])
            .select([required("entry")])
            .expect("closure");

        let reordered = NativeUnitResolver::new([reversed])
            .select([required("entry")])
            .expect("closure");

        assert_eq!(selected, reordered);
        assert_eq!(selected.units().len(), 4);

        assert!(
            selected
                .reasons(location(2))
                .expect("group reason")
                .contains(&NativeUnitInclusion::CoRetention(location(1)))
        );

        assert!(
            selected
                .reasons(location(3))
                .expect("initializer reason")
                .contains(&NativeUnitInclusion::Lifecycle(NativeRoot::Initialization))
        );

        assert!(
            selected
                .reasons(location(4))
                .expect("opaque reason")
                .contains(&NativeUnitInclusion::Opaque)
        );
    }

    #[test]
    fn optional_reference_never_demands_a_provider() {
        let optional = symbol(
            "absent",
            NativeSymbolBinding::Weak,
            NativeSymbolPresence::Optional,
        );

        let entry = unit(
            1,
            &[(
                "entry",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[optional],
            &[],
        );

        let index = index([entry], []);

        assert_eq!(
            NativeUnitResolver::new([index])
                .select([required("entry")])
                .expect("optional reference")
                .units(),
            [location(1)]
        );
    }

    #[test]
    fn mixed_fallback_unit_keeps_live_code_without_retaining_displaced_fallback_units() {
        let mixed = unit(
            1,
            &[
                (
                    "live",
                    NativeSymbolBinding::Strong,
                    NativeDefinitionSelection::Ordinary,
                ),
                (
                    "provider",
                    NativeSymbolBinding::Weak,
                    NativeDefinitionSelection::Fallback,
                ),
            ],
            &[required("provider")],
            &[],
        );

        let strong = unit(
            2,
            &[(
                "provider",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let fallback_only = unit(
            3,
            &[(
                "provider",
                NativeSymbolBinding::Weak,
                NativeDefinitionSelection::Fallback,
            )],
            &[],
            &[],
        );

        for units in [
            vec![mixed.clone(), strong.clone(), fallback_only.clone()],
            vec![strong.clone(), fallback_only.clone(), mixed.clone()],
        ] {
            let original = index(units, []);
            let bytes = original.encode().unwrap();

            let index_digest =
                NativeContentDigest::new(bray_base::sha256_reader(bytes.as_slice()).unwrap());

            let decoded =
                NativeArtifactIndex::decode(&bytes, index_digest, original.target()).unwrap();

            let resolver = NativeUnitResolver::new([decoded]);
            let selected = resolver.select([required("live")]).unwrap();

            assert_eq!(
                selected
                    .units()
                    .iter()
                    .map(|unit| unit.digest)
                    .collect::<BTreeSet<_>>(),
                [digest(1), digest(2)].into()
            );

            assert!(selected.statics().is_empty());
        }

        let selected = NativeUnitResolver::new([index([mixed, fallback_only], [])])
            .select([required("provider"), required("live")])
            .unwrap();

        assert_eq!(selected.units().len(), 2);
    }

    #[test]
    fn strong_wins_weak_and_fallback_while_duplicate_strong_is_reported() {
        let weak = unit(
            1,
            &[(
                "value",
                NativeSymbolBinding::Weak,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let strong = unit(
            2,
            &[(
                "value",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let fallback = unit(
            3,
            &[(
                "value",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Fallback,
            )],
            &[],
            &[],
        );

        let selected = NativeUnitResolver::new([index([weak, strong, fallback], [])])
            .select([required("value")])
            .expect("strong provider");

        assert_eq!(selected.units(), [location(2)]);

        let duplicate = index(
            [
                unit(
                    2,
                    &[(
                        "value",
                        NativeSymbolBinding::Strong,
                        NativeDefinitionSelection::Ordinary,
                    )],
                    &[],
                    &[],
                ),
                unit(
                    4,
                    &[(
                        "value",
                        NativeSymbolBinding::Strong,
                        NativeDefinitionSelection::Ordinary,
                    )],
                    &[],
                    &[],
                ),
            ],
            [],
        );

        assert_eq!(
            NativeUnitResolver::new([duplicate]).select([required("value")]),
            Err(NativeResolutionError::DuplicateStrong(required("value")))
        );
    }

    #[test]
    fn comdat_providers_remain_together_for_target_linker_selection() {
        let group = NativeSymbolIdentity::Name(
            NonEmptySharedStr::try_new("group").expect("test group name"),
        );

        let selection = NativeDefinitionSelection::Comdat {
            group,
            rule: NativeComdatSelection::ExactMatch,
            associative_with: None,
        };

        let index = index(
            [
                unit(
                    1,
                    &[("value", NativeSymbolBinding::Strong, selection.clone())],
                    &[],
                    &[],
                ),
                unit(
                    2,
                    &[("value", NativeSymbolBinding::Strong, selection)],
                    &[],
                    &[],
                ),
            ],
            [],
        );

        assert_eq!(
            NativeUnitResolver::new([index])
                .select([required("value")])
                .expect("COMDAT choice")
                .units()
                .len(),
            2
        );
    }

    #[test]
    fn selected_units_cannot_hide_duplicate_strong_definitions() {
        let first = unit(
            1,
            &[
                (
                    "first",
                    NativeSymbolBinding::Strong,
                    NativeDefinitionSelection::Ordinary,
                ),
                (
                    "shared",
                    NativeSymbolBinding::Strong,
                    NativeDefinitionSelection::Ordinary,
                ),
            ],
            &[],
            &[],
        );

        let second = unit(
            2,
            &[
                (
                    "second",
                    NativeSymbolBinding::Strong,
                    NativeDefinitionSelection::Ordinary,
                ),
                (
                    "shared",
                    NativeSymbolBinding::Strong,
                    NativeDefinitionSelection::Ordinary,
                ),
            ],
            &[],
            &[],
        );

        let resolver = NativeUnitResolver::new([index([first, second], [])]);

        assert_eq!(
            resolver.select([required("first"), required("second")]),
            Err(NativeResolutionError::DuplicateStrong(required("shared")))
        );
    }

    #[test]
    fn unresolved_required_symbol_is_reported() {
        let index = index([], []);

        assert_eq!(
            NativeUnitResolver::new([index]).select([required("missing")]),
            Err(NativeResolutionError::Unresolved(required("missing")))
        );
    }

    #[test]
    fn opaque_terminal_preserves_linker_resolution_of_unknown_references() {
        let entry = unit(
            1,
            &[(
                "entry",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[required("external")],
            &[],
        );

        let archive = NativeUnit::new(
            digest(2),
            NativeUnitKind::OpaqueArchive,
            NativeUnitSummary::opaque([]),
            [],
        );

        let resolver = NativeUnitResolver::new([index([entry, archive], [])]);

        assert_eq!(
            resolver
                .select([required("entry")])
                .expect("opaque terminal")
                .units(),
            [location(1), location(2)]
        );
    }

    #[test]
    fn opaque_references_close_across_actual_dependency_indexes() {
        let opaque = NativeUnit::new(
            digest(1),
            NativeUnitKind::OpaqueArchive,
            NativeUnitSummary::opaque([required("bridge"), required("unknown_native")]),
            [],
        );

        let bridge = unit(
            2,
            &[(
                "bridge",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[required("dependency")],
            &[],
        );

        let dependency = unit(
            3,
            &[(
                "dependency",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let unrelated = unit(
            4,
            &[(
                "unrelated",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let resolver = NativeUnitResolver::new([
            index([opaque, bridge], []),
            index([dependency, unrelated], []),
        ]);

        let selected = resolver
            .select([])
            .expect("known opaque dependencies must close across producers");

        let units = selected
            .units()
            .iter()
            .copied()
            .collect::<std::collections::BTreeSet<_>>();

        assert_eq!(
            units,
            [
                location(1),
                location(2),
                NativeUnitLocation {
                    artifact: 1,
                    digest: digest(3)
                }
            ]
            .into()
        );
    }

    #[test]
    fn native_dependency_is_a_terminal_provider_for_external_support() {
        let entry = NativeUnit::new(
            digest(1),
            NativeUnitKind::Bitcode,
            NativeUnitSummary::Exact {
                definitions: Arc::from([NativeDefinition::new(
                    required("entry"),
                    NativeDefinitionSelection::Ordinary,
                )]),
                references: Arc::from([required("external_support")]),
                roots: Arc::from([]),
            },
            [NativeLinkRequirement::new(
                NonEmptySharedStr::try_new("native_support").expect("test library name"),
                NativeLinkKind::System,
            )],
        );

        let resolver = NativeUnitResolver::new([index([entry], [])]);

        assert_eq!(
            resolver
                .select([required("entry")])
                .expect("native terminal")
                .units(),
            [location(1)]
        );
    }

    #[test]
    fn repeated_roots_share_one_cached_selection() {
        let entry = unit(
            1,
            &[(
                "entry",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let other = unit(
            2,
            &[(
                "other",
                NativeSymbolBinding::Strong,
                NativeDefinitionSelection::Ordinary,
            )],
            &[],
            &[],
        );

        let resolver = NativeUnitResolver::new([index([entry, other], [])]);

        let first = resolver
            .select([required("entry"), required("other"), required("entry")])
            .expect("first closure");

        let second = resolver
            .select([required("other"), required("entry")])
            .expect("cached closure");

        assert_eq!(first, second);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(resolver.selections.lock().expect("test mutex").len(), 1);
    }
}
