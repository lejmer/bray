use bray_bound_tree::{
    BoundCallableBody, BoundExpressionId, BoundNodeOrigin, BoundSourceAnchor, BoundTreeBuilder,
    BoundUnit, BoundUnitId, BoundUnitKey, BoundUnitRoot,
};
use bray_declarations::{DeclarationId, SyntaxAnchor, discover_source_unit_declarations};
use bray_parser::parse_source_unit;
use bray_source::TextSize;
use bray_symbols::{
    LocalScopeBoundary, LocalSymbolRegionId, LocalSymbolRegionKey, LocalSymbolRegionRole,
    LocalSymbolSnapshot, LocalSymbolSnapshotBuilder, ModulePathKey, PackageIdentity,
    SymbolFactKind, SymbolKey, SymbolKind, SymbolRootKey,
};

use crate::test_source_snapshot;

/// Builds one canonical recovered callable-body unit for semantic boundary tests.
pub fn test_bound_unit(unit: u32) -> BoundUnit {
    let (key, local_symbols) = callable_unit_identity(unit);

    let mut tree = BoundTreeBuilder::new(BoundUnitId::new(unit));
    let body = BoundCallableBody::error(BoundNodeOrigin::source(key.source()), None);

    let root = match tree.push_callable_body(body) {
        Ok(root) => root,
        Err(error) => panic!("test callable body must fit: {error:?}"),
    };

    match BoundUnit::try_new(
        key,
        tree.finish(),
        local_symbols,
        [],
        BoundUnitRoot::CallableBody(root),
    ) {
        Ok(unit) => unit,
        Err(error) => panic!("test bound unit must validate: {error:?}"),
    }
}

/// Builds one expression-rooted bound unit through a caller-provided tree fixture.
pub fn test_expression_unit(
    unit: u32,
    build: impl FnOnce(&mut BoundTreeBuilder, BoundNodeOrigin) -> BoundExpressionId,
) -> BoundUnit {
    let (key, local_symbols) = expression_unit_identity(unit);

    let origin = BoundNodeOrigin::source(key.source());
    let mut tree = BoundTreeBuilder::new(BoundUnitId::new(unit));
    let root = build(&mut tree, origin);

    match BoundUnit::try_new(
        key,
        tree.finish(),
        local_symbols,
        [],
        BoundUnitRoot::Expression(root),
    ) {
        Ok(unit) => unit,
        Err(error) => panic!("test expression unit must validate: {error:?}"),
    }
}

fn callable_unit_identity(unit: u32) -> (BoundUnitKey, LocalSymbolSnapshot) {
    let (source, syntax) = source_anchor();

    let owner = source_declaration(SymbolKind::Function);

    let Some(key) = BoundUnitKey::callable_body(owner.clone(), source) else {
        panic!("function must support a callable body");
    };

    let symbols = local_symbols(unit, owner, LocalSymbolRegionRole::CallableBody, syntax);

    (key, symbols)
}

fn expression_unit_identity(unit: u32) -> (BoundUnitKey, LocalSymbolSnapshot) {
    let (source, syntax) = source_anchor();

    let owner = source_declaration(SymbolKind::Constant);

    let Some(key) = BoundUnitKey::constant_template(owner.clone(), source) else {
        panic!("constant must support a constant-template unit");
    };

    let symbols = local_symbols(
        unit,
        owner,
        LocalSymbolRegionRole::DeclarationFact(SymbolFactKind::ConstantDefinition),
        syntax,
    );

    (key, symbols)
}

fn source_anchor() -> (BoundSourceAnchor, SyntaxAnchor) {
    let snapshot = test_source_snapshot("module example;");
    let parsed = parse_source_unit(&snapshot);

    assert!(parsed.diagnostics().is_empty());

    let declarations = discover_source_unit_declarations(parsed.source_unit());

    assert!(declarations.diagnostics().is_empty());

    let [part] = declarations.chunk().module_parts() else {
        panic!("test source must contain one module part");
    };

    let syntax = part.syntax_anchor();
    let source = BoundSourceAnchor::new(syntax, snapshot.version());

    (source, syntax)
}

fn source_declaration(kind: SymbolKind) -> SymbolKey {
    let Some(package) = PackageIdentity::try_new("example.package") else {
        panic!("test package identity must be valid");
    };

    let Some(path) = ModulePathKey::try_new(["example"]) else {
        panic!("test module path must be valid");
    };

    let module = SymbolKey::module(SymbolRootKey::Package(package), path);

    let Some(owner) = SymbolKey::source_declaration(module, kind, DeclarationId::new(0)) else {
        panic!("test symbol kind must support source declarations");
    };

    owner
}

fn local_symbols(
    unit: u32,
    owner: SymbolKey,
    role: LocalSymbolRegionRole,
    syntax: SyntaxAnchor,
) -> LocalSymbolSnapshot {
    let Some(region_key) = LocalSymbolRegionKey::try_new(owner, role, [syntax], None) else {
        panic!("test local symbol region key must be valid");
    };

    let mut symbols = LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(unit), region_key);

    if let Err(error) = symbols.push_scope(None, LocalScopeBoundary::Root, syntax, TextSize::ZERO) {
        panic!("test root scope must validate: {error:?}");
    }

    match symbols.finish() {
        Ok(symbols) => symbols,
        Err(error) => panic!("test local symbols must validate: {error:?}"),
    }
}
