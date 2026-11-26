use std::collections::BTreeMap;

use super::model::{RecordSet, SelectedRecords};
use crate::InterfaceValidationError;
use crate::semantic::model::{
    InterfaceCallableInstanceId, InterfaceConstantTermId, InterfaceConstantValueId,
    InterfaceDependencyContractId, InterfaceGenericSubstitutionId,
    InterfaceImplementationInstanceId, InterfaceTraitApplicationId, InterfaceTypeId,
};

pub(super) struct RecordMaps {
    substitutions: BTreeMap<u32, u32>,
    trait_applications: BTreeMap<u32, u32>,
    callable_instances: BTreeMap<u32, u32>,
    implementation_instances: BTreeMap<u32, u32>,
    types: BTreeMap<u32, u32>,
    constant_values: BTreeMap<u32, u32>,
    constant_terms: BTreeMap<u32, u32>,
    dependency_contracts: BTreeMap<u32, u32>,
    checked_templates: BTreeMap<u32, u32>,
    support_entities: BTreeMap<u32, u32>,
}

impl RecordMaps {
    pub(super) fn new(records: &SelectedRecords) -> Result<Self, InterfaceValidationError> {
        Ok(Self {
            substitutions: record_map(&records.substitutions)?,
            trait_applications: record_map(&records.trait_applications)?,
            callable_instances: record_map(&records.callable_instances)?,
            implementation_instances: record_map(&records.implementation_instances)?,
            types: record_map(&records.types)?,
            constant_values: record_map(&records.constant_values)?,
            constant_terms: record_map(&records.constant_terms)?,
            dependency_contracts: record_map(&records.dependency_contracts)?,
            checked_templates: record_map(&records.checked_templates)?,
            support_entities: record_map(&records.support_entities)?,
        })
    }

    pub(super) fn substitution_id(
        &self,
        id: InterfaceGenericSubstitutionId,
    ) -> Result<InterfaceGenericSubstitutionId, InterfaceValidationError> {
        remap_id(
            &self.substitutions,
            id.raw(),
            InterfaceGenericSubstitutionId::new,
        )
    }

    pub(super) fn trait_application_id(
        &self,
        id: InterfaceTraitApplicationId,
    ) -> Result<InterfaceTraitApplicationId, InterfaceValidationError> {
        remap_id(
            &self.trait_applications,
            id.raw(),
            InterfaceTraitApplicationId::new,
        )
    }

    pub(super) fn callable_instance_id(
        &self,
        id: InterfaceCallableInstanceId,
    ) -> Result<InterfaceCallableInstanceId, InterfaceValidationError> {
        remap_id(
            &self.callable_instances,
            id.raw(),
            InterfaceCallableInstanceId::new,
        )
    }

    pub(super) fn implementation_instance_id(
        &self,
        id: InterfaceImplementationInstanceId,
    ) -> Result<InterfaceImplementationInstanceId, InterfaceValidationError> {
        remap_id(
            &self.implementation_instances,
            id.raw(),
            InterfaceImplementationInstanceId::new,
        )
    }

    pub(super) fn type_id(
        &self,
        id: InterfaceTypeId,
    ) -> Result<InterfaceTypeId, InterfaceValidationError> {
        remap_id(&self.types, id.raw(), InterfaceTypeId::new)
    }

    pub(super) fn constant_value_id(
        &self,
        id: InterfaceConstantValueId,
    ) -> Result<InterfaceConstantValueId, InterfaceValidationError> {
        remap_id(
            &self.constant_values,
            id.raw(),
            InterfaceConstantValueId::new,
        )
    }

    pub(super) fn constant_term_id(
        &self,
        id: InterfaceConstantTermId,
    ) -> Result<InterfaceConstantTermId, InterfaceValidationError> {
        remap_id(&self.constant_terms, id.raw(), InterfaceConstantTermId::new)
    }

    pub(super) fn dependency_contract_id(
        &self,
        id: InterfaceDependencyContractId,
    ) -> Result<InterfaceDependencyContractId, InterfaceValidationError> {
        remap_id(
            &self.dependency_contracts,
            id.raw(),
            InterfaceDependencyContractId::new,
        )
    }
}

impl RecordMaps {
    pub(super) fn checked_template_id(
        &self,
        id: crate::semantic::model::InterfaceCheckedTemplateId,
    ) -> Result<crate::semantic::model::InterfaceCheckedTemplateId, InterfaceValidationError> {
        remap_id(&self.checked_templates, id.raw(), crate::semantic::model::InterfaceCheckedTemplateId::new)
    }

    pub(super) fn support_entity_id(
        &self,
        id: bray_symbols::InterfaceSupportEntityId,
    ) -> Result<bray_symbols::InterfaceSupportEntityId, InterfaceValidationError> {
        remap_id(&self.support_entities, id.raw(), bray_symbols::InterfaceSupportEntityId::new)
    }
}

/// Renumbers one selected template into the selected graph.
impl crate::semantic::model::InterfaceTemplateReferenceMap for &RecordMaps {
    type Error = InterfaceValidationError;

    fn term(&mut self, term: InterfaceConstantTermId) -> Result<InterfaceConstantTermId, Self::Error> {
        self.constant_term_id(term)
    }

    fn ty(&mut self, ty: InterfaceTypeId) -> Result<InterfaceTypeId, Self::Error> {
        self.type_id(ty)
    }

    fn substitution(&mut self, substitution: InterfaceGenericSubstitutionId) -> Result<InterfaceGenericSubstitutionId, Self::Error> {
        self.substitution_id(substitution)
    }

    fn dependency(&mut self, contract: InterfaceDependencyContractId) -> Result<InterfaceDependencyContractId, Self::Error> {
        self.dependency_contract_id(contract)
    }

    fn support(&mut self, entity: bray_symbols::InterfaceSupportEntityId) -> Result<bray_symbols::InterfaceSupportEntityId, Self::Error> {
        self.support_entity_id(entity)
    }
}

fn record_map<T>(records: &RecordSet<T>) -> Result<BTreeMap<u32, u32>, InterfaceValidationError> {
    records
        .values()
        .keys()
        .copied()
        .enumerate()
        .map(|(new, old)| {
            let new = u32::try_from(new).map_err(|_| {
                crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Index)
            })?;

            Ok((old, new))
        })
        .collect()
}

fn remap_id<T>(
    map: &BTreeMap<u32, u32>,
    old: u32,
    constructor: fn(u32) -> T,
) -> Result<T, InterfaceValidationError> {
    map.get(&old)
        .copied()
        .map(constructor)
        .ok_or(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Index,
        ))
}
