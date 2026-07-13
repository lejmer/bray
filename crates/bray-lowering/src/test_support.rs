use bray_bound_tree::{
    BoundCallableBody, BoundNodeOrigin, BoundSourceAnchor, BoundTreeBuilder, BoundUnit,
    BoundUnitId, BoundUnitKey, BoundUnitRoot,
};
use bray_declarations::{DeclarationId, discover_source_unit_declarations};
use bray_parser::parse_source_unit;
use bray_source::{
    SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextSize,
};
use bray_symbols::{
    LocalScopeBoundary, LocalSymbolRegionId, LocalSymbolRegionKey, LocalSymbolRegionRole,
    LocalSymbolSnapshot, LocalSymbolSnapshotBuilder, ModulePathKey, PackageIdentity, SymbolKey,
    SymbolKind, SymbolRootKey,
};

pub(crate) use bray_symbols::testing::available_compiler_known_symbols;

pub(crate) fn bound_unit(unit: u32) -> BoundUnit {
    let (key, local_symbols) = unit_identity(unit);

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

fn unit_identity(unit: u32) -> (BoundUnitKey, LocalSymbolSnapshot) {
    let snapshot = source();
    let parsed = parse_source_unit(&snapshot);

    assert!(parsed.diagnostics().is_empty());

    let declarations = discover_source_unit_declarations(parsed.source_unit());

    assert!(declarations.diagnostics().is_empty());

    let [part] = declarations.chunk().module_parts() else {
        panic!("test source must contain one module part");
    };

    let source = BoundSourceAnchor::new(part.syntax_anchor(), snapshot.version());
    let owner = function_key();

    let Some(key) = BoundUnitKey::callable_body(owner.clone(), source) else {
        panic!("function must support a callable body");
    };

    let Some(region_key) = LocalSymbolRegionKey::try_new(
        owner,
        LocalSymbolRegionRole::CallableBody,
        [source.syntax()],
        None,
    ) else {
        panic!("test local symbol region key must be valid");
    };

    let mut symbols = LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(unit), region_key);

    if let Err(error) = symbols.push_scope(
        None,
        LocalScopeBoundary::Root,
        source.syntax(),
        TextSize::ZERO,
    ) {
        panic!("test root scope must validate: {error:?}");
    }

    let symbols = match symbols.finish() {
        Ok(symbols) => symbols,
        Err(error) => panic!("test local symbols must validate: {error:?}"),
    };

    (key, symbols)
}

fn function_key() -> SymbolKey {
    let Some(package) = PackageIdentity::try_new("example.package") else {
        panic!("test package identity must be valid");
    };

    let Some(path) = ModulePathKey::try_new(["example"]) else {
        panic!("test module path must be valid");
    };

    let module = SymbolKey::module(SymbolRootKey::Package(package), path);

    let Some(function) =
        SymbolKey::source_declaration(module, SymbolKind::Function, DeclarationId::new(0))
    else {
        panic!("function symbols must be source-declared");
    };

    function
}

fn source() -> SourceSnapshot {
    match SourceSnapshot::new(
        SourceId::new(0),
        SourceIdentity::new(0),
        SourceOrigin::virtual_source("lowering-test"),
        SourceVersion::new(1),
        "module example;",
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => panic!("test source must fit: {error:?}"),
    }
}
