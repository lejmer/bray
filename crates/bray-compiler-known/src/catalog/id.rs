macro_rules! define_catalog_id {
    ($(#[$meta:meta])* pub struct $name:ident;) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(u32);

        impl $name {
            /// Creates a catalog-local ID from its compact value.
            pub const fn new(raw: u32) -> Self {
                Self(raw)
            }

            /// Returns this ID as a collection index when supported by the host.
            pub fn to_index(self) -> Option<usize> {
                usize::try_from(self.0).ok()
            }

            /// Returns the compact catalog-local value.
            pub const fn raw(self) -> u32 {
                self.0
            }

            /// Creates an ID from a collection index when it fits compact storage.
            pub fn try_from_index(index: usize) -> Option<Self> {
                match u32::try_from(index) {
                    Ok(raw) => Some(Self(raw)),
                    Err(_) => None,
                }
            }
        }

        impl From<$name> for u32 {
            fn from(id: $name) -> Self {
                id.raw()
            }
        }
    };
}

define_catalog_id! {
    /// Inventory-local identity for one catalog generator input.
    pub struct CatalogSourceId;
}

define_catalog_id! {
    /// Catalog-local identity for one compiler-known scope descriptor.
    pub struct CompilerKnownScopeId;
}

define_catalog_id! {
    /// Catalog-local identity for one compiler-known declaration descriptor.
    pub struct CompilerKnownDeclarationId;
}

define_catalog_id! {
    /// Catalog-local identity for one compiler-known value descriptor.
    pub struct CompilerKnownValueId;
}

define_catalog_id! {
    /// Catalog-local identity for one recognized standard-library scope.
    pub struct RecognizedStandardLibraryScopeId;
}

define_catalog_id! {
    /// Catalog-local identity for one recognized standard-library declaration.
    pub struct RecognizedStandardLibraryDeclarationId;
}

#[cfg(test)]
mod tests {
    use super::{
        CatalogSourceId, CompilerKnownDeclarationId, CompilerKnownScopeId, CompilerKnownValueId,
        RecognizedStandardLibraryDeclarationId, RecognizedStandardLibraryScopeId,
    };

    #[test]
    fn catalog_ids_are_compact_checked_indexes() {
        assert_eq!(size_of::<CatalogSourceId>(), size_of::<u32>());

        let source = CatalogSourceId::try_from_index(3);

        assert_eq!(source.map(CatalogSourceId::raw), Some(3));
        assert_eq!(source.and_then(CatalogSourceId::to_index), Some(3));
    }

    #[test]
    fn descriptor_id_categories_remain_distinct() {
        let scope = CompilerKnownScopeId::try_from_index(0);
        let declaration = CompilerKnownDeclarationId::try_from_index(0);
        let value = CompilerKnownValueId::try_from_index(0);
        let recognized_scope = RecognizedStandardLibraryScopeId::try_from_index(0);
        let recognized_declaration = RecognizedStandardLibraryDeclarationId::try_from_index(0);

        assert_eq!(scope.map(CompilerKnownScopeId::raw), Some(0));
        assert_eq!(declaration.map(CompilerKnownDeclarationId::raw), Some(0));
        assert_eq!(value.map(CompilerKnownValueId::raw), Some(0));
        assert_eq!(
            recognized_scope.map(RecognizedStandardLibraryScopeId::raw),
            Some(0)
        );
        assert_eq!(
            recognized_declaration.map(RecognizedStandardLibraryDeclarationId::raw),
            Some(0)
        );
    }
}
