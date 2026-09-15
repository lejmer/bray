use bray_bound_tree::{
    BoundCallableBody, BoundExpressionId, BoundNodeOrigin, BoundSourceAnchor, BoundTreeBuilder,
    BoundUnit, BoundUnitId, BoundUnitKey, BoundUnitRoot,
};
use bray_declarations::{DeclarationId, SyntaxAnchor, discover_source_unit_declarations};
use bray_parser::parse_source_unit;
use bray_source::TextSize;
use bray_symbols::{
    LocalScopeBoundary, LocalSymbolRegionId, LocalSymbolRegionKey, LocalSymbolRegionRole,
    LocalSymbolSnapshot, LocalSymbolSnapshotBuilder, ModulePathKey, PackageIdentity, SymbolKey,
    SymbolKind, SymbolOrdinal, SymbolQueryKind, SymbolRootKey, SynthesizedSymbolKey,
};

use crate::test_source_snapshot;

/// Builds one canonical recovered callable-body unit for semantic boundary tests.
pub fn test_bound_unit(unit: u32) -> BoundUnit {
    test_bound_unit_with_declaration(unit, 0)
}

/// Builds one recovered callable-body unit with a caller-selected declaration identity.
pub fn test_bound_unit_with_declaration(unit: u32, declaration: u32) -> BoundUnit {
    let (key, local_symbols) = callable_unit_identity(unit, declaration);

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
        BoundUnitRoot::CallableBody {
            execution: bray_symbols::CallableExecution::Synchronous,
            body: root,
        },
    ) {
        Ok(unit) => unit,
        Err(error) => panic!("test bound unit must validate: {error:?}"),
    }
}

/// Builds one runtime-default unit through a caller-provided expression fixture.
pub fn test_runtime_default_unit(
    unit: u32,
    build: impl FnOnce(&mut BoundTreeBuilder, BoundNodeOrigin) -> BoundExpressionId,
) -> BoundUnit {
    let (key, local_symbols) = runtime_default_unit_identity(unit);

    test_expression_unit(unit, key, local_symbols, build)
}

/// Builds one constant-template unit through a caller-provided expression fixture.
pub fn test_constant_template_unit(
    unit: u32,
    build: impl FnOnce(&mut BoundTreeBuilder, BoundNodeOrigin) -> BoundExpressionId,
) -> BoundUnit {
    let (key, local_symbols) = constant_template_unit_identity(unit);

    test_expression_unit(unit, key, local_symbols, build)
}

fn test_expression_unit(
    unit: u32,
    key: BoundUnitKey,
    local_symbols: LocalSymbolSnapshot,
    build: impl FnOnce(&mut BoundTreeBuilder, BoundNodeOrigin) -> BoundExpressionId,
) -> BoundUnit {
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

fn callable_unit_identity(unit: u32, declaration: u32) -> (BoundUnitKey, LocalSymbolSnapshot) {
    let (source, syntax) = source_anchor();

    let owner = source_declaration(SymbolKind::Function, declaration);

    let Some(key) = BoundUnitKey::callable_body(owner.clone(), source) else {
        panic!("function must support a callable body");
    };

    let symbols = local_symbols(unit, owner, LocalSymbolRegionRole::CallableBody, syntax);

    (key, symbols)
}

fn runtime_default_unit_identity(unit: u32) -> (BoundUnitKey, LocalSymbolSnapshot) {
    let (source, syntax) = source_anchor();

    let callable = source_declaration(SymbolKind::Function, 0);

    let parameter = SymbolKey::synthesized(SynthesizedSymbolKey::callable_parameter(
        callable,
        SymbolOrdinal::new(0),
    ));

    let owner = SymbolKey::synthesized(SynthesizedSymbolKey::callable_parameter_default_provider(
        parameter,
    ));

    let Some(key) = BoundUnitKey::runtime_default(owner.clone(), source) else {
        panic!("default provider must support a runtime-default unit");
    };

    let symbols = local_symbols(
        unit,
        owner,
        LocalSymbolRegionRole::DeclarationQuery(SymbolQueryKind::CallableParameterDefault),
        syntax,
    );

    (key, symbols)
}

fn constant_template_unit_identity(unit: u32) -> (BoundUnitKey, LocalSymbolSnapshot) {
    let (source, syntax) = source_anchor();

    let owner = source_declaration(SymbolKind::Constant, 0);

    let Some(key) = BoundUnitKey::constant_template(owner.clone(), source) else {
        panic!("constant must support a constant-template unit");
    };

    let symbols = local_symbols(
        unit,
        owner,
        LocalSymbolRegionRole::DeclarationQuery(SymbolQueryKind::ConstantDefinition),
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

fn source_declaration(kind: SymbolKind, declaration: u32) -> SymbolKey {
    let Some(package) = PackageIdentity::try_new("example.package") else {
        panic!("test package identity must be valid");
    };

    let Some(path) = ModulePathKey::try_new(["example"]) else {
        panic!("test module path must be valid");
    };

    let module = SymbolKey::module(SymbolRootKey::Package(package), path);

    let Some(owner) = SymbolKey::source_declaration(module, kind, DeclarationId::new(declaration))
    else {
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

    symbols.finish()
}
