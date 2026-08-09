use std::collections::BTreeMap;
use std::sync::Arc;

use bray_base::NonEmptySharedStr;
pub use bray_symbols::SymbolRelationshipKind;
use bray_symbols::{
    CallablePosition, ExternalSymbolKey, ImportedIdentitySurfaceError,
    ImportedPackageIdentitySurface, InterfaceSymbolId, PackageIdentity, PackageVersion, SymbolName,
};

use crate::InterfaceContentHash;

pub(super) const MAXIMUM_COMPILER_KNOWN_KEY_COMPONENTS: usize = 256;

/// Opaque package-layer identity of one selected product.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceProductIdentity(NonEmptySharedStr);

impl InterfaceProductIdentity {
    /// Creates a product identity unless its canonical representation is empty.
    pub fn try_new(value: impl Into<Arc<str>>) -> Option<Self> {
        NonEmptySharedStr::try_new(value).map(Self)
    }

    /// Returns the opaque canonical product identity.
    pub fn as_str(&self) -> &str {
        self.0.as_str()
    }
}

impl AsRef<str> for InterfaceProductIdentity {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Language-defined kind of a selected package product.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceProductKind {
    /// An importable library surface.
    Library,
    /// A product with a runtime entry point.
    Executable,
    /// A product selected for test compilation.
    Test,
}

/// Package and product identity recorded by one compiled interface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageInterfaceIdentity {
    package: PackageIdentity,
    version: PackageVersion,
    product: InterfaceProductIdentity,
    kind: InterfaceProductKind,
    public_surface: NonEmptySharedStr,
}

impl PackageInterfaceIdentity {
    /// Creates package metadata when the package layer supplied a non-empty surface identity.
    pub fn try_new(
        package: PackageIdentity,
        version: PackageVersion,
        product: InterfaceProductIdentity,
        kind: InterfaceProductKind,
        public_surface: impl Into<Arc<str>>,
    ) -> Option<Self> {
        let public_surface = NonEmptySharedStr::try_new(public_surface)?;

        Some(Self {
            package,
            version,
            product,
            kind,
            public_surface,
        })
    }

    /// Returns the package identity shared by package products.
    pub const fn package(&self) -> &PackageIdentity {
        &self.package
    }

    /// Returns the semantic version of the package that produced this interface.
    pub const fn version(&self) -> &PackageVersion {
        &self.version
    }

    /// Returns the selected product identity.
    pub const fn product(&self) -> &InterfaceProductIdentity {
        &self.product
    }

    /// Returns the selected product kind.
    pub const fn kind(&self) -> InterfaceProductKind {
        self.kind
    }

    /// Returns the package-layer public-surface identity.
    pub fn public_surface(&self) -> &str {
        self.public_surface.as_str()
    }
}

/// Compact reference into one interface's canonical dependency table.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DependencyInterfaceId(u32);

impl DependencyInterfaceId {
    /// Creates an ID from its compact numeric representation.
    pub const fn new(raw: u32) -> Self {
        Self(raw)
    }

    /// Returns the compact numeric representation.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Converts this ID to a checked collection index.
    pub fn to_index(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }

    pub(crate) fn try_from_index(index: usize) -> Option<Self> {
        u32::try_from(index).ok().map(Self)
    }
}

/// Exact identity and content expected from one selected dependency interface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceDependency {
    package: PackageIdentity,
    product: InterfaceProductIdentity,
    content_hash: InterfaceContentHash,
}

impl InterfaceDependency {
    /// Creates an immutable dependency reference.
    pub const fn new(
        package: PackageIdentity,
        product: InterfaceProductIdentity,
        content_hash: InterfaceContentHash,
    ) -> Self {
        Self {
            package,
            product,
            content_hash,
        }
    }

    /// Returns the selected dependency package.
    pub const fn package(&self) -> &PackageIdentity {
        &self.package
    }

    /// Returns the selected dependency product.
    pub const fn product(&self) -> &InterfaceProductIdentity {
        &self.product
    }

    /// Returns the exact semantic content hash expected by the producer.
    pub const fn content_hash(&self) -> InterfaceContentHash {
        self.content_hash
    }
}

/// One ordered typed relationship between interface-local symbols.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SymbolRelationship {
    kind: SymbolRelationshipKind,
    owner: InterfaceSymbolId,
    member: InterfaceSymbolId,
    ordinal: u32,
    position: CallablePosition,
    allows_mutation: bool,
}

impl SymbolRelationship {
    /// Creates one owner-relative typed relationship record.
    pub const fn new(
        kind: SymbolRelationshipKind,
        owner: InterfaceSymbolId,
        member: InterfaceSymbolId,
        ordinal: u32,
    ) -> Self {
        Self {
            kind,
            owner,
            member,
            ordinal,
            position: CallablePosition::NamedOnly,
            allows_mutation: false,
        }
    }

    /// Marks the related field as permitting mutation after initialization.
    pub const fn with_mutation(mut self) -> Self {
        self.allows_mutation = true;

        self
    }

    /// Sets the related payload field's call-position permission.
    pub const fn with_position(mut self, position: CallablePosition) -> Self {
        self.position = position;

        self
    }

    /// Returns the closed relationship category.
    pub const fn kind(self) -> SymbolRelationshipKind {
        self.kind
    }

    /// Returns the relationship owner.
    pub const fn owner(self) -> InterfaceSymbolId {
        self.owner
    }

    /// Returns the related member or referenced arm.
    pub const fn member(self) -> InterfaceSymbolId {
        self.member
    }

    /// Returns the stable owner-relative position.
    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }

    /// Returns the related payload field's call-position permission.
    pub const fn position(self) -> CallablePosition {
        self.position
    }

    /// Returns whether the related field permits mutation after initialization.
    pub const fn allows_mutation(self) -> bool {
        self.allows_mutation
    }
}

/// Target identity projected through an exported lookup edge.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceSymbolReference {
    /// A symbol defined by this interface.
    Local(InterfaceSymbolId),
    /// A symbol retaining its identity from a selected dependency.
    Dependency {
        /// Canonical dependency-table slot.
        dependency: DependencyInterfaceId,
        /// Stable symbol identity in the dependency interface.
        key: ExternalSymbolKey,
    },
    /// A language-defined symbol supplied by every compatible compiler.
    CompilerKnown(CompilerKnownSymbolReference),
}

/// A validated stable reference into the compiler-known symbol surface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompilerKnownSymbolReference(bray_symbols::SymbolKey);

impl CompilerKnownSymbolReference {
    /// Creates a reference for a compiler-known declaration or one of its synthesized children.
    pub fn try_new(key: bray_symbols::SymbolKey) -> Option<Self> {
        if is_compiler_known_key(&key) {
            Some(Self(key))
        } else {
            None
        }
    }

    /// Returns the stable symbol key expected from the compatible compiler catalog.
    pub const fn key(&self) -> &bray_symbols::SymbolKey {
        &self.0
    }

    /// Returns the exact semantic category expected from the compatible compiler catalog.
    pub fn kind(&self) -> bray_symbols::SymbolKind {
        self.0.kind()
    }
}

fn is_compiler_known_key(key: &bray_symbols::SymbolKey) -> bool {
    let mut current = key;
    let mut component_count = 0;

    loop {
        component_count += 1;

        if component_count > MAXIMUM_COMPILER_KNOWN_KEY_COMPONENTS {
            return false;
        }

        match current.data() {
            bray_symbols::SymbolKeyData::CompilerKnownDeclaration { .. } => return true,
            bray_symbols::SymbolKeyData::Synthesized(key) => current = key.subject(),
            bray_symbols::SymbolKeyData::Root(_)
            | bray_symbols::SymbolKeyData::Module { .. }
            | bray_symbols::SymbolKeyData::SourceDeclaration { .. }
            | bray_symbols::SymbolKeyData::External(_) => return false,
        }
    }
}

/// Whether an exported name is declared at its owner or projected from elsewhere.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExportedLookupKind {
    /// The target is declared directly by the lookup owner.
    Direct,
    /// The target retains an identity declared elsewhere.
    ReExport,
}

/// One ordinary name in a package or module's exported lookup surface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExportedLookupEdge {
    owner: InterfaceSymbolId,
    name: SymbolName,
    kind: ExportedLookupKind,
    target: InterfaceSymbolReference,
}

impl ExportedLookupEdge {
    /// Creates one exported ordinary-name edge.
    pub const fn new(
        owner: InterfaceSymbolId,
        name: SymbolName,
        kind: ExportedLookupKind,
        target: InterfaceSymbolReference,
    ) -> Self {
        Self {
            owner,
            name,
            kind,
            target,
        }
    }

    /// Returns the package or module supplying the exported lookup surface.
    pub const fn owner(&self) -> InterfaceSymbolId {
        self.owner
    }

    /// Returns the projected ordinary name.
    pub const fn name(&self) -> &SymbolName {
        &self.name
    }

    /// Returns whether this is a direct export or re-export.
    pub const fn kind(&self) -> ExportedLookupKind {
        self.kind
    }

    /// Returns the local or dependency-defined target identity.
    pub const fn target(&self) -> &InterfaceSymbolReference {
        &self.target
    }

    pub(super) fn remap_dependency(
        mut self,
        remap: &[DependencyInterfaceId],
    ) -> Result<Self, DependencyInterfaceId> {
        if let InterfaceSymbolReference::Dependency { dependency, .. } = &mut self.target {
            let original = *dependency;

            let Some(canonical) = original
                .to_index()
                .and_then(|index| remap.get(index))
                .copied()
            else {
                return Err(original);
            };

            *dependency = canonical;
        }

        Ok(self)
    }
}

/// Invalid semantic input supplied for package-interface identity sections.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum PackageInterfaceSurfaceBuildError {
    /// Compiled interfaces can describe only library products.
    NonLibraryProduct,
    /// The dependency table cannot use compact dependency IDs.
    DependencyCountOverflow,
    /// More than one selected dependency uses the same package identity.
    DuplicateDependencyPackage(PackageIdentity),
    /// Symbol identities do not use canonical external-key order.
    NonCanonicalSymbolOrder {
        /// Previous symbol in the supplied table.
        previous: InterfaceSymbolId,
        /// Current symbol that does not sort after `previous`.
        current: InterfaceSymbolId,
    },
    /// The decoded symbol skeleton violates its semantic identity contract.
    Identity(ImportedIdentitySurfaceError),
    /// A typed relationship references a symbol outside the identity table.
    RelationshipSymbolOutOfBounds(SymbolRelationship),
    /// A relationship's owner, member, or containment shape is invalid.
    InvalidRelationship(SymbolRelationship),
    /// Two relationships occupy the same typed owner-relative position.
    DuplicateRelationshipPosition(SymbolRelationship),
    /// An exported lookup owner is outside the identity table.
    ExportOwnerOutOfBounds(InterfaceSymbolId),
    /// Only package and module symbols can own exported lookup surfaces.
    InvalidExportOwner(InterfaceSymbolId),
    /// A local export target is outside the identity table.
    ExportTargetOutOfBounds(InterfaceSymbolId),
    /// A dependency target references a missing dependency slot.
    DependencyOutOfBounds(DependencyInterfaceId),
    /// A dependency target key belongs to a different package.
    DependencyKeyPackageMismatch(DependencyInterfaceId),
    /// A direct export does not target a declaration contained by its owner.
    InvalidDirectExportTarget(InterfaceSymbolId),
    /// Two export edges project the same ordinary name from one owner.
    DuplicateExportName {
        /// Exporting package or module.
        owner: InterfaceSymbolId,
        /// Duplicate projected name.
        name: SymbolName,
    },
}

/// Canonical package identity and symbol-surface data ready for section encoding.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct PackageInterfaceSurface {
    pub(super) identity: PackageInterfaceIdentity,
    pub(super) dependencies: Arc<[InterfaceDependency]>,
    pub(super) symbols: ImportedPackageIdentitySurface,
    pub(super) relationships: Arc<[SymbolRelationship]>,
    pub(super) exports: Arc<[ExportedLookupEdge]>,
    pub(super) symbol_index: BTreeMap<ExternalSymbolKey, InterfaceSymbolId>,
    pub(super) export_index: BTreeMap<(InterfaceSymbolId, SymbolName), usize>,
}

impl PackageInterfaceSurface {
    /// Returns the package and selected product identity.
    pub const fn identity(&self) -> &PackageInterfaceIdentity {
        &self.identity
    }

    /// Returns dependencies in canonical package-identity order.
    pub fn dependencies(&self) -> &[InterfaceDependency] {
        &self.dependencies
    }

    /// Returns the validated canonical symbol identity skeleton.
    pub const fn symbols(&self) -> &ImportedPackageIdentitySurface {
        &self.symbols
    }

    /// Returns typed relationships in canonical category and owner order.
    pub fn relationships(&self) -> &[SymbolRelationship] {
        &self.relationships
    }

    /// Returns exported lookup edges in canonical owner and name order.
    pub fn exports(&self) -> &[ExportedLookupEdge] {
        &self.exports
    }

    /// Resolves one exact external key through the immutable identity index.
    pub fn symbol_by_external_key(&self, key: &ExternalSymbolKey) -> Option<InterfaceSymbolId> {
        self.symbol_index.get(key).copied()
    }

    /// Resolves one exported ordinary name from a package or module surface.
    pub fn exported_lookup(
        &self,
        owner: InterfaceSymbolId,
        name: &str,
    ) -> Option<&ExportedLookupEdge> {
        let (_, index) = self
            .export_index
            .get_key_value(&(owner, SymbolName::try_new(name)?))?;

        self.exports.get(*index)
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::CompilerKnownDeclarationKey;
    use bray_symbols::{SymbolKey, SymbolKind, SynthesizedSymbolKey};

    use super::{CompilerKnownSymbolReference, MAXIMUM_COMPILER_KNOWN_KEY_COMPONENTS};

    #[test]
    fn compiler_known_references_bound_synthesized_key_depth() {
        let declaration = CompilerKnownDeclarationKey::try_new("TestFunction")
            .unwrap_or_else(|| panic!("test compiler-known key must be valid"));

        let mut key = SymbolKey::compiler_known_declaration(declaration, SymbolKind::Function)
            .unwrap_or_else(|| panic!("test compiler-known function key must be valid"));

        for _ in 1..MAXIMUM_COMPILER_KNOWN_KEY_COMPONENTS {
            key = SymbolKey::synthesized(SynthesizedSymbolKey::receiver_parameter(key));
        }

        assert!(CompilerKnownSymbolReference::try_new(key.clone()).is_some());

        let excessive = SymbolKey::synthesized(SynthesizedSymbolKey::receiver_parameter(key));

        assert!(CompilerKnownSymbolReference::try_new(excessive).is_none());
    }
}
