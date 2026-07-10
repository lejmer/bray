use std::sync::Arc;

use crate::{
    ModulePathKey, PackageIdentity, SymbolKind, SymbolName, SymbolOrdinal, SynthesizedSymbolRole,
};

/// The external identity rule used for a declaration under a semantic owner.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExternalDeclarationIdentity {
    /// Identity is determined by the declaration's ordinary name.
    Name(SymbolName),
    /// Identity is determined by a stable owner-relative ordinal.
    Ordinal(SymbolOrdinal),
}

/// Structured data forming one stable cross-compilation symbol identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ExternalSymbolKeyData {
    /// A package root.
    Package(PackageIdentity),
    /// A logical package-relative module.
    Module {
        /// The package key that owns the module.
        package: ExternalSymbolKey,
        /// The complete package-relative module path.
        path: ModulePathKey,
    },
    /// A declaration identified relative to its immediate semantic owner.
    Declaration {
        /// The immediate semantic owner.
        owner: ExternalSymbolKey,
        /// The exact semantic category of the declaration.
        kind: SymbolKind,
        /// The category-specific owner-relative identity rule.
        identity: ExternalDeclarationIdentity,
    },
    /// A synthesized declaration-surface symbol derived from another external symbol.
    Synthesized {
        /// The immediate semantic owner.
        owner: ExternalSymbolKey,
        /// The closed synthesized role.
        role: SynthesizedSymbolRole,
        /// The owner-relative ordinal required by repeated synthesized roles.
        ordinal: Option<SymbolOrdinal>,
    },
}

impl ExternalSymbolKeyData {
    /// Returns the exact semantic kind represented by this key data.
    pub const fn kind(&self) -> SymbolKind {
        match self {
            Self::Package(_) => SymbolKind::Package,
            Self::Module { .. } => SymbolKind::Module,
            Self::Declaration { kind, .. } => *kind,
            Self::Synthesized { role, .. } => role.kind(),
        }
    }
}

/// A cheaply cloned structured identity for an interface-addressable symbol.
///
/// The key contains semantic ownership and category-specific identity components. It never
/// contains source paths, source ranges, declaration IDs, or compilation-local IDs.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ExternalSymbolKey(Arc<ExternalSymbolKeyData>);

impl ExternalSymbolKey {
    /// Creates an external package-root key.
    pub fn package(identity: PackageIdentity) -> Self {
        Self::from_data(ExternalSymbolKeyData::Package(identity))
    }

    /// Creates a module key when the owner is a package root.
    pub fn module(package: ExternalSymbolKey, path: ModulePathKey) -> Option<Self> {
        if package.kind() != SymbolKind::Package {
            return None;
        }

        Some(Self::from_data(ExternalSymbolKeyData::Module {
            package,
            path,
        }))
    }

    /// Creates a named declaration key when the kind can be interface-addressable.
    pub fn named(owner: ExternalSymbolKey, kind: SymbolKind, name: SymbolName) -> Option<Self> {
        Self::declaration(owner, kind, ExternalDeclarationIdentity::Name(name))
    }

    /// Creates an owner-relative ordinal declaration key.
    ///
    /// This form covers parameters and independently identified declarations such as overload
    /// arms. The producer remains responsible for selecting the language-defined identity form.
    pub fn ordinal(
        owner: ExternalSymbolKey,
        kind: SymbolKind,
        ordinal: SymbolOrdinal,
    ) -> Option<Self> {
        Self::declaration(owner, kind, ExternalDeclarationIdentity::Ordinal(ordinal))
    }

    /// Creates a synthesized key when the role and ordinal shape agree.
    pub fn synthesized(
        owner: ExternalSymbolKey,
        role: SynthesizedSymbolRole,
        ordinal: Option<SymbolOrdinal>,
    ) -> Option<Self> {
        if role_requires_ordinal(role) != ordinal.is_some() {
            return None;
        }

        Some(Self::from_data(ExternalSymbolKeyData::Synthesized {
            owner,
            role,
            ordinal,
        }))
    }

    /// Returns the structured data forming this key.
    pub fn data(&self) -> &ExternalSymbolKeyData {
        &self.0
    }

    /// Returns the exact symbol kind represented by this key.
    pub fn kind(&self) -> SymbolKind {
        self.0.kind()
    }

    /// Returns the immediate semantic owner, or `None` for a package root.
    pub fn owner(&self) -> Option<&Self> {
        match self.data() {
            ExternalSymbolKeyData::Package(_) => None,
            ExternalSymbolKeyData::Module { package, .. } => Some(package),
            ExternalSymbolKeyData::Declaration { owner, .. } => Some(owner),
            ExternalSymbolKeyData::Synthesized { owner, .. } => Some(owner),
        }
    }

    /// Returns the defining package identity.
    pub fn package_identity(&self) -> &PackageIdentity {
        let mut key = self;
        loop {
            match key.data() {
                ExternalSymbolKeyData::Package(identity) => return identity,
                ExternalSymbolKeyData::Module { package, .. } => key = package,
                ExternalSymbolKeyData::Declaration { owner, .. } => key = owner,
                ExternalSymbolKeyData::Synthesized { owner, .. } => key = owner,
            }
        }
    }

    fn declaration(
        owner: ExternalSymbolKey,
        kind: SymbolKind,
        identity: ExternalDeclarationIdentity,
    ) -> Option<Self> {
        if !can_be_external_declaration(kind) {
            return None;
        }

        Some(Self::from_data(ExternalSymbolKeyData::Declaration {
            owner,
            kind,
            identity,
        }))
    }

    fn from_data(data: ExternalSymbolKeyData) -> Self {
        Self(Arc::new(data))
    }
}

const fn can_be_external_declaration(kind: SymbolKind) -> bool {
    kind.has_compilation_wide_id()
        && !matches!(
            kind,
            SymbolKind::CompilerKnownEnvironment
                | SymbolKind::Package
                | SymbolKind::Module
                | SymbolKind::ReceiverParameter
                | SymbolKind::CallableParameterDefaultProvider
                | SymbolKind::StructFieldDefaultProvider
                | SymbolKind::UnionPayloadDefaultProvider
        )
}

const fn role_requires_ordinal(role: SynthesizedSymbolRole) -> bool {
    !matches!(role, SynthesizedSymbolRole::ReceiverParameter)
}

#[cfg(test)]
mod tests {
    use super::{ExternalDeclarationIdentity, ExternalSymbolKey, ExternalSymbolKeyData};
    use crate::{
        ModulePathKey, PackageIdentity, SymbolKind, SymbolName, SymbolOrdinal,
        SynthesizedSymbolRole,
    };

    fn package_identity() -> PackageIdentity {
        match PackageIdentity::try_new("example.package") {
            Some(identity) => identity,
            None => panic!("test package identity must be valid"),
        }
    }

    fn name(value: &str) -> SymbolName {
        match SymbolName::try_new(value) {
            Some(name) => name,
            None => panic!("test symbol name must be valid"),
        }
    }

    fn package_key() -> ExternalSymbolKey {
        ExternalSymbolKey::package(package_identity())
    }

    fn module_key() -> ExternalSymbolKey {
        let Some(path) = ModulePathKey::try_new(["example", "module"]) else {
            panic!("test module path must be valid");
        };

        match ExternalSymbolKey::module(package_key(), path) {
            Some(key) => key,
            None => panic!("package root must be a valid module owner"),
        }
    }

    #[test]
    fn external_keys_are_structured_and_deterministic() {
        let first = ExternalSymbolKey::named(module_key(), SymbolKind::Function, name("run"));
        let second = ExternalSymbolKey::named(module_key(), SymbolKind::Function, name("run"));

        assert_eq!(first, second);

        let Some(first) = first else {
            panic!("function must support named external identity");
        };

        assert_eq!(first.kind(), SymbolKind::Function);
        assert_eq!(first.package_identity(), &package_identity());

        assert!(matches!(
            first.data(),
            ExternalSymbolKeyData::Declaration {
                identity: ExternalDeclarationIdentity::Name(name),
                ..
            } if name.as_str() == "run"
        ));
    }

    #[test]
    fn external_key_components_affect_identity() {
        let named = ExternalSymbolKey::named(module_key(), SymbolKind::Function, name("run"));
        let ordinal =
            ExternalSymbolKey::ordinal(module_key(), SymbolKind::Function, SymbolOrdinal::new(0));
        let other_kind = ExternalSymbolKey::named(module_key(), SymbolKind::Predicate, name("run"));

        assert_ne!(named, ordinal);
        assert_ne!(named, other_kind);
    }

    #[test]
    fn invalid_external_key_shapes_are_rejected() {
        assert_eq!(SymbolName::try_new(""), None);

        assert_eq!(
            ExternalSymbolKey::named(module_key(), SymbolKind::LocalBinding, name("local")),
            None
        );

        let Some(path) = ModulePathKey::try_new(["nested"]) else {
            panic!("test module path must be valid");
        };

        assert_eq!(ExternalSymbolKey::module(module_key(), path), None);

        assert_eq!(
            ExternalSymbolKey::synthesized(
                module_key(),
                SynthesizedSymbolRole::ReceiverParameter,
                Some(SymbolOrdinal::new(0))
            ),
            None
        );
    }

    #[test]
    fn synthesized_keys_validate_role_ordinals() {
        let implementation = ExternalSymbolKey::ordinal(
            module_key(),
            SymbolKind::InherentImplementation,
            SymbolOrdinal::new(0),
        );

        let Some(implementation) = implementation else {
            panic!("implementation must support owner-relative identity");
        };

        let inferred = ExternalSymbolKey::synthesized(
            implementation,
            SynthesizedSymbolRole::InferredImplementationTypeParameter,
            Some(SymbolOrdinal::new(1)),
        );

        let Some(inferred) = inferred else {
            panic!("inferred parameter role requires an ordinal");
        };

        assert_eq!(inferred.kind(), SymbolKind::GenericTypeParameter);
    }

    #[test]
    fn external_keys_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ExternalSymbolKey>();
        assert_send_sync::<SymbolName>();
    }
}
