use crate::{TraitApplicationId, TypeExpressionTemplate, TypeId};

/// The source type template named by one implementation declaration.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationSubjectTemplate {
    ty: TypeExpressionTemplate,
}

impl ImplementationSubjectTemplate {
    /// Creates an implementation subject template.
    pub const fn new(ty: TypeExpressionTemplate) -> Self {
        Self { ty }
    }

    /// Returns the implemented type template.
    pub const fn ty(&self) -> &TypeExpressionTemplate {
        &self.ty
    }
}

/// The checked subject type implemented by one implementation declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationSubject {
    ty: TypeId,
}

impl ImplementationSubject {
    /// Creates a checked implementation subject.
    pub const fn new(ty: TypeId) -> Self {
        Self { ty }
    }

    /// Returns the implemented semantic type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }
}

/// The stable semantic key under which one implementation participates in coherence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationCoherenceKey {
    subject: TypeId,
    trait_application: Option<TraitApplicationId>,
}

impl ImplementationCoherenceKey {
    /// Creates a coherence key from the checked implementation surface.
    pub const fn new(subject: TypeId, trait_application: Option<TraitApplicationId>) -> Self {
        Self {
            subject,
            trait_application,
        }
    }

    /// Returns the exact implemented subject type.
    pub const fn subject(self) -> TypeId {
        self.subject
    }

    /// Returns the implemented trait application for a trait implementation.
    pub const fn trait_application(self) -> Option<TraitApplicationId> {
        self.trait_application
    }
}

#[cfg(test)]
mod tests {
    use super::{ImplementationCoherenceKey, ImplementationSubject, ImplementationSubjectTemplate};

    #[test]
    fn implementation_subject_contracts_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ImplementationSubjectTemplate>();
        assert_send_sync::<ImplementationSubject>();
        assert_send_sync::<ImplementationCoherenceKey>();
    }
}
