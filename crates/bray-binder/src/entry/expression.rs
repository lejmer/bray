use bray_bound_tree::{
    BoundBlock, BoundBlockId, BoundBlockItem, BoundExpressionId, BoundNodeOrigin, BoundUnitId,
    BoundUnitKey, BoundUnitRoot,
};
use bray_symbols::{
    AnySymbolId, CallableSignatureQuery, LocalScopeId, PredicateDefinitionSymbolId,
    PredicateSignatureTemplateQuery, SymbolQueryRequest,
};
use bray_syntax::{
    EnsuresClauseSyntax, ExpressionSyntax, RequiresClauseSyntax, SyntaxKind, TypeExpressionSyntax,
    WithClauseSyntax,
};

use super::BoundUnitBindingError;
use super::support::{
    anchored_descendant, create_binder, error_type, insert_callable_inputs, insert_surface,
    map_binding_error, map_query_error, missing_owner, missing_syntax, path_context,
    push_callable_inputs,
};
use crate::binder::{Binder, BinderOutput};
use crate::binding::{ExpressionBinder, callable_normal_completion_has_value, push_contract_scope};
use crate::publication::assemble_bound_unit;
use crate::{BindingQueryContext, BoundUnitComputation, SymbolQueryProvider};

macro_rules! define_expression_unit {
    ($bind:ident, $helper:ident, $root:ident, [$($query_contract:ty),* $(,)?], $description:literal) => {
        #[doc = $description]
        pub fn $bind<C>(
            binding_context: &C,
            unit: BoundUnitId,
            key: BoundUnitKey,
        ) -> Result<BoundUnitComputation, BoundUnitBindingError<C::UpstreamError>>
        where
            C: BindingQueryContext + ?Sized,
            $(C::SymbolSemantics: SymbolQueryProvider<$query_contract>,)*
        {
            let (output, root) = $helper(binding_context, unit, key)?;

            Ok(assemble_bound_unit(output, BoundUnitRoot::$root(root)))
        }
    };
}

define_expression_unit!(
    bind_runtime_default,
    bind_runtime_default_unit,
    Expression,
    [CallableSignatureQuery],
    "Binds one runtime-default expression into committed task-local state."
);
define_expression_unit!(
    bind_constant_template,
    bind_expression_unit,
    Expression,
    [],
    "Binds one constant-template expression into committed task-local state."
);
define_expression_unit!(
    bind_embedded_constant,
    bind_embedded_constant_unit,
    Expression,
    [],
    "Binds one embedded constant expression into committed task-local state."
);
define_expression_unit!(
    bind_predicate_definition,
    bind_predicate_definition_unit,
    Expression,
    [PredicateSignatureTemplateQuery],
    "Binds one predicate-definition expression into committed task-local state."
);
define_expression_unit!(
    bind_constraint,
    bind_constraint_unit,
    ExpressionSequence,
    [],
    "Binds one constraint expression sequence into committed task-local state."
);
define_expression_unit!(
    bind_contract_clause,
    bind_contract_clause_unit,
    ExpressionSequence,
    [CallableSignatureQuery],
    "Binds one contract-clause expression sequence into committed task-local state."
);
define_expression_unit!(
    bind_target_gate,
    bind_expression_unit,
    Expression,
    [],
    "Binds one module target-selection expression into committed task-local state."
);

fn bind_constraint_unit<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<(BinderOutput, BoundBlockId), BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    bind_expression_sequence_unit(binding_context, unit, key, false, false, |_, _| Ok(()))
}

fn bind_contract_clause_unit<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<(BinderOutput, BoundBlockId), BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>,
{
    let has_result = if key.source().syntax().syntax_kind() == SyntaxKind::EnsuresClause {
        let owner = binding_context
            .symbols()
            .symbol_for_key(key.declared_owner())
            .ok_or_else(|| missing_owner(&key, None))?;

        callable_normal_completion_has_value(binding_context, owner).map_err(map_query_error)?
    } else {
        false
    };

    bind_expression_sequence_unit(
        binding_context,
        unit,
        key,
        true,
        has_result,
        |binder, scope| push_callable_inputs(binder, scope).map(|_| ()),
    )
}

fn bind_expression_unit<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<(BinderOutput, BoundExpressionId), BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    bind_expression_unit_with_scope(binding_context, unit, key, |_, _| Ok(()))
}

fn bind_embedded_constant_unit<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<(BinderOutput, BoundExpressionId), BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    if let Some(syntax) =
        anchored_descendant::<_, ExpressionSyntax>(binding_context, key.source().syntax())
    {
        return bind_expression_root_unit(
            binding_context,
            unit,
            key,
            |_, _| Ok(()),
            move |expression_binder, binder, scope| {
                expression_binder.bind_expression(binder, scope, Some(&syntax))
            },
        );
    }

    let syntax =
        anchored_descendant::<_, TypeExpressionSyntax>(binding_context, key.source().syntax())
            .ok_or_else(|| missing_syntax(&key))?;

    bind_expression_root_unit(
        binding_context,
        unit,
        key,
        |_, _| Ok(()),
        move |expression_binder, binder, scope| {
            expression_binder.bind_type_expression_value(binder, scope, &syntax)
        },
    )
}

fn bind_runtime_default_unit<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<(BinderOutput, BoundExpressionId), BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>,
{
    bind_expression_unit_with_scope(binding_context, unit, key, push_runtime_default_inputs)
}

fn bind_predicate_definition_unit<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
) -> Result<(BinderOutput, BoundExpressionId), BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<PredicateSignatureTemplateQuery>,
{
    bind_expression_unit_with_scope(binding_context, unit, key, push_predicate_inputs)
}

fn bind_expression_unit_with_scope<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
    configure_scope: impl FnOnce(
        &mut Binder<'_, C>,
        LocalScopeId,
    ) -> Result<(), BoundUnitBindingError<C::UpstreamError>>,
) -> Result<(BinderOutput, BoundExpressionId), BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    let syntax = anchored_descendant::<_, ExpressionSyntax>(binding_context, key.source().syntax())
        .ok_or_else(|| missing_syntax(&key))?;

    bind_expression_root_unit(
        binding_context,
        unit,
        key,
        configure_scope,
        move |expression_binder, binder, scope| {
            expression_binder.bind_expression(binder, scope, Some(&syntax))
        },
    )
}

fn bind_expression_root_unit<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
    configure_scope: impl FnOnce(
        &mut Binder<'_, C>,
        LocalScopeId,
    ) -> Result<(), BoundUnitBindingError<C::UpstreamError>>,
    bind_root: impl FnOnce(
        &mut ExpressionBinder,
        &mut Binder<'_, C>,
        LocalScopeId,
    )
        -> Result<BoundExpressionId, crate::binding::BindingError<C::UpstreamError>>,
) -> Result<(BinderOutput, BoundExpressionId), BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    let mut binder = create_binder(binding_context, unit, key)?;
    let root_scope = binder.unit().root_scope();

    configure_scope(&mut binder, root_scope)?;

    let path_context = path_context(&binder, root_scope)?;
    let error_type = error_type(binding_context)?;

    let mut expression_binder = match binder.unit().key().kind() {
        bray_bound_tree::BoundUnitKind::EmbeddedConstant => {
            ExpressionBinder::with_unresolved_name_diagnostics(path_context, error_type)
        }
        _ => ExpressionBinder::new(path_context, error_type),
    };

    let root =
        bind_root(&mut expression_binder, &mut binder, root_scope).map_err(map_binding_error)?;

    let output = binder.finish();

    Ok((output, root))
}

fn push_runtime_default_inputs<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
) -> Result<(), BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<CallableSignatureQuery>,
{
    let provider = binder
        .binding_context()
        .symbols()
        .symbol_for_key(binder.unit().key().declared_owner())
        .ok_or_else(|| missing_owner(binder.unit().key(), None))?;

    let subject = binder
        .binding_context()
        .symbols()
        .runtime_default_subject(provider)
        .ok_or_else(|| missing_owner(binder.unit().key(), Some(provider)))?;

    let AnySymbolId::CallableParameter(parameter) = subject else {
        return Ok(());
    };

    let parameter = binder
        .binding_context()
        .symbols()
        .callable_parameter(parameter)
        .ok_or_else(|| missing_owner(binder.unit().key(), Some(subject)))?;

    let signature = binder
        .binding_context()
        .symbol_semantics()
        .resolve_symbol_query(SymbolQueryRequest::<CallableSignatureQuery>::new(
            parameter.owner(),
        ))
        .map_err(map_query_error)?;

    insert_callable_inputs(
        binder,
        scope,
        signature.value(),
        parameter.ordinal() as usize,
    )
}

fn push_predicate_inputs<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
) -> Result<(), BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
    C::SymbolSemantics: SymbolQueryProvider<PredicateSignatureTemplateQuery>,
{
    let owner = binder
        .binding_context()
        .symbols()
        .symbol_for_key(binder.unit().key().declared_owner())
        .and_then(PredicateDefinitionSymbolId::try_from_any)
        .ok_or_else(|| missing_owner(binder.unit().key(), None))?;

    let signature = binder
        .binding_context()
        .symbol_semantics()
        .resolve_symbol_query(SymbolQueryRequest::<PredicateSignatureTemplateQuery>::new(
            owner,
        ))
        .map_err(map_query_error)?;

    for parameter in signature.value().parameters() {
        let symbol = parameter.parameter().into();

        insert_surface(binder, scope, symbol)?;

        if let Some(ty) = parameter.ty().resolved_type() {
            binder.record_value_type(bray_bound_tree::BoundReferenceTarget::Surface(symbol), ty);
        }
    }

    Ok(())
}

fn bind_expression_sequence_unit<C>(
    binding_context: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
    uses_contract_scope: bool,
    has_contract_result: bool,
    configure_scope: impl FnOnce(
        &mut Binder<'_, C>,
        LocalScopeId,
    ) -> Result<(), BoundUnitBindingError<C::UpstreamError>>,
) -> Result<(BinderOutput, BoundBlockId), BoundUnitBindingError<C::UpstreamError>>
where
    C: BindingQueryContext + ?Sized,
{
    let source = key.source();
    let syntax = source.syntax();

    let expressions = anchored_expression_sequence(binding_context, syntax)
        .ok_or_else(|| missing_syntax(&key))?;

    if expressions.is_empty() {
        return Err(missing_syntax(&key));
    }

    let origin = BoundNodeOrigin::source(source);
    let is_recovered = syntax.is_recovered();

    let mut binder = create_binder(binding_context, unit, key)?;
    let root_scope = binder.unit().root_scope();

    configure_scope(&mut binder, root_scope)?;

    let expression_scope = if uses_contract_scope {
        push_contract_scope(&mut binder, root_scope, syntax, has_contract_result)
            .map_err(map_binding_error)?
    } else {
        root_scope
    };

    let path_context = path_context(&binder, expression_scope)?;
    let error_type = error_type(binding_context)?;

    let mut expression_binder = ExpressionBinder::new(path_context, error_type);
    let mut roots = Vec::with_capacity(expressions.len());

    for expression in expressions {
        let root = expression_binder
            .bind_expression(&mut binder, expression_scope, Some(&expression))
            .map_err(map_binding_error)?;

        roots.push(root);
    }

    let is_recovered = is_recovered
        || roots.iter().any(|root| {
            binder
                .unit_view()
                .expression(*root)
                .is_none_or(bray_bound_tree::BoundExpression::is_recovered)
        });

    let block = BoundBlock::new(
        origin,
        roots.iter().copied().map(BoundBlockItem::Expression),
        is_recovered,
    );

    let root = binder
        .unit_mut()
        .tree_mut()
        .push_block(block)
        .map_err(crate::unit::BoundUnitConstructionError::from)
        .map_err(BoundUnitBindingError::Construction)?;

    let output = binder.finish();

    Ok((output, root))
}

fn anchored_expression_sequence<C>(
    binding_context: &C,
    anchor: bray_declarations::SyntaxAnchor,
) -> Option<Vec<ExpressionSyntax>>
where
    C: BindingQueryContext + ?Sized,
{
    match anchor.syntax_kind() {
        SyntaxKind::WhenClause => {
            anchored_descendant::<_, bray_syntax::WhenClauseSyntax>(binding_context, anchor)
                .map(|clause| vec![clause.condition()])
        }
        SyntaxKind::RequiresClause => {
            anchored_descendant::<_, RequiresClauseSyntax>(binding_context, anchor)
                .map(|clause| clause.expressions().collect())
        }
        SyntaxKind::EnsuresClause => {
            anchored_descendant::<_, EnsuresClauseSyntax>(binding_context, anchor)
                .map(|clause| clause.expressions().collect())
        }
        SyntaxKind::WithClause => {
            anchored_descendant::<_, WithClauseSyntax>(binding_context, anchor)
                .map(|clause| clause.expressions().collect())
        }
        _ => None,
    }
}
