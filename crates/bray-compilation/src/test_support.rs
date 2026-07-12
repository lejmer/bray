use bray_bound_tree::{BoundSourceAnchor, BoundUnitKey};
use bray_declarations::{DeclarationId, discover_source_unit_declarations};
use bray_parser::parse_source_unit;
use bray_source::{SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion};
use bray_symbols::{ModulePathKey, PackageIdentity, SymbolKey, SymbolKind, SymbolRootKey};

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
