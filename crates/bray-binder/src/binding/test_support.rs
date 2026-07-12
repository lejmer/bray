use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalSymbolRegionId, SymbolOrigin};
use bray_syntax::{BlockExpressionSyntax, SourceSyntaxNode};

use crate::BinderFactContext;
use crate::request::{BinderRequestContext, BindingContext};
use crate::unit::BoundUnitLocalBuilder;

pub(crate) fn request_and_block<'facts, C>(
    facts: &'facts C,
) -> (BinderRequestContext<'facts, C>, BlockExpressionSyntax)
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
        request_for_first_function(facts, &function),
        body.block_expression(),
    )
}

pub(crate) fn request_for_first_function<'facts, C>(
    facts: &'facts C,
    function: &bray_syntax::FunctionDeclarationSyntax,
) -> BinderRequestContext<'facts, C>
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

    BinderRequestContext::new(facts, BindingContext::CallableBody, unit)
}
