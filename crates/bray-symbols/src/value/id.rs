/// Identifies the semantic value store that issued an opaque value ID.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SemanticValueStoreId(u64);

impl SemanticValueStoreId {
    pub(super) const fn new(raw: u64) -> Self {
        Self(raw)
    }
}

/// Classifies a canonical semantic value table.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SemanticValueKind {
    /// Canonical semantic types.
    Type,
    /// Fully evaluated constant values.
    ConstantValue,
    /// Open checked constant terms.
    ConstantTerm,
    /// Ordered generic substitutions.
    GenericSubstitution,
    /// Applied traits.
    TraitApplication,
    /// Substituted callable definitions.
    CallableInstance,
    /// Substituted implementation definitions.
    ImplementationInstance,
    /// Portable dependency-contract templates.
    DependencyContractTemplate,
}

#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub(super) struct SemanticValueId {
    store: SemanticValueStoreId,
    slot: u32,
}

impl SemanticValueId {
    pub(super) const fn new(store: SemanticValueStoreId, slot: u32) -> Self {
        Self { store, slot }
    }

    pub(super) const fn store(self) -> SemanticValueStoreId {
        self.store
    }

    pub(super) fn to_index(self) -> Option<usize> {
        usize::try_from(self.slot).ok()
    }
}

pub(super) trait InternedValueId: Copy {
    const KIND: SemanticValueKind;

    fn from_value_id(id: SemanticValueId) -> Self;

    fn value_id(self) -> SemanticValueId;
}

macro_rules! define_semantic_value_ids {
    ($($(#[$meta:meta])* $id:ident => $kind:ident),+ $(,)?) => {
        $(
            $(#[$meta])*
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            pub struct $id(SemanticValueId);

            impl $id {
                /// Returns the store that issued this opaque ID.
                pub const fn store_id(self) -> SemanticValueStoreId {
                    self.0.store()
                }
            }

            impl InternedValueId for $id {
                const KIND: SemanticValueKind = SemanticValueKind::$kind;

                fn from_value_id(id: SemanticValueId) -> Self {
                    Self(id)
                }

                fn value_id(self) -> SemanticValueId {
                    self.0
                }
            }
        )+
    };
}

define_semantic_value_ids! {
    /// Identifies one canonical semantic type within a store.
    TypeId => Type,
    /// Identifies one canonical fully evaluated constant value within a store.
    ConstantValueId => ConstantValue,
    /// Identifies one canonical open or closed constant term within a store.
    ConstantTermId => ConstantTerm,
    /// Identifies one canonical ordered generic substitution within a store.
    GenericSubstitutionId => GenericSubstitution,
    /// Identifies one canonical trait application within a store.
    TraitApplicationId => TraitApplication,
    /// Identifies one canonical substituted callable definition within a store.
    CallableInstanceId => CallableInstance,
    /// Identifies one canonical selected implementation instance within a store.
    ImplementationInstanceId => ImplementationInstance,
    /// Identifies one canonical portable dependency-contract template within a store.
    DependencyContractTemplateId => DependencyContractTemplate,
}

/// A generic substitution proven to contain only concrete types and closed constant values.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConcreteGenericSubstitutionId(GenericSubstitutionId);

impl ConcreteGenericSubstitutionId {
    /// Returns the underlying canonical generic substitution.
    pub const fn substitution(self) -> GenericSubstitutionId {
        self.0
    }

    /// Returns the store that validated this substitution.
    pub const fn store_id(self) -> SemanticValueStoreId {
        self.0.store_id()
    }

    pub(super) const fn new(id: GenericSubstitutionId) -> Self {
        Self(id)
    }
}
