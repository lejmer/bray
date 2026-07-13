use bray_bound_tree::{
    BoundBlock, BoundBlockId, BoundBlockItem, BoundExpressionId, BoundNodeOrigin, BoundUnitId,
    BoundUnitKey,
};
use bray_syntax::{
    EnsuresClauseSyntax, ExpressionSyntax, RequiresClauseSyntax, SyntaxKind, WithClauseSyntax,
};

use super::BoundUnitBindingError;
use super::support::{
    anchored_descendant, create_binder, error_type, map_assembly_error, map_binding_error,
    path_context,
};
use crate::binder::{BinderOutput, BindingContext};
use crate::binding::ExpressionBinder;
use crate::publication::{
    assemble_constant_template, assemble_constraint, assemble_contract_clause,
    assemble_predicate_definition, assemble_runtime_default, direct_nested_units,
};
use crate::{BinderFactContext, BoundUnitComputation};

macro_rules! define_pending_expression_unit {
    (
        $pending:ident,
        $bind:ident,
        $assemble:ident,
        $bind_helper:ident,
        $root:ty,
        $context:expr,
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

            /// Freezes the immutable bound semantic unit for publication.
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
    bind_expression_unit,
    BoundExpressionId,
    BindingContext::Expression,
    "A committed runtime-default expression awaiting bound-unit publication.",
    "Binds one runtime-default expression into committed task-local state."
);
define_pending_expression_unit!(
    PendingBoundConstantTemplate,
    bind_constant_template,
    assemble_constant_template,
    bind_expression_unit,
    BoundExpressionId,
    BindingContext::ConstantExpression,
    "A committed constant-template expression awaiting bound-unit publication.",
    "Binds one constant-template expression into committed task-local state."
);
define_pending_expression_unit!(
    PendingBoundPredicateDefinition,
    bind_predicate_definition,
    assemble_predicate_definition,
    bind_expression_unit,
    BoundExpressionId,
    BindingContext::PredicateExpression,
    "A committed predicate-definition expression awaiting bound-unit publication.",
    "Binds one predicate-definition expression into committed task-local state."
);
define_pending_expression_unit!(
    PendingBoundConstraint,
    bind_constraint,
    assemble_constraint,
    bind_expression_sequence_unit,
    BoundBlockId,
    BindingContext::PredicateExpression,
    "A committed constraint expression sequence awaiting bound-unit publication.",
    "Binds one constraint expression sequence into committed task-local state."
);
define_pending_expression_unit!(
    PendingBoundContractClause,
    bind_contract_clause,
    assemble_contract_clause,
    bind_expression_sequence_unit,
    BoundBlockId,
    BindingContext::ContractClause,
    "A committed contract-clause expression sequence awaiting bound-unit publication.",
    "Binds one contract-clause expression sequence into committed task-local state."
);

fn bind_expression_unit<C>(
    facts: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
    context: BindingContext,
) -> Result<(BinderOutput, BoundExpressionId), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let syntax = anchored_descendant::<_, ExpressionSyntax>(facts, key.source().syntax())
        .ok_or(BoundUnitBindingError::MissingSyntax)?;

    let mut binder = create_binder(facts, unit, key, context)?;
    let root_scope = binder.unit().root_scope();
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

fn bind_expression_sequence_unit<C>(
    facts: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
    context: BindingContext,
) -> Result<(BinderOutput, BoundBlockId), BoundUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let expressions = anchored_expression_sequence(facts, key.source().syntax())
        .ok_or(BoundUnitBindingError::MissingSyntax)?;

    if expressions.is_empty() {
        return Err(BoundUnitBindingError::MissingSyntax);
    }

    let origin = BoundNodeOrigin::source(key.source());
    let is_recovered = key.source().syntax().is_recovered();

    let mut binder = create_binder(facts, unit, key, context)?;

    let root_scope = binder.unit().root_scope();
    let path_context = path_context(&binder, root_scope)?;
    let error_type = error_type(facts)?;

    let mut expression_binder = ExpressionBinder::new(path_context, error_type);
    let mut roots = Vec::with_capacity(expressions.len());

    for expression in expressions {
        let root = expression_binder
            .bind_expression(&mut binder, root_scope, Some(&expression))
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
