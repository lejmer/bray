use std::sync::OnceLock;

use bray_bound_tree::{
    BoundBlock, BoundBlockItem, BoundCallableBody, BoundCallableBodyId, BoundErrorExpression,
    BoundExpression, BoundNodeOrigin, BoundSourceAnchor, BoundTree, BoundTreeBuilder, BoundUnitId,
    BoundUnitKey,
};
use bray_declarations::{DeclarationId, discover_source_unit_declarations};
use bray_parser::parse_source_unit;
use bray_source::{
    SourceId, SourceIdentity, SourceOrigin, SourceSnapshot, SourceVersion, TextSizeOverflow,
};
use bray_symbols::{
    ModulePathKey, PackageIdentity, SemanticValueStore, SymbolKey, SymbolKind, SymbolRootKey,
    TypeData, TypeId,
};

pub(crate) use bray_symbols::testing::available_compiler_known_symbols;

pub(crate) fn control_fact_selections() -> &'static crate::ControlFactSelections {
    static SELECTIONS: OnceLock<crate::ControlFactSelections> = OnceLock::new();

    SELECTIONS.get_or_init(crate::ControlFactSelections::new)
}

pub(crate) fn semantic_values() -> &'static SemanticValueStore {
    static VALUES: OnceLock<SemanticValueStore> = OnceLock::new();

    VALUES.get_or_init(|| match SemanticValueStore::try_new() {
        Ok(values) => values,
        Err(error) => panic!("test semantic value store must be available: {error:?}"),
    })
}

pub(crate) fn callable_key() -> BoundUnitKey {
    let snapshot = match source() {
        Ok(snapshot) => snapshot,
        Err(error) => panic!("test source must fit in the source range representation: {error:?}"),
    };

    let parsed = parse_source_unit(&snapshot);

    assert!(parsed.diagnostics().is_empty());

    let declarations = discover_source_unit_declarations(parsed.source_unit());

    assert!(declarations.diagnostics().is_empty());

    let [part] = declarations.chunk().module_parts() else {
        panic!("test source must contain one module part");
    };

    let source = BoundSourceAnchor::new(part.syntax_anchor(), snapshot.version());

    let Some(package) = PackageIdentity::try_new("example.package") else {
        panic!("test package identity must be non-empty");
    };

    let Some(path) = ModulePathKey::try_new(["example"]) else {
        panic!("test module path must be non-empty");
    };

    let module = SymbolKey::module(SymbolRootKey::Package(package), path);

    let Some(owner) =
        SymbolKey::source_declaration(module, SymbolKind::Function, DeclarationId::new(0))
    else {
        panic!("function symbols must be source-declared");
    };

    let Some(key) = BoundUnitKey::callable_body(owner, source) else {
        panic!("functions must support callable-body units");
    };

    key
}

pub(crate) fn recovered_tree(
    unit: BoundUnitId,
    key: &BoundUnitKey,
) -> (BoundTree, BoundCallableBodyId) {
    let mut builder = BoundTreeBuilder::new(unit);
    let origin = BoundNodeOrigin::source(key.source());

    let Ok(root) = builder.push_callable_body(BoundCallableBody::error(origin, None)) else {
        panic!("one recovered callable body must fit in an empty test tree");
    };

    (builder.finish(), root)
}

pub(crate) fn normally_completing_recovered_tree(
    unit: BoundUnitId,
    key: &BoundUnitKey,
) -> (BoundTree, BoundCallableBodyId) {
    let mut builder = BoundTreeBuilder::new(unit);
    let origin = BoundNodeOrigin::source(key.source());

    let expression = BoundExpression::Error(BoundErrorExpression::new(origin, error_type()));

    let Ok(expression) = builder.push_expression(expression) else {
        panic!("one recovered expression must fit in an empty test tree");
    };

    let block = BoundBlock::new(origin, [BoundBlockItem::Expression(expression)], false);

    let Ok(block) = builder.push_block(block) else {
        panic!("one block must fit in the test tree");
    };

    let Ok(root) = builder.push_callable_body(BoundCallableBody::block(origin, block)) else {
        panic!("one callable body must fit in the test tree");
    };

    (builder.finish(), root)
}

pub(crate) fn error_type() -> TypeId {
    let Ok(ty) = semantic_values().intern_type(TypeData::Error) else {
        panic!("test error type must be interned");
    };

    ty
}

fn source() -> Result<SourceSnapshot, TextSizeOverflow> {
    SourceSnapshot::new(
        SourceId::new(0),
        SourceIdentity::new(0),
        SourceOrigin::virtual_source("checker-test"),
        SourceVersion::new(1),
        "module example;",
    )
}
