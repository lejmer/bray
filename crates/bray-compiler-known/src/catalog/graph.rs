use std::sync::Arc;

use super::{
    CatalogSourceInventory, CompilerKnownDeclarationDescriptor, CompilerKnownDeclarationId,
    CompilerKnownDeclarationKey, CompilerKnownScopeDescriptor, CompilerKnownScopeId,
    CompilerKnownScopeKey, CompilerKnownValueDescriptor, CompilerKnownValueId,
    CompilerKnownValueKey, RecognizedStandardLibraryDeclarationDescriptor,
    RecognizedStandardLibraryDeclarationId, RecognizedStandardLibraryDeclarationKey,
    RecognizedStandardLibraryScopeDescriptor, RecognizedStandardLibraryScopeId,
    RecognizedStandardLibraryScopeKey,
};

/// The immutable target-independent descriptor graph published by the catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilerKnownCatalog {
    pub(super) source_inventory: &'static CatalogSourceInventory,
    pub(super) compiler_known_scopes: Arc<[CompilerKnownScopeDescriptor]>,
    pub(super) compiler_known_declarations: Arc<[CompilerKnownDeclarationDescriptor]>,
    pub(super) compiler_known_values: Arc<[CompilerKnownValueDescriptor]>,
    pub(super) recognized_scopes: Arc<[RecognizedStandardLibraryScopeDescriptor]>,
    pub(super) recognized_declarations: Arc<[RecognizedStandardLibraryDeclarationDescriptor]>,
}

impl CompilerKnownCatalog {
    /// Returns the catalog-owned embedded source inventory.
    pub const fn source_inventory(&self) -> &'static CatalogSourceInventory {
        self.source_inventory
    }

    /// Returns compiler-known scopes in canonical stable-key order.
    pub fn compiler_known_scopes(&self) -> &[CompilerKnownScopeDescriptor] {
        &self.compiler_known_scopes
    }

    /// Returns compiler-known declarations in canonical stable-key order.
    pub fn compiler_known_declarations(&self) -> &[CompilerKnownDeclarationDescriptor] {
        &self.compiler_known_declarations
    }

    /// Returns compiler-known special values in canonical stable-key order.
    pub fn compiler_known_values(&self) -> &[CompilerKnownValueDescriptor] {
        &self.compiler_known_values
    }

    /// Returns recognized standard-library scopes in canonical key order.
    pub fn recognized_standard_library_scopes(
        &self,
    ) -> &[RecognizedStandardLibraryScopeDescriptor] {
        &self.recognized_scopes
    }

    /// Returns recognized standard-library declarations in canonical key order.
    pub fn recognized_standard_library_declarations(
        &self,
    ) -> &[RecognizedStandardLibraryDeclarationDescriptor] {
        &self.recognized_declarations
    }

    /// Resolves a compiler-known scope through a checked compact ID.
    pub fn compiler_known_scope(
        &self,
        id: CompilerKnownScopeId,
    ) -> Option<&CompilerKnownScopeDescriptor> {
        get_by_index(&self.compiler_known_scopes, id.to_index())
    }

    /// Resolves a compiler-known declaration through a checked compact ID.
    pub fn compiler_known_declaration(
        &self,
        id: CompilerKnownDeclarationId,
    ) -> Option<&CompilerKnownDeclarationDescriptor> {
        get_by_index(&self.compiler_known_declarations, id.to_index())
    }

    /// Resolves a compiler-known special value through a checked compact ID.
    pub fn compiler_known_value(
        &self,
        id: CompilerKnownValueId,
    ) -> Option<&CompilerKnownValueDescriptor> {
        get_by_index(&self.compiler_known_values, id.to_index())
    }

    /// Finds a compiler-known scope by stable key.
    pub fn compiler_known_scope_by_key(
        &self,
        key: &CompilerKnownScopeKey,
    ) -> Option<&CompilerKnownScopeDescriptor> {
        find_by_key(&self.compiler_known_scopes, key, |descriptor| {
            descriptor.key()
        })
    }

    /// Finds a compiler-known declaration by stable key.
    pub fn compiler_known_declaration_by_key(
        &self,
        key: &CompilerKnownDeclarationKey,
    ) -> Option<&CompilerKnownDeclarationDescriptor> {
        find_by_key(&self.compiler_known_declarations, key, |descriptor| {
            descriptor.key()
        })
    }

    /// Finds a compiler-known special value by stable key.
    pub fn compiler_known_value_by_key(
        &self,
        key: &CompilerKnownValueKey,
    ) -> Option<&CompilerKnownValueDescriptor> {
        find_by_key(&self.compiler_known_values, key, |descriptor| {
            descriptor.key()
        })
    }

    /// Resolves a recognized scope through a checked compact ID.
    pub fn recognized_standard_library_scope(
        &self,
        id: RecognizedStandardLibraryScopeId,
    ) -> Option<&RecognizedStandardLibraryScopeDescriptor> {
        get_by_index(&self.recognized_scopes, id.to_index())
    }

    /// Resolves a recognized declaration through a checked compact ID.
    pub fn recognized_standard_library_declaration(
        &self,
        id: RecognizedStandardLibraryDeclarationId,
    ) -> Option<&RecognizedStandardLibraryDeclarationDescriptor> {
        get_by_index(&self.recognized_declarations, id.to_index())
    }

    /// Finds a recognized standard-library scope by stable key.
    pub fn recognized_standard_library_scope_by_key(
        &self,
        key: &RecognizedStandardLibraryScopeKey,
    ) -> Option<&RecognizedStandardLibraryScopeDescriptor> {
        find_by_key(&self.recognized_scopes, key, |descriptor| descriptor.key())
    }

    /// Finds a recognized standard-library declaration by stable key.
    pub fn recognized_standard_library_declaration_by_key(
        &self,
        key: &RecognizedStandardLibraryDeclarationKey,
    ) -> Option<&RecognizedStandardLibraryDeclarationDescriptor> {
        find_by_key(&self.recognized_declarations, key, |descriptor| {
            descriptor.key()
        })
    }
}

fn get_by_index<T>(items: &[T], index: Option<usize>) -> Option<&T> {
    index.and_then(|index| items.get(index))
}

fn find_by_key<'items, T, K>(
    items: &'items [T],
    key: &K,
    item_key: impl Fn(&T) -> &K,
) -> Option<&'items T>
where
    K: Ord,
{
    items
        .binary_search_by(|item| item_key(item).cmp(key))
        .ok()
        .and_then(|index| items.get(index))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use bray_source::{TextRange, TextSize};

    use super::CompilerKnownCatalog;
    use crate::catalog::{
        CatalogDeclarationKind, CatalogDeclarationSurface, CatalogScopeLocation,
        CatalogSourceAnchor, CompilerKnownDeclarationDescriptor, CompilerKnownDeclarationId,
        CompilerKnownDeclarationKey, CompilerKnownDeclarationOwner, CompilerKnownScopeDescriptor,
        CompilerKnownScopeId, CompilerKnownScopeKey, generator_input_inventory,
    };
    use crate::{AvailabilityRule, ImplementationHook, RepresentationRole};

    #[test]
    fn catalog_access_is_checked_and_stable_keyed() {
        let catalog = catalog();

        assert_eq!(
            catalog
                .compiler_known_declaration(CompilerKnownDeclarationId::new(0))
                .map(|descriptor| descriptor.key().as_str()),
            Some("RawPointer")
        );
        assert_eq!(
            catalog.compiler_known_declaration(CompilerKnownDeclarationId::new(2)),
            None
        );

        assert_eq!(
            catalog
                .compiler_known_declaration_by_key(&declaration_key("RawPointerRead"))
                .map(CompilerKnownDeclarationDescriptor::id),
            Some(CompilerKnownDeclarationId::new(1))
        );
        assert_eq!(
            catalog.compiler_known_declaration_by_key(&declaration_key("Missing")),
            None
        );

        assert_eq!(catalog.source_inventory(), generator_input_inventory());
    }

    #[test]
    fn catalog_is_immutable_shareable_data() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CompilerKnownCatalog>();

        assert_eq!(catalog(), catalog());
    }

    fn catalog() -> CompilerKnownCatalog {
        CompilerKnownCatalog {
            source_inventory: generator_input_inventory(),
            compiler_known_scopes: Arc::from([scope_descriptor()]),
            compiler_known_declarations: Arc::from([
                declaration_descriptor(0, "RawPointer"),
                declaration_descriptor(1, "RawPointerRead"),
            ]),
            compiler_known_values: Arc::from([]),
            recognized_scopes: Arc::from([]),
            recognized_declarations: Arc::from([]),
        }
    }

    fn scope_descriptor() -> CompilerKnownScopeDescriptor {
        CompilerKnownScopeDescriptor {
            id: CompilerKnownScopeId::new(0),
            key: scope_key("Ambient"),
            location: CatalogScopeLocation::Ambient,
            declaration_ids: Arc::from([
                CompilerKnownDeclarationId::new(0),
                CompilerKnownDeclarationId::new(1),
            ]),
            value_ids: Arc::from([]),
        }
    }

    fn declaration_descriptor(id: u32, key: &str) -> CompilerKnownDeclarationDescriptor {
        CompilerKnownDeclarationDescriptor {
            id: CompilerKnownDeclarationId::new(id),
            key: declaration_key(key),
            owner: CompilerKnownDeclarationOwner::Scope(CompilerKnownScopeId::new(0)),
            kind: CatalogDeclarationKind::Function,
            surface: declaration_surface(),
            representation_role: Some(RepresentationRole::RawPointer),
            implementation_hook: Some(ImplementationHook::RawPointerRead),
            availability_rule: AvailabilityRule::RawMemory,
        }
    }

    fn declaration_surface() -> CatalogDeclarationSurface {
        let source = generator_input_inventory().sources()[0].id();

        let anchor = CatalogSourceAnchor {
            source,
            range: TextRange::new(TextSize::new(1), TextSize::new(5)),
        };

        CatalogDeclarationSurface(anchor)
    }

    fn scope_key(value: &str) -> CompilerKnownScopeKey {
        match CompilerKnownScopeKey::try_new(value) {
            Some(key) => key,
            None => panic!("test scope key is valid"),
        }
    }

    fn declaration_key(value: &str) -> CompilerKnownDeclarationKey {
        match CompilerKnownDeclarationKey::try_new(value) {
            Some(key) => key,
            None => panic!("test declaration key is valid"),
        }
    }
}
