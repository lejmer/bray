use bray_symbols::{
    CallableInstanceId, ConstantTermId, ConstantValueId, DependencyContractTemplateId,
    GenericSubstitutionId, ImplementationInstanceId, SemanticValueStore, TraitApplicationId,
    TypeId,
};

use crate::InterfaceSemantics;

use super::{ImportedSemantics, InterfaceSemanticInternError, InterfaceSymbolResolver};

pub(in crate::semantic) struct InternState {
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
    pub(super) fn new(semantics: &InterfaceSemantics) -> Self {
        Self {
            types: vec![None; semantics.types.len()],
            constant_values: vec![None; semantics.constant_values.len()],
            constant_terms: vec![None; semantics.constant_terms.len()],
            dependency_contracts: vec![None; semantics.dependency_contracts.len()],
            substitutions: vec![None; semantics.substitutions.len()],
            trait_applications: vec![None; semantics.trait_applications.len()],
            callable_instances: vec![None; semantics.callable_instances.len()],
            implementation_instances: vec![None; semantics.implementation_instances.len()],
        }
    }

    pub(super) fn from_imported(semantics: &ImportedSemantics) -> Self {
        Self {
            types: semantics.types.iter().copied().map(Some).collect(),
            constant_values: semantics
                .constant_values
                .iter()
                .copied()
                .map(Some)
                .collect(),
            constant_terms: semantics.constant_terms.iter().copied().map(Some).collect(),
            dependency_contracts: semantics
                .dependency_contracts
                .iter()
                .copied()
                .map(Some)
                .collect(),
            substitutions: semantics.substitutions.iter().copied().map(Some).collect(),
            trait_applications: semantics
                .trait_applications
                .iter()
                .copied()
                .map(Some)
                .collect(),
            callable_instances: semantics
                .callable_instances
                .iter()
                .copied()
                .map(Some)
                .collect(),
            implementation_instances: semantics
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
        semantics: &InterfaceSemantics,
        store: &SemanticValueStore,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<(), InterfaceSemanticInternError> {
        self.intern_substitutions(semantics, store, symbols)?;
        self.intern_dependency_contracts(semantics, store, symbols)?;
        self.intern_trait_applications(semantics, store, symbols)?;
        self.intern_callable_instances(semantics, store, symbols)?;
        self.intern_implementation_instances(semantics, store, symbols)?;
        self.intern_types(semantics, store, symbols)?;
        self.intern_constant_values(semantics, store, symbols)?;

        self.intern_constant_terms(semantics, store, symbols)
    }
}
