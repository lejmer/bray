use std::borrow::Cow;

use super::CompilerKnownCatalogRoleRegistry;
use super::{
    CatalogDeclarationSurface, CatalogDeclarationSurfaceSyntax, CatalogTypeSurface,
    CatalogTypeSurfaceSyntax, CompilerKnownDeclarationDescriptor, CompilerKnownDeclarationId,
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
    pub(super) compiler_known_scopes: Cow<'static, [CompilerKnownScopeDescriptor]>,
    pub(super) compiler_known_declarations: Cow<'static, [CompilerKnownDeclarationDescriptor]>,
    pub(super) compiler_known_values: Cow<'static, [CompilerKnownValueDescriptor]>,
    pub(super) role_registry: CompilerKnownCatalogRoleRegistry,
    pub(super) recognized_scopes: Cow<'static, [RecognizedStandardLibraryScopeDescriptor]>,
    pub(super) recognized_declarations:
        Cow<'static, [RecognizedStandardLibraryDeclarationDescriptor]>,
    pub(super) declaration_surfaces: Cow<'static, [CatalogDeclarationSurfaceSyntax]>,
    pub(super) type_surfaces: Cow<'static, [CatalogTypeSurfaceSyntax]>,
}

impl CompilerKnownCatalog {
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

    /// Returns typed role bindings without string or stable-key lookup.
    pub const fn role_registry(&self) -> &CompilerKnownCatalogRoleRegistry {
        &self.role_registry
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

    /// Returns pre-parsed declaration surfaces in source-anchor order.
    pub fn declaration_surfaces(&self) -> &[CatalogDeclarationSurfaceSyntax] {
        &self.declaration_surfaces
    }

    /// Returns pre-parsed type-expression surfaces in source-anchor order.
    pub fn type_surfaces(&self) -> &[CatalogTypeSurfaceSyntax] {
        &self.type_surfaces
    }

    /// Resolves one declaration surface without reparsing Bray source.
    pub fn declaration_surface(
        &self,
        surface: CatalogDeclarationSurface,
    ) -> Option<&CatalogDeclarationSurfaceSyntax> {
        self.declaration_surfaces
            .binary_search_by_key(&surface.anchor(), |syntax| syntax.surface().anchor())
            .ok()
            .and_then(|index| self.declaration_surfaces.get(index))
    }

    /// Resolves one type-expression surface without reparsing Bray source.
    pub fn type_surface(&self, surface: CatalogTypeSurface) -> Option<&CatalogTypeSurfaceSyntax> {
        self.type_surfaces
            .binary_search_by_key(&surface.anchor(), |syntax| syntax.surface().anchor())
            .ok()
            .and_then(|index| self.type_surfaces.get(index))
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
    use std::borrow::Cow;

    use super::{CompilerKnownCatalog, CompilerKnownCatalogRoleRegistry};
    use crate::catalog::test_support::{declaration_key, declaration_surface};
    use crate::catalog::{
        CatalogDeclarationKind, CatalogScopeLocation, CompilerKnownDeclarationDescriptor,
        CompilerKnownDeclarationId, CompilerKnownDeclarationOwner, CompilerKnownScopeDescriptor,
        CompilerKnownScopeId, CompilerKnownScopeKey,
    };
    use crate::{AvailabilityRule, COMPILER_KNOWN_CATALOG, ImplementationHook, RepresentationRole};

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
    }

    #[test]
    fn catalog_is_immutable_shareable_data() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CompilerKnownCatalog>();

        assert_eq!(catalog(), catalog());
    }

    #[test]
    fn published_catalog_uses_borrowed_static_tables() {
        assert!(matches!(
            COMPILER_KNOWN_CATALOG.compiler_known_scopes,
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            COMPILER_KNOWN_CATALOG.compiler_known_declarations,
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            COMPILER_KNOWN_CATALOG.compiler_known_values,
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            COMPILER_KNOWN_CATALOG.role_registry.representations,
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            COMPILER_KNOWN_CATALOG.role_registry.implementations,
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            COMPILER_KNOWN_CATALOG.declaration_surfaces,
            Cow::Borrowed(_)
        ));
        assert!(matches!(
            COMPILER_KNOWN_CATALOG.type_surfaces,
            Cow::Borrowed(_)
        ));
    }

    fn catalog() -> CompilerKnownCatalog {
        let declarations = vec![
            declaration_descriptor(0, "RawPointer", Some(RepresentationRole::RawPointer), None),
            declaration_descriptor(
                1,
                "RawPointerRead",
                None,
                Some(ImplementationHook::RawPointerRead),
            ),
        ];

        let role_registry = CompilerKnownCatalogRoleRegistry::from_descriptors(&declarations, &[]);

        CompilerKnownCatalog {
            compiler_known_scopes: Cow::Owned(vec![scope_descriptor()]),
            compiler_known_declarations: Cow::Owned(declarations),
            compiler_known_values: Cow::Borrowed(&[]),
            role_registry,
            recognized_scopes: Cow::Borrowed(&[]),
            recognized_declarations: Cow::Borrowed(&[]),
            declaration_surfaces: Cow::Borrowed(&[]),
            type_surfaces: Cow::Borrowed(&[]),
        }
    }

    fn scope_descriptor() -> CompilerKnownScopeDescriptor {
        CompilerKnownScopeDescriptor {
            id: CompilerKnownScopeId::new(0),
            key: scope_key("Ambient"),
            location: CatalogScopeLocation::Ambient,
            declaration_ids: Cow::Owned(vec![
                CompilerKnownDeclarationId::new(0),
                CompilerKnownDeclarationId::new(1),
            ]),
            value_ids: Cow::Borrowed(&[]),
        }
    }

    fn declaration_descriptor(
        id: u32,
        key: &str,
        representation_role: Option<RepresentationRole>,
        implementation_hook: Option<ImplementationHook>,
    ) -> CompilerKnownDeclarationDescriptor {
        CompilerKnownDeclarationDescriptor {
            id: CompilerKnownDeclarationId::new(id),
            key: declaration_key(key),
            owner: CompilerKnownDeclarationOwner::Scope(CompilerKnownScopeId::new(0)),
            kind: CatalogDeclarationKind::Function,
            surface: declaration_surface(),
            representation_role,
            implementation_hook,
            availability_rule: AvailabilityRule::RawMemory,
        }
    }

    fn scope_key(value: &str) -> CompilerKnownScopeKey {
        match CompilerKnownScopeKey::try_new(value) {
            Some(key) => key,
            None => panic!("test scope key is valid"),
        }
    }
}
