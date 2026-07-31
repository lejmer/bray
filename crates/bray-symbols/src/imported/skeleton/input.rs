use std::sync::Arc;

use bray_base::shared_slice;

use crate::{
    CallablePosition, ExternalSymbolKey, ImportedInterfaceId, ImportedPackageIdentitySurface,
    InterfaceSymbolId, SymbolName, SymbolRelationshipKind,
};

/// One owner-relative relationship in an imported interface surface.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedSymbolRelationship {
    kind: SymbolRelationshipKind,
    owner: InterfaceSymbolId,
    member: InterfaceSymbolId,
    ordinal: u32,
    position: CallablePosition,
    allows_mutation: bool,
}

impl ImportedSymbolRelationship {
    /// Creates one imported typed relationship.
    pub const fn new(
        kind: SymbolRelationshipKind,
        owner: InterfaceSymbolId,
        member: InterfaceSymbolId,
        ordinal: u32,
    ) -> Self {
        Self {
            kind,
            owner,
            member,
            ordinal,
            position: CallablePosition::NamedOnly,
            allows_mutation: false,
        }
    }

    /// Marks the related field as permitting mutation after initialization.
    pub const fn with_mutation(mut self) -> Self {
        self.allows_mutation = true;

        self
    }

    /// Sets the related payload field's call-position permission.
    pub const fn with_position(mut self, position: CallablePosition) -> Self {
        self.position = position;

        self
    }

    /// Returns the relationship category.
    pub const fn kind(self) -> SymbolRelationshipKind {
        self.kind
    }

    /// Returns the interface-local owner.
    pub const fn owner(self) -> InterfaceSymbolId {
        self.owner
    }

    /// Returns the interface-local member.
    pub const fn member(self) -> InterfaceSymbolId {
        self.member
    }

    /// Returns the owner-relative position.
    pub const fn ordinal(self) -> u32 {
        self.ordinal
    }

    /// Returns the related payload field's call-position permission.
    pub const fn position(self) -> CallablePosition {
        self.position
    }

    /// Returns whether the related field permits mutation after initialization.
    pub const fn allows_mutation(self) -> bool {
        self.allows_mutation
    }
}

/// One exported ordinary-name edge with its target reduced to stable semantic identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedLookupEdge {
    owner: InterfaceSymbolId,
    name: SymbolName,
    target: ExternalSymbolKey,
}

impl ImportedLookupEdge {
    /// Creates one imported ordinary-name edge.
    pub const fn new(
        owner: InterfaceSymbolId,
        name: SymbolName,
        target: ExternalSymbolKey,
    ) -> Self {
        Self {
            owner,
            name,
            target,
        }
    }

    /// Returns the interface-local lookup owner.
    pub const fn owner(&self) -> InterfaceSymbolId {
        self.owner
    }

    /// Returns the exported ordinary name.
    pub const fn name(&self) -> &SymbolName {
        &self.name
    }

    /// Returns the defining target identity.
    pub const fn target(&self) -> &ExternalSymbolKey {
        &self.target
    }
}

/// One validated interface identity surface used to create semantic symbols.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportedSymbolSkeletonInput {
    interface: ImportedInterfaceId,
    symbols: ImportedPackageIdentitySurface,
    relationships: Arc<[ImportedSymbolRelationship]>,
    lookups: Arc<[ImportedLookupEdge]>,
}

impl ImportedSymbolSkeletonInput {
    /// Creates immutable origin-neutral symbol input.
    pub fn new(
        interface: ImportedInterfaceId,
        symbols: ImportedPackageIdentitySurface,
        relationships: impl IntoIterator<Item = ImportedSymbolRelationship>,
        lookups: impl IntoIterator<Item = ImportedLookupEdge>,
    ) -> Self {
        Self {
            interface,
            symbols,
            relationships: shared_slice(relationships),
            lookups: shared_slice(lookups),
        }
    }

    /// Returns the loaded interface containing these symbols.
    pub const fn interface(&self) -> ImportedInterfaceId {
        self.interface
    }

    /// Returns the validated symbol identity surface.
    pub const fn symbols(&self) -> &ImportedPackageIdentitySurface {
        &self.symbols
    }

    /// Returns typed relationships supplied by the interface.
    pub fn relationships(&self) -> &[ImportedSymbolRelationship] {
        &self.relationships
    }

    /// Returns exported ordinary-name edges.
    pub fn lookups(&self) -> &[ImportedLookupEdge] {
        &self.lookups
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        ImportedInterfaceId, ImportedLookupEdge, ImportedSymbolRelationship,
        ImportedSymbolSkeletonInput, InterfaceSymbolId, SymbolRelationshipKind,
    };

    use super::super::test_support::{interface_fixture, symbol_name};

    #[test]
    fn construction_inputs_preserve_typed_relationship_and_lookup_contracts() {
        let fixture = interface_fixture(6, "example.package", "run");

        assert_eq!(fixture.input.interface(), ImportedInterfaceId::new(6));

        assert_eq!(
            fixture.input.symbols().package_symbol_id(),
            InterfaceSymbolId::new(0)
        );

        assert_eq!(
            fixture.input.relationships(),
            [
                ImportedSymbolRelationship::new(
                    SymbolRelationshipKind::PackageModule,
                    InterfaceSymbolId::new(0),
                    InterfaceSymbolId::new(1),
                    0,
                ),
                ImportedSymbolRelationship::new(
                    SymbolRelationshipKind::ModuleMember,
                    InterfaceSymbolId::new(1),
                    InterfaceSymbolId::new(2),
                    0,
                ),
            ]
        );

        assert_eq!(
            fixture.input.lookups(),
            [ImportedLookupEdge::new(
                InterfaceSymbolId::new(1),
                symbol_name("run"),
                fixture.function_key,
            )]
        );
    }

    #[test]
    fn construction_inputs_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<ImportedSymbolSkeletonInput>();
        assert_send_sync::<ImportedSymbolRelationship>();
        assert_send_sync::<ImportedLookupEdge>();
    }
}
