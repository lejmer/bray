use std::sync::Arc;

use bray_base::{shared_slice, shared_str};
use bray_declarations::DeclarationId;

use crate::SymbolKind;

/// A canonical package identity supplied by the package layer.
///
/// Symbol infrastructure treats the value as opaque. Package selection and canonicalization own
/// its syntax and normalization rules.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PackageIdentity(Arc<str>);

impl PackageIdentity {
    /// Creates a package identity unless the canonical representation is empty.
    pub fn try_new(value: impl Into<Arc<str>>) -> Option<Self> {
        let value = shared_str(value);
        if value.is_empty() {
            return None;
        }

        Some(Self(value))
    }

    /// Returns the opaque canonical package identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for PackageIdentity {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// A non-empty logical module path used in deterministic symbol keys.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ModulePathKey(Arc<[Arc<str>]>);

impl ModulePathKey {
    /// Creates a logical path when it contains at least one non-empty segment.
    pub fn try_new<I, S>(segments: I) -> Option<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<Arc<str>>,
    {
        let segments: Vec<Arc<str>> = segments.into_iter().map(Into::into).collect();
        if segments.is_empty() || segments.iter().any(|segment| segment.is_empty()) {
            return None;
        }

        Some(Self(shared_slice(segments)))
    }

    /// Iterates over the path segments in semantic order.
    pub fn segments(&self) -> impl ExactSizeIterator<Item = &str> {
        self.0.iter().map(AsRef::as_ref)
    }
}

/// A stable source-order ordinal used when a key permits repeated unnamed entities.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SymbolOrdinal(u32);

impl SymbolOrdinal {
    /// Creates an ordinal from its stable source-order value.
    pub const fn new(value: u32) -> Self {
        Self(value)
    }

    /// Returns the stable source-order value.
    pub const fn raw(self) -> u32 {
        self.0
    }

    /// Converts the ordinal to a checked collection index for the current target.
    pub fn to_index(self) -> Option<usize> {
        usize::try_from(self.0).ok()
    }
}

/// Identifies a deterministic symbol-tree root independently of numeric symbol IDs.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SymbolRootKey {
    /// The single compiler-known environment root.
    CompilerKnownEnvironment,
    /// A package root identified by the package layer.
    Package(PackageIdentity),
}

impl SymbolRootKey {
    /// Returns the semantic symbol kind represented by this root key.
    pub const fn kind(&self) -> SymbolKind {
        match self {
            Self::CompilerKnownEnvironment => SymbolKind::CompilerKnownEnvironment,
            Self::Package(_) => SymbolKind::Package,
        }
    }
}

/// The closed role vocabulary for synthesized declaration-surface symbols.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SynthesizedSymbolRole {
    /// The receiver parameter attached to a callable member.
    ReceiverParameter,
    /// An inferred type parameter attached to an implementation.
    InferredImplementationTypeParameter,
    /// An inferred constant parameter attached to an implementation.
    InferredImplementationConstParameter,
    /// A provider that binds a callable parameter default.
    CallableParameterDefaultProvider,
    /// A provider that binds a struct field default.
    StructFieldDefaultProvider,
    /// A provider that binds a union payload field default.
    UnionPayloadDefaultProvider,
}

impl SynthesizedSymbolRole {
    /// Returns the exact symbol kind produced by this synthesized role.
    pub const fn kind(self) -> SymbolKind {
        match self {
            Self::ReceiverParameter => SymbolKind::ReceiverParameter,
            Self::InferredImplementationTypeParameter => SymbolKind::GenericTypeParameter,
            Self::InferredImplementationConstParameter => SymbolKind::GenericConstParameter,
            Self::CallableParameterDefaultProvider => SymbolKind::CallableParameterDefaultProvider,
            Self::StructFieldDefaultProvider => SymbolKind::StructFieldDefaultProvider,
            Self::UnionPayloadDefaultProvider => SymbolKind::UnionPayloadDefaultProvider,
        }
    }
}

/// A deterministic key for a synthesized declaration-surface symbol.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SynthesizedSymbolKey {
    role: SynthesizedSymbolRole,
    subject: SymbolKey,
    ordinal: Option<SymbolOrdinal>,
}

impl SynthesizedSymbolKey {
    /// Creates the implicit receiver key for a callable member.
    pub fn receiver_parameter(subject: SymbolKey) -> Self {
        Self::without_ordinal(SynthesizedSymbolRole::ReceiverParameter, subject)
    }

    /// Creates an inferred implementation type-parameter key.
    pub fn inferred_implementation_type_parameter(
        subject: SymbolKey,
        ordinal: SymbolOrdinal,
    ) -> Self {
        Self::with_ordinal(
            SynthesizedSymbolRole::InferredImplementationTypeParameter,
            subject,
            ordinal,
        )
    }

    /// Creates an inferred implementation constant-parameter key.
    pub fn inferred_implementation_const_parameter(
        subject: SymbolKey,
        ordinal: SymbolOrdinal,
    ) -> Self {
        Self::with_ordinal(
            SynthesizedSymbolRole::InferredImplementationConstParameter,
            subject,
            ordinal,
        )
    }

    /// Creates a callable parameter default-provider key.
    pub fn callable_parameter_default_provider(subject: SymbolKey) -> Self {
        Self::without_ordinal(
            SynthesizedSymbolRole::CallableParameterDefaultProvider,
            subject,
        )
    }

    /// Creates a struct field default-provider key.
    pub fn struct_field_default_provider(subject: SymbolKey) -> Self {
        Self::without_ordinal(SynthesizedSymbolRole::StructFieldDefaultProvider, subject)
    }

    /// Creates a union payload field default-provider key.
    pub fn union_payload_default_provider(subject: SymbolKey) -> Self {
        Self::without_ordinal(SynthesizedSymbolRole::UnionPayloadDefaultProvider, subject)
    }

    /// Returns this key's synthesized role.
    pub const fn role(&self) -> SynthesizedSymbolRole {
        self.role
    }

    /// Returns the symbol key from which this symbol is synthesized.
    pub const fn subject(&self) -> &SymbolKey {
        &self.subject
    }

    /// Returns the stable ordinal when the role permits repeated synthesized symbols.
    pub const fn ordinal(&self) -> Option<SymbolOrdinal> {
        self.ordinal
    }

    /// Returns the exact symbol kind produced by this key.
    pub const fn kind(&self) -> SymbolKind {
        self.role.kind()
    }

    fn without_ordinal(role: SynthesizedSymbolRole, subject: SymbolKey) -> Self {
        Self {
            role,
            subject,
            ordinal: None,
        }
    }

    fn with_ordinal(
        role: SynthesizedSymbolRole,
        subject: SymbolKey,
        ordinal: SymbolOrdinal,
    ) -> Self {
        Self {
            role,
            subject,
            ordinal: Some(ordinal),
        }
    }
}

/// Structured data forming a deterministic semantic symbol key.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SymbolKeyData {
    /// A package or compiler-known environment root.
    Root(SymbolRootKey),
    /// A logical module under a package or compiler-known environment.
    Module {
        /// The root owning the logical module.
        owner: SymbolRootKey,
        /// The module's complete logical path.
        path: ModulePathKey,
    },
    /// A source declaration under its immediate semantic owner.
    SourceDeclaration {
        /// The immediate semantic owner.
        owner: SymbolKey,
        /// The semantic kind introduced by the declaration.
        kind: SymbolKind,
        /// The declaration discovery identity.
        declaration: DeclarationId,
    },
    /// A symbol synthesized from another semantic surface symbol.
    Synthesized(SynthesizedSymbolKey),
}

impl SymbolKeyData {
    /// Returns the semantic symbol kind represented by this key data.
    pub const fn kind(&self) -> SymbolKind {
        match self {
            Self::Root(root) => root.kind(),
            Self::Module { .. } => SymbolKind::Module,
            Self::SourceDeclaration { kind, .. } => *kind,
            Self::Synthesized(key) => key.kind(),
        }
    }
}

/// A cheaply cloned deterministic semantic construction key for a surface symbol.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SymbolKey(Arc<SymbolKeyData>);

impl SymbolKey {
    /// Creates the single compiler-known environment root key.
    pub fn compiler_known_environment() -> Self {
        Self::root(SymbolRootKey::CompilerKnownEnvironment)
    }

    /// Creates a package root key.
    pub fn package(identity: PackageIdentity) -> Self {
        Self::root(SymbolRootKey::Package(identity))
    }

    /// Creates a logical module key under a root.
    pub fn module(owner: SymbolRootKey, path: ModulePathKey) -> Self {
        Self(Arc::new(SymbolKeyData::Module { owner, path }))
    }

    /// Creates a key for a source declaration under its immediate semantic owner.
    ///
    /// Returns `None` for roots, modules, synthesized-only kinds, and body-local kinds.
    pub fn source_declaration(
        owner: SymbolKey,
        kind: SymbolKind,
        declaration: DeclarationId,
    ) -> Option<Self> {
        if !kind.can_be_source_declared() {
            return None;
        }

        Some(Self(Arc::new(SymbolKeyData::SourceDeclaration {
            owner,
            kind,
            declaration,
        })))
    }

    /// Creates a key for a synthesized declaration-surface symbol.
    pub fn synthesized(key: SynthesizedSymbolKey) -> Self {
        Self(Arc::new(SymbolKeyData::Synthesized(key)))
    }

    /// Returns the structured data forming this key.
    pub fn data(&self) -> &SymbolKeyData {
        &self.0
    }

    /// Returns the semantic symbol kind represented by this key.
    pub fn kind(&self) -> SymbolKind {
        self.0.kind()
    }

    fn root(root: SymbolRootKey) -> Self {
        Self(Arc::new(SymbolKeyData::Root(root)))
    }
}

#[cfg(test)]
mod tests {
    use bray_declarations::DeclarationId;

    use super::{
        ModulePathKey, PackageIdentity, SymbolKey, SymbolOrdinal, SymbolRootKey,
        SynthesizedSymbolKey, SynthesizedSymbolRole,
    };
    use crate::SymbolKind;

    fn package_identity() -> PackageIdentity {
        match PackageIdentity::try_new("example.package") {
            Some(identity) => identity,
            None => panic!("test package identity is non-empty"),
        }
    }

    fn module_path() -> ModulePathKey {
        match ModulePathKey::try_new(["example", "module"]) {
            Some(path) => path,
            None => panic!("test module path is valid"),
        }
    }

    fn module_key() -> SymbolKey {
        SymbolKey::module(SymbolRootKey::Package(package_identity()), module_path())
    }

    fn source_key(kind: SymbolKind, declaration: u32) -> SymbolKey {
        match SymbolKey::source_declaration(module_key(), kind, DeclarationId::new(declaration)) {
            Some(key) => key,
            None => panic!("test symbol kind is source-declared"),
        }
    }

    #[test]
    fn package_and_module_path_keys_reject_empty_components() {
        assert_eq!(PackageIdentity::try_new(""), None);
        assert_eq!(ModulePathKey::try_new(Vec::<&str>::new()), None);
        assert_eq!(ModulePathKey::try_new(["example", ""]), None);

        let path = module_path();

        assert_eq!(path.segments().len(), 2);
        assert_eq!(path.segments().collect::<Vec<_>>(), ["example", "module"]);
    }

    #[test]
    fn independently_constructed_keys_are_deterministic() {
        let first = source_key(SymbolKind::Function, 4);
        let second = source_key(SymbolKind::Function, 4);

        assert_eq!(first, second);
        assert_eq!(first.kind(), SymbolKind::Function);
    }

    #[test]
    fn duplicate_declarations_remain_distinct() {
        let first = source_key(SymbolKind::Function, 4);
        let duplicate = source_key(SymbolKind::Function, 5);

        assert_ne!(first, duplicate);
    }

    #[test]
    fn roots_and_module_owners_participate_in_identity() {
        let package_module = module_key();

        let compiler_module =
            SymbolKey::module(SymbolRootKey::CompilerKnownEnvironment, module_path());

        assert_ne!(package_module, compiler_module);
        assert_eq!(package_module.kind(), SymbolKind::Module);
        assert_eq!(compiler_module.kind(), SymbolKind::Module);

        assert_ne!(
            SymbolKey::package(package_identity()),
            SymbolKey::compiler_known_environment()
        );
    }

    #[test]
    fn source_keys_reject_non_source_symbol_kinds() {
        let owner = module_key();

        assert_eq!(
            SymbolKey::source_declaration(
                owner.clone(),
                SymbolKind::ReceiverParameter,
                DeclarationId::new(0)
            ),
            None
        );

        assert_eq!(
            SymbolKey::source_declaration(owner, SymbolKind::LocalBinding, DeclarationId::new(1)),
            None
        );
    }

    #[test]
    fn synthesized_roles_determine_kinds_and_ordinals() {
        let implementation = source_key(SymbolKind::InherentImplementation, 8);

        let first = SynthesizedSymbolKey::inferred_implementation_type_parameter(
            implementation.clone(),
            SymbolOrdinal::new(0),
        );

        let second = SynthesizedSymbolKey::inferred_implementation_type_parameter(
            implementation,
            SymbolOrdinal::new(1),
        );

        assert_eq!(
            first.role(),
            SynthesizedSymbolRole::InferredImplementationTypeParameter
        );

        assert_eq!(first.kind(), SymbolKind::GenericTypeParameter);
        assert_eq!(first.ordinal(), Some(SymbolOrdinal::new(0)));
        assert_ne!(first, second);

        let receiver =
            SynthesizedSymbolKey::receiver_parameter(source_key(SymbolKind::TypeCallableMember, 9));

        assert_eq!(receiver.kind(), SymbolKind::ReceiverParameter);
        assert_eq!(receiver.ordinal(), None);

        assert_eq!(
            SymbolKey::synthesized(receiver).kind(),
            SymbolKind::ReceiverParameter
        );
    }

    #[test]
    fn keys_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<PackageIdentity>();
        assert_send_sync::<ModulePathKey>();
        assert_send_sync::<SymbolKey>();
    }
}
