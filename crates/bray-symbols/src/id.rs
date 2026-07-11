use crate::{SymbolKind, kind::for_each_compilation_symbol_kind};

mod sealed {
    pub trait Sealed {}
}

/// Identifies one exact compilation-wide symbol category at the type level.
///
/// This trait is sealed so imported fact keys cannot claim a category that does not correspond
/// to one of Bray's exact typed symbol IDs.
pub trait ExactSymbolId: sealed::Sealed + Copy {
    /// The semantic kind represented by this exact ID type.
    const KIND: SymbolKind;

    /// Recovers this exact category from type-erased infrastructure identity.
    fn try_from_any(id: AnySymbolId) -> Option<Self>;

    /// Returns the underlying compilation-wide symbol ID.
    fn symbol_id(self) -> SymbolId;
}

/// A compact compilation-local handle identifying one exact surface symbol.
///
/// Raw values are meaningful only within the compilation or immutable symbol snapshot that
/// issued them. Persisted identities must use [`crate::SymbolKey`] instead.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SymbolId(u32);

impl SymbolId {
    /// Creates a compilation-local ID from its numeric representation.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the compilation-local numeric representation.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Converts this ID to a checked table index for the current target.
    pub fn to_index(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }

    /// Creates an ID from a collection index when the index fits the compact representation.
    pub fn try_from_index(index: usize) -> Option<Self> {
        u32::try_from(index).ok().map(Self)
    }
}

macro_rules! define_symbol_ids {
    ($($documentation:literal; $id:ident => $variant:ident : $kind:ident),+ $(,)?) => {
        $(
            #[doc = concat!("The exact typed ID of a `", stringify!($kind), "` symbol.")]
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            pub struct $id(SymbolId);

            impl $id {
                /// Returns the underlying compilation-wide symbol ID.
                pub const fn symbol_id(self) -> SymbolId {
                    self.0
                }

                /// Returns the stable semantic kind associated with this exact ID type.
                pub const fn kind(self) -> SymbolKind {
                    SymbolKind::$kind
                }

                /// Creates an exact typed ID from a compilation-wide symbol ID.
                pub const fn from_symbol_id(id: SymbolId) -> Self {
                    Self(id)
                }
            }

            impl sealed::Sealed for $id {}

            impl ExactSymbolId for $id {
                const KIND: SymbolKind = SymbolKind::$kind;

                fn try_from_any(id: AnySymbolId) -> Option<Self> {
                    match id {
                        AnySymbolId::$variant(id) => Some(id),
                        _ => None,
                    }
                }

                fn symbol_id(self) -> SymbolId {
                    self.symbol_id()
                }
            }

            impl From<$id> for SymbolId {
                fn from(id: $id) -> Self {
                    id.symbol_id()
                }
            }
        )+

        /// A closed type-erased reference to any compilation-wide surface symbol.
        ///
        /// Exact typed IDs remain the canonical storage and API types. This erasure is intended
        /// for heterogeneous infrastructure such as diagnostics and tooling.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum AnySymbolId {
            $(
                #[doc = concat!("A `", stringify!($kind), "` symbol ID.")]
                $variant($id),
            )+
        }

        impl AnySymbolId {
            pub(crate) const fn from_kind(kind: SymbolKind, id: SymbolId) -> Option<Self> {
                match kind {
                    $(SymbolKind::$kind => Some(Self::$variant($id::from_symbol_id(id))),)+
                    _ => None,
                }
            }

            /// Returns the underlying compilation-wide symbol ID.
            pub const fn symbol_id(self) -> SymbolId {
                match self {
                    $(Self::$variant(id) => id.symbol_id(),)+
                }
            }

            /// Returns the exact symbol kind retained by this erased ID.
            pub const fn kind(self) -> SymbolKind {
                match self {
                    $(Self::$variant(id) => id.kind(),)+
                }
            }
        }

        $(
            impl From<$id> for AnySymbolId {
                fn from(id: $id) -> Self {
                    Self::$variant(id)
                }
            }
        )+
    };
}

for_each_compilation_symbol_kind!(define_symbol_ids);

macro_rules! define_family_id {
    (
        $(#[$meta:meta])*
        $family:ident {
            $($variant:ident($id:ident)),+ $(,)?
        }
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum $family {
            $(
                #[doc = concat!("A `", stringify!($id), "` family member.")]
                $variant($id),
            )+
        }

        impl $family {
            /// Returns the underlying compilation-wide symbol ID.
            pub const fn symbol_id(self) -> SymbolId {
                match self {
                    $(Self::$variant(id) => id.symbol_id(),)+
                }
            }

            /// Returns the exact symbol kind retained by this family ID.
            pub const fn kind(self) -> SymbolKind {
                match self {
                    $(Self::$variant(id) => id.kind(),)+
                }
            }

            /// Erases this family ID while retaining its exact kind.
            pub fn into_any(self) -> AnySymbolId {
                match self {
                    $(Self::$variant(id) => id.into(),)+
                }
            }
        }

        $(
            impl From<$id> for $family {
                fn from(id: $id) -> Self {
                    Self::$variant(id)
                }
            }
        )+
    };
}

define_family_id! {
    /// Identifies a root that can own logical modules.
    SymbolRootId {
        Package(PackageSymbolId),
        CompilerKnown(CompilerKnownEnvironmentSymbolId),
    }
}

define_family_id! {
    /// Identifies a declaration-surface callable that can own callable parameters.
    CallableSymbolId {
        Function(FunctionSymbolId),
        TypeMember(TypeCallableMemberSymbolId),
        TraitMember(TraitCallableMemberSymbolId),
        TraitFulfillment(TraitCallableFulfillmentSymbolId),
        Constructor(ConstructorSymbolId),
        Finalizer(FinalizerSymbolId),
        Destructor(DestructorSymbolId),
        ScopeEnter(ScopeEnterSymbolId),
        ScopeExit(ScopeExitSymbolId),
        TraitFinalizer(TraitFinalizerRequirementSymbolId),
        TraitDestructor(TraitDestructorRequirementSymbolId),
        TraitScopeEnter(TraitScopeEnterRequirementSymbolId),
        TraitScopeExit(TraitScopeExitRequirementSymbolId),
        TraitScopeEnterFulfillment(TraitScopeEnterFulfillmentSymbolId),
        TraitScopeExitFulfillment(TraitScopeExitFulfillmentSymbolId),
    }
}

define_family_id! {
    /// Identifies a predicate declaration that can own predicate parameters.
    PredicateDefinitionSymbolId {
        Predicate(PredicateSymbolId),
        TraitMember(TraitPredicateMemberSymbolId),
        TraitFulfillment(TraitPredicateFulfillmentSymbolId),
    }
}

define_family_id! {
    /// Identifies the semantic owner of a logical module.
    ModuleOwnerId {
        Package(PackageSymbolId),
        CompilerKnownEnvironment(CompilerKnownEnvironmentSymbolId),
    }
}

define_family_id! {
    /// Identifies a source-declared named structural type.
    NamedTypeSymbolId {
        Struct(StructSymbolId),
        Union(UnionSymbolId),
    }
}

define_family_id! {
    /// Identifies any implementation declaration.
    ImplementationSymbolId {
        Inherent(InherentImplementationSymbolId),
        UnnamedTrait(UnnamedTraitImplementationSymbolId),
        NamedTrait(NamedTraitImplementationSymbolId),
    }
}

define_family_id! {
    /// Identifies a generic parameter.
    GenericParameterSymbolId {
        Type(GenericTypeParameterSymbolId),
        Const(GenericConstParameterSymbolId),
    }
}

define_family_id! {
    /// Identifies a declaration-surface parameter.
    ParameterSymbolId {
        GenericType(GenericTypeParameterSymbolId),
        GenericConst(GenericConstParameterSymbolId),
        Callable(CallableParameterSymbolId),
        Predicate(PredicateParameterSymbolId),
        Receiver(ReceiverParameterSymbolId),
    }
}

#[cfg(test)]
mod tests {
    use std::mem::size_of;

    use super::{
        AnySymbolId, CompilerKnownEnvironmentSymbolId, FunctionSymbolId, ModuleOwnerId,
        PackageSymbolId, ParameterSymbolId, ReceiverParameterSymbolId, StructSymbolId, SymbolId,
        SymbolRootId,
    };
    use crate::SymbolKind;

    #[test]
    fn exact_ids_retain_kind_after_erasure() {
        let raw = SymbolId::try_from_index(7);

        let Some(raw) = raw else {
            panic!("small symbol ID must fit in the target index type");
        };

        let function = FunctionSymbolId::from_symbol_id(raw);
        let structure = StructSymbolId::from_symbol_id(raw);
        let function = AnySymbolId::from(function);
        let structure = AnySymbolId::from(structure);

        assert_eq!(function.symbol_id(), structure.symbol_id());
        assert_eq!(function.kind(), SymbolKind::Function);
        assert_eq!(structure.kind(), SymbolKind::Struct);
        assert_ne!(function, structure);
    }

    #[test]
    fn symbol_ids_are_compact_and_use_checked_indices() {
        assert_eq!(size_of::<SymbolId>(), size_of::<u32>());
        assert_eq!(size_of::<FunctionSymbolId>(), size_of::<u32>());

        let Some(id) = SymbolId::try_from_index(42) else {
            panic!("small symbol ID must fit in u32");
        };

        assert_eq!(id.raw(), 42);
        assert_eq!(id.to_index(), Some(42));

        if usize::BITS > u32::BITS {
            assert_eq!(SymbolId::try_from_index((u32::MAX as usize) + 1), None);
        }
    }

    #[test]
    fn root_and_module_owner_families_distinguish_their_roots() {
        let Some(raw) = SymbolId::try_from_index(0) else {
            panic!("zero must fit in u32");
        };

        let package = PackageSymbolId::from_symbol_id(raw);
        let environment = CompilerKnownEnvironmentSymbolId::from_symbol_id(raw);

        assert_eq!(SymbolRootId::from(package).kind(), SymbolKind::Package);

        assert_eq!(
            SymbolRootId::from(environment).kind(),
            SymbolKind::CompilerKnownEnvironment
        );

        assert_ne!(
            ModuleOwnerId::from(package),
            ModuleOwnerId::from(environment)
        );
    }

    #[test]
    fn parameter_family_preserves_the_exact_parameter_category() {
        let Some(raw) = SymbolId::try_from_index(3) else {
            panic!("small symbol ID must fit in u32");
        };

        let receiver = ReceiverParameterSymbolId::from_symbol_id(raw);
        let parameter = ParameterSymbolId::from(receiver);

        assert_eq!(parameter.symbol_id(), raw);
        assert_eq!(parameter.kind(), SymbolKind::ReceiverParameter);

        assert_eq!(
            parameter.into_any(),
            AnySymbolId::ReceiverParameter(receiver)
        );
    }

    #[test]
    fn ids_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<SymbolId>();
        assert_send_sync::<AnySymbolId>();
        assert_send_sync::<FunctionSymbolId>();
    }
}
