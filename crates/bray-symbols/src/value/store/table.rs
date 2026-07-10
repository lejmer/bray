use std::{collections::HashMap, sync::Arc};

use super::super::{
    CallableInstanceData, CallableInstanceId, ConstantTermData, ConstantTermId, ConstantValueData,
    ConstantValueId, DependencyContractTemplateData, DependencyContractTemplateId,
    GenericSubstitutionData, GenericSubstitutionId, ImplementationInstanceData,
    ImplementationInstanceId, SemanticValueStoreError, SemanticValueStoreId, TraitApplicationData,
    TraitApplicationId, TypeData, TypeId,
    id::{InternedValueId, SemanticValueId},
};

pub(super) struct InternTable<T, I> {
    entries: Vec<Arc<T>>,
    indices: HashMap<Arc<T>, u32>,
    marker: std::marker::PhantomData<I>,
}

impl<T, I> InternTable<T, I>
where
    T: Eq + std::hash::Hash,
    I: InternedValueId,
{
    pub(super) fn new() -> Self {
        Self {
            entries: Vec::new(),
            indices: HashMap::new(),
            marker: std::marker::PhantomData,
        }
    }

    pub(super) fn intern(
        &mut self,
        store: SemanticValueStoreId,
        data: T,
    ) -> Result<I, SemanticValueStoreError> {
        if let Some(slot) = self.indices.get(&data) {
            return Ok(I::from_value_id(SemanticValueId::new(store, *slot)));
        }

        let slot = u32::try_from(self.entries.len())
            .map_err(|_| SemanticValueStoreError::CapacityExhausted { kind: I::KIND })?;
        let data = Arc::new(data);
        self.entries.push(Arc::clone(&data));
        self.indices.insert(data, slot);

        Ok(I::from_value_id(SemanticValueId::new(store, slot)))
    }

    pub(super) fn get(
        &self,
        store: SemanticValueStoreId,
        id: I,
    ) -> Result<&T, SemanticValueStoreError> {
        let id = id.value_id();
        validate_store(store, id)?;
        let Some(index) = id.to_index() else {
            return Err(SemanticValueStoreError::UnknownId { kind: I::KIND });
        };
        self.entries
            .get(index)
            .map(AsRef::as_ref)
            .ok_or(SemanticValueStoreError::UnknownId { kind: I::KIND })
    }

    pub(super) fn get_shared(
        &self,
        store: SemanticValueStoreId,
        id: I,
    ) -> Result<Arc<T>, SemanticValueStoreError> {
        let id = id.value_id();
        validate_store(store, id)?;
        let Some(index) = id.to_index() else {
            return Err(SemanticValueStoreError::UnknownId { kind: I::KIND });
        };
        self.entries
            .get(index)
            .map(Arc::clone)
            .ok_or(SemanticValueStoreError::UnknownId { kind: I::KIND })
    }
}

fn validate_store(
    expected: SemanticValueStoreId,
    id: SemanticValueId,
) -> Result<(), SemanticValueStoreError> {
    let actual = id.store();
    if actual != expected {
        return Err(SemanticValueStoreError::ForeignId { expected, actual });
    }

    Ok(())
}

pub(super) struct SemanticTables {
    pub(super) types: InternTable<TypeData, TypeId>,
    pub(super) constant_values: InternTable<ConstantValueData, ConstantValueId>,
    pub(super) constant_terms: InternTable<ConstantTermData, ConstantTermId>,
    pub(super) substitutions: InternTable<GenericSubstitutionData, GenericSubstitutionId>,
    pub(super) trait_applications: InternTable<TraitApplicationData, TraitApplicationId>,
    pub(super) callable_instances: InternTable<CallableInstanceData, CallableInstanceId>,
    pub(super) implementation_instances:
        InternTable<ImplementationInstanceData, ImplementationInstanceId>,
    pub(super) dependency_contracts:
        InternTable<DependencyContractTemplateData, DependencyContractTemplateId>,
}

impl SemanticTables {
    pub(super) fn new() -> Self {
        Self {
            types: InternTable::new(),
            constant_values: InternTable::new(),
            constant_terms: InternTable::new(),
            substitutions: InternTable::new(),
            trait_applications: InternTable::new(),
            callable_instances: InternTable::new(),
            implementation_instances: InternTable::new(),
            dependency_contracts: InternTable::new(),
        }
    }
}
