use std::collections::BTreeMap;

use crate::chunk::{DeclarationChunk, DiscoveredDeclaration, DiscoveredModulePart};
use crate::id::{ContainerId, DeclarationId, ModulePartId};
use crate::name::{DeclarationName, ModulePath};
use crate::record::{
    ContainerKind, ContainerRecord, ContainerRecordInput, DeclarationKind, DeclarationRecord,
    DeclarationRecordInput, ModulePartRecord, ModulePartRecordInput,
};
use crate::table::DeclarationTable;

/// Merges source-unit declaration chunks into one deterministic declaration table.
pub fn merge_declaration_chunks(
    chunks: impl IntoIterator<Item = DeclarationChunk>,
) -> DeclarationTable {
    let mut builder = TableBuilder::new();

    for chunk in source_order_chunks(chunks) {
        builder.push_chunk(chunk);
    }

    builder.finish()
}

fn source_order_chunks(
    chunks: impl IntoIterator<Item = DeclarationChunk>,
) -> Vec<DeclarationChunk> {
    let mut indexed_chunks = chunks.into_iter().enumerate().collect::<Vec<_>>();

    indexed_chunks.sort_by_key(|(index, chunk)| (chunk.source_id(), *index));
    indexed_chunks.into_iter().map(|(_, chunk)| chunk).collect()
}

struct TableBuilder {
    declarations: Vec<DeclarationRecord>,
    containers: Vec<ContainerBuilder>,
    module_parts: Vec<ModulePartRecord>,
    module_index: BTreeMap<ModulePath, ContainerId>,
}

impl TableBuilder {
    fn new() -> Self {
        let root_container =
            ContainerBuilder::new(ContainerId::from_index(0), ContainerKind::Root, None, None);

        Self {
            declarations: Vec::new(),
            containers: vec![root_container],
            module_parts: Vec::new(),
            module_index: BTreeMap::new(),
        }
    }

    fn push_chunk(&mut self, chunk: DeclarationChunk) {
        for part in chunk.into_module_parts() {
            self.push_module_part(part);
        }
    }

    fn push_module_part(&mut self, part: DiscoveredModulePart) {
        let module_container = self.module_container_for(&part.path);

        // ModulePath clones share immutable segment storage across records and indexes.
        let module_name = Some(DeclarationName::Path(part.path.clone()));

        let module_declaration = self.push_declaration_record(DeclarationRecordInput {
            id: self.next_declaration_id(),
            kind: DeclarationKind::Module,
            owning_container: self.root_container_id(),
            name: module_name,
            source_id: part.source_id,
            syntax_kind: part.syntax_kind,
            full_range: part.full_range,
            is_recovered: part.is_recovered,
            child_container: Some(module_container),
        });

        self.container_mut(self.root_container_id())
            .declarations
            .push(module_declaration);

        let mut part_declarations = Vec::with_capacity(part.declarations.len());

        for declaration in part.declarations {
            let declaration_id = self.push_discovered_declaration(module_container, declaration);

            part_declarations.push(declaration_id);
        }

        let module_part_id = self.next_module_part_id();

        self.module_parts
            .push(ModulePartRecord::new(ModulePartRecordInput {
                id: module_part_id,
                module_container,
                declaration: module_declaration,
                source_id: part.source_id,
                syntax_kind: part.syntax_kind,
                full_range: part.full_range,
                is_recovered: part.is_recovered,
                declarations: part_declarations.into_boxed_slice(),
            }));

        self.container_mut(module_container)
            .module_parts
            .push(module_part_id);
    }

    fn push_discovered_declaration(
        &mut self,
        owning_container: ContainerId,
        declaration: DiscoveredDeclaration,
    ) -> DeclarationId {
        let child_container =
            self.child_container_for_declaration(declaration.kind, owning_container);

        let children = declaration.children;

        let declaration_id = self.push_declaration_record(DeclarationRecordInput {
            id: self.next_declaration_id(),
            kind: declaration.kind,
            owning_container,
            name: declaration.name,
            source_id: declaration.source_id,
            syntax_kind: declaration.syntax_kind,
            full_range: declaration.full_range,
            is_recovered: declaration.is_recovered,
            child_container,
        });

        self.container_mut(owning_container)
            .declarations
            .push(declaration_id);

        if let Some(child_container) = child_container {
            for child in children {
                self.push_discovered_declaration(child_container, child);
            }
        }

        declaration_id
    }

    fn push_declaration_record(&mut self, input: DeclarationRecordInput) -> DeclarationId {
        let id = input.id;

        self.declarations.push(DeclarationRecord::new(input));

        id
    }

    fn module_container_for(&mut self, path: &ModulePath) -> ContainerId {
        if let Some(container_id) = self.module_index.get(path) {
            return *container_id;
        }

        let container_id = self.next_container_id();

        // ModulePath clones share immutable segment storage between the container and lookup key.
        let container_path = path.clone();

        self.module_index.insert(path.clone(), container_id);

        self.containers.push(ContainerBuilder::new(
            container_id,
            ContainerKind::Module,
            Some(self.root_container_id()),
            Some(container_path),
        ));

        container_id
    }

    fn child_container_for_declaration(
        &mut self,
        kind: DeclarationKind,
        parent: ContainerId,
    ) -> Option<ContainerId> {
        let container_kind = kind.child_container_kind()?;

        let container_id = self.next_container_id();

        self.containers.push(ContainerBuilder::new(
            container_id,
            container_kind,
            Some(parent),
            None,
        ));

        Some(container_id)
    }

    const fn root_container_id(&self) -> ContainerId {
        ContainerId::new(0)
    }

    fn next_declaration_id(&self) -> DeclarationId {
        DeclarationId::from_index(self.declarations.len())
    }

    fn next_container_id(&self) -> ContainerId {
        ContainerId::from_index(self.containers.len())
    }

    fn next_module_part_id(&self) -> ModulePartId {
        ModulePartId::from_index(self.module_parts.len())
    }

    fn container_mut(&mut self, id: ContainerId) -> &mut ContainerBuilder {
        let Some(index) = id.to_index() else {
            panic!("container ID must fit in usize");
        };

        match self.containers.get_mut(index) {
            Some(container) => container,
            None => panic!("container ID must refer to an existing container"),
        }
    }

    fn finish(self) -> DeclarationTable {
        DeclarationTable::new(
            self.root_container_id(),
            self.declarations.into_boxed_slice(),
            self.containers
                .into_iter()
                .map(ContainerBuilder::finish)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
            self.module_parts.into_boxed_slice(),
            self.module_index,
        )
    }
}

struct ContainerBuilder {
    id: ContainerId,
    kind: ContainerKind,
    parent: Option<ContainerId>,
    module_path: Option<ModulePath>,
    declarations: Vec<DeclarationId>,
    module_parts: Vec<ModulePartId>,
}

impl ContainerBuilder {
    fn new(
        id: ContainerId,
        kind: ContainerKind,
        parent: Option<ContainerId>,
        module_path: Option<ModulePath>,
    ) -> Self {
        Self {
            id,
            kind,
            parent,
            module_path,
            declarations: Vec::new(),
            module_parts: Vec::new(),
        }
    }

    fn finish(self) -> ContainerRecord {
        ContainerRecord::new(ContainerRecordInput {
            id: self.id,
            kind: self.kind,
            parent: self.parent,
            module_path: self.module_path,
            declarations: self.declarations.into_boxed_slice(),
            module_parts: self.module_parts.into_boxed_slice(),
        })
    }
}

#[cfg(test)]
mod tests {
    use bray_syntax::SyntaxKind;
    use bray_testing::{test_source_at as source, test_source_store as source_store};

    use super::merge_declaration_chunks;
    use crate::discover_source_unit_declarations;
    use crate::name::ModulePath;
    use crate::record::{ContainerKind, DeclarationKind};
    use crate::test_support::parse_valid_source_unit;

    #[test]
    fn merge_combines_multiple_source_units_that_contribute_to_one_module() {
        let sources = source_store([
            "module core.io;\nfunc read() {}\n",
            "module core.io {\nconst Size: Int = 1;\n}\n",
        ]);

        let first =
            discover_source_unit_declarations(&parse_valid_source_unit(source(&sources, 0)));

        let second =
            discover_source_unit_declarations(&parse_valid_source_unit(source(&sources, 1)));

        let forward = merge_declaration_chunks([first.clone(), second.clone()]);
        let reverse = merge_declaration_chunks([second, first]);

        assert_eq!(forward, reverse);

        let module_path = ModulePath::new(["core", "io"]);

        let module = match forward.module_container(&module_path) {
            Some(module) => module,
            None => panic!("expected core.io module container"),
        };

        assert_eq!(forward.module_containers().count(), 1);
        assert_eq!(module.module_parts().len(), 2);

        let declaration_kinds = module
            .declarations()
            .iter()
            .map(|id| declaration_kind(&forward, *id))
            .collect::<Vec<_>>();

        assert_eq!(
            declaration_kinds,
            [DeclarationKind::Function, DeclarationKind::Constant]
        );

        let part_source_ids = module
            .module_parts()
            .iter()
            .map(|id| module_part_source_id(&forward, *id))
            .collect::<Vec<_>>();

        assert_eq!(
            part_source_ids,
            [
                source(&sources, 0).source_id(),
                source(&sources, 1).source_id()
            ]
        );
    }

    #[test]
    fn merge_populates_child_containers_for_container_owning_declarations() {
        let sources = source_store([concat!(
            "module core;\n",
            "struct Resource { value: Int; construct() -> Self {} func make() {} }\n",
            "union Maybe { Some(value: Int); None; }\n",
            "trait Scope { type Item; const Size: Int; enter() -> Guard; func show(); }\n",
            "impl Resource { type Item = Element; const Size: Int = 1; construct() -> Self {} exit() {} func make() {} }",
        )]);

        let chunk =
            discover_source_unit_declarations(&parse_valid_source_unit(source(&sources, 0)));

        let table = merge_declaration_chunks([chunk]);

        let module = match table.module_container(&ModulePath::new(["core"])) {
            Some(module) => module,
            None => panic!("expected core module container"),
        };

        assert_eq!(
            module
                .declarations()
                .iter()
                .filter_map(|id| child_container_kind(&table, *id))
                .collect::<Vec<_>>(),
            [
                ContainerKind::Type,
                ContainerKind::Type,
                ContainerKind::Trait,
                ContainerKind::Implementation
            ]
        );

        let [struct_id, union_id, trait_id, implementation_id] = module.declarations() else {
            panic!(
                "expected four module declarations: {:?}",
                module.declarations()
            );
        };

        assert_eq!(
            child_declaration_kinds(&table, *struct_id),
            [
                DeclarationKind::StructField,
                DeclarationKind::TypeConstructorMember,
                DeclarationKind::TypeCallableMember,
            ]
        );

        assert_eq!(
            child_declaration_kinds(&table, *union_id),
            [DeclarationKind::UnionVariant, DeclarationKind::UnionVariant]
        );

        assert_eq!(
            child_declaration_kinds(&table, *trait_id),
            [
                DeclarationKind::TraitTypeMember,
                DeclarationKind::TraitConstantMember,
                DeclarationKind::TraitScopeEnterRequirement,
                DeclarationKind::TraitCallableMember,
            ]
        );

        assert_eq!(
            child_declaration_kinds(&table, *implementation_id),
            [
                DeclarationKind::ImplementationTypeMemberBinding,
                DeclarationKind::Constant,
                DeclarationKind::TypeConstructorMember,
                DeclarationKind::ScopeExitMember,
                DeclarationKind::TypeCallableMember,
            ]
        );

        assert_eq!(
            child_identifier_names(&table, *struct_id),
            ["value", "make"]
        );

        assert_eq!(
            child_keyword_names(&table, *implementation_id),
            [SyntaxKind::ConstructKeyword, SyntaxKind::ExitKeyword]
        );
    }

    #[test]
    fn merge_preserves_member_containers_from_multiple_module_parts() {
        let sources = source_store([
            "module core;\nstruct Point { x: Int; }\n",
            "module core;\nimpl Point { func translate() {} }\n",
        ]);

        let first =
            discover_source_unit_declarations(&parse_valid_source_unit(source(&sources, 0)));

        let second =
            discover_source_unit_declarations(&parse_valid_source_unit(source(&sources, 1)));

        let table = merge_declaration_chunks([first, second]);

        let module = match table.module_container(&ModulePath::new(["core"])) {
            Some(module) => module,
            None => panic!("expected core module container"),
        };

        let [struct_id, implementation_id] = module.declarations() else {
            panic!(
                "expected two module declarations: {:?}",
                module.declarations()
            );
        };

        assert_eq!(module.module_parts().len(), 2);

        assert_eq!(
            child_declaration_kinds(&table, *struct_id),
            [DeclarationKind::StructField]
        );

        assert_eq!(
            child_declaration_kinds(&table, *implementation_id),
            [DeclarationKind::TypeCallableMember]
        );
    }

    #[test]
    fn merge_populates_child_containers_for_signatures_and_variants() {
        let sources = source_store([concat!(
            "module core;\n",
            "func main<T, const N: Int>(value: T) {}\n",
            "predicate valid<T, const N: Int>(value: T);\n",
            "union Maybe { Some(value: Int, fallback: Int); }",
        )]);

        let chunk =
            discover_source_unit_declarations(&parse_valid_source_unit(source(&sources, 0)));

        let table = merge_declaration_chunks([chunk]);

        let module = match table.module_container(&ModulePath::new(["core"])) {
            Some(module) => module,
            None => panic!("expected core module container"),
        };

        let [function_id, predicate_id, union_id] = module.declarations() else {
            panic!(
                "expected three module declarations: {:?}",
                module.declarations()
            );
        };

        assert_eq!(
            child_container_kind(&table, *function_id),
            Some(ContainerKind::Signature)
        );

        assert_eq!(
            child_container_kind(&table, *predicate_id),
            Some(ContainerKind::Signature)
        );

        assert_eq!(
            child_declaration_kinds(&table, *function_id),
            [
                DeclarationKind::GenericTypeParameter,
                DeclarationKind::GenericConstParameter,
                DeclarationKind::CallableParameter,
            ]
        );

        assert_eq!(
            child_identifier_names(&table, *function_id),
            ["T", "N", "value"]
        );

        assert_eq!(
            child_declaration_kinds(&table, *predicate_id),
            [
                DeclarationKind::GenericTypeParameter,
                DeclarationKind::GenericConstParameter,
                DeclarationKind::PredicateParameter,
            ]
        );

        assert_eq!(
            child_identifier_names(&table, *predicate_id),
            ["T", "N", "value"]
        );

        let [variant_id] = child_declaration_ids(&table, *union_id) else {
            panic!("expected one union variant");
        };

        assert_eq!(
            child_container_kind(&table, *variant_id),
            Some(ContainerKind::Variant)
        );

        assert_eq!(
            child_declaration_kinds(&table, *variant_id),
            [
                DeclarationKind::UnionPayloadField,
                DeclarationKind::UnionPayloadField
            ]
        );

        assert_eq!(
            child_identifier_names(&table, *variant_id),
            ["value", "fallback"]
        );
    }

    fn declaration_kind(
        table: &crate::DeclarationTable,
        id: crate::DeclarationId,
    ) -> DeclarationKind {
        match table.declaration(id) {
            Some(declaration) => declaration.kind(),
            None => panic!("declaration ID should exist: {id:?}"),
        }
    }

    fn module_part_source_id(
        table: &crate::DeclarationTable,
        id: crate::ModulePartId,
    ) -> bray_source::SourceId {
        match table.module_part(id) {
            Some(part) => part.source_id(),
            None => panic!("module part ID should exist: {id:?}"),
        }
    }

    fn child_container_kind(
        table: &crate::DeclarationTable,
        declaration_id: crate::DeclarationId,
    ) -> Option<ContainerKind> {
        let declaration = match table.declaration(declaration_id) {
            Some(declaration) => declaration,
            None => panic!("declaration ID should exist: {declaration_id:?}"),
        };

        let child_container = declaration.child_container()?;

        match table.container(child_container) {
            Some(container) => Some(container.kind()),
            None => panic!("child container ID should exist: {child_container:?}"),
        }
    }

    fn child_declaration_kinds(
        table: &crate::DeclarationTable,
        declaration_id: crate::DeclarationId,
    ) -> Vec<DeclarationKind> {
        child_declaration_ids(table, declaration_id)
            .iter()
            .map(|id| declaration_kind(table, *id))
            .collect()
    }

    fn child_identifier_names(
        table: &crate::DeclarationTable,
        declaration_id: crate::DeclarationId,
    ) -> Vec<&str> {
        child_declaration_ids(table, declaration_id)
            .iter()
            .filter_map(|id| declaration_identifier_name(table, *id))
            .collect()
    }

    fn child_keyword_names(
        table: &crate::DeclarationTable,
        declaration_id: crate::DeclarationId,
    ) -> Vec<SyntaxKind> {
        child_declaration_ids(table, declaration_id)
            .iter()
            .filter_map(|id| declaration_keyword_name(table, *id))
            .collect()
    }

    fn child_declaration_ids(
        table: &crate::DeclarationTable,
        declaration_id: crate::DeclarationId,
    ) -> &[crate::DeclarationId] {
        let declaration = match table.declaration(declaration_id) {
            Some(declaration) => declaration,
            None => panic!("declaration ID should exist: {declaration_id:?}"),
        };

        let child_container = match declaration.child_container() {
            Some(child_container) => child_container,
            None => panic!("declaration should have a child container: {declaration_id:?}"),
        };

        match table.container(child_container) {
            Some(container) => container.declarations(),
            None => panic!("child container ID should exist: {child_container:?}"),
        }
    }

    fn declaration_identifier_name(
        table: &crate::DeclarationTable,
        declaration_id: crate::DeclarationId,
    ) -> Option<&str> {
        let declaration = match table.declaration(declaration_id) {
            Some(declaration) => declaration,
            None => panic!("declaration ID should exist: {declaration_id:?}"),
        };

        declaration
            .name()
            .and_then(crate::DeclarationName::as_identifier)
    }

    fn declaration_keyword_name(
        table: &crate::DeclarationTable,
        declaration_id: crate::DeclarationId,
    ) -> Option<SyntaxKind> {
        let declaration = match table.declaration(declaration_id) {
            Some(declaration) => declaration,
            None => panic!("declaration ID should exist: {declaration_id:?}"),
        };

        declaration
            .name()
            .and_then(crate::DeclarationName::as_keyword)
    }
}
