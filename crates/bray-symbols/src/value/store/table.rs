use std::{collections::HashMap, sync::Arc};

use super::super::SemanticValueKind;
use super::super::{
    CallableInstanceData, CallableInstanceId, ConstantTermData, ConstantTermId, ConstantValueData,
    ConstantValueId, DependencyContractTemplateData, DependencyContractTemplateId,
    GenericSubstitutionData, GenericSubstitutionId, ImplementationInstanceData,
    ImplementationInstanceId, SemanticValueStoreError, SemanticValueStoreId, TraitApplicationData,
    TraitApplicationId, TypeData, TypeId,
    id::{InternedValueId, SemanticValueId},
};

pub(super) struct InternTable<T, I> {
    entries: HashMap<SemanticValueId, Arc<T>>,
    indices: HashMap<Arc<T>, SemanticValueId>,
    next_slot: u32,
    marker: std::marker::PhantomData<I>,
}

impl<T, I> InternTable<T, I>
where
    T: Eq + std::hash::Hash,
    I: InternedValueId,
{
    pub(super) fn new() -> Self {
        Self {
            entries: HashMap::new(),
            indices: HashMap::new(),
            next_slot: 0,
            marker: std::marker::PhantomData,
        }
    }
}

impl<T, I> Clone for InternTable<T, I> {
    fn clone(&self) -> Self {
        // Copy-on-write forks retain immutable values while owning independent indices.
        Self {
            entries: self.entries.clone(),
            indices: self.indices.clone(),
            next_slot: self.next_slot,
            marker: std::marker::PhantomData,
        }
    }
}

impl<T, I> InternTable<T, I>
where
    T: Eq + std::hash::Hash,
    I: InternedValueId,
{
    pub(super) fn intern(
        &mut self,
        store: SemanticValueStoreId,
        data: T,
    ) -> Result<I, SemanticValueStoreError> {
        if let Some(id) = self.indices.get(&data) {
            return Ok(I::from_value_id(*id));
        }

        let slot = self.next_slot;

        self.next_slot = slot
            .checked_add(1)
            .ok_or(SemanticValueStoreError::CapacityExhausted { kind: I::KIND })?;

        let data = Arc::new(data);
        let id = SemanticValueId::new(store, slot);

        // The identity table and structural index retain the same immutable allocation.
        self.entries.insert(id, Arc::clone(&data));
        self.indices.insert(data, id);

        Ok(I::from_value_id(id))
    }

    pub(super) fn get(
        &self,
        store: SemanticValueStoreId,
        id: I,
    ) -> Result<&T, SemanticValueStoreError> {
        let id = id.value_id();

        self.entries
            .get(&id)
            .map(AsRef::as_ref)
            .ok_or_else(|| missing_id(store, id, I::KIND))
    }

    pub(super) fn get_shared(
        &self,
        store: SemanticValueStoreId,
        id: I,
    ) -> Result<Arc<T>, SemanticValueStoreError> {
        let id = id.value_id();

        self.entries
            .get(&id)
            .map(Arc::clone)
            .ok_or_else(|| missing_id(store, id, I::KIND))
    }
}

fn missing_id(
    expected: SemanticValueStoreId,
    id: SemanticValueId,
    kind: SemanticValueKind,
) -> SemanticValueStoreError {
    let actual = id.store();

    if actual != expected {
        return SemanticValueStoreError::ForeignId { expected, actual };
    }

    SemanticValueStoreError::UnknownId { kind }
}

#[derive(Clone)]
pub(super) struct SemanticTables {
    pub(super) types: Arc<InternTable<TypeData, TypeId>>,
    pub(super) constant_values: Arc<InternTable<ConstantValueData, ConstantValueId>>,
    pub(super) constant_terms: Arc<InternTable<ConstantTermData, ConstantTermId>>,
    pub(super) substitutions: Arc<InternTable<GenericSubstitutionData, GenericSubstitutionId>>,
    pub(super) trait_applications: Arc<InternTable<TraitApplicationData, TraitApplicationId>>,
    pub(super) callable_instances: Arc<InternTable<CallableInstanceData, CallableInstanceId>>,
    pub(super) implementation_instances:
        Arc<InternTable<ImplementationInstanceData, ImplementationInstanceId>>,
    pub(super) dependency_contracts:
        Arc<InternTable<DependencyContractTemplateData, DependencyContractTemplateId>>,
}

impl SemanticTables {
    pub(super) fn new() -> Self {
        Self {
            types: Arc::new(InternTable::new()),
            constant_values: Arc::new(InternTable::new()),
            constant_terms: Arc::new(InternTable::new()),
            substitutions: Arc::new(InternTable::new()),
            trait_applications: Arc::new(InternTable::new()),
            callable_instances: Arc::new(InternTable::new()),
            implementation_instances: Arc::new(InternTable::new()),
            dependency_contracts: Arc::new(InternTable::new()),
        }
    }
}
