use bray_bound_tree::{
    BoundExpressionId, BoundUnitId, BoundUnitKey, ControlFlowCheckedConstantTemplateUnit,
    ControlFlowCheckedConstraintUnit, ControlFlowCheckedContractClauseUnit,
    ControlFlowCheckedPredicateDefinitionUnit, ControlFlowCheckedRuntimeDefaultUnit,
};
use bray_checker::ControlFlowChecker;
use bray_syntax::ExpressionSyntax;

use super::CheckedUnitBindingError;
use super::support::{
    anchored_descendant, error_type, map_assembly_error, map_binding_error, path_context, request,
};
use crate::binding::ExpressionBinder;
use crate::publication::{
    assemble_constant_template, assemble_constraint, assemble_contract_clause,
    assemble_predicate_definition, assemble_runtime_default, direct_nested_units,
};
use crate::request::{BinderRequestResult, BindingContext};
use crate::{BinderCancellation, BinderFactContext, CheckedUnitComputation};

macro_rules! define_pending_expression_unit {
    (
        $pending:ident,
        $bind:ident,
        $result:ty,
        $assemble:ident,
        $context:expr,
        $pending_description:literal,
        $bind_description:literal
    ) => {
        #[doc = $pending_description]
        pub struct $pending {
            request: BinderRequestResult,
            nested_units: Vec<BoundUnitKey>,
            root: BoundExpressionId,
        }

        impl $pending {
            /// Returns directly nested anonymous callable keys in canonical source order.
            pub fn nested_units(&self) -> &[BoundUnitKey] {
                &self.nested_units
            }

            /// Runs control-flow checking and assembles the immutable staged expression unit.
            pub fn finish<C, K>(
                self,
                checker: &C,
                cancellation: &K,
            ) -> Result<CheckedUnitComputation<$result>, CheckedUnitBindingError>
            where
                C: ControlFlowChecker + ?Sized,
                K: BinderCancellation + ?Sized,
            {
                $assemble(
                    self.request,
                    self.nested_units,
                    checker,
                    cancellation,
                    self.root,
                )
                .map_err(map_assembly_error)
            }
        }

        #[doc = $bind_description]
        pub fn $bind<C>(
            facts: &C,
            unit: BoundUnitId,
            key: BoundUnitKey,
        ) -> Result<$pending, CheckedUnitBindingError>
        where
            C: BinderFactContext + ?Sized,
        {
            let (request, root) = bind_expression_unit(facts, unit, key, $context)?;

            let nested_units = direct_nested_units(request.unit().key(), request.dependencies());

            Ok($pending {
                request,
                nested_units,
                root,
            })
        }
    };
}

define_pending_expression_unit!(
    PendingControlFlowRuntimeDefault,
    bind_runtime_default,
    ControlFlowCheckedRuntimeDefaultUnit,
    assemble_runtime_default,
    BindingContext::Expression,
    "A committed runtime-default expression awaiting required nested facts and checking.",
    "Binds one runtime-default expression into committed task-local state."
);
define_pending_expression_unit!(
    PendingControlFlowConstantTemplate,
    bind_constant_template,
    ControlFlowCheckedConstantTemplateUnit,
    assemble_constant_template,
    BindingContext::ConstantExpression,
    "A committed constant-template expression awaiting required nested facts and checking.",
    "Binds one constant-template expression into committed task-local state."
);
define_pending_expression_unit!(
    PendingControlFlowPredicateDefinition,
    bind_predicate_definition,
    ControlFlowCheckedPredicateDefinitionUnit,
    assemble_predicate_definition,
    BindingContext::PredicateExpression,
    "A committed predicate-definition expression awaiting required nested facts and checking.",
    "Binds one predicate-definition expression into committed task-local state."
);
define_pending_expression_unit!(
    PendingControlFlowConstraint,
    bind_constraint,
    ControlFlowCheckedConstraintUnit,
    assemble_constraint,
    BindingContext::PredicateExpression,
    "A committed constraint expression awaiting required nested facts and checking.",
    "Binds one constraint expression into committed task-local state."
);
define_pending_expression_unit!(
    PendingControlFlowContractClause,
    bind_contract_clause,
    ControlFlowCheckedContractClauseUnit,
    assemble_contract_clause,
    BindingContext::ContractClause,
    "A committed contract-clause expression awaiting required nested facts and checking.",
    "Binds one contract-clause expression into committed task-local state."
);

fn bind_expression_unit<C>(
    facts: &C,
    unit: BoundUnitId,
    key: BoundUnitKey,
    context: BindingContext,
) -> Result<(BinderRequestResult, BoundExpressionId), CheckedUnitBindingError>
where
    C: BinderFactContext + ?Sized,
{
    let syntax = anchored_descendant::<_, ExpressionSyntax>(facts, key.source().syntax())
        .ok_or(CheckedUnitBindingError::MissingSyntax)?;

    let mut request = request(facts, unit, key, context)?;
    let root_scope = request.unit().root_scope();
    let path_context = path_context(&request, root_scope)?;
    let error_type = error_type(facts)?;
    let mut binder = ExpressionBinder::new(path_context, error_type);

    let root = binder
        .bind_expression(&mut request, root_scope, Some(&syntax))
        .map_err(map_binding_error)?;

    let request = request
        .finish()
        .map_err(|_| CheckedUnitBindingError::Construction)?;

    Ok((request, root))
}
