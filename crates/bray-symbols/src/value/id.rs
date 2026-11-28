/// Identifies the semantic value store that issued an opaque value ID.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SemanticValueStoreId(u32);

impl SemanticValueStoreId {
    pub(super) const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the process-local numeric identity used in structured diagnostics.
    pub const fn raw(self) -> u64 {
        self.0 as u64
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
    /// Selected static references.
    StaticReference,
    /// Portable dependency-contract templates.
    DependencyContractTemplate,
}

/// One interned value.
///
/// Slots follow interning order, which depends on thread scheduling. Order and
/// hashing therefore follow a content digest, so maps, sets and conditions keyed
/// by values iterate the same way in every build. The slot breaks ties only
/// between distinct values whose digests collide.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub(super) struct SemanticValueId {
    digest: u64,
    store: SemanticValueStoreId,
    slot: u32,
}

impl std::hash::Hash for SemanticValueId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.digest.hash(state);
    }
}

impl SemanticValueId {
    /// Identifies `data` by a digest of its contents, where referenced values
    /// contribute their own digests.
    pub(super) fn new(data: &impl std::hash::Hash, store: SemanticValueStoreId, slot: u32) -> Self {
        let mut digest = std::hash::DefaultHasher::new();

        data.hash(&mut digest);

        Self { digest: std::hash::Hasher::finish(&digest), store, slot }
    }

    pub(super) const fn store(self) -> SemanticValueStoreId {
        self.store
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

                /// Returns the stable slot within the issuing semantic value store.
                pub const fn slot(self) -> u32 {
                    self.0.slot
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
    /// Identifies one canonical selected static reference within a store.
    StaticReferenceId => StaticReference,
    /// Identifies one canonical portable dependency-contract template within a store.
    DependencyContractTemplateId => DependencyContractTemplate,
}
