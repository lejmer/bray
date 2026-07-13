use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalSymbolRegionId, SymbolOrigin};
use bray_syntax::{
    BlockExpressionSyntax, SourceSyntaxNode, SyntaxCast, SyntaxWalkControl, SyntaxWalkEvent,
    SyntaxWalkRoot, walk_syntax_node,
};

use crate::BinderFactContext;
use crate::binder::{Binder, BindingContext};
use crate::lookup::{NameAccess, PathBindingContext};
use crate::unit::BoundUnitLocalBuilder;

pub(crate) fn binder_and_block<C>(facts: &C) -> (Binder<'_, C>, BlockExpressionSyntax)
where
    C: BinderFactContext + ?Sized,
{
    let source_unit = match facts.syntax().source_units() {
        [source_unit] => source_unit,
        source_units => panic!("test must contain one source unit: {}", source_units.len()),
    };

    let Some(function) = source_unit.function_declarations().next() else {
        panic!("test source must contain a function");
    };

    let Some(body) = function.callable_body_block_expression() else {
        panic!("test function must contain a body");
    };

    (
        binder_for_first_function(facts, &function),
        body.block_expression(),
    )
}

pub(crate) fn binder_for_first_function<'facts, C>(
    facts: &'facts C,
    function: &bray_syntax::FunctionDeclarationSyntax,
) -> Binder<'facts, C>
where
    C: BinderFactContext + ?Sized,
{
    let Some(symbol) = facts
        .symbols()
        .functions()
        .iter()
        .find(|symbol| symbol.origin() == SymbolOrigin::Source)
    else {
        panic!("test graph must contain a source function");
    };

    let source = BoundSourceAnchor::new(
        SyntaxAnchor::from_node(function),
        function.source().version(),
    );

    let key = match BoundUnitKey::callable_body(symbol.key().clone(), source) {
        Some(key) => key,
        None => panic!("test function must own a callable body"),
    };

    let unit = match BoundUnitLocalBuilder::new(
        BoundUnitId::new(30),
        key,
        LocalSymbolRegionId::new(30),
        function.full_range().start(),
    ) {
        Ok(unit) => unit,
        Err(error) => panic!("test bound unit must build: {error:?}"),
    };

    Binder::new(facts, BindingContext::CallableBody, unit)
}

pub(crate) fn internal_path_context<C>(
    facts: &C,
    scope: bray_symbols::LocalScopeId,
) -> PathBindingContext
where
    C: BinderFactContext + ?Sized,
{
    let Some(module) = facts
        .symbols()
        .modules()
        .iter()
        .find(|module| module.origin() == SymbolOrigin::Source)
    else {
        panic!("test graph must contain a source module");
    };

    PathBindingContext::new(scope, module.id(), module.owner(), NameAccess::Internal)
}

pub(crate) fn first_descendant<T>(root: &impl SyntaxWalkRoot) -> Option<T>
where
    T: SyntaxCast,
{
    let mut result = None;

    walk_syntax_node(root, |event| {
        let SyntaxWalkEvent::EnterNode(node) = event else {
            return SyntaxWalkControl::Continue;
        };

        if node.kind() != T::KIND {
            return SyntaxWalkControl::Continue;
        }

        result = node.cast();

        SyntaxWalkControl::Stop
    });

    result
}
