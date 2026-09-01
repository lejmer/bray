/// An outer binding-query failure that is not a source diagnostic.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum BindingQueryError<Upstream = std::convert::Infallible> {
    /// Cancellation was observed before the requested query could complete.
    Cancelled,
    /// Semantic checking could not complete because a typed dependency was unavailable.
    CheckerInfrastructure(bray_checker::CheckerInfrastructureError),
    /// The semantic value store rejected a lookup or interning operation.
    SemanticValue(bray_symbols::SemanticValueStoreError),
    /// A required dependency could not be supplied by the coordinating query layer.
    ///
    /// Ordinary malformed source must instead produce an error-aware semantic value with structured
    /// diagnostics.
    DependencyUnavailable,
    /// The exact source construct selected for a bound unit is unavailable.
    MissingSyntax {
        /// The exact versioned source construct that could not be recovered.
        source: bray_bound_tree::BoundSourceAnchor,
    },
    /// The expected unit owner or a known related owner record is absent.
    MissingOwner {
        /// The exact versioned source construct being bound.
        source: bray_bound_tree::BoundSourceAnchor,
        /// The stable identity expected to resolve as the unit owner.
        owner: bray_symbols::SymbolKey,
        /// A resolved related symbol whose record or owner relationship is absent, when known.
        symbol: Option<bray_symbols::AnySymbolId>,
    },
    /// A resolved unit owner has no containing logical module.
    MissingModule {
        /// The exact versioned source construct being bound.
        source: bray_bound_tree::BoundSourceAnchor,
        /// The resolved owner whose containing module is absent.
        owner: bray_symbols::AnySymbolId,
    },
    /// A surface symbol supplied to a bound unit has no valid local lookup name.
    InvalidSurfaceName {
        /// The exact versioned source construct being bound.
        source: bray_bound_tree::BoundSourceAnchor,
        /// The surface symbol whose name violates the lookup contract.
        symbol: bray_symbols::AnySymbolId,
    },
    /// Bound-tree or local-symbol construction rejected an exact relationship.
    Construction(crate::BoundUnitConstructionError),
    /// Binding violated an exact local contract.
    Binding(crate::BindingError<Upstream>),
    /// Bound-unit assembly rejected the completed local structures.
    Assembly(crate::BoundUnitAssemblyError),
    /// The coordinating query layer returned one of its own exact failures.
    Upstream(Upstream),
}

impl BindingQueryError {
    /// Widens a locally produced failure to a query boundary with an upstream error type.
    pub fn with_upstream<Upstream>(self) -> BindingQueryError<Upstream> {
        match self {
            Self::Cancelled => BindingQueryError::Cancelled,
            Self::CheckerInfrastructure(error) => BindingQueryError::CheckerInfrastructure(error),
            Self::SemanticValue(error) => BindingQueryError::SemanticValue(error),
            Self::DependencyUnavailable => BindingQueryError::DependencyUnavailable,
            Self::MissingSyntax { source } => BindingQueryError::MissingSyntax { source },
            Self::MissingOwner {
                source,
                owner,
                symbol,
            } => BindingQueryError::MissingOwner {
                source,
                owner,
                symbol,
            },
            Self::MissingModule { source, owner } => {
                BindingQueryError::MissingModule { source, owner }
            }
            Self::InvalidSurfaceName { source, symbol } => {
                BindingQueryError::InvalidSurfaceName { source, symbol }
            }
            Self::Construction(error) => BindingQueryError::Construction(error),
            Self::Binding(error) => BindingQueryError::Binding(error.with_upstream()),
            Self::Assembly(error) => BindingQueryError::Assembly(error),
            Self::Upstream(error) => match error {},
        }
    }
}

/// The outer result of binding-query access or evaluation.
pub type BindingQueryResult<T, Upstream = std::convert::Infallible> =
    Result<T, BindingQueryError<Upstream>>;

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundNodeKind, BoundTreeBuildError, BoundUnitId};
    use bray_symbols::{GenericSubstitutionShapeError, SemanticValueKind, SemanticValueStoreError};

    use super::BindingQueryError;
    use crate::unit::test_support::fixture;
    use crate::{BindingError, BoundUnitConstructionError};

    #[test]
    fn semantic_value_failures_survive_upstream_widening() {
        let error = SemanticValueStoreError::UnknownId {
            kind: SemanticValueKind::Type,
        };

        assert_eq!(
            BindingQueryError::SemanticValue(error).with_upstream::<u8>(),
            BindingQueryError::SemanticValue(error)
        );
    }

    #[test]
    fn local_binding_failures_survive_upstream_widening() {
        let construction =
            BoundUnitConstructionError::BoundTree(BoundTreeBuildError::ForeignNode {
                expected: BoundUnitId::new(4),
                actual: BoundUnitId::new(9),
                kind: BoundNodeKind::Expression,
            });

        assert_eq!(
            BindingQueryError::Construction(construction).with_upstream::<u8>(),
            BindingQueryError::Construction(construction)
        );

        assert_eq!(
            BindingQueryError::Binding(BindingError::ControlTargetMismatch).with_upstream::<u8>(),
            BindingQueryError::Binding(BindingError::ControlTargetMismatch)
        );

        let substitution = GenericSubstitutionShapeError::ArgumentCountMismatch {
            parameter_count: 2,
            argument_count: 1,
        };

        assert_eq!(
            BindingQueryError::Binding(BindingError::GenericSubstitution(substitution))
                .with_upstream::<u8>(),
            BindingQueryError::Binding(BindingError::GenericSubstitution(substitution))
        );
    }

    #[test]
    fn missing_unit_context_survives_upstream_widening() {
        let fixture = fixture();
        let source = fixture.key.source();

        // Unit keys and symbol keys are Arc-backed stable identities.
        let owner = fixture.key.declared_owner().clone();

        let error = BindingQueryError::MissingOwner {
            source,
            owner: owner.clone(),
            symbol: None,
        };

        assert_eq!(
            error.with_upstream::<u8>(),
            BindingQueryError::MissingOwner {
                source,
                owner,
                symbol: None,
            },
        );
    }
}
