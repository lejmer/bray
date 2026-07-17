use bray_bound_tree::{BoundNodeOrigin, BoundSourceAnchor};
use bray_symbols::ProductIdentity;

/// Source or compiler-generated product that owns one MIR unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirSourceOrigin {
    /// A checked source snapshot.
    Source(BoundSourceAnchor),
    /// A compiler-generated executable host.
    ExecutableHost(ProductIdentity),
}

/// Source-correlated provenance for a MIR element.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirSourceAnchor {
    /// Source or synthesized provenance inherited from checked HIR.
    Source(BoundNodeOrigin),
    /// Provenance belonging to a compiler-generated executable host.
    ExecutableHost(ProductIdentity),
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
            (Self::Source(_), MirSourceOrigin::ExecutableHost(_))
            | (Self::ExecutableHost(_), MirSourceOrigin::Source(_)) => false,
        }
    }
}

impl From<BoundSourceAnchor> for MirSourceAnchor {
    fn from(source: BoundSourceAnchor) -> Self {
        Self::Source(BoundNodeOrigin::source(source))
    }
}
