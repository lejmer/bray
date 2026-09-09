use bray_bound_tree::BoundUnitKey;
use bray_declarations::SyntaxAnchor;
use bray_symbols::{
    CallableTypeTemplate, LocalBindingSymbolId, LocalScopeId, SymbolName, SymbolOrdinal,
};
use bray_syntax::{
    LambdaExpressionSyntax, ParameterListSyntax, SourceSyntaxNode, SyntaxTree, SyntaxWalkControl,
    SyntaxWalkEvent, TypeExpressionSyntax, walk_syntax_node,
};

use super::BoundUnitBindingError;
use super::support::{map_query_error, missing_syntax};
use crate::binder::Binder;
use crate::{BindingError, BindingQueryContext};

pub(super) fn callable_contract_source(
    tree: &SyntaxTree,
    clause: SyntaxAnchor,
) -> Option<SyntaxAnchor> {
    let source = tree
        .source_units()
        .iter()
        .find(|unit| unit.source().source_id() == clause.source_id())?;

    let mut owner = None;

    walk_syntax_node(source, |event| {
        let SyntaxWalkEvent::EnterNode(node) = event else {
            return SyntaxWalkControl::Continue;
        };

        let range = node.full_range();
        let target = clause.full_range();

        if range.start() > target.start() || range.end() < target.end() {
            return SyntaxWalkControl::SkipChildren;
        }

        if SyntaxAnchor::from_node(&node) == clause {
            return SyntaxWalkControl::Stop;
        }

        if node.cast::<LambdaExpressionSyntax>().is_some()
            || node
                .cast::<TypeExpressionSyntax>()
                .is_some_and(|ty| ty.func_keyword().is_some())
        {
            owner = Some(SyntaxAnchor::from_node(&node));
        }

        SyntaxWalkControl::Continue
    });

    owner
}

pub(super) fn install_contract_parameters<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
    source: SyntaxAnchor,
    signature: &CallableTypeTemplate,
) -> Result<Vec<LocalBindingSymbolId>, BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    let parameters = source
        .find_descendant::<ParameterListSyntax>(binder.binding_context().syntax())
        .ok_or_else(|| missing_syntax(binder.unit().key()))?;

    let parameters = parameters.parameters().collect::<Vec<_>>();

    if parameters.len() != signature.parameters().len() {
        return Err(BoundUnitBindingError::Assembly(
            crate::BoundUnitAssemblyError::InvalidBoundUnit(
                bray_bound_tree::BoundUnitBuildError::ContractParameterCountMismatch {
                    expected: signature.parameters().len(),
                    actual: parameters.len(),
                },
            ),
        ));
    }

    let mut inputs = Vec::with_capacity(parameters.len());

    for (index, (parameter, formal)) in parameters.iter().zip(signature.parameters()).enumerate() {
        let source = SyntaxAnchor::from_node(parameter);

        let name = SymbolName::try_new(formal.name().as_str()).ok_or(
            BoundUnitBindingError::Binding(BindingError::SyntaxContract(source)),
        )?;

        let ordinal = u32::try_from(index)
            .map(SymbolOrdinal::new)
            .map_err(|_| BoundUnitBindingError::Binding(BindingError::IdentityCapacityExceeded))?;

        let input = binder
            .unit_mut()
            .push_binding(scope, name, [source], Some(ordinal), source.is_recovered())
            .map_err(BoundUnitBindingError::Construction)?;

        binder
            .unit_mut()
            .activate_local(scope, input)
            .map_err(BoundUnitBindingError::Construction)?;

        inputs.push(input);
    }

    Ok(inputs)
}

pub(super) fn input_signature<C>(
    context: &C,
    key: &BoundUnitKey,
    source: SyntaxAnchor,
) -> Result<
    bray_diagnostics::DiagnosticResult<CallableTypeTemplate>,
    BoundUnitBindingError<C::UpstreamError>,
>
where
    C: BindingQueryContext + ?Sized,
{
    let owner = context
        .symbols()
        .symbol_for_key(key.declared_owner())
        .ok_or_else(|| super::support::missing_owner(key, None))?;

    context
        .callable_contract_input_signature(owner, source)
        .map_err(map_query_error)
}

#[cfg(test)]
mod tests {
    use super::callable_contract_source;
    use crate::BindingQueryContext;
    use crate::query::test_support::TestFixture;
    use bray_declarations::SyntaxAnchor;
    use bray_syntax::{EnsuresClauseSyntax, SyntaxWalkControl, SyntaxWalkEvent, walk_syntax_node};

    #[test]
    fn contract_inputs_use_the_nearest_callable_occurrence() {
        let fixture = TestFixture::from_source(
            r#"
            module app;

            const fixture: i32 = 1;

            func outer(pos outer_value: bool)
                ensures(true)
            {
                let value: func(pos inner_value: bool) -> bool
                    ensures(inner_value) = lambda(pos lambda_value: bool) -> bool
                    when(lambda_value)
                    {
                        ensures(result)
                    }
                {
                    return lambda_value;
                };
            }
            "#,
        );

        let context = fixture.context();
        let tree = context.syntax();
        let mut owners = Vec::new();

        walk_syntax_node(&tree.source_units()[0], |event| {
            if let SyntaxWalkEvent::EnterNode(node) = event
                && node.cast::<EnsuresClauseSyntax>().is_some()
            {
                owners.push(
                    callable_contract_source(tree, SyntaxAnchor::from_node(&node))
                        .map(|source| source.syntax_kind()),
                );
            }

            SyntaxWalkControl::Continue
        });

        assert_eq!(
            owners,
            vec![
                None,
                Some(bray_syntax::SyntaxKind::TypeExpression),
                Some(bray_syntax::SyntaxKind::LambdaExpression)
            ]
        );
    }
}
