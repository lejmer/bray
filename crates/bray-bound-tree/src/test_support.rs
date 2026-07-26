use bray_declarations::{DeclarationId, SyntaxAnchor, discover_source_unit_declarations};
use bray_source::{
    SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextRange, TextSize,
    TextSizeOverflow,
};
use bray_symbols::{
    LocalScopeBoundary, LocalSymbolRegionId, LocalSymbolRegionKey, LocalSymbolRegionRole,
    LocalSymbolSnapshotBuilder, ModulePathKey, PackageIdentity, SemanticValueStore, SymbolKey,
    SymbolKind, SymbolRootKey, SynthesizedSymbolKey, TypeData,
};
use bray_syntax::{
    ModuleDirectivesSyntax, ModuleModifiersSyntax, PathSyntax, SourceUnitModuleDeclarationSyntax,
    SourceUnitSyntax, SyntaxKind, SyntaxToken, SyntaxTrivia,
};

use crate::{
    BoundBlock, BoundBlockItem, BoundCallableBody, BoundErrorExpression, BoundExpression,
    BoundExpressionId, BoundNodeOrigin, BoundSourceAnchor, BoundTreeBuilder, BoundUnit,
    BoundUnitId, BoundUnitKey, BoundUnitRoot,
};

pub(crate) use crate::testing::push_expression;

pub(crate) fn error_expression() -> BoundExpression {
    BoundExpression::Error(BoundErrorExpression::new(
        BoundNodeOrigin::source(source_anchor()),
        error_type(),
    ))
}

pub(crate) fn error_type() -> bray_symbols::TypeId {
    let store = semantic_values();

    error_type_in(&store)
}

pub(crate) fn error_type_in(store: &SemanticValueStore) -> bray_symbols::TypeId {
    let Ok(ty) = store.intern_type(TypeData::Error) else {
        panic!("test error type must be interned");
    };

    ty
}

pub(crate) fn semantic_values() -> SemanticValueStore {
    let Ok(store) = SemanticValueStore::try_new() else {
        panic!("test semantic store ID must be available");
    };

    store
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

pub(crate) fn expression_unit(
    unit: BoundUnitId,
    build: impl FnOnce(&mut BoundTreeBuilder, BoundNodeOrigin) -> Vec<BoundExpressionId>,
) -> (BoundUnit, Vec<BoundExpressionId>) {
    let owner = symbol_key(SymbolKind::Function, 0);

    let Some(key) = BoundUnitKey::callable_body(owner.clone(), source_anchor()) else {
        panic!("function must support a callable body");
    };

    let origin = BoundNodeOrigin::source(key.source());

    let mut tree = BoundTreeBuilder::new(unit);

    let expressions = build(&mut tree, origin);
    let items = expressions.iter().copied().map(BoundBlockItem::Expression);

    let block = match tree.push_block(BoundBlock::new(origin, items, false)) {
        Ok(block) => block,
        Err(error) => panic!("test block must be valid: {error:?}"),
    };

    let root = match tree.push_callable_body(BoundCallableBody::block(origin, block)) {
        Ok(root) => root,
        Err(error) => panic!("test callable body must be valid: {error:?}"),
    };

    let region = LocalSymbolRegionId::new(unit.raw());

    let Some(region_key) = LocalSymbolRegionKey::try_new(
        owner,
        LocalSymbolRegionRole::CallableBody,
        [key.source().syntax()],
        None,
    ) else {
        panic!("callable test key must form a local symbol region");
    };

    let mut symbols = LocalSymbolSnapshotBuilder::new(region, region_key);

    if let Err(error) = symbols.push_scope(
        None,
        LocalScopeBoundary::Root,
        key.source().syntax(),
        key.source().syntax().full_range().start(),
    ) {
        panic!("test root scope must validate: {error:?}");
    }

    let symbols = match symbols.finish() {
        Ok(symbols) => symbols,
        Err(error) => panic!("test symbols must validate: {error:?}"),
    };

    let unit = match BoundUnit::try_new(
        key,
        tree.finish(),
        symbols,
        [],
        BoundUnitRoot::CallableBody(root),
    ) {
        Ok(unit) => unit,
        Err(error) => panic!("test bound unit must validate: {error:?}"),
    };

    (unit, expressions)
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
