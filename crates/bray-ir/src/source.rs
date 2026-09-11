use bray_bound_tree::{BoundNodeOrigin, BoundSourceAnchor};
use bray_symbols::{CallableDefinitionId, ProductIdentity};

use crate::{MirHelperReference, MirImportedExecutableKey};

/// Source or compiler-generated product that owns one MIR unit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirSourceOrigin {
    /// An exact callable declaration whose body is compiler-provided.
    CompilerProvidedCallable(CallableDefinitionId),
    /// A checked source snapshot.
    Source(BoundSourceAnchor),
    /// A compiler-generated executable host.
    ExecutableHost(ProductIdentity),
    /// A compiler-generated type-specialized lifecycle definition.
    GeneratedLifecycle(MirHelperReference),
    /// A checked executable template imported from a compiled dependency.
    ImportedExecutable(MirImportedExecutableKey),
}

/// Source-correlated provenance for a MIR element.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirSourceAnchor {
    /// Provenance belonging to an exact compiler-provided callable body.
    CompilerProvidedCallable(CallableDefinitionId),
    /// Source or synthesized provenance inherited from checked HIR.
    Source(BoundNodeOrigin),
    /// Provenance belonging to a compiler-generated executable host.
    ExecutableHost(ProductIdentity),
    /// Provenance belonging to a compiler-generated type-specialized lifecycle definition.
    GeneratedLifecycle(MirHelperReference),
    /// Provenance retained by an imported checked executable template.
    ImportedExecutable(MirImportedExecutableKey),
}

impl MirSourceAnchor {
    /// Creates a source-backed MIR anchor.
    pub const fn source(origin: BoundNodeOrigin) -> Self {
        Self::Source(origin)
    }

    /// Creates a compiler-generated executable-host anchor.
    pub const fn executable_host(product: ProductIdentity) -> Self {
        Self::ExecutableHost(product)
    }

    /// Creates compiler-generated lifecycle provenance.
    pub const fn generated_lifecycle(reference: MirHelperReference) -> Self {
        Self::GeneratedLifecycle(reference)
    }

    /// Creates provenance for a checked body imported from a compiled dependency.
    pub const fn imported_executable(key: MirImportedExecutableKey) -> Self {
        Self::ImportedExecutable(key)
    }

    pub(crate) fn belongs_to(&self, owner: &MirSourceOrigin) -> bool {
        match (self, owner) {
            (
                Self::CompilerProvidedCallable(anchor),
                MirSourceOrigin::CompilerProvidedCallable(owner),
            ) => anchor == owner,
            (Self::CompilerProvidedCallable(_), _)
            | (_, MirSourceOrigin::CompilerProvidedCallable(_)) => false,
            (Self::Source(anchor), MirSourceOrigin::Source(owner)) => {
                let anchor = anchor.source_anchor();

                anchor.syntax().source_id() == owner.syntax().source_id()
                    && anchor.source_version() == owner.source_version()
            }
            (Self::ExecutableHost(anchor), MirSourceOrigin::ExecutableHost(owner)) => {
                anchor == owner
            }
            (Self::GeneratedLifecycle(anchor), MirSourceOrigin::GeneratedLifecycle(owner)) => {
                anchor == owner
            }
            (Self::ImportedExecutable(anchor), MirSourceOrigin::ImportedExecutable(owner)) => {
                anchor == owner
            }
            (Self::Source(_), MirSourceOrigin::ExecutableHost(_))
            | (Self::Source(_), MirSourceOrigin::GeneratedLifecycle(_))
            | (Self::Source(_), MirSourceOrigin::ImportedExecutable(_))
            | (Self::ExecutableHost(_), MirSourceOrigin::Source(_))
            | (Self::ExecutableHost(_), MirSourceOrigin::GeneratedLifecycle(_))
            | (Self::ExecutableHost(_), MirSourceOrigin::ImportedExecutable(_))
            | (Self::GeneratedLifecycle(_), MirSourceOrigin::Source(_))
            | (Self::GeneratedLifecycle(_), MirSourceOrigin::ExecutableHost(_))
            | (Self::GeneratedLifecycle(_), MirSourceOrigin::ImportedExecutable(_))
            | (Self::ImportedExecutable(_), MirSourceOrigin::Source(_))
            | (Self::ImportedExecutable(_), MirSourceOrigin::ExecutableHost(_))
            | (Self::ImportedExecutable(_), MirSourceOrigin::GeneratedLifecycle(_)) => false,
        }
    }
}

impl From<BoundSourceAnchor> for MirSourceAnchor {
    fn from(source: BoundSourceAnchor) -> Self {
        Self::Source(BoundNodeOrigin::source(source))
    }
}
