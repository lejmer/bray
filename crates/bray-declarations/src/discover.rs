use bray_source::{SourceId, TextRange};
use bray_syntax::{
    BlockModuleDeclarationSyntax, CallableContractDeclarationSyntax,
    CallableOverloadDeclarationSyntax, ConstantDeclarationSyntax, ExportDeclarationSyntax,
    FunctionDeclarationSyntax, ImplementationOverloadDeclarationSyntax,
    InherentImplementationDeclarationSyntax, NamedTraitImplementationDeclarationSyntax, PathSyntax,
    PredicateDeclarationSyntax, SourceSyntaxNode, SourceUnitModuleDeclarationSyntax,
    SourceUnitSyntax, StructDeclarationSyntax, SyntaxCast, SyntaxKind, SyntaxNodeView, SyntaxToken,
    SyntaxWalkControl, SyntaxWalkEvent, TraitDeclarationSyntax, UnionDeclarationSyntax,
    UnnamedTraitImplementationDeclarationSyntax, UsingDeclarationSyntax, walk_source_unit,
};

use crate::chunk::{DeclarationChunk, DiscoveredDeclaration, DiscoveredModulePart};
use crate::name::{DeclarationName, ImplementationDeclarationName, ModulePath};
use crate::record::DeclarationKind;

/// Discovers module parts and module-level declarations from one source unit.
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
}

impl SourceUnitDiscoverer {
    fn new(source_id: SourceId) -> Self {
        Self {
            source_id,
            module_parts: Vec::new(),
            source_unit_module_part: None,
            module_part_stack: Vec::new(),
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

                self.record_module_declaration(view, declaration_kind);

                SyntaxWalkControl::SkipChildren
            }
        }
    }

    fn exit_node(&mut self, view: SyntaxNodeView<'_>) -> SyntaxWalkControl {
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

    fn record_module_declaration(
        &mut self,
        view: SyntaxNodeView<'_>,
        declaration_kind: DeclarationKind,
    ) {
        let Some(part_index) = self.active_module_part() else {
            return;
        };

        let declaration = DiscoveredDeclaration::new(
            declaration_kind,
            declaration_name(view, declaration_kind),
            view.source().source_id(),
            view.kind(),
            view.full_range(),
            view.is_recovered(),
        );

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

fn declaration_kind_for_syntax(kind: SyntaxKind) -> Option<DeclarationKind> {
    match kind {
        SyntaxKind::UsingDeclaration => Some(DeclarationKind::Using),
        SyntaxKind::ExportDeclaration => Some(DeclarationKind::Export),
        SyntaxKind::ConstantDeclaration => Some(DeclarationKind::Constant),
        SyntaxKind::FunctionDeclaration => Some(DeclarationKind::Function),
        SyntaxKind::PredicateDeclaration => Some(DeclarationKind::Predicate),
        SyntaxKind::CallableContractDeclaration => Some(DeclarationKind::CallableContract),
        SyntaxKind::CallableOverloadDeclaration => Some(DeclarationKind::CallableOverload),
        SyntaxKind::ImplementationOverloadDeclaration => {
            Some(DeclarationKind::ImplementationOverload)
        }
        SyntaxKind::StructDeclaration => Some(DeclarationKind::Struct),
        SyntaxKind::UnionDeclaration => Some(DeclarationKind::Union),
        SyntaxKind::TraitDeclaration => Some(DeclarationKind::Trait),
        SyntaxKind::InherentImplementationDeclaration => {
            Some(DeclarationKind::InherentImplementation)
        }
        SyntaxKind::UnnamedTraitImplementationDeclaration => {
            Some(DeclarationKind::UnnamedTraitImplementation)
        }
        SyntaxKind::NamedTraitImplementationDeclaration => {
            Some(DeclarationKind::NamedTraitImplementation)
        }
        _ => None,
    }
}

fn declaration_name(
    view: SyntaxNodeView<'_>,
    declaration_kind: DeclarationKind,
) -> Option<DeclarationName> {
    match declaration_kind {
        DeclarationKind::Using => {
            let declaration = cast_node::<UsingDeclarationSyntax>(view, "using declaration");

            path_declaration_name(&declaration.path())
        }
        DeclarationKind::Export => {
            let declaration = cast_node::<ExportDeclarationSyntax>(view, "export declaration");

            path_declaration_name(&declaration.path())
        }
        DeclarationKind::Constant => {
            let declaration = cast_node::<ConstantDeclarationSyntax>(view, "constant declaration");

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::Function => {
            let declaration = cast_node::<FunctionDeclarationSyntax>(view, "function declaration");

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::Predicate => {
            let declaration =
                cast_node::<PredicateDeclarationSyntax>(view, "predicate declaration");

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::CallableContract => {
            let declaration = cast_node::<CallableContractDeclarationSyntax>(
                view,
                "callable contract declaration",
            );

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::Struct => {
            let declaration = cast_node::<StructDeclarationSyntax>(view, "struct declaration");

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::Union => {
            let declaration = cast_node::<UnionDeclarationSyntax>(view, "union declaration");

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::Trait => {
            let declaration = cast_node::<TraitDeclarationSyntax>(view, "trait declaration");

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::CallableOverload => {
            let declaration = cast_node::<CallableOverloadDeclarationSyntax>(
                view,
                "callable overload declaration",
            );

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::ImplementationOverload => {
            let declaration = cast_node::<ImplementationOverloadDeclarationSyntax>(
                view,
                "implementation overload declaration",
            );

            implementation_declaration_name(
                &declaration.implementation_overload_subject().path(),
                Some(&declaration.trait_path()),
            )
        }
        DeclarationKind::InherentImplementation => {
            let declaration = cast_node::<InherentImplementationDeclarationSyntax>(
                view,
                "inherent implementation declaration",
            );

            implementation_declaration_name(&declaration.implementation_subject().path(), None)
        }
        DeclarationKind::UnnamedTraitImplementation => {
            let declaration = cast_node::<UnnamedTraitImplementationDeclarationSyntax>(
                view,
                "unnamed trait implementation declaration",
            );

            implementation_declaration_name(
                &declaration.implementation_subject().path(),
                Some(&declaration.trait_application().path()),
            )
        }
        DeclarationKind::NamedTraitImplementation => {
            let declaration = cast_node::<NamedTraitImplementationDeclarationSyntax>(
                view,
                "named trait implementation declaration",
            );

            identifier_declaration_name(view.source().text(), declaration.identifier_token())
        }
        DeclarationKind::Module => None,
    }
}

fn cast_node<T>(view: SyntaxNodeView<'_>, description: &'static str) -> T
where
    T: SyntaxCast,
{
    match view.cast::<T>() {
        Some(node) => node,
        None => panic!("{description} view should cast"),
    }
}

fn path_declaration_name(path: &PathSyntax) -> Option<DeclarationName> {
    let path = path_from_syntax(path);

    if path.is_empty() {
        None
    } else {
        Some(DeclarationName::Path(path))
    }
}

fn identifier_declaration_name(source_text: &str, token: SyntaxToken) -> Option<DeclarationName> {
    token_text(source_text, token).map(DeclarationName::Identifier)
}

fn implementation_declaration_name(
    subject_path: &PathSyntax,
    trait_path: Option<&PathSyntax>,
) -> Option<DeclarationName> {
    let subject = path_from_syntax(subject_path);
    let trait_path = trait_path
        .map(path_from_syntax)
        .filter(|path| !path.is_empty());

    if subject.is_empty() && trait_path.is_none() {
        return None;
    }

    Some(DeclarationName::Implementation(
        ImplementationDeclarationName::new(subject, trait_path),
    ))
}

fn path_from_syntax(path: &PathSyntax) -> ModulePath {
    path_from_identifier_tokens(path.source().text(), path.identifier_tokens())
}

fn path_from_identifier_tokens(
    source_text: &str,
    tokens: impl IntoIterator<Item = SyntaxToken>,
) -> ModulePath {
    ModulePath::new(
        tokens
            .into_iter()
            .filter_map(|token| token_text(source_text, token)),
    )
}

fn token_text(source_text: &str, token: SyntaxToken) -> Option<String> {
    let text = token.text(source_text)?;

    if text.is_empty() {
        return None;
    }

    Some(text.to_owned())
}

#[cfg(test)]
mod tests {
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
    fn source_unit_discovery_skips_nested_declaration_bodies() {
        let sources = source_store([
            "module core;\nstruct Box { const Nested: Int = 1; }\nconst Top: Int = 2;",
        ]);

        let source_unit = parse_valid_source_unit(source(&sources, 0));
        let chunk = discover_source_unit_declarations(&source_unit);

        let [part] = chunk.module_parts() else {
            panic!("expected one module part: {:?}", chunk.module_parts());
        };

        let declaration_kinds = part
            .declarations()
            .iter()
            .map(|declaration| declaration.kind())
            .collect::<Vec<_>>();

        assert_eq!(
            declaration_kinds,
            [DeclarationKind::Struct, DeclarationKind::Constant]
        );
    }

    fn identifier_name(declaration: &crate::DiscoveredDeclaration) -> Option<&str> {
        match declaration.name() {
            Some(DeclarationName::Identifier(name)) => Some(name.as_str()),
            Some(DeclarationName::Path(_) | DeclarationName::Implementation(_)) | None => None,
        }
    }

    fn implementation_name(
        declaration: &crate::DiscoveredDeclaration,
    ) -> Option<&ImplementationDeclarationName> {
        match declaration.name() {
            Some(DeclarationName::Implementation(name)) => Some(name),
            Some(DeclarationName::Identifier(_) | DeclarationName::Path(_)) | None => None,
        }
    }
}
