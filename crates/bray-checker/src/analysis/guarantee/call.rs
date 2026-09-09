use bray_bound_tree::{
    BoundExpressionId, CallableProofDependency, CallableProofObligation, CallableProofTarget,
};
use bray_symbols::{
    CallableContractClause, ConstantTermData, ConstantTermId, ExecutionProperty,
    SemanticValueStoreError,
};

use crate::contract::{
    MAX_CONDITION_STEPS, applicable_postconditions, instantiate_condition,
    instantiate_condition_with_budget, preconditions_hold,
};
use crate::guarantee::execution_property_coverage;

use super::flow::{DomainState, GuaranteeDomain};
use crate::constant::shape::storage_observation_subject;

/// Entry observations fix guard activation independently of the selected body's later effects.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct CallCompletion {
    arguments: Vec<ConstantTermId>,
    clauses: Vec<CallableContractClause>,
    pure_dependencies: Option<Vec<CallableProofDependency>>,
}

impl GuaranteeDomain<'_> {
    pub(super) fn call_dependencies(
        &self,
        expression: BoundExpressionId,
        property: ExecutionProperty,
        state: &DomainState,
    ) -> Result<Option<Vec<CallableProofDependency>>, SemanticValueStoreError> {
        if self.input.is_inert_call(expression) {
            return Ok(Some(Vec::new()));
        }

        let Some(call) = self.input.call(expression) else {
            return Ok(None);
        };

        let Some(arguments) = self.call_arguments(expression, state)? else {
            return Ok(None);
        };

        let assumptions = state.conditions.iter().copied().collect::<Vec<_>>();

        if !preconditions_hold(self.values, call.conditions(), &arguments, &assumptions)? {
            return Ok(None);
        }

        let mut remaining = MAX_CONDITION_STEPS;

        let coverage = execution_property_coverage(
            self.values,
            call.conditions(),
            property,
            &assumptions,
            &mut |condition| instantiate_condition(self.values, condition, &arguments),
            &mut remaining,
        )?;

        Ok(coverage.map(|guarantees| {
            guarantees
                .into_iter()
                .map(|guarantee| {
                    CallableProofDependency::new(
                        CallableProofTarget::Call(expression),
                        CallableProofObligation::Execution(guarantee),
                    )
                })
                .collect()
        }))
    }

    fn call_arguments(
        &self,
        expression: BoundExpressionId,
        state: &DomainState,
    ) -> Result<Option<Vec<ConstantTermId>>, SemanticValueStoreError> {
        let Some(call) = self
            .input
            .call(expression)
            .filter(|call| call.arguments().len() < MAX_CONDITION_STEPS)
        else {
            return Ok(None);
        };

        let mut arguments = Vec::new();

        for (index, argument) in call.arguments().iter().enumerate() {
            let observation = match argument {
                crate::ExecutionCallArgument::Expression(argument) => {
                    self.current_expression(state, *argument)?
                }
                crate::ExecutionCallArgument::Constant(term) => Some(*term),
            };

            let term = match observation {
                Some(term) => term,
                None => {
                    let Some(term) = self.fresh_call_observation(expression, index)? else {
                        return Ok(None);
                    };

                    term
                }
            };

            arguments.push(term);
        }

        Ok(Some(arguments))
    }

    pub(super) fn prepare_call_completion(
        &self,
        expression: BoundExpressionId,
        state: &DomainState,
    ) -> Result<Option<CallCompletion>, SemanticValueStoreError> {
        let Some(call) = self.input.call(expression) else {
            return Ok(None);
        };

        let Some(arguments) = self.call_arguments(expression, state)? else {
            return Ok(None);
        };

        let assumptions = state.conditions.iter().copied().collect::<Vec<_>>();

        let mut remaining = MAX_CONDITION_STEPS;

        // Normal completion establishes checked postconditions even when entry requirements
        // needed runtime checks. Execution guarantees still require proof before invocation.
        let clauses = applicable_postconditions(
            self.values,
            call.conditions(),
            &arguments,
            &assumptions,
            &mut remaining,
        )?;

        let pure_dependencies =
            self.call_dependencies(expression, ExecutionProperty::Pure, state)?;

        Ok(Some(CallCompletion {
            arguments,
            clauses,
            pure_dependencies,
        }))
    }

    pub(super) fn complete_call(
        &self,
        expression: BoundExpressionId,
        state: &mut DomainState,
    ) -> Result<(), SemanticValueStoreError> {
        let Some(call) = self.input.call(expression) else {
            return Ok(());
        };

        let Some(completion) = state.call_completions.remove(&expression) else {
            return Ok(());
        };

        let mut arguments = completion.arguments;
        let count = arguments.len();

        if completion.pure_dependencies.is_none() {
            for (index, argument) in arguments.iter_mut().enumerate() {
                let Some(current) = self.fresh_call_observation(expression, count + index)? else {
                    return Ok(());
                };

                *argument = current;
            }
        }

        let mut remaining = MAX_CONDITION_STEPS;

        for (source, current) in call.arguments().iter().zip(&arguments) {
            let crate::ExecutionCallArgument::Expression(source) = source else {
                continue;
            };

            if call.borrows_argument(*source)
                && let Some(subject) = self.input.expression(*source)
                && let Some(subject) =
                    storage_observation_subject(self.values, subject, &mut remaining)?
            {
                state.observations.insert(subject, *current);
            }
        }

        let Some(returned) = call.result_observation(self.values)? else {
            return Ok(());
        };

        arguments.push(returned);

        for clause in completion.clauses {
            let Some(condition) = clause
                .predicate()
                .and_then(|predicate| predicate.condition())
            else {
                continue;
            };

            let Some(condition) = instantiate_condition_with_budget(
                self.values,
                condition,
                &arguments,
                &mut remaining,
            )?
            else {
                continue;
            };

            if state.conditions.len() < MAX_CONDITION_STEPS {
                state.conditions.insert((condition, true));

                state.dependencies.insert(CallableProofDependency::new(
                    CallableProofTarget::Call(expression),
                    CallableProofObligation::Postcondition(clause.ordinal()),
                ));
            }
        }

        state
            .dependencies
            .extend(completion.pure_dependencies.into_iter().flatten());

        state.evaluated_values.insert(expression, returned);
        state.observations.insert(returned, returned);

        Ok(())
    }

    fn fresh_call_observation(
        &self,
        expression: BoundExpressionId,
        index: usize,
    ) -> Result<Option<ConstantTermId>, SemanticValueStoreError> {
        let Some(ordinal) = self
            .input
            .call(expression)
            .and_then(|call| call.observation_ordinal(index))
        else {
            return Ok(None);
        };

        self.values
            .intern_constant_term(ConstantTermData::CallableArgument(ordinal))
            .map(Some)
    }
}
