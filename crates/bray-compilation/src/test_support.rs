use bray_bound_tree::{
    BoundCallableBody, BoundNodeOrigin, BoundSourceAnchor, BoundTreeBuilder, BoundUnitId,
    BoundUnitKey, CheckedCallableBody,
};
use bray_declarations::{DeclarationId, discover_source_unit_declarations};
use bray_parser::parse_source_unit;
use bray_source::{
    SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextSize,
};
use bray_symbols::{
    LocalScopeBoundary, LocalSymbolRegionId, LocalSymbolRegionKey, LocalSymbolRegionRole,
    LocalSymbolSnapshotBuilder, ModulePathKey, PackageIdentity, SymbolKey, SymbolKind,
    SymbolRootKey,
};

pub(crate) fn callable_body_key(declaration: u32) -> BoundUnitKey {
    valid_key(BoundUnitKey::callable_body(
        symbol_key(SymbolKind::Function, declaration),
        source_anchor(),
    ))
}

pub(crate) fn constant_template_key(declaration: u32) -> BoundUnitKey {
    valid_key(BoundUnitKey::constant_template(
        symbol_key(SymbolKind::Constant, declaration),
        source_anchor(),
    ))
}

pub(crate) fn checked_callable_body(declaration: u32) -> (BoundUnitKey, CheckedCallableBody) {
    let owner = symbol_key(SymbolKind::Function, declaration);
    let key = valid_key(BoundUnitKey::callable_body(owner.clone(), source_anchor()));
    let unit = BoundUnitId::new(declaration);

    let mut tree = BoundTreeBuilder::new(unit);
    let body = BoundCallableBody::error(BoundNodeOrigin::source(key.source()), None);

    let root = match tree.push_callable_body(body) {
        Ok(root) => root,
        Err(error) => panic!("test callable body must fit: {error:?}"),
    };

    let syntax = key.source().syntax();

    let Some(region_key) =
        LocalSymbolRegionKey::try_new(owner, LocalSymbolRegionRole::CallableBody, [syntax], None)
    else {
        panic!("test callable region key must be valid");
    };

    let mut locals =
        LocalSymbolSnapshotBuilder::new(LocalSymbolRegionId::new(declaration), region_key);

    if let Err(error) = locals.push_scope(None, LocalScopeBoundary::Root, syntax, TextSize::ZERO) {
        panic!("test root scope must be valid: {error:?}");
    }

    let locals = match locals.finish() {
        Ok(locals) => locals,
        Err(error) => panic!("test local snapshot must be valid: {error:?}"),
    };

    let checked = match CheckedCallableBody::try_new(&key, tree.finish(), locals, [], root) {
        Ok(checked) => checked,
        Err(error) => panic!("test callable body must be publishable: {error:?}"),
    };

    (key, checked)
}

fn source_anchor() -> BoundSourceAnchor {
    let snapshot = match SourceSnapshot::new(
        SourceId::new(0),
        SourceIdentity::new(0),
        SourceOrigin::virtual_source("compilation-test"),
        SourceVersion::new(1),
        "module example;",
    ) {
        Ok(snapshot) => snapshot,
        Err(error) => panic!("test source must be representable: {error:?}"),
    };

    let syntax = parse_source_unit(&snapshot);
    let declarations = discover_source_unit_declarations(syntax.source_unit());

    let [module] = declarations.chunk().module_parts() else {
        panic!("test source must produce one module part");
    };

    BoundSourceAnchor::new(module.syntax_anchor(), SourceVersion::new(1))
}

fn symbol_key(kind: SymbolKind, declaration: u32) -> SymbolKey {
    let Some(package) = PackageIdentity::try_new("example.package") else {
        panic!("test package identity must be valid");
    };

    let Some(path) = ModulePathKey::try_new(["example"]) else {
        panic!("test module path must be valid");
    };

    let owner = SymbolKey::module(SymbolRootKey::Package(package), path);

    let Some(key) = SymbolKey::source_declaration(owner, kind, DeclarationId::new(declaration))
    else {
        panic!("test declaration kind must be source-declared");
    };

    key
}

fn valid_key(key: Option<BoundUnitKey>) -> BoundUnitKey {
    match key {
        Some(key) => key,
        None => panic!("test owner must support the requested unit category"),
    }
}
