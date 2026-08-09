use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{
    GenericSubstitutionId, PredicateDefinitionSymbolId, PredicateParameterSymbolId, TypeId,
};

use crate::BoundExpressionId;

/// One checked source argument supplied to a predicate parameter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct SelectedPredicateArgument {
    expression: BoundExpressionId,
    parameter: PredicateParameterSymbolId,
    ty: TypeId,
}

impl SelectedPredicateArgument {
    /// Creates one checked predicate argument mapping.
    pub const fn new(
        expression: BoundExpressionId,
        parameter: PredicateParameterSymbolId,
        ty: TypeId,
    ) -> Self {
        Self {
            expression,
            parameter,
            ty,
        }
    }

    /// Returns the supplied source expression.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the selected predicate parameter.
    pub const fn parameter(self) -> PredicateParameterSymbolId {
        self.parameter
    }

    /// Returns the parameter type after generic substitution.
    pub const fn ty(self) -> TypeId {
        self.ty
    }
}

/// The exact predicate application selected for one source call.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct SelectedPredicateApplication {
    callee: BoundExpressionId,
    predicate: PredicateDefinitionSymbolId,
    substitution: GenericSubstitutionId,
    result_type: TypeId,
    arguments: Arc<[SelectedPredicateArgument]>,
}

impl SelectedPredicateApplication {
    /// Creates one checked predicate application.
    pub fn new(
        callee: BoundExpressionId,
        predicate: PredicateDefinitionSymbolId,
        substitution: GenericSubstitutionId,
        result_type: TypeId,
        arguments: impl IntoIterator<Item = SelectedPredicateArgument>,
    ) -> Self {
        Self {
            callee,
            predicate,
            substitution,
            result_type,
            arguments: shared_slice(arguments),
        }
    }

    /// Returns the source callee expression resolved to the predicate.
    pub const fn callee(&self) -> BoundExpressionId {
        self.callee
    }

    /// Returns the exact predicate declaration.
    pub const fn predicate(&self) -> PredicateDefinitionSymbolId {
        self.predicate
    }

    /// Returns the complete generic substitution.
    pub const fn substitution(&self) -> GenericSubstitutionId {
        self.substitution
    }

    /// Returns the boolean result type.
    pub const fn result_type(&self) -> TypeId {
        self.result_type
    }

    /// Returns arguments in source evaluation order.
    pub fn arguments(&self) -> &[SelectedPredicateArgument] {
        &self.arguments
    }
}
