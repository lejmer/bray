use std::borrow::Cow;

use crate::{AvailabilityRule, ImplementationHook, RepresentationRole};

use super::{
    CatalogDeclarationSurface, CatalogPath, CatalogTokenSpelling, CatalogTypeSurface,
    CompilerKnownDeclarationId, CompilerKnownDeclarationKey, CompilerKnownScopeId,
    CompilerKnownScopeKey, CompilerKnownValueId, CompilerKnownValueKey,
    RecognizedStandardLibraryDeclarationId, RecognizedStandardLibraryDeclarationKey,
    RecognizedStandardLibraryScopeId, RecognizedStandardLibraryScopeKey,
};

/// Declaration category represented by an embedded Bray surface.
///
/// This is catalog source shape, not a semantic symbol kind. Owner context can
/// map the same surface category to different symbol categories later.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CatalogDeclarationKind {
    /// Constant declaration.
    Constant,
    /// Function declaration.
    Function,
    /// Predicate declaration.
    Predicate,
    /// Callable contract declaration.
    CallableContract,
    /// Callable overload declaration.
    CallableOverload,
    /// Implementation overload declaration.
    ImplementationOverload,
    /// Struct declaration.
    Struct,
    /// Union declaration.
    Union,
    /// Trait declaration.
    Trait,
    /// Inherent implementation declaration.
    InherentImplementation,
    /// Unnamed trait implementation declaration.
    UnnamedTraitImplementation,
    /// Named trait implementation declaration.
    NamedTraitImplementation,
    /// Struct field declaration.
    StructField,
    /// Union variant declaration.
    UnionVariant,
    /// Union payload field declaration.
    UnionPayloadField,
    /// Type constructor member declaration.
    TypeConstructorMember,
    /// Type callable member declaration.
    TypeCallableMember,
    /// Finalizer lifecycle member declaration.
    FinalizerMember,
    /// Destructor lifecycle member declaration.
    DestructorMember,
    /// Scope-enter lifecycle member declaration.
    ScopeEnterMember,
    /// Scope-exit lifecycle member declaration.
    ScopeExitMember,
    /// Trait constant member declaration.
    TraitConstantMember,
    /// Trait type-valued member declaration.
    TraitTypeMember,
    /// Trait predicate member declaration.
    TraitPredicateMember,
    /// Trait callable member declaration.
    TraitCallableMember,
    /// Trait finalizer requirement declaration.
    TraitFinalizerRequirement,
    /// Trait destructor requirement declaration.
    TraitDestructorRequirement,
    /// Trait scope-enter requirement declaration.
    TraitScopeEnterRequirement,
    /// Trait scope-exit requirement declaration.
    TraitScopeExitRequirement,
    /// Implementation type-valued member binding.
    ImplementationTypeMemberBinding,
}

/// Declaration context supplied when an embedded Bray surface is validated.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CatalogSurfaceContext {
    /// The declaration belongs directly to a catalog scope.
    Scope,
    /// The declaration belongs to a declaration of the given category.
    Declaration(CatalogDeclarationKind),
}

/// Location represented by a compiler-known scope descriptor.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CatalogScopeLocation {
    /// The compiler-known environment's ambient lookup scope.
    Ambient,
    /// A compiler-known logical module path.
    Module(CatalogPath),
}

/// The direct semantic owner of a compiler-known declaration descriptor.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CompilerKnownDeclarationOwner {
    /// The declaration belongs directly to a compiler-known scope.
    Scope(CompilerKnownScopeId),
    /// The declaration is nested under another compiler-known declaration.
    Declaration(CompilerKnownDeclarationId),
}

/// Immutable descriptor for one merged compiler-known scope.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilerKnownScopeDescriptor {
    pub(super) id: CompilerKnownScopeId,
    pub(super) key: CompilerKnownScopeKey,
    pub(super) location: CatalogScopeLocation,
    pub(super) declaration_ids: Cow<'static, [CompilerKnownDeclarationId]>,
    pub(super) value_ids: Cow<'static, [CompilerKnownValueId]>,
}

impl CompilerKnownScopeDescriptor {
    /// Returns the compact catalog-local scope identity.
    pub const fn id(&self) -> CompilerKnownScopeId {
        self.id
    }

    /// Returns the stable identity used to merge source contributions.
    pub const fn key(&self) -> &CompilerKnownScopeKey {
        &self.key
    }

    /// Returns the ambient or module location represented by the scope.
    pub const fn location(&self) -> &CatalogScopeLocation {
        &self.location
    }

    /// Returns directly owned declarations in canonical key order.
    pub fn declaration_ids(&self) -> &[CompilerKnownDeclarationId] {
        &self.declaration_ids
    }

    /// Returns directly owned special values in canonical key order.
    pub fn value_ids(&self) -> &[CompilerKnownValueId] {
        &self.value_ids
    }
}

/// Immutable compiler-known declaration surface and metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilerKnownDeclarationDescriptor {
    pub(super) id: CompilerKnownDeclarationId,
    pub(super) key: CompilerKnownDeclarationKey,
    pub(super) owner: CompilerKnownDeclarationOwner,
    pub(super) kind: CatalogDeclarationKind,
    pub(super) surface: CatalogDeclarationSurface,
    pub(super) representation_role: Option<RepresentationRole>,
    pub(super) implementation_hook: Option<ImplementationHook>,
    pub(super) availability_rule: AvailabilityRule,
}

impl CompilerKnownDeclarationDescriptor {
    /// Returns the compact catalog-local declaration identity.
    pub const fn id(&self) -> CompilerKnownDeclarationId {
        self.id
    }

    /// Returns the stable language identity of this declaration.
    pub const fn key(&self) -> &CompilerKnownDeclarationKey {
        &self.key
    }

    /// Returns the direct scope or declaration owner.
    pub const fn owner(&self) -> CompilerKnownDeclarationOwner {
        self.owner
    }

    /// Returns the embedded Bray declaration category.
    pub const fn kind(&self) -> CatalogDeclarationKind {
        self.kind
    }

    /// Returns the stable handle to generated pre-parsed declaration syntax.
    pub const fn surface(&self) -> CatalogDeclarationSurface {
        self.surface
    }

    /// Returns protected representation behavior when this declaration has it.
    pub const fn representation_role(&self) -> Option<RepresentationRole> {
        self.representation_role
    }

    /// Returns compiler-provided behavior when this declaration has it.
    pub const fn implementation_hook(&self) -> Option<ImplementationHook> {
        self.implementation_hook
    }

    /// Returns the target-availability predicate selected by the catalog.
    pub const fn availability_rule(&self) -> AvailabilityRule {
        self.availability_rule
    }
}

/// Immutable descriptor for a language-known value that is not a declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompilerKnownValueDescriptor {
    pub(super) id: CompilerKnownValueId,
    pub(super) key: CompilerKnownValueKey,
    pub(super) owner_scope: CompilerKnownScopeId,
    pub(super) spelling: CatalogTokenSpelling,
    pub(super) type_surface: CatalogTypeSurface,
    pub(super) representation_role: RepresentationRole,
    pub(super) availability_rule: AvailabilityRule,
}

impl CompilerKnownValueDescriptor {
    /// Returns the compact catalog-local value identity.
    pub const fn id(&self) -> CompilerKnownValueId {
        self.id
    }

    /// Returns the stable language identity of this value.
    pub const fn key(&self) -> &CompilerKnownValueKey {
        &self.key
    }

    /// Returns the compiler-known scope owning this value.
    pub const fn owner_scope(&self) -> CompilerKnownScopeId {
        self.owner_scope
    }

    /// Returns the exact token spelling recognized for this value.
    pub const fn spelling(&self) -> &CatalogTokenSpelling {
        &self.spelling
    }

    /// Returns the stable handle to generated pre-parsed type syntax.
    pub const fn type_surface(&self) -> CatalogTypeSurface {
        self.type_surface
    }

    /// Returns the value's language-defined representation role.
    pub const fn representation_role(&self) -> RepresentationRole {
        self.representation_role
    }

    /// Returns the target-availability predicate selected by the catalog.
    pub const fn availability_rule(&self) -> AvailabilityRule {
        self.availability_rule
    }
}

/// The direct owner of a recognized standard-library declaration contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RecognizedStandardLibraryDeclarationOwner {
    /// The declaration belongs directly to a recognized package path.
    Scope(RecognizedStandardLibraryScopeId),
    /// The declaration is nested under another recognized declaration.
    Declaration(RecognizedStandardLibraryDeclarationId),
}

/// Immutable descriptor for one recognized standard-library path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecognizedStandardLibraryScopeDescriptor {
    pub(super) id: RecognizedStandardLibraryScopeId,
    pub(super) key: RecognizedStandardLibraryScopeKey,
    pub(super) path: CatalogPath,
    pub(super) declaration_ids: Cow<'static, [RecognizedStandardLibraryDeclarationId]>,
}

impl RecognizedStandardLibraryScopeDescriptor {
    /// Returns the compact catalog-local scope identity.
    pub const fn id(&self) -> RecognizedStandardLibraryScopeId {
        self.id
    }

    /// Returns the stable catalog identity of this recognized scope.
    pub const fn key(&self) -> &RecognizedStandardLibraryScopeKey {
        &self.key
    }

    /// Returns the ordinary package path at which recognition is attempted.
    pub const fn path(&self) -> &CatalogPath {
        &self.path
    }

    /// Returns directly owned declarations in canonical key order.
    pub fn declaration_ids(&self) -> &[RecognizedStandardLibraryDeclarationId] {
        &self.declaration_ids
    }
}

/// Immutable recognition contract for one standard-library declaration.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RecognizedStandardLibraryDeclarationDescriptor {
    pub(super) id: RecognizedStandardLibraryDeclarationId,
    pub(super) key: RecognizedStandardLibraryDeclarationKey,
    pub(super) owner: RecognizedStandardLibraryDeclarationOwner,
    pub(super) kind: CatalogDeclarationKind,
    pub(super) surface: CatalogDeclarationSurface,
    pub(super) implementation_hook: Option<ImplementationHook>,
    pub(super) availability_rule: AvailabilityRule,
}

impl RecognizedStandardLibraryDeclarationDescriptor {
    /// Returns the compact catalog-local declaration identity.
    pub const fn id(&self) -> RecognizedStandardLibraryDeclarationId {
        self.id
    }

    /// Returns the stable recognition identity of this declaration.
    pub const fn key(&self) -> &RecognizedStandardLibraryDeclarationKey {
        &self.key
    }

    /// Returns the direct recognized scope or declaration owner.
    pub const fn owner(&self) -> RecognizedStandardLibraryDeclarationOwner {
        self.owner
    }

    /// Returns the expected declaration category.
    pub const fn kind(&self) -> CatalogDeclarationKind {
        self.kind
    }

    /// Returns the stable handle to generated pre-parsed declaration syntax.
    pub const fn surface(&self) -> CatalogDeclarationSurface {
        self.surface
    }

    /// Returns recognized compiler behavior when this declaration has it.
    pub const fn implementation_hook(&self) -> Option<ImplementationHook> {
        self.implementation_hook
    }

    /// Returns the target-availability predicate selected by the catalog.
    pub const fn availability_rule(&self) -> AvailabilityRule {
        self.availability_rule
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};

    use super::{
        CatalogDeclarationKind, CompilerKnownDeclarationDescriptor, CompilerKnownDeclarationOwner,
    };
    use crate::catalog::{
        CatalogDeclarationSurface, CatalogSourceAnchor, CompilerKnownDeclarationId,
        CompilerKnownDeclarationKey, generator_input_inventory,
    };
    use crate::{AvailabilityRule, ImplementationHook, RepresentationRole};

    #[test]
    fn descriptors_retain_typed_identity_owner_surface_and_metadata() {
        let descriptor = declaration_descriptor(
            0,
            "RawPointerRead",
            CompilerKnownDeclarationOwner::Declaration(declaration_id(1)),
        );

        assert_eq!(descriptor.id().raw(), 0);
        assert_eq!(descriptor.key().as_str(), "RawPointerRead");

        assert_eq!(
            descriptor.owner(),
            CompilerKnownDeclarationOwner::Declaration(declaration_id(1))
        );

        assert_eq!(descriptor.kind(), CatalogDeclarationKind::Function);
        assert_eq!(descriptor.surface().anchor().source().raw(), 0);

        assert_eq!(
            descriptor.representation_role(),
            Some(RepresentationRole::RawPointer)
        );
        assert_eq!(
            descriptor.implementation_hook(),
            Some(ImplementationHook::RawPointerRead)
        );
        assert_eq!(descriptor.availability_rule(), AvailabilityRule::RawMemory);
    }

    #[test]
    fn descriptor_contracts_are_shareable_between_compilations() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<CompilerKnownDeclarationDescriptor>();
    }

    fn declaration_descriptor(
        id: u32,
        key: &str,
        owner: CompilerKnownDeclarationOwner,
    ) -> CompilerKnownDeclarationDescriptor {
        CompilerKnownDeclarationDescriptor {
            id: declaration_id(id),
            key: declaration_key(key),
            owner,
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

    fn declaration_key(value: &str) -> CompilerKnownDeclarationKey {
        match CompilerKnownDeclarationKey::try_new(value) {
            Some(key) => key,
            None => panic!("test declaration key is valid"),
        }
    }

    fn declaration_id(raw: u32) -> CompilerKnownDeclarationId {
        CompilerKnownDeclarationId::new(raw)
    }
}
