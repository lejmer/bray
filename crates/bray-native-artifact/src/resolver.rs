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
    (symbol.identity().clone(), symbol.version().map(str::to_owned))
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
        from: NativeContentDigest,
        /// Required native symbol.
        symbol: NativeSymbolContract,
    },
    /// Native initialization or finalization requires the unit.
    Lifecycle(NativeRoot),
    /// Another selected unit requires this unit to be retained with it.
    CoRetention(NativeContentDigest),
    /// The unit has no exact symbol summary and remains a conservative link input.
    Opaque,
}

/// Closed native units in deterministic dependency-compatible order, with inclusion reasons.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NativeUnitSelection {
    units: Vec<NativeContentDigest>,
    reasons: BTreeMap<NativeContentDigest, BTreeSet<NativeUnitInclusion>>,
}

impl NativeUnitSelection {
    /// Referencing units precede their providers, except within unavoidable cycles.
    pub fn units(&self) -> &[NativeContentDigest] {
        &self.units
    }

    /// Returns all distinct reasons for retaining one unit.
    pub fn reasons(&self, unit: NativeContentDigest) -> Option<&BTreeSet<NativeUnitInclusion>> {
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
}

/// Indexes one validated artifact once and closes native demand without inspecting payloads.
#[derive(Debug)]
pub struct NativeUnitResolver {
    index: NativeArtifactIndex,
    providers: BTreeMap<SymbolKey, Vec<(usize, usize)>>,
    groups: BTreeMap<usize, BTreeSet<usize>>,
    selections: Mutex<BTreeMap<Vec<NativeSymbolContract>, Result<Arc<NativeUnitSelection>, NativeResolutionError>>>,
}

impl NativeUnitResolver {
    /// Constructs a resolver from an already validated artifact index.
    pub fn new(index: NativeArtifactIndex) -> Self {
        let mut providers: BTreeMap<_, Vec<_>> = BTreeMap::new();

        for (unit_index, unit) in index.units().iter().enumerate() {
            if let NativeUnitSummary::Exact { definitions, .. } = unit.summary() {
                for (definition_index, definition) in definitions.iter().enumerate() {
                    providers
                        .entry(symbol_key(definition.symbol()))
                        .or_default()
                        .push((unit_index, definition_index));
                }
            }
        }

        let mut groups: BTreeMap<usize, BTreeSet<usize>> = BTreeMap::new();

        for group in index.co_retention_groups() {
            let members = group
                .members()
                .iter()
                .map(|digest| {
                    index.units().binary_search_by_key(digest, |unit| unit.digest())
                        .expect("validated co-retention member must be indexed")
                })
                .collect::<BTreeSet<_>>();

            for &member in &members {
                groups.entry(member).or_default().extend(members.iter().copied());
            }
        }

        Self { index, providers, groups, selections: Mutex::new(BTreeMap::new()) }
    }

    /// Returns the validated artifact whose units this resolver owns.
    pub const fn index(&self) -> &NativeArtifactIndex {
        &self.index
    }

    /// Closes required symbols, lifecycle roots, opaque inputs and co-retention groups.
    pub fn select(
        &self,
        demands: impl IntoIterator<Item = NativeSymbolContract>,
    ) -> Result<Arc<NativeUnitSelection>, NativeResolutionError> {
        let demands = demands.into_iter().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();

        if let Some(selection) = self.selections.lock()
            .expect("native selection cache mutex poisoned")
            .get(&demands) {
            return selection.clone();
        }

        let selection = self.select_uncached(&demands, &BTreeMap::new(), &[]).map(Arc::new);

        self.selections.lock()
            .expect("native selection cache mutex poisoned")
            .insert(demands, selection.clone());

        selection
    }

    /// Closes demand with product definitions, allowing ordinary strong units to displace weak ones.
    pub fn select_with_provided(
        &self,
        demands: impl IntoIterator<Item = NativeSymbolContract>,
        provided: impl IntoIterator<Item = (NativeSymbolIdentity, NativeSymbolBinding)>,
    ) -> Result<NativeUnitSelection, NativeResolutionError> {
        let demands = demands.into_iter().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();
        let mut definitions = BTreeMap::new();

        for (identity, binding) in provided {
            let existing = definitions.entry(identity).or_insert(binding);

            if binding == NativeSymbolBinding::Strong {
                *existing = binding;
            }
        }

        self.select_uncached(&demands, &definitions, &[])
    }

    /// Closes from known owned units when native publication starts from physical entries.
    pub fn select_seeded(
        &self,
        seeds: impl IntoIterator<Item = NativeContentDigest>,
    ) -> Result<NativeUnitSelection, NativeResolutionError> {
        let seeds = seeds.into_iter().collect::<BTreeSet<_>>().into_iter().collect::<Vec<_>>();

        self.select_uncached(&[], &BTreeMap::new(), &seeds)
    }

    fn select_uncached(
        &self,
        demands: &[NativeSymbolContract],
        provided: &BTreeMap<NativeSymbolIdentity, NativeSymbolBinding>,
        seeds: &[NativeContentDigest],
    ) -> Result<NativeUnitSelection, NativeResolutionError> {
        let mut state = SelectionState::default();

        for seed in seeds {
            let index = self.index.units().binary_search_by_key(seed, |unit| unit.digest())
                .expect("seeded native unit must belong to the resolver index");

            state.include(index, NativeUnitInclusion::Seed, None);
        }

        for demand in demands {
            self.require(&demand, NativeUnitInclusion::Demand(demand.clone()), None, provided, &mut state)?;
        }

        for (unit_index, unit) in self.index.units().iter().enumerate() {
            match unit.summary() {
                NativeUnitSummary::Exact { roots, .. } => {
                    for &root in roots.iter() {
                        state.include(unit_index, NativeUnitInclusion::Lifecycle(root), None);
                    }
                }
                NativeUnitSummary::Opaque => {
                    state.include(unit_index, NativeUnitInclusion::Opaque, None);
                }
            }
        }

        while let Some(unit_index) = state.pending.pop_first() {
            let unit = &self.index.units()[unit_index];

            if let Some(group) = self.groups.get(&unit_index) {
                for &member in group {
                    if member != unit_index {
                        state.include(
                            member,
                            NativeUnitInclusion::CoRetention(unit.digest()),
                            Some(unit_index),
                        );
                    }
                }
            }

            if let NativeUnitSummary::Exact { references, .. } = unit.summary() {
                for reference in references.iter() {
                    self.require(
                        reference,
                        NativeUnitInclusion::Reference {
                            from: unit.digest(),
                            symbol: reference.clone(),
                        },
                        Some(unit_index),
                        provided,
                        &mut state,
                    )?;
                }
            }
        }

        for providers in self.providers.values() {
            let selected = providers.iter().filter(|(unit_index, _)| state.reasons.contains_key(unit_index))
                .map(|&(unit_index, definition_index)| {
                    let NativeUnitSummary::Exact { definitions, .. } = self.index.units()[unit_index].summary() else {
                        unreachable!("only exact definitions enter the provider index");
                    };

                    &definitions[definition_index]
                }).collect::<Vec<_>>();

            if duplicate_strong(selected.iter().copied()) {
                return Err(NativeResolutionError::DuplicateStrong(selected[0].symbol().clone()));
            }
        }

        let order = dependency_order(&state.reasons, &state.edges);
        let units = order.into_iter().map(|index| self.index.units()[index].digest()).collect();

        let reasons = state.reasons.into_iter().map(|(index, reasons)| {
            (self.index.units()[index].digest(), reasons)
        }).collect();

        Ok(NativeUnitSelection { units, reasons })
    }

    fn require(
        &self,
        symbol: &NativeSymbolContract,
        reason: NativeUnitInclusion,
        from: Option<usize>,
        provided: &BTreeMap<NativeSymbolIdentity, NativeSymbolBinding>,
        state: &mut SelectionState,
    ) -> Result<(), NativeResolutionError> {
        if symbol.presence() == NativeSymbolPresence::Optional {
            return Ok(());
        }

        let provided_binding = symbol.version().is_none()
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

            for (unit_index, unit) in self.index.units().iter().enumerate() {
                if matches!(unit.summary(), NativeUnitSummary::Opaque) {
                    opaque_terminal = true;

                    if let Some(from) = from {
                        state.edges.entry(from).or_default().insert(unit_index);
                    }
                }
            }

            if opaque_terminal || from.is_some_and(|unit| !self.index.units()[unit].native_links().is_empty()) {
                // Opaque code or an explicit native dependency is a terminal linker input.
                return Ok(());
            }

            return Err(NativeResolutionError::Unresolved(symbol.clone()));
        };

        let mut ordinary = Vec::new();
        let mut fallback = Vec::new();

        for &(unit_index, definition_index) in candidates {
            let NativeUnitSummary::Exact { definitions, .. } = self.index.units()[unit_index].summary() else {
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

        let candidates = if ordinary.is_empty() { &fallback } else { &ordinary };

        let strong = candidates.iter().filter(|(_, definition)| {
            definition.symbol().binding() == NativeSymbolBinding::Strong
        }).collect::<Vec<_>>();

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
        }

        Ok(())
    }
}

fn duplicate_strong<'a>(
    definitions: impl IntoIterator<Item = &'a crate::NativeDefinition>,
) -> bool {
    let definitions = definitions.into_iter().collect::<Vec<_>>();

    let ordinary = definitions.iter().copied()
        .filter(|definition| !matches!(definition.selection(), NativeDefinitionSelection::Fallback))
        .collect::<Vec<_>>();

    let relevant = if ordinary.is_empty() { &definitions } else { &ordinary };

    let strong = relevant.iter().copied()
        .filter(|definition| definition.symbol().binding() == NativeSymbolBinding::Strong)
        .collect::<Vec<_>>();

    strong.len() > 1 && strong.iter().any(|definition| {
        matches!(definition.selection(), NativeDefinitionSelection::Ordinary)
            || matches!(definition.selection(), NativeDefinitionSelection::Comdat {
                rule: crate::NativeComdatSelection::NoDuplicates, ..
            })
    })
}

#[derive(Default)]
struct SelectionState {
    reasons: BTreeMap<usize, BTreeSet<NativeUnitInclusion>>,
    pending: BTreeSet<usize>,
    edges: BTreeMap<usize, BTreeSet<usize>>,
}

impl SelectionState {
    fn include(&mut self, unit: usize, reason: NativeUnitInclusion, from: Option<usize>) {
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
            self.edges.entry(from).or_default().insert(unit);
        }
    }
}

fn dependency_order(
    reasons: &BTreeMap<usize, BTreeSet<NativeUnitInclusion>>,
    edges: &BTreeMap<usize, BTreeSet<usize>>,
) -> Vec<usize> {
    let mut order = Vec::with_capacity(reasons.len());
    let mut active = BTreeSet::new();
    let mut done = BTreeSet::new();

    for &root in reasons.keys() {
        let mut stack = vec![(root, false)];

        while let Some((unit, exiting)) = stack.pop() {
            if exiting {
                active.remove(&unit);

                if done.insert(unit) {
                    order.push(unit);
                }
            } else if !done.contains(&unit) && active.insert(unit) {
                stack.push((unit, true));

                if let Some(dependencies) = edges.get(&unit) {
                    stack.extend(dependencies.iter().rev().filter(|next| !active.contains(next)).map(|&next| (next, false)));
                }
            }
        }
    }

    order.reverse();

    order
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_base::NonEmptySharedStr;
    use bray_symbols::{
        NativeLinkKind, NativeLinkRequirement, NativeSymbolBinding, NativeSymbolContract,
        NativeSymbolIdentity, NativeSymbolPresence,
    };
    use bray_target::NativeTarget;

    use super::{NativeResolutionError, NativeUnitInclusion, NativeUnitResolver};
    use crate::{
        NativeArtifactIndex, NativeCoRetentionGroup, NativeComdatSelection, NativeContentDigest, NativeDefinition,
        NativeDefinitionSelection, NativeRoot, NativeUnit, NativeUnitKind, NativeUnitSummary,
    };

    fn digest(value: u8) -> NativeContentDigest {
        NativeContentDigest::new([value; 32])
    }

    fn symbol(name: &str, binding: NativeSymbolBinding, presence: NativeSymbolPresence) -> NativeSymbolContract {
        NativeSymbolContract::new(
            NativeSymbolIdentity::Name(NonEmptySharedStr::try_new(name).expect("test name")),
            None,
            binding,
            presence,
        )
    }

    fn required(name: &str) -> NativeSymbolContract {
        symbol(name, NativeSymbolBinding::Strong, NativeSymbolPresence::Required)
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
                definitions: definitions.iter().map(|(name, binding, selection)| {
                    NativeDefinition::new(
                        symbol(name, *binding, NativeSymbolPresence::Required),
                        selection.clone(),
                    )
                }).collect::<Vec<_>>().into(),
                references: Arc::from(references),
                roots: Arc::from(roots),
            },
            [],
            [],
        )
    }

    fn index(units: impl IntoIterator<Item = NativeUnit>, groups: impl IntoIterator<Item = NativeCoRetentionGroup>) -> NativeArtifactIndex {
        NativeArtifactIndex::try_new(NativeTarget::X86_64WindowsMsvc, digest(99), units, groups)
            .expect("test native index")
    }

    #[test]
    fn closes_cyclic_code_and_addressed_data_without_selecting_disconnected_units() {
        let first = unit(1, &[("entry", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[required("table")], &[]);
        let data = unit(2, &[("table", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[required("callback")], &[]);
        let callback = unit(3, &[("callback", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[required("entry")], &[]);
        let unused = unit(4, &[("unused", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[], &[]);
        let index = index([unused, callback, first, data], []);

        let selected = NativeUnitResolver::new(index).select([required("entry")]).expect("cyclic closure");

        assert_eq!(selected.units().iter().copied().collect::<std::collections::BTreeSet<_>>(), [digest(1), digest(2), digest(3)].into());
        assert!(selected.reasons(digest(2)).expect("data reason").contains(&NativeUnitInclusion::Reference { from: digest(1), symbol: required("table") }));
    }

    #[test]
    fn referencing_unit_precedes_its_native_provider() {
        let caller = unit(1, &[("caller", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[required("callee")], &[]);
        let callee = unit(2, &[("callee", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[], &[]);

        let selection = NativeUnitResolver::new(index([callee, caller], []))
            .select([required("caller")]).expect("linked pair");

        assert_eq!(selection.units(), [digest(1), digest(2)]);
    }

    #[test]
    fn owned_unit_seeds_close_through_the_same_reference_graph() {
        let owner = unit(1, &[("entry", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[required("support")], &[]);
        let support = unit(2, &[("support", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[], &[]);
        let unused = unit(3, &[("unused", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[], &[]);

        let selected = NativeUnitResolver::new(index([unused, support, owner], []))
            .select_seeded([digest(1)])
            .expect("owned-unit closure");

        assert_eq!(selected.units(), [digest(1), digest(2)]);
        assert!(selected.reasons(digest(1)).unwrap().contains(&NativeUnitInclusion::Seed));
    }

    #[test]
    fn product_definition_satisfies_selected_unit_reference() {
        let caller = unit(1, &[("caller", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[required("callee")], &[]);
        let callee = unit(2, &[("callee", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[], &[]);
        let provided = NativeSymbolIdentity::Name(NonEmptySharedStr::try_new("callee").expect("test name"));

        let selection = NativeUnitResolver::new(index([callee, caller], []))
            .select_with_provided([required("caller")], [(provided, NativeSymbolBinding::Strong)])
            .expect("product-satisfied closure");

        assert_eq!(selection.units(), [digest(1)]);
    }

    #[test]
    fn weak_product_definition_only_yields_to_an_ordinary_strong_provider() {
        let caller = unit(1, &[("caller", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[required("callee")], &[]);
        let strong = unit(2, &[("callee", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[], &[]);
        let weak = unit(3, &[("callee", NativeSymbolBinding::Weak, NativeDefinitionSelection::Ordinary)], &[], &[]);
        let fallback = unit(4, &[("callee", NativeSymbolBinding::Strong, NativeDefinitionSelection::Fallback)], &[], &[]);
        let provided = || (NativeSymbolIdentity::Name(NonEmptySharedStr::try_new("callee").expect("test name")), NativeSymbolBinding::Weak);

        let selected = NativeUnitResolver::new(index([caller.clone(), strong, weak.clone(), fallback.clone()], []))
            .select_with_provided([required("caller")], [provided()])
            .expect("strong provider overrides weak product definition");

        assert_eq!(selected.units(), [digest(1), digest(2)]);

        for provider in [weak, fallback] {
            let selected = NativeUnitResolver::new(index([caller.clone(), provider], []))
                .select_with_provided([required("caller")], [provided()])
                .expect("weak product definition satisfies reference");

            assert_eq!(selected.units(), [digest(1)]);
        }
    }

    #[test]
    fn retains_initializers_groups_and_opaque_units_in_stable_order() {
        let entry = unit(1, &[("entry", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[], &[]);
        let grouped = unit(2, &[("grouped", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[], &[]);
        let initializer = unit(3, &[("init", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[], &[NativeRoot::Initialization]);
        let opaque = NativeUnit::new(digest(4), NativeUnitKind::OpaqueArchive, NativeUnitSummary::Opaque, [], []);
        let group = NativeCoRetentionGroup::try_new([digest(1), digest(2)]).expect("group");
        let first = index([entry.clone(), grouped.clone(), initializer.clone(), opaque.clone()], [group.clone()]);
        let reversed = index([opaque, initializer, grouped, entry], [group]);
        let selected = NativeUnitResolver::new(first).select([required("entry")]).expect("closure");
        let reordered = NativeUnitResolver::new(reversed).select([required("entry")]).expect("closure");

        assert_eq!(selected, reordered);
        assert_eq!(selected.units().len(), 4);
        assert!(selected.reasons(digest(2)).expect("group reason").contains(&NativeUnitInclusion::CoRetention(digest(1))));
        assert!(selected.reasons(digest(3)).expect("initializer reason").contains(&NativeUnitInclusion::Lifecycle(NativeRoot::Initialization)));
        assert!(selected.reasons(digest(4)).expect("opaque reason").contains(&NativeUnitInclusion::Opaque));
    }

    #[test]
    fn optional_reference_never_demands_a_provider() {
        let optional = symbol("absent", NativeSymbolBinding::Weak, NativeSymbolPresence::Optional);
        let entry = unit(1, &[("entry", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[optional], &[]);
        let index = index([entry], []);

        assert_eq!(NativeUnitResolver::new(index).select([required("entry")]).expect("optional reference").units(), [digest(1)]);
    }

    #[test]
    fn strong_wins_weak_and_fallback_while_duplicate_strong_is_reported() {
        let weak = unit(1, &[("value", NativeSymbolBinding::Weak, NativeDefinitionSelection::Ordinary)], &[], &[]);
        let strong = unit(2, &[("value", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[], &[]);
        let fallback = unit(3, &[("value", NativeSymbolBinding::Strong, NativeDefinitionSelection::Fallback)], &[], &[]);

        let selected = NativeUnitResolver::new(index([weak, strong, fallback], []))
            .select([required("value")]).expect("strong provider");

        assert_eq!(selected.units(), [digest(2)]);

        let duplicate = index([
            unit(2, &[("value", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[], &[]),
            unit(4, &[("value", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[], &[]),
        ], []);

        assert_eq!(NativeUnitResolver::new(duplicate).select([required("value")]), Err(NativeResolutionError::DuplicateStrong(required("value"))));
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

        let index = index([
            unit(1, &[("value", NativeSymbolBinding::Strong, selection.clone())], &[], &[]),
            unit(2, &[("value", NativeSymbolBinding::Strong, selection)], &[], &[]),
        ], []);

        assert_eq!(NativeUnitResolver::new(index).select([required("value")]).expect("COMDAT choice").units().len(), 2);
    }

    #[test]
    fn selected_units_cannot_hide_duplicate_strong_definitions() {
        let first = unit(1, &[
            ("first", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary),
            ("shared", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary),
        ], &[], &[]);

        let second = unit(2, &[
            ("second", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary),
            ("shared", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary),
        ], &[], &[]);

        let resolver = NativeUnitResolver::new(index([first, second], []));

        assert_eq!(resolver.select([required("first"), required("second")]),
            Err(NativeResolutionError::DuplicateStrong(required("shared"))));
    }

    #[test]
    fn unresolved_required_symbol_is_reported() {
        let index = index([], []);

        assert_eq!(NativeUnitResolver::new(index).select([required("missing")]), Err(NativeResolutionError::Unresolved(required("missing"))));
    }

    #[test]
    fn opaque_terminal_preserves_linker_resolution_of_unknown_references() {
        let entry = unit(1, &[("entry", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[required("external")], &[]);
        let archive = NativeUnit::new(digest(2), NativeUnitKind::OpaqueArchive, NativeUnitSummary::Opaque, [], []);
        let resolver = NativeUnitResolver::new(index([entry, archive], []));

        assert_eq!(resolver.select([required("entry")]).expect("opaque terminal").units(), [digest(1), digest(2)]);
    }

    #[test]
    fn native_dependency_is_a_terminal_provider_for_external_support() {
        let entry = NativeUnit::new(
            digest(1),
            NativeUnitKind::Bitcode,
            NativeUnitSummary::Exact {
                definitions: Arc::from([NativeDefinition::new(
                    required("entry"), NativeDefinitionSelection::Ordinary,
                )]),
                references: Arc::from([required("external_support")]),
                roots: Arc::from([]),
            },
            [NativeLinkRequirement::new(
                NonEmptySharedStr::try_new("native_support").expect("test library name"),
                NativeLinkKind::System,
            )],
            [],
        );

        let resolver = NativeUnitResolver::new(index([entry], []));

        assert_eq!(resolver.select([required("entry")]).expect("native terminal").units(), [digest(1)]);
    }

    #[test]
    fn repeated_roots_share_one_cached_selection() {
        let entry = unit(1, &[("entry", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[], &[]);
        let other = unit(2, &[("other", NativeSymbolBinding::Strong, NativeDefinitionSelection::Ordinary)], &[], &[]);
        let resolver = NativeUnitResolver::new(index([entry, other], []));
        let first = resolver.select([required("entry"), required("other"), required("entry")]).expect("first closure");
        let second = resolver.select([required("other"), required("entry")]).expect("cached closure");

        assert_eq!(first, second);
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(resolver.selections.lock().expect("test mutex").len(), 1);
    }
}
