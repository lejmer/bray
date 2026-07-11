use bray_declarations::{
    DeclarationChunkResult, DeclarationTable, discover_source_unit_declarations,
    merge_declaration_chunks,
};
use bray_testing::{test_source_at, test_source_store};

use super::{
    NeverCancelSymbolCompletion, SymbolCompletionLevel, SymbolCompletionPlanError,
    SymbolFactCompletionRequest, SymbolFactKind,
};
use crate::{AnySymbolId, PackageIdentity, SymbolGraph, SymbolId};

#[test]
fn recursive_completion_uses_owned_preorder_and_applicable_facts() {
    let graph = graph(&["module app; func make<T, const N: Int>(first: T = 1, second: T) {}"]);
    let package = AnySymbolId::from(graph.packages()[0].id());

    let plan = match graph.completion_plan(
        package,
        SymbolCompletionLevel::DeclarationSurface,
        &NeverCancelSymbolCompletion,
    ) {
        Ok(plan) => plan,
        Err(error) => panic!("completion plan should build: {error:?}"),
    };

    let symbols = plan
        .units()
        .iter()
        .map(|unit| unit.symbol().kind())
        .collect::<Vec<_>>();

    assert_eq!(
        symbols,
        [
            crate::SymbolKind::Package,
            crate::SymbolKind::Module,
            crate::SymbolKind::Function,
            crate::SymbolKind::GenericTypeParameter,
            crate::SymbolKind::GenericConstParameter,
            crate::SymbolKind::CallableParameter,
            crate::SymbolKind::CallableParameterDefaultProvider,
            crate::SymbolKind::CallableParameter,
        ]
    );

    let function = &plan.units()[2];

    assert_eq!(
        function.facts(),
        [
            SymbolFactKind::Directives,
            SymbolFactKind::GenericParameters,
            SymbolFactKind::GenericConstraints,
            SymbolFactKind::CallableSignature,
            SymbolFactKind::CallableContracts,
        ]
    );

    assert!(plan.units()[0].facts().is_empty());
    assert!(plan.units()[6].facts().is_empty());

    let default_requests = plan
        .requests()
        .iter()
        .filter(|request| request.kind() == SymbolFactKind::CallableParameterDefault)
        .collect::<Vec<_>>();

    assert_eq!(default_requests.len(), 1);
    assert_eq!(default_requests[0].symbol(), plan.units()[5].symbol());
}

#[test]
fn identity_completion_traverses_without_forcing_facts() {
    let graph = graph(&["module app; func main() {}"]);
    let package = AnySymbolId::from(graph.packages()[0].id());

    let plan = match graph.completion_plan(
        package,
        SymbolCompletionLevel::Identity,
        &NeverCancelSymbolCompletion,
    ) {
        Ok(plan) => plan,
        Err(error) => panic!("identity completion should build: {error:?}"),
    };

    assert_eq!(plan.units().len(), 3);
    assert!(plan.requests().is_empty());
    assert!(plan.units().iter().all(|unit| unit.facts().is_empty()));
}

#[test]
fn recovered_symbols_and_defaults_remain_completable() {
    let graph = graph(&["module app; func make(value: Int = ) {}"]);
    let package = AnySymbolId::from(graph.packages()[0].id());

    let plan = match graph.completion_plan(
        package,
        SymbolCompletionLevel::DeclarationSurface,
        &NeverCancelSymbolCompletion,
    ) {
        Ok(plan) => plan,
        Err(error) => panic!("recovered completion should build: {error:?}"),
    };

    assert!(plan.units().iter().any(|unit| matches!(
        unit.symbol(),
        AnySymbolId::CallableParameterDefaultProvider(_)
    )));

    assert!(
        plan.requests()
            .iter()
            .any(|request| request.kind() == SymbolFactKind::CallableParameterDefault)
    );
}

#[test]
fn absent_field_defaults_are_not_forced() {
    let graph = graph(&[
        "module app; struct Config { first: Int; second: Int = 1; } union Choice { Value(first: Int, second: Int = 1); }",
    ]);
    let package = AnySymbolId::from(graph.packages()[0].id());

    let plan = match graph.completion_plan(
        package,
        SymbolCompletionLevel::DeclarationSurface,
        &NeverCancelSymbolCompletion,
    ) {
        Ok(plan) => plan,
        Err(error) => panic!("field completion should build: {error:?}"),
    };

    let default_requests = plan
        .requests()
        .iter()
        .filter(|request| {
            matches!(
                request.kind(),
                SymbolFactKind::StructFieldDefault | SymbolFactKind::UnionPayloadFieldDefault
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(default_requests.len(), 2);
}

#[test]
fn referenced_semantic_recursion_does_not_expand_owned_completion() {
    let graph = graph(&["module app; func recurse() { recurse() }"]);
    let function = AnySymbolId::from(graph.functions()[0].id());

    let plan = match graph.completion_plan(
        function,
        SymbolCompletionLevel::DeclarationSurface,
        &NeverCancelSymbolCompletion,
    ) {
        Ok(plan) => plan,
        Err(error) => panic!("recursive declaration should complete: {error:?}"),
    };

    assert_eq!(plan.units().len(), 1);
    assert!(
        plan.requests()
            .iter()
            .all(|request| request.symbol() == function)
    );
}

#[test]
fn completion_rejects_symbols_outside_the_graph() {
    let graph = graph(&["module app; func main() {}"]);
    let unknown = AnySymbolId::Function(crate::FunctionSymbolId::from_symbol_id(SymbolId::new(99)));

    assert_eq!(
        graph.completion_plan(
            unknown,
            SymbolCompletionLevel::DeclarationSurface,
            &NeverCancelSymbolCompletion,
        ),
        Err(SymbolCompletionPlanError::UnknownSymbol(unknown))
    );
}

#[test]
fn completion_contracts_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<SymbolCompletionLevel>();
    assert_send_sync::<SymbolFactKind>();
    assert_send_sync::<super::SymbolCompletionPlan>();
    assert_send_sync::<SymbolFactCompletionRequest>();
}

fn graph(source_texts: &[&str]) -> SymbolGraph {
    let table = declaration_table(source_texts);

    let Some(package) = PackageIdentity::try_new("test.package") else {
        panic!("test package identity should be valid");
    };

    match SymbolGraph::build_source(package, &table) {
        Ok(graph) => graph,
        Err(error) => panic!("test symbol graph should build: {error:?}"),
    }
}

fn declaration_table(source_texts: &[&str]) -> DeclarationTable {
    let sources = test_source_store(source_texts);
    let chunks = (0u32..)
        .take(source_texts.len())
        .map(|index| declaration_chunk(&sources, index))
        .collect::<Vec<_>>();

    let result = merge_declaration_chunks(chunks.iter());
    let (table, _diagnostics) = result.into_parts();

    table
}

fn declaration_chunk(sources: &bray_source::SourceStore, index: u32) -> DeclarationChunkResult {
    let parsed = bray_parser::parse_source_unit(test_source_at(sources, index));

    discover_source_unit_declarations(parsed.source_unit())
}
