use bray_bound_tree::{BoundSourceAnchor, BoundUnitId, BoundUnitKey};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{LocalSymbolRegionId, SymbolOrigin};
use bray_syntax::{BlockExpressionSyntax, SourceSyntaxNode, SyntaxCast, SyntaxWalkRoot};

use crate::BindingQueryContext;
use crate::binder::Binder;
use crate::lookup::{NameAccess, PathBindingContext};
use crate::unit::BoundUnitLocalBuilder;

pub(crate) fn binder_and_block<C>(binding_context: &C) -> (Binder<'_, C>, BlockExpressionSyntax)
where
    C: BindingQueryContext + ?Sized,
{
    let source_unit = match binding_context.syntax().source_units() {
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
        binder_for_first_function(binding_context, &function),
        body.block_expression(),
    )
}

pub(crate) fn binder_for_first_function<'binding_context, C>(
    binding_context: &'binding_context C,
    function: &bray_syntax::FunctionDeclarationSyntax,
) -> Binder<'binding_context, C>
where
    C: BindingQueryContext + ?Sized,
{
    let Some(symbol) = binding_context
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

    Binder::new(binding_context, unit)
}

pub(crate) fn internal_path_context<C>(
    binding_context: &C,
    scope: bray_symbols::LocalScopeId,
) -> PathBindingContext
where
    C: BindingQueryContext + ?Sized,
{
    let Some(module) = binding_context
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
    bray_testing::first_syntax_descendant(root)
}
