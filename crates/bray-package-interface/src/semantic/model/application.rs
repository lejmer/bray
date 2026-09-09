use std::sync::Arc;

use super::{InterfaceConstantTermId, InterfaceGenericSubstitutionId, InterfaceTypeId};
use crate::InterfaceSymbolReference;

/// A callable proof dependency with artifact-local substitution and dispatch identities.
pub type InterfaceCallableEvidenceTarget = bray_symbols::CallableEvidenceTarget<
    super::InterfaceCallableInstanceId,
    (InterfaceTypeId, super::InterfaceTraitApplicationId),
>;

/// One artifact-stable generic argument.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceGenericArgument {
    /// A semantic type argument.
    Type(InterfaceTypeId),
    /// An open or closed constant argument.
    Constant(InterfaceConstantTermId),
}

/// One parameter-to-argument binding in declaration order.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceGenericBinding {
    pub(crate) parameter: InterfaceSymbolReference,
    pub(crate) argument: InterfaceGenericArgument,
}

impl InterfaceGenericBinding {
    /// Creates one generic binding.
    pub const fn new(
        parameter: InterfaceSymbolReference,
        argument: InterfaceGenericArgument,
    ) -> Self {
        Self {
            parameter,
            argument,
        }
    }
}

/// One artifact-stable generic substitution.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceGenericSubstitution {
    pub(crate) owner: InterfaceSymbolReference,
    pub(crate) bindings: Arc<[InterfaceGenericBinding]>,
}

impl InterfaceGenericSubstitution {
    /// Creates a substitution in generic parameter order.
    pub fn new(
        owner: InterfaceSymbolReference,
        bindings: impl IntoIterator<Item = InterfaceGenericBinding>,
    ) -> Self {
        Self {
            owner,
            bindings: bindings.into_iter().collect(),
        }
    }
}

/// One applied trait using artifact-local semantic references.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceTraitApplication {
    pub(crate) definition: InterfaceSymbolReference,
    pub(crate) substitution: InterfaceGenericSubstitutionId,
}

impl InterfaceTraitApplication {
    /// Creates an applied trait record.
    pub const fn new(
        definition: InterfaceSymbolReference,
        substitution: InterfaceGenericSubstitutionId,
    ) -> Self {
        Self {
            definition,
            substitution,
        }
    }
}

/// One selected implementation witness using artifact-local semantic references.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceImplementationInstance {
    pub(crate) definition: InterfaceSymbolReference,
    pub(crate) substitution: InterfaceGenericSubstitutionId,
}

/// One substituted callable definition using artifact-local semantic references.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCallableInstance {
    pub(crate) definition: InterfaceSymbolReference,
    pub(crate) substitution: InterfaceGenericSubstitutionId,
}

impl InterfaceCallableInstance {
    /// Creates a substituted callable instance.
    pub const fn new(
        definition: InterfaceSymbolReference,
        substitution: InterfaceGenericSubstitutionId,
    ) -> Self {
        Self {
            definition,
            substitution,
        }
    }
}

impl InterfaceImplementationInstance {
    /// Creates a selected implementation instance.
    pub const fn new(
        definition: InterfaceSymbolReference,
        substitution: InterfaceGenericSubstitutionId,
    ) -> Self {
        Self {
            definition,
            substitution,
        }
    }
}
