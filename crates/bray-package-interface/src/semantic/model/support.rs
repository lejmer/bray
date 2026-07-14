use bray_symbols::{ExternalSymbolKey, InterfaceSupportEntityId};

use super::{InterfaceCheckedTemplateId, InterfaceTraitApplicationId, InterfaceTypeId};
use crate::InterfaceSymbolReference;

/// A declaration reference available to one source-independent checked template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceTemplateReference {
    /// An ordinary local or dependency interface symbol.
    Symbol(InterfaceSymbolReference),
    /// An interface-private declaration support entity.
    Support(InterfaceSupportEntityId),
}

/// A selected implementation required by one source-independent checked template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceImplementationReference {
    /// An ordinary local or dependency implementation symbol.
    Symbol(InterfaceSymbolReference),
    /// An interface-private implementation support entity.
    Support(InterfaceSupportEntityId),
}

/// An interface-private implementation retained for checked-template instantiation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceSupportImplementation {
    declaration: ExternalSymbolKey,
    subject: InterfaceTypeId,
    trait_application: Option<InterfaceTraitApplicationId>,
}

impl InterfaceSupportImplementation {
    /// Creates one private implementation record with its stable declaration identity.
    pub const fn new(
        declaration: ExternalSymbolKey,
        subject: InterfaceTypeId,
        trait_application: Option<InterfaceTraitApplicationId>,
    ) -> Self {
        Self {
            declaration,
            subject,
            trait_application,
        }
    }

    /// Returns the stable declaration identity without publishing an imported symbol.
    pub const fn declaration(&self) -> &ExternalSymbolKey {
        &self.declaration
    }

    /// Returns the implemented subject type.
    pub const fn subject(&self) -> InterfaceTypeId {
        self.subject
    }

    /// Returns the applied trait when this is a trait implementation.
    pub const fn trait_application(&self) -> Option<InterfaceTraitApplicationId> {
        self.trait_application
    }
}

/// One private support-graph entity excluded from ordinary imported lookup.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceSupportEntity {
    /// A private declaration referenced by a checked template.
    Declaration(ExternalSymbolKey),
    /// A private selected implementation referenced by a checked template.
    Implementation(InterfaceSupportImplementation),
    /// A checked declaration-owned template stored in the template table.
    CheckedTemplate(InterfaceCheckedTemplateId),
}
