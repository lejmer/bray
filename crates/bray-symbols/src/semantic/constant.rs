use crate::{
    AnyConstantDefinitionId, ConcreteGenericSubstitutionId, ConstantTermId,
    ImplementationInstanceId, TypeId,
};

/// A checked source-independent constant definition template.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConstantDefinition {
    ty: TypeId,
    term: ConstantTermId,
}

impl ConstantDefinition {
    /// Creates a checked constant definition template.
    pub const fn new(ty: TypeId, term: ConstantTermId) -> Self {
        Self { ty, term }
    }

    /// Returns the declared constant type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    /// Returns the checked open or closed definition term.
    pub const fn term(self) -> ConstantTermId {
        self.term
    }
}

/// Marks an invalid constant definition whose diagnostics belong to the query result.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ErrorConstantDefinition;

/// The checked declaration state of a constant definition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ConstantDefinitionState {
    /// A checked constant template is available.
    Defined(ConstantDefinition),
    /// A trait constant member requires a fulfillment value.
    Required,
    /// Checking failed and diagnostics are retained by the query result.
    Error(ErrorConstantDefinition),
}

/// The symbol-domain inputs required to evaluate one concrete constant instance.
///
/// The compilation query key adds the selected target profile and snapshot identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ConstantInstanceKey {
    definition: AnyConstantDefinitionId,
    substitution: ConcreteGenericSubstitutionId,
    selected_implementation: Option<ImplementationInstanceId>,
}

impl ConstantInstanceKey {
    /// Creates a concrete constant-instance key.
    pub const fn new(
        definition: AnyConstantDefinitionId,
        substitution: ConcreteGenericSubstitutionId,
        selected_implementation: Option<ImplementationInstanceId>,
    ) -> Self {
        Self {
            definition,
            substitution,
            selected_implementation,
        }
    }

    /// Returns the exact constant definition category.
    pub const fn definition(self) -> AnyConstantDefinitionId {
        self.definition
    }

    /// Returns the validated concrete generic substitution.
    pub const fn substitution(self) -> ConcreteGenericSubstitutionId {
        self.substitution
    }

    /// Returns the selected implementation witness when trait lookup participates.
    pub const fn selected_implementation(self) -> Option<ImplementationInstanceId> {
        self.selected_implementation
    }
}

#[cfg(test)]
mod tests {
    use super::{ConstantDefinition, ConstantDefinitionState, ConstantInstanceKey};

    #[test]
    fn constant_fact_values_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ConstantDefinition>();
        assert_send_sync::<ConstantDefinitionState>();
        assert_send_sync::<ConstantInstanceKey>();
    }
}
