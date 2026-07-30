use bray_bound_tree::{BoundNodeOrigin, BoundSourceAnchor};
use bray_symbols::ProductIdentity;

use crate::MirHelperReference;

/// Source or compiler-generated product that owns one MIR unit.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirSourceOrigin {
    /// A checked source snapshot.
    Source(BoundSourceAnchor),
    /// A compiler-generated executable host.
    ExecutableHost(ProductIdentity),
    /// A compiler-generated type-specialized lifecycle definition.
    GeneratedLifecycle(MirHelperReference),
}

/// Source-correlated provenance for a MIR element.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirSourceAnchor {
    /// Source or synthesized provenance inherited from checked HIR.
    Source(BoundNodeOrigin),
    /// Provenance belonging to a compiler-generated executable host.
    ExecutableHost(ProductIdentity),
    /// Provenance belonging to a compiler-generated type-specialized lifecycle definition.
    GeneratedLifecycle(MirHelperReference),
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

    pub(crate) fn belongs_to(&self, owner: &MirSourceOrigin) -> bool {
        match (self, owner) {
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
            (Self::Source(_), MirSourceOrigin::ExecutableHost(_))
            | (Self::Source(_), MirSourceOrigin::GeneratedLifecycle(_))
            | (Self::ExecutableHost(_), MirSourceOrigin::Source(_))
            | (Self::ExecutableHost(_), MirSourceOrigin::GeneratedLifecycle(_))
            | (Self::GeneratedLifecycle(_), MirSourceOrigin::Source(_))
            | (Self::GeneratedLifecycle(_), MirSourceOrigin::ExecutableHost(_)) => false,
        }
    }
}

impl From<BoundSourceAnchor> for MirSourceAnchor {
    fn from(source: BoundSourceAnchor) -> Self {
        Self::Source(BoundNodeOrigin::source(source))
    }
}
