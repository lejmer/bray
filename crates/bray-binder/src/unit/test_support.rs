use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey};
use bray_declarations::{
    SyntaxAnchor, discover_source_unit_declarations, merge_declaration_chunks,
};
use bray_source::{SourceVersion, TextSize};
use bray_symbols::{
    LocalBindingSymbolId, LocalScopeBoundary, LocalScopeId, LocalSymbolRegionId, PackageIdentity,
    SymbolGraph, SymbolName, SymbolOrdinal, SymbolOrigin,
};
use bray_testing::{test_source_at, test_source_store};

use super::builder::BoundUnitLocalBuilder;
use super::{AnonymousCallableBoundary, BoundUnitConstructionResult};

pub(crate) struct Fixture {
    pub(crate) key: BoundUnitKey,
    pub(crate) first: SyntaxAnchor,
    pub(crate) second: SyntaxAnchor,
    pub(crate) foreign: SyntaxAnchor,
    pub(crate) version: SourceVersion,
}

pub(crate) fn fixture() -> Fixture {
    let sources = test_source_store([
        concat!(
            "module app;\n",
            "func main()\n",
            "{\n",
            "}\n",
            "func next()\n",
            "{\n",
            "}",
        ),
        concat!("module other;\n", "func foreign()\n", "{\n", "}",),
    ]);

    let parsed = bray_parser::parse_source_unit(test_source_at(&sources, 0));
    let chunk = discover_source_unit_declarations(parsed.source_unit());
    let merged = merge_declaration_chunks([&chunk]);

    let foreign_parsed = bray_parser::parse_source_unit(test_source_at(&sources, 1));
    let foreign_chunk = discover_source_unit_declarations(foreign_parsed.source_unit());
    let foreign_merged = merge_declaration_chunks([&foreign_chunk]);

    let Some(package) = PackageIdentity::try_new("test.package") else {
        panic!("test package identity must be valid");
    };

    let graph = match SymbolGraph::build_source(package, merged.table()) {
        Ok(graph) => graph,
        Err(error) => panic!("test symbol graph must build: {error:?}"),
    };

    let source_functions = graph
        .functions()
        .iter()
        .filter(|function| function.origin() == SymbolOrigin::Source)
        .collect::<Vec<_>>();

    let [function, second] = source_functions.as_slice() else {
        panic!("test source must declare two functions");
    };

    let Some(first) = function.syntax_anchor() else {
        panic!("test source function must retain its syntax anchor");
    };

    let Some(second) = second.syntax_anchor() else {
        panic!("second test source function must retain its syntax anchor");
    };

    let version = SourceVersion::new(3);
    let source = BoundSourceAnchor::new(first, version);

    let key = match BoundUnitKey::callable_body(function.key().clone(), source) {
        Some(key) => key,
        None => panic!("function must own a callable body"),
    };

    let Some(foreign_package) = PackageIdentity::try_new("foreign.package") else {
        panic!("foreign test package identity must be valid");
    };

    let foreign_graph = match SymbolGraph::build_source(foreign_package, foreign_merged.table()) {
        Ok(graph) => graph,
        Err(error) => panic!("foreign test symbol graph must build: {error:?}"),
    };

    let foreign_functions = foreign_graph
        .functions()
        .iter()
        .filter(|function| function.origin() == SymbolOrigin::Source)
        .collect::<Vec<_>>();

    let [foreign] = foreign_functions.as_slice() else {
        panic!("foreign test source must declare one function");
    };

    let Some(foreign) = foreign.syntax_anchor() else {
        panic!("foreign test source function must retain its syntax anchor");
    };

    Fixture {
        key,
        first,
        second,
        foreign,
        version,
    }
}

pub(crate) fn builder(fixture: &Fixture, region: LocalSymbolRegionId) -> BoundUnitLocalBuilder {
    match BoundUnitLocalBuilder::new(
        BoundUnitId::new(7),
        fixture.key.clone(),
        region,
        TextSize::ZERO,
    ) {
        Ok(builder) => builder,
        Err(error) => panic!("test unit builder must build: {error:?}"),
    }
}

pub(super) fn push_scope(
    builder: &mut BoundUnitLocalBuilder,
    parent: LocalScopeId,
    boundary: LocalScopeBoundary,
    syntax: SyntaxAnchor,
    visibility_start: u32,
) -> LocalScopeId {
    match builder.push_scope(parent, boundary, syntax, TextSize::new(visibility_start)) {
        Ok(scope) => scope,
        Err(error) => panic!("test scope must build: {error:?}"),
    }
}

pub(crate) fn push_binding(
    builder: &mut BoundUnitLocalBuilder,
    scope: LocalScopeId,
    syntax: SyntaxAnchor,
    is_recovered: bool,
) -> LocalBindingSymbolId {
    representative_binding(
        builder,
        scope,
        [syntax],
        SymbolOrdinal::new(0),
        is_recovered,
    )
}

pub(super) fn representative_binding(
    builder: &mut BoundUnitLocalBuilder,
    scope: LocalScopeId,
    anchors: impl IntoIterator<Item = SyntaxAnchor>,
    ordinal: SymbolOrdinal,
    is_recovered: bool,
) -> LocalBindingSymbolId {
    match builder.push_binding(
        scope,
        symbol_name("value"),
        anchors,
        Some(ordinal),
        is_recovered,
    ) {
        Ok(binding) => binding,
        Err(error) => panic!("test binding must build: {error:?}"),
    }
}

pub(super) fn push_root_anonymous_callable(
    builder: &mut BoundUnitLocalBuilder,
    introduction_scope: LocalScopeId,
    fixture: &Fixture,
) -> AnonymousCallableBoundary {
    match builder.push_root_anonymous_callable(
        introduction_scope,
        BoundSourceAnchor::new(fixture.first, fixture.version),
        fixture.first.full_range().start(),
        Some(SymbolOrdinal::new(0)),
        false,
    ) {
        Ok(boundary) => boundary,
        Err(error) => panic!("anonymous callable must build: {error:?}"),
    }
}

pub(super) fn finish(builder: BoundUnitLocalBuilder) -> BoundUnitConstructionResult {
    match builder.finish() {
        Ok(result) => result,
        Err(error) => panic!("test unit must freeze: {error:?}"),
    }
}

pub(super) fn symbol_name(name: &str) -> SymbolName {
    match SymbolName::try_new(name) {
        Some(name) => name,
        None => panic!("test symbol name must be non-empty"),
    }
}
