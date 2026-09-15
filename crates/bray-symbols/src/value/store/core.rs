use std::sync::{
    Arc, Mutex, MutexGuard,
    atomic::{AtomicU64, Ordering},
};

use crate::{
    GenericArgument, GenericOwnerId, GenericParameterSymbolId, NamedTypeSymbolId, SymbolGraph,
};

use super::{
    super::{
        CallableInstanceData, CallableInstanceId, ConstantTermData, ConstantTermId,
        ConstantValueData, ConstantValueId, ConstantValueKind, DependencyContractTemplateData,
        DependencyContractTemplateId, GenericSubstitutionData, GenericSubstitutionId,
        ImplementationInstanceData, ImplementationInstanceId, IntegerConstant,
        SemanticValueStoreCreateError, SemanticValueStoreError, SemanticValueStoreId,
        TraitApplicationData, TraitApplicationId, TypeData, TypeId,
    },
    table::SemanticTables,
};

#[cfg(test)]
use super::validation::{
    validate_callable_instance_data, validate_constant_term_data, validate_constant_value_data,
    validate_dependency_template_data, validate_implementation_instance_data,
    validate_substitution_data, validate_trait_application_data, validate_type_data,
};

static NEXT_STORE_ID: AtomicU64 = AtomicU64::new(1);

/// A thread-safe canonical store for immutable semantic values.
///
/// Values are structurally interned. Equal data returns one exact typed ID within this store.
/// A fork accepts inherited IDs while remaining independent from subsequent parent mutations.
/// Callers supply well-formed values referencing IDs issued here or inherited when forking.
/// Looking up an unknown ID panics because it violates that compiler contract.
pub struct SemanticValueStore {
    id: SemanticValueStoreId,
    tables: Mutex<SemanticTables>,
}

impl SemanticValueStore {
    /// Returns the type beneath every borrow layer without granting access authority.
    pub fn unborrowed_type(&self, mut ty: TypeId) -> TypeId {
        loop {
            let data = self.type_data(ty);

            let TypeData::Borrow { target, .. } = data.as_ref() else {
                return ty;
            };

            ty = *target;
        }
    }

    /// Interns a named type using its declaration's generic parameters as open arguments.
    pub fn intern_open_named_type(
        &self,
        symbols: &SymbolGraph,
        definition: NamedTypeSymbolId,
    ) -> Result<Option<TypeId>, SemanticValueStoreError> {
        let owner = definition.into_any();

        let (type_parameters, const_parameters) = match definition {
            NamedTypeSymbolId::Struct(id) => {
                let Some(symbol) = symbols.structure(id) else {
                    return Ok(None);
                };

                (
                    symbol.generic_type_parameters(),
                    symbol.generic_const_parameters(),
                )
            }
            NamedTypeSymbolId::Union(id) => {
                let Some(symbol) = symbols.union(id) else {
                    return Ok(None);
                };

                (
                    symbol.generic_type_parameters(),
                    symbol.generic_const_parameters(),
                )
            }
        };

        let mut parameters = type_parameters
            .iter()
            .copied()
            .map(GenericParameterSymbolId::Type)
            .chain(
                const_parameters
                    .iter()
                    .copied()
                    .map(GenericParameterSymbolId::Const),
            )
            .collect::<Vec<_>>();

        parameters.sort_by_key(|parameter| match parameter {
            GenericParameterSymbolId::Type(parameter) => symbols
                .generic_type_parameter(*parameter)
                .map(|parameter| parameter.ordinal()),
            GenericParameterSymbolId::Const(parameter) => symbols
                .generic_const_parameter(*parameter)
                .map(|parameter| parameter.ordinal()),
        });

        let arguments = parameters
            .iter()
            .copied()
            .map(|parameter| self.intern_generic_parameter_argument(parameter))
            .collect::<Result<Vec<_>, _>>()?;

        let Some(owner) = super::super::GenericOwnerId::try_new(owner) else {
            return Ok(None);
        };

        let Ok(substitution) = GenericSubstitutionData::try_new(owner, parameters, arguments)
        else {
            return Ok(None);
        };

        let substitution = self.intern_generic_substitution(substitution)?;

        let ty = self.intern_type(TypeData::Named {
            definition,
            substitution,
        })?;

        Ok(Some(ty))
    }

    /// Interns a named type whose declaration has no generic parameters.
    pub fn intern_non_generic_named_type(
        &self,
        definition: NamedTypeSymbolId,
    ) -> Result<TypeId, SemanticValueStoreError> {
        let Some(owner) = super::super::GenericOwnerId::try_new(definition.into_any()) else {
            unreachable!("named type symbols are generic owners");
        };

        let substitution = match GenericSubstitutionData::try_new(
            owner,
            std::iter::empty::<GenericParameterSymbolId>(),
            std::iter::empty::<super::super::GenericArgument>(),
        ) {
            Ok(substitution) => substitution,
            Err(_) => unreachable!("empty substitutions are valid for non-generic named types"),
        };

        let substitution = self.intern_generic_substitution(substitution)?;

        self.intern_type(TypeData::Named {
            definition,
            substitution,
        })
    }

    /// Creates an empty semantic value store with a process-unique checking identity.
    pub fn try_new() -> Result<Self, SemanticValueStoreCreateError> {
        Ok(Self {
            id: next_store_id()?,
            tables: Mutex::new(SemanticTables::new()),
        })
    }

    /// Creates an independent store that can read values inherited from this store.
    pub fn fork(&self) -> Result<Self, SemanticValueStoreCreateError> {
        let tables = self.tables();

        Ok(Self {
            id: next_store_id()?,
            // Each semantic table remains shared until the child changes that value category.
            tables: Mutex::new(tables.clone()),
        })
    }

    /// Returns the identity used for values first interned by this store.
    pub const fn id(&self) -> SemanticValueStoreId {
        self.id
    }

    /// Interns one canonical semantic type.
    pub fn intern_type(&self, data: TypeData) -> Result<TypeId, SemanticValueStoreError> {
        let mut tables = self.tables();

        #[cfg(test)]
        validate_type_data(&tables, self.id, &data);

        Arc::make_mut(&mut tables.types).intern(self.id, data)
    }

    /// Interns the open semantic argument represented by one generic parameter.
    pub fn intern_generic_parameter_argument(
        &self,
        parameter: GenericParameterSymbolId,
    ) -> Result<GenericArgument, SemanticValueStoreError> {
        match parameter {
            GenericParameterSymbolId::Type(parameter) => self
                .intern_type(TypeData::TypeParameter(parameter))
                .map(GenericArgument::Type),
            GenericParameterSymbolId::Const(parameter) => self
                .intern_constant_term(ConstantTermData::Parameter(parameter))
                .map(GenericArgument::Constant),
        }
    }

    /// Returns immutable data for a type issued by this store.
    pub fn type_data(&self, id: TypeId) -> Arc<TypeData> {
        self.tables().types.get_shared(self.id, id)
    }

    /// Interns one canonical fully evaluated constant value.
    pub fn intern_constant_value(
        &self,
        data: ConstantValueData,
    ) -> Result<ConstantValueId, SemanticValueStoreError> {
        let mut tables = self.tables();

        #[cfg(test)]
        validate_constant_value_data(&tables, self.id, &data);

        Arc::make_mut(&mut tables.constant_values).intern(self.id, data)
    }

    /// Interns the recovery constant value for one semantic type.
    pub fn intern_error_constant_value(
        &self,
        ty: TypeId,
    ) -> Result<ConstantValueId, SemanticValueStoreError> {
        self.intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Error))
    }

    /// Returns immutable data for a constant value issued by this store.
    pub fn constant_value_data(&self, id: ConstantValueId) -> Arc<ConstantValueData> {
        self.tables().constant_values.get_shared(self.id, id)
    }

    /// Interns one canonical checked constant term.
    pub fn intern_constant_term(
        &self,
        data: ConstantTermData,
    ) -> Result<ConstantTermId, SemanticValueStoreError> {
        let mut tables = self.tables();

        #[cfg(test)]
        validate_constant_term_data(&tables, self.id, &data);

        Arc::make_mut(&mut tables.constant_terms).intern(self.id, data)
    }

    /// Returns immutable data for a constant term issued by this store.
    pub fn constant_term_data(&self, id: ConstantTermId) -> Arc<ConstantTermData> {
        self.tables().constant_terms.get_shared(self.id, id)
    }

    /// Returns the integer represented by a checked constant term when it is already known.
    pub fn constant_term_integer(&self, id: ConstantTermId) -> Option<IntegerConstant> {
        let data = self.constant_term_data(id);

        match data.as_ref() {
            ConstantTermData::Typed { term, .. } => self.constant_term_integer(*term),
            ConstantTermData::IntegerLiteral { value, .. } => Some(value.clone()),
            ConstantTermData::Value(value) => {
                let value = self.constant_value_data(*value);

                let ConstantValueKind::Integer(value) = value.kind() else {
                    return None;
                };

                Some(value.clone())
            }
            _ => None,
        }
    }

    /// Interns one canonical ordered generic substitution.
    pub fn intern_generic_substitution(
        &self,
        data: GenericSubstitutionData,
    ) -> Result<GenericSubstitutionId, SemanticValueStoreError> {
        let mut tables = self.tables();

        #[cfg(test)]
        validate_substitution_data(&tables, self.id, &data);

        Arc::make_mut(&mut tables.substitutions).intern(self.id, data)
    }

    /// Returns immutable data for a generic substitution issued by this store.
    pub fn generic_substitution_data(
        &self,
        id: GenericSubstitutionId,
    ) -> Arc<GenericSubstitutionData> {
        self.tables().substitutions.get_shared(self.id, id)
    }

    /// Interns the same inherited generic bindings for a declaration that consumes them.
    pub fn inherit_generic_substitution(
        &self,
        id: GenericSubstitutionId,
        owner: GenericOwnerId,
    ) -> Result<GenericSubstitutionId, SemanticValueStoreError> {
        let substitution = self.generic_substitution_data(id);

        self.intern_generic_substitution(substitution.with_owner(owner))
    }

    /// Returns whether all arguments are closed values without unresolved generic types.
    pub fn substitution_is_concrete(&self, id: GenericSubstitutionId) -> bool {
        super::concrete::substitution_is_concrete(&self.tables(), self.id, id)
    }

    /// Interns one canonical trait application.
    pub fn intern_trait_application(
        &self,
        data: TraitApplicationData,
    ) -> Result<TraitApplicationId, SemanticValueStoreError> {
        let mut tables = self.tables();

        #[cfg(test)]
        validate_trait_application_data(&tables, self.id, data);

        Arc::make_mut(&mut tables.trait_applications).intern(self.id, data)
    }

    /// Returns immutable data for a trait application issued by this store.
    pub fn trait_application_data(&self, id: TraitApplicationId) -> Arc<TraitApplicationData> {
        self.tables().trait_applications.get_shared(self.id, id)
    }

    /// Interns one canonical substituted callable definition.
    pub fn intern_callable_instance(
        &self,
        data: CallableInstanceData,
    ) -> Result<CallableInstanceId, SemanticValueStoreError> {
        let mut tables = self.tables();

        #[cfg(test)]
        validate_callable_instance_data(&tables, self.id, data);

        Arc::make_mut(&mut tables.callable_instances).intern(self.id, data)
    }

    /// Returns immutable data for a callable instance issued by this store.
    pub fn callable_instance_data(&self, id: CallableInstanceId) -> Arc<CallableInstanceData> {
        self.tables().callable_instances.get_shared(self.id, id)
    }

    /// Interns one canonical selected implementation instance.
    pub fn intern_implementation_instance(
        &self,
        data: ImplementationInstanceData,
    ) -> Result<ImplementationInstanceId, SemanticValueStoreError> {
        let mut tables = self.tables();

        #[cfg(test)]
        validate_implementation_instance_data(&tables, self.id, data);

        Arc::make_mut(&mut tables.implementation_instances).intern(self.id, data)
    }

    /// Returns immutable data for an implementation instance issued by this store.
    pub fn implementation_instance_data(
        &self,
        id: ImplementationInstanceId,
    ) -> Arc<ImplementationInstanceData> {
        self.tables()
            .implementation_instances
            .get_shared(self.id, id)
    }

    /// Interns one normalized portable dependency-contract template.
    pub fn intern_dependency_contract_template(
        &self,
        data: DependencyContractTemplateData,
    ) -> Result<DependencyContractTemplateId, SemanticValueStoreError> {
        let mut tables = self.tables();

        #[cfg(test)]
        validate_dependency_template_data(&tables, self.id, &data);

        Arc::make_mut(&mut tables.dependency_contracts).intern(self.id, data)
    }

    /// Returns the dependency-contract template with no requirements.
    pub fn empty_dependency_contract_template(
        &self,
    ) -> Result<DependencyContractTemplateId, SemanticValueStoreError> {
        self.intern_dependency_contract_template(DependencyContractTemplateData::new([]))
    }

    /// Returns immutable data for a dependency-contract template issued by this store.
    pub fn dependency_contract_template_data(
        &self,
        id: DependencyContractTemplateId,
    ) -> Arc<DependencyContractTemplateData> {
        self.tables().dependency_contracts.get_shared(self.id, id)
    }

    fn tables(&self) -> MutexGuard<'_, SemanticTables> {
        // Interned entries and their structural index are published under the same lock.
        self.tables
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn next_store_id() -> Result<SemanticValueStoreId, SemanticValueStoreCreateError> {
    let raw = NEXT_STORE_ID
        .try_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
            current.checked_add(1)
        })
        .map_err(|_| SemanticValueStoreCreateError::IdentitySpaceExhausted)?;

    Ok(SemanticValueStoreId::new(raw))
}

#[cfg(test)]
mod tests {
    use std::{sync::Arc, thread};

    use super::SemanticValueStore;
    use crate::{
        AnySymbolId, CallableDefinitionId, CallableInstanceData, ConstantBinaryOperation,
        ConstantTermData, ConstantValueData, ConstantValueKind, DependencyContractTemplateData,
        DependencyRequirement, DependencyRequirementKind, DependencySubject, DependencySubjectRoot,
        FunctionSymbolId, GenericArgument, GenericConstParameterSymbolId, GenericOwnerId,
        GenericParameterSymbolId, GenericSubstitutionData, GenericTypeParameterSymbolId,
        ImplementationInstanceData, ImplementationSymbolId, InherentImplementationSymbolId,
        IntegerConstant, IntegerSign, NamedTypeSymbolId, StructSymbolId, SymbolId, SymbolOrdinal,
        TraitApplicationData, TraitSymbolId, TypeData,
    };

    fn store() -> SemanticValueStore {
        match SemanticValueStore::try_new() {
            Ok(store) => store,
            Err(error) => panic!("semantic store creation failed: {error:?}"),
        }
    }

    fn generic_owner(symbol: AnySymbolId) -> GenericOwnerId {
        match GenericOwnerId::try_new(symbol) {
            Some(owner) => owner,
            None => panic!("test symbol must support generic substitutions"),
        }
    }

    fn empty_substitution(
        store: &SemanticValueStore,
        symbol: AnySymbolId,
    ) -> crate::GenericSubstitutionId {
        let data = GenericSubstitutionData::try_new(
            generic_owner(symbol),
            std::iter::empty::<GenericParameterSymbolId>(),
            std::iter::empty::<GenericArgument>(),
        );

        let data = match data {
            Ok(data) => data,
            Err(error) => panic!("empty substitution construction failed: {error:?}"),
        };

        match store.intern_generic_substitution(data) {
            Ok(id) => id,
            Err(error) => panic!("empty substitution interning failed: {error:?}"),
        }
    }

    fn concrete_named_type(store: &SemanticValueStore, raw: u32) -> crate::TypeId {
        let definition = StructSymbolId::from_symbol_id(SymbolId::new(raw));

        let substitution = empty_substitution(store, definition.into());

        let data = TypeData::Named {
            definition: NamedTypeSymbolId::from(definition),
            substitution,
        };

        match store.intern_type(data) {
            Ok(id) => id,
            Err(error) => panic!("named type interning failed: {error:?}"),
        }
    }

    #[test]
    fn equal_values_reuse_one_id() {
        let store = store();
        let first = store.intern_type(TypeData::Error);
        let second = store.intern_type(TypeData::Error);

        assert_eq!(first, second);
    }

    #[test]
    fn typed_integer_terms_preserve_known_integer_queries() {
        let store = store();
        let ty = concrete_named_type(&store, 1);
        let integer = IntegerConstant::new(IntegerSign::NonNegative, [42]);

        let term = store
            .intern_constant_term(ConstantTermData::IntegerLiteral {
                ty: crate::TargetSizedIntegerType::Usize,
                value: integer.clone(),
            })
            .unwrap_or_else(|error| panic!("integer term must intern: {error:?}"));

        let typed = store
            .intern_constant_term(ConstantTermData::typed(term, ty))
            .unwrap_or_else(|error| panic!("typed integer term must intern: {error:?}"));

        assert_eq!(store.constant_term_integer(typed), Some(integer));
    }

    #[test]
    fn foreign_ids_are_rejected() {
        let first = store();
        let second = store();

        let Ok(id) = first.intern_type(TypeData::Error) else {
            panic!("error type interning must succeed");
        };

        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = second.type_data(id);
            }))
            .is_err()
        );
    }

    #[test]
    fn forked_stores_share_inherited_values_without_sharing_mutations() {
        let parent = store();

        let Ok(inherited) = parent.intern_type(TypeData::Error) else {
            panic!("inherited type must intern");
        };

        let child = match parent.fork() {
            Ok(child) => child,
            Err(error) => panic!("semantic store fork failed: {error:?}"),
        };

        assert_eq!(child.type_data(inherited).as_ref(), &TypeData::Error);

        let parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(9));

        let Ok(child_value) = child.intern_type(TypeData::TypeParameter(parameter)) else {
            panic!("child type must intern");
        };

        assert_eq!(child_value.store_id(), child.id());

        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = parent.type_data(child_value);
            }))
            .is_err()
        );
    }

    #[test]
    fn foreign_references_are_rejected_before_interning() {
        let first = store();
        let second = store();

        let Ok(foreign) = first.intern_type(TypeData::Error) else {
            panic!("error type interning must succeed");
        };

        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = second.intern_type(TypeData::Slice(foreign));
            }))
            .is_err()
        );
    }

    #[test]
    fn every_semantic_table_uses_structural_canonicalization() {
        let store = store();

        let dependency = DependencyContractTemplateData::new([]);
        let first_dependency = store.intern_dependency_contract_template(dependency.clone());
        let second_dependency = store.intern_dependency_contract_template(dependency);

        assert_eq!(first_dependency, second_dependency);

        let ty = concrete_named_type(&store, 10);

        let value = ConstantValueData::new(ty, ConstantValueKind::Boolean(true));
        let first_value = store.intern_constant_value(value.clone());
        let second_value = store.intern_constant_value(value);

        assert_eq!(first_value, second_value);

        let Ok(value) = first_value else {
            panic!("constant value interning must succeed");
        };

        let term = ConstantTermData::Value(value);

        assert_eq!(
            store.intern_constant_term(term.clone()),
            store.intern_constant_term(term)
        );

        let trait_definition = TraitSymbolId::from_symbol_id(SymbolId::new(11));
        let trait_substitution = empty_substitution(&store, trait_definition.into());

        let application = TraitApplicationData::new(trait_definition, trait_substitution);

        assert_eq!(
            store.intern_trait_application(application),
            store.intern_trait_application(application)
        );

        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(12));

        let callable = match CallableDefinitionId::try_new(function.into()) {
            Some(callable) => callable,
            None => panic!("function must be a callable definition"),
        };

        let callable_substitution = empty_substitution(&store, function.into());
        let callable = CallableInstanceData::new(callable, callable_substitution);

        assert_eq!(
            store.intern_callable_instance(callable),
            store.intern_callable_instance(callable)
        );

        let implementation = InherentImplementationSymbolId::from_symbol_id(SymbolId::new(13));
        let implementation_substitution = empty_substitution(&store, implementation.into());

        let implementation = ImplementationInstanceData::new(
            ImplementationSymbolId::from(implementation),
            implementation_substitution,
        );

        assert_eq!(
            store.intern_implementation_instance(implementation),
            store.intern_implementation_instance(implementation)
        );

        let data = TypeData::Slice(ty);

        assert_eq!(store.intern_type(data.clone()), store.intern_type(data));
    }

    #[test]
    fn concrete_substitutions_are_distinct_from_open_substitutions() {
        let store = store();

        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(20));
        let owner = generic_owner(function.into());

        let type_parameter = GenericTypeParameterSymbolId::from_symbol_id(SymbolId::new(21));
        let const_parameter = GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(22));

        let ty = concrete_named_type(&store, 23);

        let value = match store
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Boolean(true)))
        {
            Ok(value) => value,
            Err(error) => panic!("constant value interning failed: {error:?}"),
        };

        let term = match store.intern_constant_term(ConstantTermData::Value(value)) {
            Ok(term) => term,
            Err(error) => panic!("constant term interning failed: {error:?}"),
        };

        let concrete_data = GenericSubstitutionData::try_new(
            owner,
            [
                GenericParameterSymbolId::from(type_parameter),
                GenericParameterSymbolId::from(const_parameter),
            ],
            [GenericArgument::Type(ty), GenericArgument::Constant(term)],
        );

        let concrete_data = match concrete_data {
            Ok(data) => data,
            Err(error) => panic!("concrete substitution construction failed: {error:?}"),
        };

        let concrete = match store.intern_generic_substitution(concrete_data) {
            Ok(id) => id,
            Err(error) => panic!("concrete substitution interning failed: {error:?}"),
        };

        assert!(store.substitution_is_concrete(concrete));

        let open_type = match store.intern_type(TypeData::TypeParameter(type_parameter)) {
            Ok(ty) => ty,
            Err(error) => panic!("open type interning failed: {error:?}"),
        };

        let open_data = GenericSubstitutionData::try_new(
            owner,
            [GenericParameterSymbolId::from(type_parameter)],
            [GenericArgument::Type(open_type)],
        );

        let open_data = match open_data {
            Ok(data) => data,
            Err(error) => panic!("open substitution construction failed: {error:?}"),
        };

        let open = match store.intern_generic_substitution(open_data) {
            Ok(id) => id,
            Err(error) => panic!("open substitution interning failed: {error:?}"),
        };

        assert!(!store.substitution_is_concrete(open));

        // A surrounding concrete aggregate does not close an unresolved argument.
        for (element, expected) in [(ty, true), (open_type, false)] {
            let nested = store.intern_type(TypeData::tuple([element])).unwrap();

            let data = GenericSubstitutionData::try_new(
                owner,
                [GenericParameterSymbolId::from(type_parameter)],
                [GenericArgument::Type(nested)],
            )
            .unwrap();

            let substitution = store.intern_generic_substitution(data).unwrap();
            assert_eq!(store.substitution_is_concrete(substitution), expected);
        }
    }

    #[test]
    fn application_substitutions_must_belong_to_the_definition() {
        let store = store();

        let function = FunctionSymbolId::from_symbol_id(SymbolId::new(30));
        let substitution = empty_substitution(&store, function.into());
        let trait_definition = TraitSymbolId::from_symbol_id(SymbolId::new(31));
        let application = TraitApplicationData::new(trait_definition, substitution);

        let _expected = generic_owner(trait_definition.into());
        let _actual = generic_owner(function.into());

        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let _ = store.intern_trait_application(application);
            }))
            .is_err()
        );
    }

    #[test]
    fn open_constant_term_order_is_part_of_identity() {
        let store = store();

        let parameter = GenericConstParameterSymbolId::from_symbol_id(SymbolId::new(40));

        let left = match store.intern_constant_term(ConstantTermData::Parameter(parameter)) {
            Ok(term) => term,
            Err(error) => panic!("parameter term interning failed: {error:?}"),
        };

        let ty = concrete_named_type(&store, 41);

        let value = match store
            .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Boolean(true)))
        {
            Ok(value) => value,
            Err(error) => panic!("constant value interning failed: {error:?}"),
        };

        let right = match store.intern_constant_term(ConstantTermData::Value(value)) {
            Ok(term) => term,
            Err(error) => panic!("value term interning failed: {error:?}"),
        };

        let first = store.intern_constant_term(ConstantTermData::Binary {
            operation: ConstantBinaryOperation::Add,
            left,
            right,
        });

        let second = store.intern_constant_term(ConstantTermData::Binary {
            operation: ConstantBinaryOperation::Add,
            left: right,
            right: left,
        });

        assert_ne!(first, second);
    }

    #[test]
    fn equivalent_dependency_templates_ignore_input_order_and_duplicates() {
        let store = store();

        let first_subject =
            DependencySubject::root(DependencySubjectRoot::Parameter(SymbolOrdinal::new(0)));

        let second_subject = DependencySubject::root(DependencySubjectRoot::Result);

        let first =
            DependencyRequirement::direct(first_subject, DependencyRequirementKind::StorageAlive);

        let second = DependencyRequirement::direct(
            second_subject,
            DependencyRequirementKind::StorageInitialized,
        );

        let left =
            DependencyContractTemplateData::new([first.clone(), second.clone(), first.clone()]);

        let right = DependencyContractTemplateData::new([second, first]);

        assert_eq!(
            store.intern_dependency_contract_template(left),
            store.intern_dependency_contract_template(right)
        );
    }

    #[test]
    fn concurrent_equal_construction_reuses_one_id() {
        let store = Arc::new(store());

        let mut threads = Vec::new();

        for _ in 0..8 {
            let store = Arc::clone(&store);
            threads.push(thread::spawn(move || store.intern_type(TypeData::Error)));
        }

        let mut ids = Vec::new();

        for thread in threads {
            let result = match thread.join() {
                Ok(result) => result,
                Err(_) => panic!("semantic interning thread panicked"),
            };

            let id = match result {
                Ok(id) => id,
                Err(error) => panic!("semantic interning failed: {error:?}"),
            };

            ids.push(id);
        }

        assert!(ids.windows(2).all(|pair| pair[0] == pair[1]));
    }

    #[test]
    fn semantic_store_is_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<SemanticValueStore>();
    }

    #[test]
    fn unborrowed_types_remove_only_outer_borrow_layers() {
        let values = store();
        let unit = values.intern_type(TypeData::tuple([])).unwrap();

        let shared = values
            .intern_type(TypeData::Borrow {
                kind: crate::BorrowKind::Shared,
                target: unit,
            })
            .unwrap();

        let mutable = values
            .intern_type(TypeData::Borrow {
                kind: crate::BorrowKind::Mutable,
                target: shared,
            })
            .unwrap();

        let nullable = values.intern_type(TypeData::Nullable(shared)).unwrap();

        for ty in [unit, shared, mutable] {
            assert_eq!(values.unborrowed_type(ty), unit);
        }

        assert_eq!(values.unborrowed_type(nullable), nullable);
        let foreign = store().intern_type(TypeData::tuple([])).unwrap();

        assert!(
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(
                || values.unborrowed_type(foreign)
            ))
            .is_err()
        );
    }
}
