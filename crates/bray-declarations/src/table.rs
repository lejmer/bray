use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};

use crate::id::{ContainerId, DeclarationId, ModulePartId};
use crate::name::ModulePath;
use crate::record::{
    ContainerKind, ContainerRecord, DeclarationKind, DeclarationRecord, ModulePartRecord,
};

/// Immutable declaration discovery table for one compilation input set.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationTable {
    root_container: ContainerId,
    declarations: Box<[DeclarationRecord]>,
    containers: Box<[ContainerRecord]>,
    module_parts: Box<[ModulePartRecord]>,
    module_index: BTreeMap<ModulePath, ContainerId>,
    using_index: Box<[Box<[DeclarationId]>]>,
}

impl Hash for DeclarationTable {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.root_container.hash(state);
        self.declarations.hash(state);
        self.containers.hash(state);
        self.module_parts.hash(state);
        self.module_index.hash(state);
    }
}

impl DeclarationTable {
    pub(crate) fn new(
        root_container: ContainerId,
        declarations: Box<[DeclarationRecord]>,
        containers: Box<[ContainerRecord]>,
        module_parts: Box<[ModulePartRecord]>,
        module_index: BTreeMap<ModulePath, ContainerId>,
    ) -> Self {
        let using_index = module_parts
            .iter()
            .map(|part| {
                part.declarations()
                    .iter()
                    .copied()
                    .filter(|id| {
                        id.to_index()
                            .and_then(|index| declarations.get(index))
                            .is_none_or(|declaration| declaration.kind() == DeclarationKind::Using)
                    })
                    .collect()
            })
            .collect();

        Self {
            root_container,
            declarations,
            containers,
            module_parts,
            module_index,
            using_index,
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

    /// Returns using declarations in source order for one module part.
    ///
    /// Unresolved declaration references remain visible for caller validation.
    pub fn using_declarations(&self, part: ModulePartId) -> Option<&[DeclarationId]> {
        self.using_index.get(part.to_index()?).map(AsRef::as_ref)
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
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    use super::DeclarationTable;
    use crate::record::{DeclarationKind, ModulePartRecord, ModulePartRecordInput};
    use crate::{DeclarationId, ModulePartId};

    fn discovered_table(inputs: &[&str]) -> DeclarationTable {
        let sources = bray_testing::test_source_store(inputs.iter().copied());

        let chunks = (0..inputs.len())
            .map(|index| {
                let source = bray_testing::test_source_at(&sources, u32::try_from(index).unwrap());
                let syntax = crate::test_support::parse_valid_source_unit_for_test(source);

                crate::discover_source_unit_declarations(&syntax)
            })
            .collect::<Vec<_>>();

        crate::merge_declaration_chunks(chunks.iter())
            .into_parts()
            .0
    }

    #[test]
    fn using_lookup_keeps_each_module_part_in_source_order() {
        let table = discovered_table(&[
            r#"
            module app;

            func first()
            {
            }

            using library.one;

            func second()
            {
            }

            using library.two;
            "#,
            r#"
            module app
            {
                func third()
                {
                }

                using library.three;
            }

            module empty
            {
                func fourth()
                {
                }
            }
            "#,
        ]);

        for part in table.module_parts() {
            let expected = part
                .declarations()
                .iter()
                .copied()
                .filter(|id| table.declaration(*id).unwrap().kind() == DeclarationKind::Using)
                .collect::<Vec<_>>();

            assert_eq!(
                table.using_declarations(part.id()),
                Some(expected.as_slice())
            );
        }

        assert_eq!(table.using_declarations(ModulePartId::new(u32::MAX)), None);

        assert_eq!(
            table.using_declarations(ModulePartId::new(2)),
            Some([].as_slice())
        );
    }

    #[test]
    fn using_lookup_excludes_unselected_module_contributions() {
        let sources = bray_testing::test_source_store([
            r#"
            module app.enabled;

            using library.one;

            func present()
            {
            }
            "#,
            r#"
            module app.disabled;

            using library.two;

            func absent()
            {
            }
            "#,
        ]);

        let chunks = (0..2)
            .map(|index| {
                let source = bray_testing::test_source_at(&sources, index);
                let syntax = crate::test_support::parse_valid_source_unit_for_test(source);

                crate::discover_source_unit_declarations(&syntax)
            })
            .collect::<Vec<_>>();

        let result = crate::merge_selected_declaration_chunks(
            chunks.iter(),
            |part| part.path().dotted() == "app.enabled",
            |_| true,
        );

        let table = result.table();

        assert_eq!(table.module_parts().len(), 1);

        let imports = table.using_declarations(ModulePartId::new(0)).unwrap();

        assert_eq!(imports.len(), 1);
        assert_eq!(table.declaration(imports[0]).unwrap().kind(), DeclarationKind::Using);
        assert!(table.module_container(&crate::ModulePath::new(["app", "disabled"])).is_none());
    }

    #[test]
    fn using_lookup_preserves_unresolved_references_between_imports() {
        let table = discovered_table(&[r#"
        module app;

        using library.one;

        func ordinary()
        {
        }

        using library.two;
        "#]);

        let part = &table.module_parts()[0];
        let imports = table.using_declarations(part.id()).unwrap();
        let unresolved = DeclarationId::new(u32::MAX);
        let candidates = vec![imports[0], unresolved, imports[1]];
        let declarations = vec![imports[0], part.declarations()[1], unresolved, imports[1]];

        let malformed = ModulePartRecord::new(ModulePartRecordInput {
            id: part.id(),
            module_container: part.module_container(),
            declaration: part.declaration(),
            syntax: part.syntax_anchor(),
            surface: part.surface().clone(),
            declarations: declarations.into_boxed_slice(),
        });

        let table = DeclarationTable::new(
            table.root_container,
            table.declarations,
            table.containers,
            vec![malformed].into_boxed_slice(),
            table.module_index,
        );

        assert_eq!(
            table.using_declarations(ModulePartId::new(0)),
            Some(candidates.as_slice())
        );
    }

    #[test]
    fn using_index_preserves_the_logical_table_hash() {
        let table = discovered_table(&[r#"
        module app;

        using library.one;

        func ordinary()
        {
        }
        "#]);

        let mut original = DefaultHasher::new();

        table.root_container.hash(&mut original);
        table.declarations.hash(&mut original);
        table.containers.hash(&mut original);
        table.module_parts.hash(&mut original);
        table.module_index.hash(&mut original);

        let mut indexed = DefaultHasher::new();

        table.hash(&mut indexed);

        assert_eq!(indexed.finish(), original.finish());
    }

    #[test]
    fn declaration_tables_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<DeclarationTable>();
    }
}
