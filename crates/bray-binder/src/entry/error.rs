use bray_bound_tree::BoundSourceAnchor;
use bray_symbols::{AnySymbolId, SemanticValueStoreError, SymbolKey};

use crate::{BindingError, BoundUnitConstructionError};

/// A failure outside ordinary source diagnostics while binding one semantic unit.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum BoundUnitBindingError<Upstream = std::convert::Infallible> {
    /// Cancellation was observed before a complete unit could be returned.
    Cancelled,
    /// Semantic checking could not complete because a typed dependency was unavailable.
    CheckerInfrastructure(bray_checker::CheckerInfrastructureError),
    /// The coordinating query layer returned one of its own exact failures.
    Upstream(Upstream),
    /// The requested unit key does not identify supported source-backed syntax.
    InvalidUnitKey,
    /// The source syntax selected by the unit key is unavailable.
    MissingSyntax {
        /// The exact versioned source construct that could not be recovered.
        source: BoundSourceAnchor,
    },
    /// The unit owner does not resolve to a source symbol in the supplied graph.
    MissingOwner {
        /// The exact versioned source construct being bound.
        source: BoundSourceAnchor,
        /// The stable identity expected to resolve as the unit owner.
        owner: SymbolKey,
        /// A resolved related symbol whose record or owner relationship is absent, when known.
        symbol: Option<AnySymbolId>,
    },
    /// The unit owner is not contained by a logical module.
    MissingModule {
        /// The exact versioned source construct being bound.
        source: BoundSourceAnchor,
        /// The resolved owner whose containing module is absent.
        owner: AnySymbolId,
    },
    /// A surface symbol supplied to the unit has no valid local lookup name.
    InvalidSurfaceName {
        /// The exact versioned source construct being bound.
        source: BoundSourceAnchor,
        /// The surface symbol whose name violates the lookup contract.
        symbol: AnySymbolId,
    },
    /// A canonical semantic value could not be created.
    SemanticValue(SemanticValueStoreError),
    /// A bound-tree or local-symbol allocation reached its capacity limit.
    Construction(BoundUnitConstructionError),
    /// Binding could not establish a complete recovery-aware root.
    Binding(BindingError<Upstream>),
}
