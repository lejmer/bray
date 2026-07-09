use bray_diagnostics::DiagnosticBag;
use bray_source::SourceId;
use bray_syntax::{
    BlockModuleDeclarationSyntax, SourceSyntaxNode, SourceUnitModuleDeclarationSyntax,
    SourceUnitSyntax, SyntaxKind, SyntaxNodeView, SyntaxWalkControl, SyntaxWalkEvent,
    walk_source_unit,
};

use super::children::declaration_children;
use super::names::{declaration_kind_for_syntax, declaration_name, path_from_syntax};
use super::surface::{declaration_surface, module_surface};
use crate::chunk::{DeclarationChunk, DiscoveredDeclaration, DiscoveredModulePart};
use crate::name::{DeclarationName, ModulePath};
use crate::record::DeclarationKind;
use crate::{DeclarationChunkResult, DeclarationSurface, SyntaxAnchor};

/// Discovers module parts and declarations from one source unit.
pub fn discover_source_unit_declarations(source_unit: &SourceUnitSyntax) -> DeclarationChunkResult {
    let mut discoverer = SourceUnitDiscoverer::new(source_unit.source().source_id());

    walk_source_unit(source_unit, |event| discoverer.visit(event));

    DeclarationChunkResult::new(discoverer.finish(), DiagnosticBag::new())
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
            && declaration_kind.walks_child_declarations()
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
            SyntaxAnchor::from_node(&view),
            module_surface(view),
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
            SyntaxAnchor::from_node(&view),
            module_surface(view),
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

        let mut declaration = DeclarationBuilder::new(
            declaration_kind,
            declaration_name(view, declaration_kind),
            SyntaxAnchor::from_node(&view),
            declaration_surface(view, declaration_kind),
        );

        declaration.extend_children(declaration_children(view, declaration_kind));

        if declaration_kind.walks_child_declarations() {
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
    syntax: SyntaxAnchor,
    surface: DeclarationSurface,
    children: Vec<DiscoveredDeclaration>,
}

impl DeclarationBuilder {
    fn new(
        kind: DeclarationKind,
        name: Option<DeclarationName>,
        syntax: SyntaxAnchor,
        surface: DeclarationSurface,
    ) -> Self {
        Self {
            kind,
            name,
            syntax,
            surface,
            children: Vec::new(),
        }
    }

    fn finish(self) -> DiscoveredDeclaration {
        DiscoveredDeclaration::new(
            self.kind,
            self.name,
            self.syntax,
            self.surface,
            self.children.into_boxed_slice(),
        )
    }

    fn extend_children(&mut self, children: impl IntoIterator<Item = DiscoveredDeclaration>) {
        self.children.extend(children);
    }
}

struct ModulePartBuilder {
    path: ModulePath,
    syntax: SyntaxAnchor,
    surface: DeclarationSurface,
    declarations: Vec<DiscoveredDeclaration>,
}

impl ModulePartBuilder {
    fn new(path: ModulePath, syntax: SyntaxAnchor, surface: DeclarationSurface) -> Self {
        Self {
            path,
            syntax,
            surface,
            declarations: Vec::new(),
        }
    }

    fn finish(self) -> DiscoveredModulePart {
        DiscoveredModulePart::new(
            self.path,
            self.syntax,
            self.surface,
            self.declarations.into_boxed_slice(),
        )
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{TextRange, TextSize};
    use bray_syntax::SyntaxKind;
    use bray_testing::{test_source_at as source, test_source_store as source_store};

    use super::discover_source_unit_declarations;
    use crate::name::{DeclarationName, ImplementationDeclarationName, ModulePath};
    use crate::record::DeclarationKind;
    use crate::test_support::{
        parse_recovered_source_unit_for_test, parse_valid_source_unit_for_test,
    };

    #[test]
    fn source_unit_discovery_preserves_recovered_module_parts_and_following_declarations() {
        let source_text = "module core\nusing std.io;";
        let sources = source_store([source_text]);
        let snapshot = source(&sources, 0);

        let source_unit = parse_recovered_source_unit_for_test(snapshot);
        let result = discover_source_unit_declarations(&source_unit);

        let chunk = result.chunk();

        assert!(result.diagnostics().is_empty());

        let [part] = chunk.module_parts() else {
            panic!("expected one module part: {:?}", chunk.module_parts());
        };

        assert_eq!(part.path().dotted(), "core");
        assert_eq!(part.source_id(), snapshot.source_id());
        assert_eq!(part.syntax_kind(), SyntaxKind::SourceUnitModuleDeclaration);

        assert_eq!(
            part.full_range(),
            TextRange::new(TextSize::ZERO, TextSize::new(12))
        );

        assert!(part.is_recovered());

        let [using_declaration] = part.declarations() else {
            panic!(
                "expected one declaration after the recovered module: {:?}",
                part.declarations()
            );
        };

        assert_eq!(using_declaration.kind(), DeclarationKind::Using);

        assert_eq!(
            using_declaration.full_range(),
            TextRange::new(TextSize::new(12), TextSize::new(25))
        );

        assert!(!using_declaration.is_recovered());
    }

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

        let source_unit = parse_valid_source_unit_for_test(source(&sources, 0));
        let result = discover_source_unit_declarations(&source_unit);
        let chunk = result.chunk();

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

        let source_unit = parse_valid_source_unit_for_test(source(&sources, 0));
        let result = discover_source_unit_declarations(&source_unit);

        let chunk = result.chunk();

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

    #[test]
    fn source_unit_discovery_records_signature_and_payload_children() {
        let sources = source_store([concat!(
            "module core;\n",
            "struct Resource<T, const N: Int> { ",
            "construct origin(value: T, count: Int) -> Self {} ",
            "async finalize(value: T) -> Unit {} ",
            "trusted destruct(item: T) {} ",
            "enter(scope: T) -> Guard {} ",
            "exit(scope: Guard) {} ",
            "func make<U, const M: Int>(value: U) {} ",
            "}\n",
            "union Maybe<T> { Some(value: T, fallback: T); }\n",
            "trait Scope<T> { ",
            "predicate ready(value: T); ",
            "enter(scope: T) -> Guard; ",
            "func show<U, const M: Int>(value: U); ",
            "}\n",
            "predicate valid<T, const N: Int>(value: T);\n",
            "func main<T, const N: Int>(value: T) {}",
        )]);

        let source_unit = parse_valid_source_unit_for_test(source(&sources, 0));
        let result = discover_source_unit_declarations(&source_unit);

        let chunk = result.chunk();

        let [part] = chunk.module_parts() else {
            panic!("expected one module part: {:?}", chunk.module_parts());
        };

        let resource = identifier_declaration(part.declarations(), "Resource");

        assert_eq!(
            declaration_kinds(resource.children()),
            [
                DeclarationKind::GenericTypeParameter,
                DeclarationKind::GenericConstParameter,
                DeclarationKind::TypeConstructorMember,
                DeclarationKind::FinalizerMember,
                DeclarationKind::DestructorMember,
                DeclarationKind::ScopeEnterMember,
                DeclarationKind::ScopeExitMember,
                DeclarationKind::TypeCallableMember,
            ]
        );

        let constructor =
            kind_declaration(resource.children(), DeclarationKind::TypeConstructorMember);

        assert_eq!(
            declaration_kinds(constructor.children()),
            [
                DeclarationKind::CallableParameter,
                DeclarationKind::CallableParameter
            ]
        );

        assert_eq!(identifier_names(constructor.children()), ["value", "count"]);

        let method = identifier_declaration(resource.children(), "make");

        assert_eq!(
            declaration_kinds(method.children()),
            [
                DeclarationKind::GenericTypeParameter,
                DeclarationKind::GenericConstParameter,
                DeclarationKind::CallableParameter,
            ]
        );

        assert_eq!(identifier_names(method.children()), ["U", "M", "value"]);

        let maybe = identifier_declaration(part.declarations(), "Maybe");
        let some = identifier_declaration(maybe.children(), "Some");

        assert_eq!(
            declaration_kinds(some.children()),
            [
                DeclarationKind::UnionPayloadField,
                DeclarationKind::UnionPayloadField
            ]
        );

        assert_eq!(identifier_names(some.children()), ["value", "fallback"]);

        let scope = identifier_declaration(part.declarations(), "Scope");
        let trait_predicate = identifier_declaration(scope.children(), "ready");
        let trait_callable = identifier_declaration(scope.children(), "show");

        assert_eq!(
            declaration_kinds(trait_predicate.children()),
            [DeclarationKind::PredicateParameter]
        );

        assert_eq!(identifier_names(trait_predicate.children()), ["value"]);

        assert_eq!(
            declaration_kinds(trait_callable.children()),
            [
                DeclarationKind::GenericTypeParameter,
                DeclarationKind::GenericConstParameter,
                DeclarationKind::CallableParameter,
            ]
        );

        assert_eq!(
            identifier_names(trait_callable.children()),
            ["U", "M", "value"]
        );

        let predicate = identifier_declaration(part.declarations(), "valid");

        assert_eq!(
            declaration_kinds(predicate.children()),
            [
                DeclarationKind::GenericTypeParameter,
                DeclarationKind::GenericConstParameter,
                DeclarationKind::PredicateParameter,
            ]
        );

        assert_eq!(identifier_names(predicate.children()), ["T", "N", "value"]);

        let function = identifier_declaration(part.declarations(), "main");

        assert_eq!(
            declaration_kinds(function.children()),
            [
                DeclarationKind::GenericTypeParameter,
                DeclarationKind::GenericConstParameter,
                DeclarationKind::CallableParameter,
            ]
        );

        assert_eq!(identifier_names(function.children()), ["T", "N", "value"]);
    }

    #[test]
    fn source_unit_discovery_records_syntax_anchors_and_surface_metadata() {
        let source_text = concat!(
            "@test internal module core { ",
            "@copy public struct Resource<T> with(copyable) { public mut value: T; } ",
            "@test @abi(\"C\") public async trusted func main<T, const N: Int>",
            "(pos value: Int = 1, mut tail: Bool,) -> Unit ",
            "requires(valid) ensures(done) with(static_ok) uses(core.io) {} ",
            "public callable Mapper<T> with(copyable) = func(value: T) -> Bool; ",
            "union Maybe { @tag(1) Some(pos value: Int); } ",
            "}"
        );

        let sources = source_store([source_text]);
        let snapshot = source(&sources, 0);
        let source_unit = parse_valid_source_unit_for_test(snapshot);
        let result = discover_source_unit_declarations(&source_unit);

        let chunk = result.chunk();

        let [part] = chunk.module_parts() else {
            panic!("expected one module part: {:?}", chunk.module_parts());
        };

        assert_eq!(part.syntax_anchor().source_id(), snapshot.source_id());

        assert_eq!(
            part.syntax_anchor().syntax_kind(),
            SyntaxKind::BlockModuleDeclaration
        );

        assert_eq!(
            part.surface().visibility(),
            Some(SyntaxKind::InternalKeyword)
        );

        assert_eq!(part.surface().modifiers(), &[]);

        assert_eq!(
            anchor_kinds(part.surface().directives()),
            [SyntaxKind::TestDirective]
        );

        let resource = identifier_declaration(part.declarations(), "Resource");

        assert_eq!(
            resource.syntax_anchor().syntax_kind(),
            SyntaxKind::StructDeclaration
        );

        assert_eq!(
            resource.surface().visibility(),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(
            anchor_kinds(resource.surface().directives()),
            [SyntaxKind::CopyDirective]
        );

        assert_eq!(
            anchor_kinds(resource.surface().constraints()),
            [SyntaxKind::WithClause]
        );

        let field = identifier_declaration(resource.children(), "value");

        assert_eq!(
            field.surface().visibility(),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(field.surface().modifiers(), &[SyntaxKind::MutKeyword]);

        let function = identifier_declaration(part.declarations(), "main");

        assert_eq!(
            function.surface().visibility(),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(
            function.surface().modifiers(),
            &[SyntaxKind::AsyncKeyword, SyntaxKind::TrustedKeyword]
        );

        assert_eq!(
            anchor_kinds(function.surface().directives()),
            [SyntaxKind::TestDirective, SyntaxKind::AbiDirective]
        );

        assert_eq!(
            anchor_kinds(function.surface().contract_clauses()),
            [
                SyntaxKind::RequiresClause,
                SyntaxKind::EnsuresClause,
                SyntaxKind::WithClause,
                SyntaxKind::UsesClause
            ]
        );

        let value_parameter = identifier_declaration(function.children(), "value");
        let tail_parameter = identifier_declaration(function.children(), "tail");

        assert_eq!(
            value_parameter.surface().modifiers(),
            &[SyntaxKind::PosKeyword]
        );

        assert_eq!(
            tail_parameter.surface().modifiers(),
            &[SyntaxKind::MutKeyword]
        );

        let callable_contract = identifier_declaration(part.declarations(), "Mapper");

        assert_eq!(
            callable_contract.surface().visibility(),
            Some(SyntaxKind::PublicKeyword)
        );

        assert_eq!(
            anchor_kinds(callable_contract.surface().constraints()),
            [SyntaxKind::WithClause]
        );

        let union = identifier_declaration(part.declarations(), "Maybe");
        let variant = identifier_declaration(union.children(), "Some");

        assert_eq!(
            anchor_kinds(variant.surface().directives()),
            [SyntaxKind::TagDirective]
        );

        let payload_field = identifier_declaration(variant.children(), "value");

        assert_eq!(
            payload_field.surface().modifiers(),
            &[SyntaxKind::PosKeyword]
        );
    }

    fn declaration_kinds(declarations: &[crate::DiscoveredDeclaration]) -> Vec<DeclarationKind> {
        declarations
            .iter()
            .map(crate::DiscoveredDeclaration::kind)
            .collect()
    }

    fn identifier_names(declarations: &[crate::DiscoveredDeclaration]) -> Vec<&str> {
        declarations.iter().filter_map(identifier_name).collect()
    }

    fn identifier_declaration<'declarations>(
        declarations: &'declarations [crate::DiscoveredDeclaration],
        name: &str,
    ) -> &'declarations crate::DiscoveredDeclaration {
        match declarations
            .iter()
            .find(|declaration| identifier_name(declaration) == Some(name))
        {
            Some(declaration) => declaration,
            None => panic!("expected identifier declaration named {name}: {declarations:?}"),
        }
    }

    fn kind_declaration(
        declarations: &[crate::DiscoveredDeclaration],
        kind: DeclarationKind,
    ) -> &crate::DiscoveredDeclaration {
        match declarations
            .iter()
            .find(|declaration| declaration.kind() == kind)
        {
            Some(declaration) => declaration,
            None => panic!("expected declaration kind {kind:?}: {declarations:?}"),
        }
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

    fn anchor_kinds(anchors: &[crate::SyntaxAnchor]) -> Vec<SyntaxKind> {
        anchors.iter().map(|anchor| anchor.syntax_kind()).collect()
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
