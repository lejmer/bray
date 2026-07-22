use bray_bound_tree::{BoundExpressionId, DeclaredValueTypeEvidence, DeclaredValueTypeTerm};
use bray_symbols::{AnonymousCallableSymbolId, TypeExpressionTemplate};

/// Type and identity evidence supplied by one independently checked nested callable unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NestedCallableEvidence {
    expression: BoundExpressionId,
    callable: AnonymousCallableSymbolId,
    value_type: DeclaredValueTypeEvidence,
}

impl NestedCallableEvidence {
    /// Creates evidence connecting an outer expression to its nested callable fact.
    pub fn new(
        expression: BoundExpressionId,
        callable: AnonymousCallableSymbolId,
        callable_type: TypeExpressionTemplate,
    ) -> Self {
        Self {
            expression,
            callable,
            value_type: DeclaredValueTypeEvidence::new(
                DeclaredValueTypeTerm::Expression(expression),
                callable_type,
            ),
        }
    }

    /// Returns the outer anonymous-callable expression.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the exact nested local callable identity.
    pub const fn callable(&self) -> AnonymousCallableSymbolId {
        self.callable
    }

    pub(super) const fn value_type(&self) -> &DeclaredValueTypeEvidence {
        &self.value_type
    }
}
