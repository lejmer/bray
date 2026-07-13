use bray_declarations::{DeclarationId, SyntaxAnchor, discover_source_unit_declarations};
use bray_source::{
    SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextRange, TextSize,
    TextSizeOverflow,
};
use bray_symbols::{
    ModulePathKey, PackageIdentity, SemanticValueStore, SymbolKey, SymbolKind, SymbolRootKey,
    SynthesizedSymbolKey, TypeData,
};
use bray_syntax::{
    ModuleDirectivesSyntax, ModuleModifiersSyntax, PathSyntax, SourceUnitModuleDeclarationSyntax,
    SourceUnitSyntax, SyntaxKind, SyntaxToken, SyntaxTrivia,
};

use crate::{BoundErrorExpression, BoundExpression, BoundNodeOrigin, BoundSourceAnchor};

pub(crate) fn error_expression() -> BoundExpression {
    BoundExpression::Error(BoundErrorExpression::new(
        BoundNodeOrigin::source(source_anchor()),
        error_type(),
    ))
}

pub(crate) fn error_type() -> bray_symbols::TypeId {
    let Ok(store) = SemanticValueStore::try_new() else {
        panic!("test semantic store ID must be available");
    };

    let Ok(ty) = store.intern_type(TypeData::Error) else {
        panic!("test error type must be interned");
    };

    ty
}

pub(crate) fn source_anchor() -> BoundSourceAnchor {
    source_anchor_with_version(1)
}

pub(crate) fn source_anchor_with_version(version: u64) -> BoundSourceAnchor {
    let snapshot = match test_source() {
        Ok(snapshot) => snapshot,
        Err(error) => panic!("test source must fit in the source range representation: {error:?}"),
    };

    let source_unit = source_unit(snapshot.clone());

    let declarations = discover_source_unit_declarations(&source_unit);

    assert!(declarations.diagnostics().is_empty());

    BoundSourceAnchor::new(
        module_anchor(declarations.chunk()),
        SourceVersion::new(version),
    )
}

pub(crate) fn symbol_key(kind: SymbolKind, declaration: u32) -> SymbolKey {
    let Some(package) = PackageIdentity::try_new("example.package") else {
        panic!("test package identity is non-empty");
    };

    let Some(path) = ModulePathKey::try_new(["example"]) else {
        panic!("test module path is non-empty");
    };

    let owner = SymbolKey::module(SymbolRootKey::Package(package), path);

    let Some(key) = SymbolKey::source_declaration(owner, kind, DeclarationId::new(declaration))
    else {
        panic!("test symbol kind must be source-declared");
    };

    key
}

pub(crate) fn runtime_default_key(declaration: u32) -> SymbolKey {
    let parameter = symbol_key(SymbolKind::CallableParameter, declaration);
    let provider = SynthesizedSymbolKey::callable_parameter_default_provider(parameter);

    SymbolKey::synthesized(provider)
}

fn module_anchor(chunk: &bray_declarations::DeclarationChunk) -> SyntaxAnchor {
    let [part] = chunk.module_parts() else {
        panic!("test source must contain exactly one module part");
    };

    part.syntax_anchor()
}

fn source_unit(snapshot: SourceSnapshot) -> SourceUnitSyntax {
    let mut source_unit = SourceUnitSyntax::builder(snapshot.clone());

    source_unit.push_source_unit_module_declaration(module_declaration(snapshot));
    source_unit.push_token(token(SyntaxKind::EndOfFileToken, 15, 15));

    source_unit.build()
}

fn module_declaration(snapshot: SourceSnapshot) -> SourceUnitModuleDeclarationSyntax {
    let mut declaration =
        SourceUnitModuleDeclarationSyntax::builder(snapshot.clone(), TextSize::ZERO);

    declaration.push_module_directives(
        ModuleDirectivesSyntax::builder(snapshot.clone(), TextSize::ZERO).build(),
    );
    declaration.push_module_modifiers(
        ModuleModifiersSyntax::builder(snapshot.clone(), TextSize::ZERO).build(),
    );
    declaration.push_module_keyword(
        token(SyntaxKind::ModuleKeyword, 0, 6).with_trailing_trivia([SyntaxTrivia::whitespace(
            TextRange::new(TextSize::new(6), TextSize::new(7)),
        )]),
    );

    let mut path = PathSyntax::builder(snapshot);

    path.push_identifier_token(token(SyntaxKind::IdentifierToken, 7, 14));

    declaration.push_module_path(path.build());
    declaration.push_semicolon_token(token(SyntaxKind::SemicolonToken, 14, 15));

    declaration.build()
}

fn token(kind: SyntaxKind, start: u32, end: u32) -> SyntaxToken {
    SyntaxToken::new(
        kind,
        TextRange::new(TextSize::new(start), TextSize::new(end)),
    )
}

fn test_source() -> Result<SourceSnapshot, TextSizeOverflow> {
    SourceSnapshot::new(
        SourceId::new(0),
        SourceIdentity::new(0),
        SourceOrigin::virtual_source("bound-tree-test"),
        SourceVersion::new(1),
        "module example;",
    )
}
