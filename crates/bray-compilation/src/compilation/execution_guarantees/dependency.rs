use std::collections::BTreeSet;

use bray_binder::SymbolQueryProvider;

use bray_bound_tree::{BoundCallableTarget, BoundUnitKey};
use bray_checker::{
    ExecutionCallEvidence, ExecutionDependencyTarget, ExecutionObligation, ExecutionProperty,
};
use bray_declarations::SyntaxAnchor;
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    CallableExecutionTarget, CallableInstanceId, GenericSubstitutionId,
    ImplementationRequirementKey, TypeId,
};

use super::imported::{ExecutionProofGraph, ExecutionProofOwner};
use crate::compilation::{Compilation, implementation};
use crate::fact::{CancellationToken, FactQueryError};

pub(super) type CheckedExecutionTarget =
    CallableExecutionTarget<CallableInstanceId, TypeId, ImplementationRequirementKey>;

/// A new invocation must establish entry conditions. A checked proof edge has already done so.
pub(super) enum ExecutionProofEntry<'a> {
    Invocation(Option<&'a ExecutionCallEvidence>),
    Dependency,
}

impl Compilation {
    pub(super) fn execution_dependency_target(
        &self,
        target: ExecutionDependencyTarget,
        substitution: Option<GenericSubstitutionId>,
        cancellation: &CancellationToken,
    ) -> Result<Option<CheckedExecutionTarget>, FactQueryError> {
        let values = self.semantic_value_store()?;

        let map_callable = |callable| {
            let callable = values.intern_callable_instance(callable)?;

            substitution
                .map(|parent| values.substitute_callable_instance(callable, parent))
                .transpose()
                .map(|selected| selected.unwrap_or(callable))
        };

        Ok(match target {
            ExecutionDependencyTarget::Callable(BoundCallableTarget::Declaration(callable)) => {
                Some(CallableExecutionTarget::Callable(map_callable(callable)?))
            }
            ExecutionDependencyTarget::Callable(
                BoundCallableTarget::Indirect(ty) | BoundCallableTarget::TraitView { ty, .. },
            ) => Some(CallableExecutionTarget::Indirect(
                substitution
                    .map(|parent| values.substitute_type(ty, parent))
                    .transpose()?
                    .unwrap_or(ty),
            )),
            ExecutionDependencyTarget::Requirement { callable, dispatch } => {
                let binding = self.binding_context(cancellation)?;

                let requirement = implementation::dispatch_requirement(&binding, dispatch)
                    .map_err(FactQueryError::from)?;

                let requirement = if let Some(parent) = substitution {
                    ImplementationRequirementKey::new(
                        values.substitute_type(requirement.subject(), parent)?,
                        values.substitute_trait_application(
                            requirement.trait_application(),
                            parent,
                        )?,
                    )
                } else {
                    requirement
                };

                Some(CallableExecutionTarget::Requirement {
                    callable: map_callable(callable)?,
                    requirement,
                })
            }
            ExecutionDependencyTarget::Callable(
                BoundCallableTarget::Predicate(_) | BoundCallableTarget::Anonymous(_),
            ) => None,
            ExecutionDependencyTarget::Default(_) => {
                panic!("default proof dependencies are expanded at their owning unit")
            }
        })
    }

    fn requirement_promises(
        &self,
        callable: CallableInstanceId,
        required: ExecutionObligation,
        diagnostics: &mut DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<bool, FactQueryError> {
        let ExecutionObligation::Property(property, None) = required else {
            return Ok(false);
        };

        let callable = *self
            .semantic_value_store()?
            .callable_instance_data(callable);

        let binding = self.binding_context(cancellation)?;

        let contracts = binding
            .resolve_symbol_query(bray_symbols::SymbolQueryRequest::<
                bray_symbols::CallableContractsQuery,
            >::new(callable.definition().callable_symbol()))
            .map_err(crate::compilation::binder::binding_query_error)?;

        diagnostics.add_range(contracts.diagnostics().iter().cloned());

        if contracts.diagnostics().has_errors()
            || !contracts.value().invocation_preconditions().is_empty()
        {
            return Ok(false);
        }

        // Fulfillment checking owns this unconditional trait promise. Generic proof
        // retains the exact requirement, without inferring a new public premise.
        Ok(contracts
            .value()
            .invocation_behavior()
            .execution_properties()
            .contains(&property)
            || self.is_constant_callable(callable, cancellation)?)
    }

    pub(super) fn selected_execution_target(
        &self,
        target: CheckedExecutionTarget,
        required: ExecutionObligation,
        entry: ExecutionProofEntry<'_>,
        permit_premises: bool,
        graph: &mut ExecutionProofGraph,
        assertions: &mut BTreeSet<(SyntaxAnchor, ExecutionProperty)>,
        diagnostics: &mut DiagnosticBag,
        cancellation: &CancellationToken,
    ) -> Result<
        Option<(
            ExecutionProofOwner,
            ExecutionObligation,
            Option<BoundUnitKey>,
        )>,
        FactQueryError,
    > {
        let values = self.semantic_value_store()?;

        let callable = match target {
            CallableExecutionTarget::Callable(callable) => *values.callable_instance_data(callable),
            CallableExecutionTarget::Requirement {
                callable,
                requirement,
            } => {
                let binding = self.binding_context(cancellation)?;

                let selected = implementation::witness_callable(&binding, callable, requirement)
                    .map_err(FactQueryError::from)?;

                match selected {
                    Some((callable, _, _)) => callable,
                    None if (permit_premises && required.is_internal())
                        || self.requirement_promises(
                            callable,
                            required,
                            diagnostics,
                            cancellation,
                        )? =>
                    {
                        let owner = ExecutionProofOwner::Premise(callable, requirement);

                        graph.insert((owner, required), BTreeSet::new());

                        return Ok(Some((owner, required, None)));
                    }
                    None => return Ok(None),
                }
            }
            CallableExecutionTarget::Indirect(ty) => {
                let ExecutionObligation::Property(property, None) = required else {
                    return Ok(None);
                };

                if !matches!(values.type_data(ty).as_ref(), bray_symbols::TypeData::Callable(callable)
                    if callable.phase_behaviors().invocation().execution_properties().contains(&property))
                {
                    return Ok(None);
                }

                let owner = ExecutionProofOwner::Indirect(ty);

                graph.insert((owner, required), BTreeSet::new());

                return Ok(Some((owner, required, None)));
            }
        };

        if self.intrinsic_projection_obligation(callable, required, cancellation)? {
            let owner = match self
                .symbol_graph()?
                .declaration_syntax_anchor(callable.definition().symbol())
            {
                Some(anchor) => ExecutionProofOwner::Source(anchor, None),
                None => ExecutionProofOwner::Imported(callable),
            };

            graph.insert((owner, required), BTreeSet::new());

            return Ok(Some((owner, required, None)));
        }

        self.declared_execution_dependency(
            callable,
            required,
            entry,
            graph,
            assertions,
            diagnostics,
            cancellation,
        )
    }
}
