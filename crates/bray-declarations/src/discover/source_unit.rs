use bray_source::{SourceId, TextRange};
use bray_syntax::{
    BlockModuleDeclarationSyntax, SourceSyntaxNode, SourceUnitModuleDeclarationSyntax,
    SourceUnitSyntax, SyntaxKind, SyntaxNodeView, SyntaxWalkControl, SyntaxWalkEvent,
    walk_source_unit,
};

use super::names::{declaration_kind_for_syntax, declaration_name, path_from_syntax};
use crate::chunk::{DeclarationChunk, DiscoveredDeclaration, DiscoveredModulePart};
use crate::name::{DeclarationName, ModulePath};
use crate::record::DeclarationKind;

/// Discovers module parts and declarations from one source unit.
pub fn discover_source_unit_declarations(source_unit: &SourceUnitSyntax) -> DeclarationChunk {
    let mut discoverer = SourceUnitDiscoverer::new(source_unit.source().source_id());

    walk_source_unit(source_unit, |event| discoverer.visit(event));

    discoverer.finish()
}

struct SourceUnitDiscoverer {
    source_id: SourceId,
    module_parts: Vec<ModulePartBuilder>,
    source_unit_module_part: Option<usize>,
    module_part_stack: Vec<usize>,
    declaration_stack: Vec<DeclarationBuilder>,
}

impl SourceUnitDiscoverer {
    fn new(source_id: SourceId) -> Self {
        Self {
            source_id,
            module_parts: Vec::new(),
            source_unit_module_part: None,
            module_part_stack: Vec::new(),
            declaration_stack: Vec::new(),
        }
    }

    fn visit(&mut self, event: SyntaxWalkEvent<'_>) -> SyntaxWalkControl {
        match event {
            SyntaxWalkEvent::EnterNode(view) => self.enter_node(view),
            SyntaxWalkEvent::Token(_) => SyntaxWalkControl::Continue,
            SyntaxWalkEvent::ExitNode(view) => self.exit_node(view),
        }
    }

    fn enter_node(&mut self, view: SyntaxNodeView<'_>) -> SyntaxWalkControl {
        match view.kind() {
            SyntaxKind::SourceUnitModuleDeclaration => {
                self.enter_source_unit_module_declaration(view);

                SyntaxWalkControl::SkipChildren
            }
            SyntaxKind::BlockModuleDeclaration => {
                self.enter_block_module_declaration(view);

                SyntaxWalkControl::Continue
            }
            kind => {
                let Some(declaration_kind) = declaration_kind_for_syntax(kind) else {
                    return SyntaxWalkControl::Continue;
                };

                self.enter_declaration(view, declaration_kind)
            }
        }
    }

    fn exit_node(&mut self, view: SyntaxNodeView<'_>) -> SyntaxWalkControl {
        if let Some(declaration_kind) = declaration_kind_for_syntax(view.kind())
            && declaration_kind.child_container_kind().is_some()
        {
            self.exit_container_declaration(declaration_kind);
        }

        if view.kind() == SyntaxKind::BlockModuleDeclaration {
            match self.module_part_stack.pop() {
                Some(_) => {}
                None => panic!("block module exit must have a matching module part"),
            }
        }

        SyntaxWalkControl::Continue
    }

    fn enter_source_unit_module_declaration(&mut self, view: SyntaxNodeView<'_>) {
        let declaration = match view.cast::<SourceUnitModuleDeclarationSyntax>() {
            Some(declaration) => declaration,
            None => panic!("source-unit module declaration view should cast"),
        };

        let part = ModulePartBuilder::new(
            path_from_syntax(&declaration.module_path()),
            self.source_id,
            view.kind(),
            view.full_range(),
            view.is_recovered(),
        );

        let part_index = self.push_module_part(part);

        self.source_unit_module_part = Some(part_index);
    }

    fn enter_block_module_declaration(&mut self, view: SyntaxNodeView<'_>) {
        let declaration = match view.cast::<BlockModuleDeclarationSyntax>() {
            Some(declaration) => declaration,
            None => panic!("block module declaration view should cast"),
        };

        let part = ModulePartBuilder::new(
            path_from_syntax(&declaration.module_path()),
            self.source_id,
            view.kind(),
            view.full_range(),
            view.is_recovered(),
        );

        let part_index = self.push_module_part(part);

        self.module_part_stack.push(part_index);
    }

    fn enter_declaration(
        &mut self,
        view: SyntaxNodeView<'_>,
        declaration_kind: DeclarationKind,
    ) -> SyntaxWalkControl {
        if self.active_module_part().is_none() {
            return SyntaxWalkControl::SkipChildren;
        }

        let declaration = DeclarationBuilder::new(
            declaration_kind,
            declaration_name(view, declaration_kind),
            view.source().source_id(),
            view.kind(),
            view.full_range(),
            view.is_recovered(),
        );

        if declaration_kind.child_container_kind().is_some() {
            self.declaration_stack.push(declaration);

            return SyntaxWalkControl::Continue;
        }

        self.push_completed_declaration(declaration.finish());

        SyntaxWalkControl::SkipChildren
    }

    fn exit_container_declaration(&mut self, declaration_kind: DeclarationKind) {
        let declaration = match self.declaration_stack.pop() {
            Some(declaration) => declaration,
            None => panic!("container declaration exit must have a matching declaration"),
        };

        if declaration.kind != declaration_kind {
            panic!("container declaration exit kind must match the active declaration");
        }

        self.push_completed_declaration(declaration.finish());
    }

    fn push_completed_declaration(&mut self, declaration: DiscoveredDeclaration) {
        if let Some(parent) = self.declaration_stack.last_mut() {
            parent.children.push(declaration);
            return;
        }

        let Some(part_index) = self.active_module_part() else {
            return;
        };

        match self.module_parts.get_mut(part_index) {
            Some(part) => part.declarations.push(declaration),
            None => panic!("active module part index must refer to a module part"),
        }
    }

    fn active_module_part(&self) -> Option<usize> {
        self.module_part_stack
            .last()
            .copied()
            .or(self.source_unit_module_part)
    }

    fn push_module_part(&mut self, part: ModulePartBuilder) -> usize {
        let part_index = self.module_parts.len();

        self.module_parts.push(part);

        part_index
    }

    fn finish(self) -> DeclarationChunk {
        DeclarationChunk::new(
            self.source_id,
            self.module_parts
                .into_iter()
                .map(ModulePartBuilder::finish)
                .collect::<Vec<_>>()
                .into_boxed_slice(),
        )
    }
}

struct DeclarationBuilder {
    kind: DeclarationKind,
    name: Option<DeclarationName>,
    source_id: SourceId,
    syntax_kind: SyntaxKind,
    full_range: TextRange,
    is_recovered: bool,
    children: Vec<DiscoveredDeclaration>,
}

impl DeclarationBuilder {
    fn new(
        kind: DeclarationKind,
        name: Option<DeclarationName>,
        source_id: SourceId,
        syntax_kind: SyntaxKind,
        full_range: TextRange,
        is_recovered: bool,
    ) -> Self {
        Self {
            kind,
            name,
            source_id,
            syntax_kind,
            full_range,
            is_recovered,
            children: Vec::new(),
        }
    }

    fn finish(self) -> DiscoveredDeclaration {
        DiscoveredDeclaration::new(
            self.kind,
            self.name,
            self.source_id,
            self.syntax_kind,
            self.full_range,
            self.is_recovered,
            self.children.into_boxed_slice(),
        )
    }
}

struct ModulePartBuilder {
    path: ModulePath,
    source_id: SourceId,
    syntax_kind: SyntaxKind,
    full_range: TextRange,
    is_recovered: bool,
    declarations: Vec<DiscoveredDeclaration>,
}

impl ModulePartBuilder {
    fn new(
        path: ModulePath,
        source_id: SourceId,
        syntax_kind: SyntaxKind,
        full_range: TextRange,
        is_recovered: bool,
    ) -> Self {
        Self {
            path,
            source_id,
            syntax_kind,
            full_range,
            is_recovered,
            declarations: Vec::new(),
        }
    }

    fn finish(self) -> DiscoveredModulePart {
        DiscoveredModulePart::new(
            self.path,
            self.source_id,
            self.syntax_kind,
            self.full_range,
            self.is_recovered,
            self.declarations.into_boxed_slice(),
        )
    }
}

#[cfg(test)]
mod tests {
    use bray_syntax::SyntaxKind;
    use bray_testing::{test_source_at as source, test_source_store as source_store};

    use super::discover_source_unit_declarations;
    use crate::name::{DeclarationName, ImplementationDeclarationName, ModulePath};
    use crate::record::DeclarationKind;
    use crate::test_support::parse_valid_source_unit;

    #[test]
    fn source_unit_discovery_records_module_level_declarations_in_source_order() {
        let sources = source_store([concat!(
            "module core.io;\n",
            "using std.io;\n",
            "@symbol(\"entry\") func main() {}\n",
            "@copy struct Point {}\n",
            "overload draw = {fast}\n",
            "overload Point(Display) = {point_display}\n",
            "impl Point {}\n",
            "impl Point(Display) {}\n",
            "impl PointDisplay = Point(Display) {}\n",
        )]);

        let source_unit = parse_valid_source_unit(source(&sources, 0));
        let chunk = discover_source_unit_declarations(&source_unit);

        let [part] = chunk.module_parts() else {
            panic!("expected one module part: {:?}", chunk.module_parts());
        };

        assert_eq!(part.path().dotted(), "core.io");

        let declaration_kinds = part
            .declarations()
            .iter()
            .map(|declaration| declaration.kind())
            .collect::<Vec<_>>();

        assert_eq!(
            declaration_kinds,
            [
                DeclarationKind::Using,
                DeclarationKind::Function,
                DeclarationKind::Struct,
                DeclarationKind::CallableOverload,
                DeclarationKind::ImplementationOverload,
                DeclarationKind::InherentImplementation,
                DeclarationKind::UnnamedTraitImplementation,
                DeclarationKind::NamedTraitImplementation,
            ]
        );

        let identifier_names = part
            .declarations()
            .iter()
            .filter_map(identifier_name)
            .collect::<Vec<_>>();

        assert_eq!(identifier_names, ["main", "Point", "draw", "PointDisplay"]);

        let implementation_names = part
            .declarations()
            .iter()
            .filter_map(implementation_name)
            .map(|name| {
                (
                    name.subject().dotted(),
                    name.trait_path().map(ModulePath::dotted),
                )
            })
            .collect::<Vec<_>>();

        assert_eq!(
            implementation_names,
            [
                (String::from("Point"), Some(String::from("Display"))),
                (String::from("Point"), None),
                (String::from("Point"), Some(String::from("Display"))),
            ]
        );
    }

    #[test]
    fn source_unit_discovery_records_container_members_in_source_order() {
        let sources = source_store([concat!(
            "module core;\n",
            "struct Resource { value: Int; construct origin() -> Self {} finalize() {} func make() {} }\n",
            "union Maybe { Some(value: Int); None; }\n",
            "trait Scope { type Item; const Size: Int; predicate ready(); enter() -> Guard; func show(); }\n",
            "impl Resource { type Item = Element; const Size: Int = 1; predicate ready(); construct() -> Self {} exit() {} func make() {} }",
        )]);

        let source_unit = parse_valid_source_unit(source(&sources, 0));
        let chunk = discover_source_unit_declarations(&source_unit);

        let [part] = chunk.module_parts() else {
            panic!("expected one module part: {:?}", chunk.module_parts());
        };

        let top_level_kinds = part
            .declarations()
            .iter()
            .map(|declaration| declaration.kind())
            .collect::<Vec<_>>();

        assert_eq!(
            top_level_kinds,
            [
                DeclarationKind::Struct,
                DeclarationKind::Union,
                DeclarationKind::Trait,
                DeclarationKind::InherentImplementation,
            ]
        );

        let [
            struct_declaration,
            union_declaration,
            trait_declaration,
            implementation,
        ] = part.declarations()
        else {
            panic!(
                "expected four top-level declarations: {:?}",
                part.declarations()
            );
        };

        assert_eq!(
            declaration_kinds(struct_declaration.children()),
            [
                DeclarationKind::StructField,
                DeclarationKind::TypeConstructorMember,
                DeclarationKind::FinalizerMember,
                DeclarationKind::TypeCallableMember,
            ]
        );

        assert_eq!(
            declaration_kinds(union_declaration.children()),
            [DeclarationKind::UnionVariant, DeclarationKind::UnionVariant]
        );

        assert_eq!(
            declaration_kinds(trait_declaration.children()),
            [
                DeclarationKind::TraitTypeMember,
                DeclarationKind::TraitConstantMember,
                DeclarationKind::TraitPredicateMember,
                DeclarationKind::TraitScopeEnterRequirement,
                DeclarationKind::TraitCallableMember,
            ]
        );

        assert_eq!(
            declaration_kinds(implementation.children()),
            [
                DeclarationKind::ImplementationTypeMemberBinding,
                DeclarationKind::Constant,
                DeclarationKind::Predicate,
                DeclarationKind::TypeConstructorMember,
                DeclarationKind::ScopeExitMember,
                DeclarationKind::TypeCallableMember,
            ]
        );

        assert_eq!(
            struct_declaration
                .children()
                .iter()
                .filter_map(identifier_name)
                .collect::<Vec<_>>(),
            ["value", "origin", "make"]
        );

        assert_eq!(
            implementation
                .children()
                .iter()
                .filter_map(keyword_name)
                .collect::<Vec<_>>(),
            [SyntaxKind::ConstructKeyword, SyntaxKind::ExitKeyword]
        );
    }

    fn declaration_kinds(declarations: &[crate::DiscoveredDeclaration]) -> Vec<DeclarationKind> {
        declarations
            .iter()
            .map(crate::DiscoveredDeclaration::kind)
            .collect()
    }

    fn identifier_name(declaration: &crate::DiscoveredDeclaration) -> Option<&str> {
        match declaration.name() {
            Some(DeclarationName::Identifier(name)) => Some(name.as_str()),
            Some(
                DeclarationName::Keyword(_)
                | DeclarationName::Path(_)
                | DeclarationName::Implementation(_),
            )
            | None => None,
        }
    }

    fn keyword_name(declaration: &crate::DiscoveredDeclaration) -> Option<SyntaxKind> {
        match declaration.name() {
            Some(DeclarationName::Keyword(kind)) => Some(*kind),
            Some(
                DeclarationName::Identifier(_)
                | DeclarationName::Path(_)
                | DeclarationName::Implementation(_),
            )
            | None => None,
        }
    }

    fn implementation_name(
        declaration: &crate::DiscoveredDeclaration,
    ) -> Option<&ImplementationDeclarationName> {
        match declaration.name() {
            Some(DeclarationName::Implementation(name)) => Some(name),
            Some(
                DeclarationName::Identifier(_)
                | DeclarationName::Keyword(_)
                | DeclarationName::Path(_),
            )
            | None => None,
        }
    }
}
