use crate::{TraitApplicationId, TypeId};

/// The exact subject and trait application requiring an implementation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImplementationRequirementKey {
    subject: TypeId,
    trait_application: TraitApplicationId,
}

impl ImplementationRequirementKey {
    /// Creates an exact implementation requirement key.
    pub const fn new(subject: TypeId, trait_application: TraitApplicationId) -> Self {
        Self {
            subject,
            trait_application,
        }
    }

    /// Returns the exact semantic subject type.
    pub const fn subject(self) -> TypeId {
        self.subject
    }

    /// Returns the exact applied trait requirement.
    pub const fn trait_application(self) -> TraitApplicationId {
        self.trait_application
    }
}

#[cfg(test)]
mod tests {
    use super::ImplementationRequirementKey;

    #[test]
    fn implementation_requirement_keys_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ImplementationRequirementKey>();
    }
}
