use crate::SymbolKind;

/// Identifies one exact local semantic region in a compilation snapshot.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LocalSymbolRegionId(u32);

impl LocalSymbolRegionId {
    /// Creates a compilation-local region ID from its numeric representation.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the compilation-local numeric representation.
    pub const fn raw(self) -> u32 {
        self.0
    }
}

macro_rules! define_local_ids {
    ($($id:ident => $variant:ident : $kind:ident),+ $(,)?) => {
        $(
            #[doc = concat!("Identifies one `", stringify!($kind), "` symbol in its owning region.")]
            #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
            pub struct $id {
                region: LocalSymbolRegionId,
                slot: u32,
            }

            impl $id {
                pub(super) const fn new(region: LocalSymbolRegionId, slot: u32) -> Self {
                    Self { region, slot }
                }

                /// Returns the region that owns this local symbol.
                pub const fn region(self) -> LocalSymbolRegionId {
                    self.region
                }

                /// Returns the exact semantic kind represented by this ID.
                pub const fn kind(self) -> SymbolKind {
                    SymbolKind::$kind
                }

                /// Returns this identity's region-local allocation ordinal.
                pub const fn ordinal(self) -> u32 {
                    self.slot
                }

                pub(super) fn to_index(self) -> Option<usize> {
                    usize::try_from(self.slot).ok()
                }
            }
        )+

        /// A closed type-erased reference to any region-scoped local symbol.
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub enum AnyLocalSymbolId {
            $(
                #[doc = concat!("A `", stringify!($kind), "` local symbol ID.")]
                $variant($id),
            )+
        }

        impl AnyLocalSymbolId {
            /// Returns the region that owns this local symbol.
            pub const fn region(self) -> LocalSymbolRegionId {
                match self {
                    $(Self::$variant(id) => id.region(),)+
                }
            }

            /// Returns the exact semantic kind retained by this erased ID.
            pub const fn kind(self) -> SymbolKind {
                match self {
                    $(Self::$variant(id) => id.kind(),)+
                }
            }
        }

        $(
            impl From<$id> for AnyLocalSymbolId {
                fn from(id: $id) -> Self {
                    Self::$variant(id)
                }
            }
        )+
    };
}

define_local_ids! {
    LocalBindingSymbolId => Binding: LocalBinding,
    LocalConstantSymbolId => Constant: LocalConstant,
    AnonymousCallableSymbolId => AnonymousCallable: AnonymousCallable,
    AnonymousCallableParameterSymbolId => AnonymousCallableParameter: AnonymousCallableParameter,
    PostconditionResultSymbolId => PostconditionResult: PostconditionResult,
}

/// Identifies one lexical scope in its owning local semantic region.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LocalScopeId {
    region: LocalSymbolRegionId,
    slot: u32,
}

impl LocalScopeId {
    pub(super) const fn new(region: LocalSymbolRegionId, slot: u32) -> Self {
        Self { region, slot }
    }

    /// Returns the region that owns this lexical scope.
    pub const fn region(self) -> LocalSymbolRegionId {
        self.region
    }

    pub(super) fn to_index(self) -> Option<usize> {
        usize::try_from(self.slot).ok()
    }
}
