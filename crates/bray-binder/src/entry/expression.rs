use bray_bound_tree::{
    BoundBlock, BoundBlockId, BoundBlockItem, BoundExpressionId, BoundNodeOrigin, BoundUnitId,
    BoundUnitKey,
};
use bray_symbols::{
    AnySymbolId, CallableSignatureFact, LocalScopeId, PredicateDefinitionSymbolId,
    PredicateSignatureTemplateFact, SymbolFactRequest, SymbolName,
};
use bray_syntax::{
    EnsuresClauseSyntax, ExpressionSyntax, RequiresClauseSyntax, SyntaxKind, WithClauseSyntax,
};

use super::BoundUnitBindingError;
use super::support::{
    anchored_descendant, create_binder, error_type, map_assembly_error, map_binding_error,
    path_context,
};
use crate::binder::{Binder, BinderOutput, BindingContext};
use crate::binding::{ExpressionBinder, callable_normal_completion_has_value, push_contract_scope};
use crate::publication::{
    assemble_constant_template, assemble_constraint, assemble_contract_clause,
    assemble_predicate_definition, assemble_runtime_default, direct_nested_units,
};
use crate::{BinderFactContext, BoundUnitComputation, SymbolFactProvider};

macro_rules! define_pending_expression_unit {
    (
        $pending:ident,
        $bind:ident,
        $assemble:ident,
        $bind_helper:ident,
        $root:ty,
        $context:expr,
        [$($fact_contract:ty),* $(,)?],
        $pending_description:literal,
        $bind_description:literal
    ) => {
        #[doc = $pending_description]
        pub struct $pending {
            output: BinderOutput,
            nested_units: Vec<BoundUnitKey>,
            root: $root,
        }

        impl $pending {
            /// Returns directly nested anonymous callable keys in canonical source order.
            pub fn nested_units(&self) -> &[BoundUnitKey] {
                &self.nested_units
            }

            /// Completes and returns the bound semantic unit.
            pub fn finish(self) -> Result<BoundUnitComputation, BoundUnitBindingError> {
                $assemble(self.output, self.nested_units, self.root).map_err(map_assembly_error)
            }
        }

        #[doc = $bind_description]
        pub fn $bind<C>(
            facts: &C,
            unit: BoundUnitId,
            key: BoundUnitKey,
        ) -> Result<$pending, BoundUnitBindingError>
        where
            C: BinderFactContext + ?Sized,
            $(C::SymbolFacts: SymbolFactProvider<$fact_contract>,)*
        {
            let (output, root) = $bind_helper(facts, unit, key, $context)?;

            let nested_units = direct_nested_units(output.unit().key(), output.dependencies());

            Ok($pending {
                output,
                nested_units,
                root,
            })
        }
    };
}

define_pending_expression_unit!(
    PendingBoundRuntimeDefault,
    bind_runtime_default,
    assemble_runtime_default,
    bind_runtime_default_unit,
    BoundExpressionId,
    BindingContext::Expression,
    [CallableSignatureFact],
    "A bound runtime-default expression ready to complete its semantic unit.",
    "Binds one runtime-default expression into committed task-local state."
);
define_pending_expression_unit!(
    PendingBoundConstantTemplate,
    bind_constant_template,
    assemble_constant_template,
    bind_expression_unit,
    BoundExpressionId,
    BindingContext::ConstantExpression,
    [],
    "A bound constant-template expression ready to complete its semantic unit.",
    "Binds one constant-template expression into committed task-local state."
);
define_pending_expression_unit!(
    PendingBoundPredicateDefinition,
    bind_predicate_definition,
    assemble_predicate_definition,
    bind_predicate_definition_unit,
    BoundExpressionId,
    BindingContext::PredicateExpression,
    [PredicateSignatureTemplateFact],
    "A bound predicate-definition expression ready to complete its semantic unit.",
    "Binds one predicate-definition expression into committed task-local state."
);
define_pending_expression_unit!(
    PendingBoundConstraint,
    bind_constraint,
    assemble_constraint,
    bind_constraint_unit,
    BoundBlockId,
    BindingContext::PredicateExpression,
    [],
    "A bound constraint expression sequence ready to complete its semantic unit.",
    "Binds one constraint expression sequence into committed task-local state."
);
define_pending_expression_unit!(
    PendingBoundContractClause,
    bind_contract_clause,
    assemble_contract_clause,
    bind_contract_clause_unit,
    BoundBlockId,
    BindingContext::ContractClause,
    [CallableSignatureFact],
    "A bound contract-clause expression sequence ready to complete its semantic unit.",
    "Binds one contract-clause expression sequence into committed task-local state."
);

fn bind_constraint_unit<C>(
    facts: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
    context: BindingContext,
) -> Result<(BinderOutput, BoundBlockId), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    bind_expression_sequence_unit(facts, unit, key, context, false)
}

fn bind_contract_clause_unit<C>(
    facts: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
    context: BindingContext,
) -> Result<(BinderOutput, BoundBlockId), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>,
{
    let has_result = if key.source().syntax().syntax_kind() == SyntaxKind::EnsuresClause {
        let owner = facts
            .symbols()
            .symbol_for_key(key.declared_owner())
            .ok_or(BoundUnitBindingError::MissingOwner)?;

        callable_normal_completion_has_value(facts, owner).map_err(map_fact_error)?
    } else {
        false
    };

    bind_expression_sequence_unit(facts, unit, key, context, has_result)
}

fn bind_expression_unit<C>(
    facts: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
    context: BindingContext,
) -> Result<(BinderOutput, BoundExpressionId), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    bind_expression_unit_with_scope(facts, unit, key, context, |_, _| Ok(()))
}

fn bind_runtime_default_unit<C>(
    facts: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
    context: BindingContext,
) -> Result<(BinderOutput, BoundExpressionId), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>,
{
    bind_expression_unit_with_scope(facts, unit, key, context, push_runtime_default_inputs)
}

fn bind_predicate_definition_unit<C>(
    facts: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
    context: BindingContext,
) -> Result<(BinderOutput, BoundExpressionId), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<PredicateSignatureTemplateFact>,
{
    bind_expression_unit_with_scope(facts, unit, key, context, push_predicate_inputs)
}

fn bind_expression_unit_with_scope<C>(
    facts: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
    context: BindingContext,
    configure_scope: impl FnOnce(&mut Binder<'_, C>, LocalScopeId) -> Result<(), BoundUnitBindingError>,
) -> Result<(BinderOutput, BoundExpressionId), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let syntax = anchored_descendant::<_, ExpressionSyntax>(facts, key.source().syntax())
        .ok_or(BoundUnitBindingError::MissingSyntax)?;

    let mut binder = create_binder(facts, unit, key, context)?;

    let root_scope = binder.unit().root_scope();

    configure_scope(&mut binder, root_scope)?;

    let path_context = path_context(&binder, root_scope)?;
    let error_type = error_type(facts)?;

    let mut expression_binder = ExpressionBinder::new(path_context, error_type);

    let root = expression_binder
        .bind_expression(&mut binder, root_scope, Some(&syntax))
        .map_err(map_binding_error)?;

    let output = binder
        .finish()
        .map_err(|_| BoundUnitBindingError::Construction)?;

    Ok((output, root))
}

fn push_runtime_default_inputs<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
) -> Result<(), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<CallableSignatureFact>,
{
    let provider = binder
        .facts()
        .symbols()
        .symbol_for_key(binder.unit().key().declared_owner())
        .ok_or(BoundUnitBindingError::MissingOwner)?;

    let subject = binder
        .facts()
        .symbols()
        .runtime_default_subject(provider)
        .ok_or(BoundUnitBindingError::MissingOwner)?;

    let AnySymbolId::CallableParameter(parameter) = subject else {
        return Ok(());
    };

    let parameter = binder
        .facts()
        .symbols()
        .callable_parameter(parameter)
        .ok_or(BoundUnitBindingError::MissingOwner)?;

    let signature = binder
        .facts()
        .symbol_facts()
        .symbol_fact(SymbolFactRequest::<CallableSignatureFact>::new(
            parameter.owner(),
        ))
        .map_err(map_fact_error)?;

    if let Some(receiver) = signature.value().receiver() {
        insert_named_surface(binder, scope, receiver.parameter().into(), "self")?;
    }

    for parameter in signature
        .value()
        .parameters()
        .iter()
        .take(parameter.ordinal() as usize)
    {
        insert_source_surface(binder, scope, (*parameter).into())?;
    }

    Ok(())
}

fn push_predicate_inputs<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
) -> Result<(), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
    C::SymbolFacts: SymbolFactProvider<PredicateSignatureTemplateFact>,
{
    let owner = binder
        .facts()
        .symbols()
        .symbol_for_key(binder.unit().key().declared_owner())
        .and_then(PredicateDefinitionSymbolId::try_from_any)
        .ok_or(BoundUnitBindingError::MissingOwner)?;

    let signature = binder
        .facts()
        .symbol_facts()
        .symbol_fact(SymbolFactRequest::<PredicateSignatureTemplateFact>::new(
            owner,
        ))
        .map_err(map_fact_error)?;

    for parameter in signature.value().parameters() {
        insert_source_surface(binder, scope, parameter.parameter().into())?;
    }

    Ok(())
}

fn insert_source_surface<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
    symbol: AnySymbolId,
) -> Result<(), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let declaration = binder
        .facts()
        .symbols()
        .symbol_key(symbol)
        .and_then(bray_symbols::SymbolKey::source_declaration_id)
        .and_then(|declaration| binder.facts().declarations().declaration(declaration))
        .ok_or(BoundUnitBindingError::MissingOwner)?;

    let name = declaration
        .name()
        .and_then(bray_declarations::DeclarationName::as_identifier)
        .ok_or(BoundUnitBindingError::MissingOwner)?;

    insert_named_surface(binder, scope, symbol, name)
}

fn insert_named_surface<C>(
    binder: &mut Binder<'_, C>,
    scope: LocalScopeId,
    symbol: AnySymbolId,
    name: &str,
) -> Result<(), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let name = SymbolName::try_new(name).ok_or(BoundUnitBindingError::MissingOwner)?;

    binder
        .unit_mut()
        .insert_surface_name(scope, name, symbol)
        .map_err(|_| BoundUnitBindingError::Construction)
}

fn bind_expression_sequence_unit<C>(
    facts: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
    context: BindingContext,
    has_contract_result: bool,
) -> Result<(BinderOutput, BoundBlockId), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let source = key.source();
    let syntax = source.syntax();

    let expressions =
        anchored_expression_sequence(facts, syntax).ok_or(BoundUnitBindingError::MissingSyntax)?;

    if expressions.is_empty() {
        return Err(BoundUnitBindingError::MissingSyntax);
    }

    let origin = BoundNodeOrigin::source(source);
    let is_recovered = syntax.is_recovered();

    let mut binder = create_binder(facts, unit, key, context)?;

    let root_scope = binder.unit().root_scope();

    let expression_scope = if context == BindingContext::ContractClause {
        push_contract_scope(&mut binder, root_scope, syntax, has_contract_result)
            .map_err(map_binding_error)?
    } else {
        root_scope
    };

    let path_context = path_context(&binder, expression_scope)?;
    let error_type = error_type(facts)?;

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
        .map_err(|_| BoundUnitBindingError::Construction)?;

    let output = binder
        .finish()
        .map_err(|_| BoundUnitBindingError::Construction)?;

    Ok((output, root))
}

const fn map_fact_error(error: crate::BinderFactError) -> BoundUnitBindingError {
    match error {
        crate::BinderFactError::Cancelled => BoundUnitBindingError::Cancelled,
        crate::BinderFactError::DependencyUnavailable => BoundUnitBindingError::Binding,
    }
}

fn anchored_expression_sequence<C>(
    facts: &C,
    anchor: bray_declarations::SyntaxAnchor,
) -> Option<Vec<ExpressionSyntax>>
where
    C: BinderFactContext + ?Sized,
{
    match anchor.syntax_kind() {
        SyntaxKind::RequiresClause => anchored_descendant::<_, RequiresClauseSyntax>(facts, anchor)
            .map(|clause| clause.expressions().collect()),
        SyntaxKind::EnsuresClause => anchored_descendant::<_, EnsuresClauseSyntax>(facts, anchor)
            .map(|clause| clause.expressions().collect()),
        SyntaxKind::WithClause => anchored_descendant::<_, WithClauseSyntax>(facts, anchor)
            .map(|clause| clause.expressions().collect()),
        _ => None,
    }
}
