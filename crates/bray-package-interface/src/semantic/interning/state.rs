use bray_symbols::{
    CallableInstanceId, ConstantTermId, ConstantValueId, DependencyContractTemplateId,
    GenericSubstitutionId, ImplementationInstanceId, SemanticValueStore, TraitApplicationId,
    TypeId,
};

use crate::InterfaceSemanticFacts;

use super::{ImportedSemanticFacts, InterfaceSemanticInternError, InterfaceSymbolResolver};

pub(super) struct InternState {
    pub(super) types: Vec<Option<TypeId>>,
    pub(super) constant_values: Vec<Option<ConstantValueId>>,
    pub(super) constant_terms: Vec<Option<ConstantTermId>>,
    pub(super) dependency_contracts: Vec<Option<DependencyContractTemplateId>>,
    pub(super) substitutions: Vec<Option<GenericSubstitutionId>>,
    pub(super) trait_applications: Vec<Option<TraitApplicationId>>,
    pub(super) callable_instances: Vec<Option<CallableInstanceId>>,
    pub(super) implementation_instances: Vec<Option<ImplementationInstanceId>>,
}

impl InternState {
    pub(super) fn new(facts: &InterfaceSemanticFacts) -> Self {
        Self {
            types: vec![None; facts.types.len()],
            constant_values: vec![None; facts.constant_values.len()],
            constant_terms: vec![None; facts.constant_terms.len()],
            dependency_contracts: vec![None; facts.dependency_contracts.len()],
            substitutions: vec![None; facts.substitutions.len()],
            trait_applications: vec![None; facts.trait_applications.len()],
            callable_instances: vec![None; facts.callable_instances.len()],
            implementation_instances: vec![None; facts.implementation_instances.len()],
        }
    }

    pub(super) fn from_imported(facts: &ImportedSemanticFacts) -> Self {
        Self {
            types: facts.types.iter().copied().map(Some).collect(),
            constant_values: facts.constant_values.iter().copied().map(Some).collect(),
            constant_terms: facts.constant_terms.iter().copied().map(Some).collect(),
            dependency_contracts: facts
                .dependency_contracts
                .iter()
                .copied()
                .map(Some)
                .collect(),
            substitutions: facts.substitutions.iter().copied().map(Some).collect(),
            trait_applications: facts.trait_applications.iter().copied().map(Some).collect(),
            callable_instances: facts.callable_instances.iter().copied().map(Some).collect(),
            implementation_instances: facts
                .implementation_instances
                .iter()
                .copied()
                .map(Some)
                .collect(),
        }
    }

    pub(super) fn has_pending(&self) -> bool {
        self.resolved_count() < self.total_count()
    }

    pub(super) fn resolved_count(&self) -> usize {
        self.types.iter().flatten().count()
            + self.constant_values.iter().flatten().count()
            + self.constant_terms.iter().flatten().count()
            + self.dependency_contracts.iter().flatten().count()
            + self.substitutions.iter().flatten().count()
            + self.trait_applications.iter().flatten().count()
            + self.callable_instances.iter().flatten().count()
            + self.implementation_instances.iter().flatten().count()
    }

    pub(super) fn total_count(&self) -> usize {
        self.types.len()
            + self.constant_values.len()
            + self.constant_terms.len()
            + self.dependency_contracts.len()
            + self.substitutions.len()
            + self.trait_applications.len()
            + self.callable_instances.len()
            + self.implementation_instances.len()
    }

    pub(super) fn intern_pass(
        &mut self,
        facts: &InterfaceSemanticFacts,
        store: &SemanticValueStore,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<(), InterfaceSemanticInternError> {
        self.intern_substitutions(facts, store, symbols)?;
        self.intern_dependency_contracts(facts, store, symbols)?;
        self.intern_trait_applications(facts, store, symbols)?;
        self.intern_callable_instances(facts, store, symbols)?;
        self.intern_implementation_instances(facts, store, symbols)?;
        self.intern_types(facts, store, symbols)?;
        self.intern_constant_values(facts, store, symbols)?;
        self.intern_constant_terms(facts, store, symbols)
    }
}
