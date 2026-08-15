use std::collections::BTreeMap;

use crate::semantic::model::{
    InterfaceCallableInstance, InterfaceCallableParameterDefault, InterfaceCallableSignature,
    InterfaceCoherenceRecord, InterfaceConstantTerm, InterfaceConstantValue, InterfaceConstraint,
    InterfaceDeclaredType, InterfaceDependencyContract, InterfaceGenericDeclaration,
    InterfaceGenericSubstitution, InterfaceImplementationInstance, InterfaceImplementationRecord,
    InterfaceRuntimeRequirement, InterfaceTargetPropertyDependency, InterfaceTraitApplication,
    InterfaceType,
};

pub(super) struct RecordSet<T> {
    values: BTreeMap<u32, T>,
}

impl<T> RecordSet<T> {
    pub(super) const fn new() -> Self {
        Self {
            values: BTreeMap::new(),
        }
    }

    pub(super) fn insert(&mut self, index: u32, value: T) {
        self.values.insert(index, value);
    }

    pub(super) const fn values(&self) -> &BTreeMap<u32, T> {
        &self.values
    }

    pub(super) fn into_values(self) -> impl Iterator<Item = T> {
        self.values.into_values()
    }
}

pub(super) struct SelectedRecords {
    pub(super) substitutions: RecordSet<InterfaceGenericSubstitution>,
    pub(super) trait_applications: RecordSet<InterfaceTraitApplication>,
    pub(super) callable_instances: RecordSet<InterfaceCallableInstance>,
    pub(super) implementation_instances: RecordSet<InterfaceImplementationInstance>,
    pub(super) types: RecordSet<InterfaceType>,
    pub(super) constant_values: RecordSet<InterfaceConstantValue>,
    pub(super) constant_terms: RecordSet<InterfaceConstantTerm>,
    pub(super) dependency_contracts: RecordSet<InterfaceDependencyContract>,
    pub(super) constraints: RecordSet<InterfaceConstraint>,
    pub(super) callable_signatures: RecordSet<InterfaceCallableSignature>,
    pub(super) generic_declarations: RecordSet<InterfaceGenericDeclaration>,
    pub(super) callable_parameter_defaults: RecordSet<InterfaceCallableParameterDefault>,
    pub(super) declared_types: RecordSet<InterfaceDeclaredType>,
    pub(super) implementations: RecordSet<InterfaceImplementationRecord>,
    pub(super) coherence: RecordSet<InterfaceCoherenceRecord>,
    pub(super) target_dependencies: RecordSet<InterfaceTargetPropertyDependency>,
    pub(super) runtime_requirements: RecordSet<InterfaceRuntimeRequirement>,
}

impl SelectedRecords {
    pub(super) const fn new() -> Self {
        Self {
            substitutions: RecordSet::new(),
            trait_applications: RecordSet::new(),
            callable_instances: RecordSet::new(),
            implementation_instances: RecordSet::new(),
            types: RecordSet::new(),
            constant_values: RecordSet::new(),
            constant_terms: RecordSet::new(),
            dependency_contracts: RecordSet::new(),
            constraints: RecordSet::new(),
            callable_signatures: RecordSet::new(),
            generic_declarations: RecordSet::new(),
            callable_parameter_defaults: RecordSet::new(),
            declared_types: RecordSet::new(),
            implementations: RecordSet::new(),
            coherence: RecordSet::new(),
            target_dependencies: RecordSet::new(),
            runtime_requirements: RecordSet::new(),
        }
    }
}
