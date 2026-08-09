use std::collections::BTreeMap;

use crate::id::{ContainerId, DeclarationId, ModulePartId};
use crate::name::ModulePath;
use crate::record::{ContainerKind, ContainerRecord, DeclarationRecord, ModulePartRecord};

/// Immutable declaration discovery table for one compilation input set.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct DeclarationTable {
    root_container: ContainerId,
    declarations: Box<[DeclarationRecord]>,
    containers: Box<[ContainerRecord]>,
    module_parts: Box<[ModulePartRecord]>,
    module_index: BTreeMap<ModulePath, ContainerId>,
}

impl DeclarationTable {
    pub(crate) fn new(
        root_container: ContainerId,
        declarations: Box<[DeclarationRecord]>,
        containers: Box<[ContainerRecord]>,
        module_parts: Box<[ModulePartRecord]>,
        module_index: BTreeMap<ModulePath, ContainerId>,
    ) -> Self {
        Self {
            root_container,
            declarations,
            containers,
            module_parts,
            module_index,
        }
    }

    /// Returns the root declaration container.
    pub const fn root_container(&self) -> ContainerId {
        self.root_container
    }

    /// Returns all declaration records in stable ID order.
    pub fn declarations(&self) -> &[DeclarationRecord] {
        &self.declarations
    }

    /// Returns all container records in stable ID order.
    pub fn containers(&self) -> &[ContainerRecord] {
        &self.containers
    }

    /// Returns all module-part records in stable ID order.
    pub fn module_parts(&self) -> &[ModulePartRecord] {
        &self.module_parts
    }

    /// Returns a declaration record by ID.
    pub fn declaration(&self, id: DeclarationId) -> Option<&DeclarationRecord> {
        self.declarations.get(id.to_index()?)
    }

    /// Returns a container record by ID.
    pub fn container(&self, id: ContainerId) -> Option<&ContainerRecord> {
        self.containers.get(id.to_index()?)
    }

    /// Returns a module-part record by ID.
    pub fn module_part(&self, id: ModulePartId) -> Option<&ModulePartRecord> {
        self.module_parts.get(id.to_index()?)
    }

    /// Returns module containers in stable container ID order.
    pub fn module_containers(&self) -> impl Iterator<Item = &ContainerRecord> {
        self.containers
            .iter()
            .filter(|container| container.kind() == ContainerKind::Module)
    }

    /// Returns the module container ID for a syntactic module path.
    pub fn module_container_id(&self, path: &ModulePath) -> Option<ContainerId> {
        self.module_index.get(path).copied()
    }

    /// Returns the module container for a syntactic module path.
    pub fn module_container(&self, path: &ModulePath) -> Option<&ContainerRecord> {
        self.container(self.module_container_id(path)?)
    }
}

#[cfg(test)]
mod tests {
    use super::DeclarationTable;

    #[test]
    fn declaration_tables_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<DeclarationTable>();
    }
}
