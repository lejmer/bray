use std::sync::Arc;

use bray_base::shared_slice;

use super::{CallableContractClause, CallableContractClauseKind, CallableExecutionGuarantee};
use crate::SymbolOrdinal;

/// Clause metadata shared by checked source and imported callable contracts.
pub trait CallableConditionClause: Copy {
    /// Returns the clause's semantic role.
    fn kind(&self) -> CallableContractClauseKind;

    /// Returns the enclosing execution-entry guard, if any.
    fn guard(&self) -> Option<SymbolOrdinal>;
}

impl CallableConditionClause for CallableContractClause {
    fn kind(&self) -> CallableContractClauseKind {
        (*self).kind()
    }

    fn guard(&self) -> Option<SymbolOrdinal> {
        (*self).guard()
    }
}

/// Declaration conditions and execution promises, independent of body behavior.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableConditionSet<C = CallableContractClause> {
    invocation_preconditions: Arc<[C]>,
    static_constraints: Arc<[C]>,
    normal_completion_postconditions: Arc<[C]>,
    entry_guards: Arc<[C]>,
    guarded_postconditions: Arc<[C]>,
    execution_guarantees: Arc<[CallableExecutionGuarantee]>,
}

impl<C: CallableConditionClause> CallableConditionSet<C> {
    /// Groups declaration clauses by their semantic role.
    pub fn new(clauses: impl IntoIterator<Item = C>) -> Self {
        let mut invocation_preconditions = Vec::new();
        let mut static_constraints = Vec::new();
        let mut normal_completion_postconditions = Vec::new();
        let mut entry_guards = Vec::new();
        let mut guarded_postconditions = Vec::new();

        for clause in clauses {
            match clause.kind() {
                CallableContractClauseKind::Requires => invocation_preconditions.push(clause),
                CallableContractClauseKind::Ensures if clause.guard().is_some() => {
                    guarded_postconditions.push(clause);
                }
                CallableContractClauseKind::Ensures => {
                    normal_completion_postconditions.push(clause);
                }
                CallableContractClauseKind::Guard => entry_guards.push(clause),
                CallableContractClauseKind::Static => static_constraints.push(clause),
            }
        }

        Self {
            invocation_preconditions: shared_slice(invocation_preconditions),
            static_constraints: shared_slice(static_constraints),
            normal_completion_postconditions: shared_slice(normal_completion_postconditions),
            entry_guards: shared_slice(entry_guards),
            guarded_postconditions: shared_slice(guarded_postconditions),
            execution_guarantees: shared_slice([]),
        }
    }
}

/// Shared condition operations for source and imported callable contracts.
pub trait CallableConditions {
    /// The contract's source or interface clause representation.
    type Clause: CallableConditionClause;

    /// Returns the grouped declaration conditions.
    fn conditions(&self) -> &CallableConditionSet<Self::Clause>;

    /// Returns the grouped conditions for deliberate contract replacement.
    fn conditions_mut(&mut self) -> &mut CallableConditionSet<Self::Clause>;

    /// Replaces the declaration conditions while retaining the callable's other contracts.
    fn with_conditions(mut self, conditions: CallableConditionSet<Self::Clause>) -> Self
    where
        Self: Sized,
    {
        *self.conditions_mut() = conditions;

        self
    }

    /// Returns all clauses grouped by semantic role, retaining guard references.
    fn clauses(&self) -> impl Iterator<Item = &Self::Clause> {
        let conditions = self.conditions();

        conditions
            .invocation_preconditions
            .iter()
            .chain(conditions.static_constraints.iter())
            .chain(conditions.normal_completion_postconditions.iter())
            .chain(conditions.entry_guards.iter())
            .chain(conditions.guarded_postconditions.iter())
    }

    /// Replaces clauses after every conversion succeeds, preserving execution promises.
    ///
    /// Converted clauses are regrouped by role. The converter must preserve guard ordinals
    /// referenced by execution promises or update those promises before publishing the contract.
    fn try_map_clauses<E>(
        &mut self,
        map: impl FnMut(Self::Clause) -> Result<Self::Clause, E>,
    ) -> Result<(), E> {
        *self.conditions_mut() = self.try_convert_conditions(map)?;

        Ok(())
    }

    /// Converts clause identities while retaining their grouped roles and execution promises.
    fn try_convert_conditions<C: CallableConditionClause, E>(
        &self,
        map: impl FnMut(Self::Clause) -> Result<C, E>,
    ) -> Result<CallableConditionSet<C>, E> {
        let clauses = self
            .clauses()
            .copied()
            .map(map)
            .collect::<Result<Vec<_>, _>>()?;

        let mut replacement = CallableConditionSet::new(clauses);

        replacement.execution_guarantees = Arc::clone(&self.conditions().execution_guarantees);

        Ok(replacement)
    }

    /// Attaches declared execution promises without certifying their implementations.
    fn with_execution_guarantees(
        mut self,
        guarantees: impl IntoIterator<Item = CallableExecutionGuarantee>,
    ) -> Self
    where
        Self: Sized,
    {
        self.conditions_mut().execution_guarantees =
            bray_base::sorted_unique_shared_slice(guarantees);

        self
    }

    /// Returns declared promises whose execution proofs must be validated separately.
    fn execution_guarantees(&self) -> &[CallableExecutionGuarantee] {
        &self.conditions().execution_guarantees
    }

    /// Returns execution-entry predicates in declaration order, including nested guard ancestry.
    fn entry_guards(&self) -> &[Self::Clause] {
        &self.conditions().entry_guards
    }

    /// Returns postconditions available only when their execution-entry guards held.
    fn guarded_postconditions(&self) -> &[Self::Clause] {
        &self.conditions().guarded_postconditions
    }

    /// Returns preconditions checked before callable invocation.
    fn invocation_preconditions(&self) -> &[Self::Clause] {
        &self.conditions().invocation_preconditions
    }

    /// Returns constraints checked while selecting or instantiating the callable.
    fn static_constraints(&self) -> &[Self::Clause] {
        &self.conditions().static_constraints
    }

    /// Returns postconditions established exclusively after normal completion.
    fn normal_completion_postconditions(&self) -> &[Self::Clause] {
        &self.conditions().normal_completion_postconditions
    }
}

impl<C: CallableConditionClause> CallableConditions for CallableConditionSet<C> {
    type Clause = C;

    fn conditions(&self) -> &Self {
        self
    }

    fn conditions_mut(&mut self) -> &mut Self {
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{CallableConditionSet, CallableConditions};
    use crate::{
        CallableContractClause, CallableContractClauseKind, CallableExecutionGuarantee,
        ExecutionProperty, PredicateSemanticSummary, SemanticValueStore, SymbolOrdinal,
    };

    #[test]
    fn condition_defaults_group_clauses_and_replace_them_atomically() {
        let values = SemanticValueStore::try_new().unwrap();

        let predicate =
            PredicateSemanticSummary::new(values.empty_dependency_contract_template().unwrap());

        let clause = |ordinal, kind, guard: Option<u32>| {
            CallableContractClause::new(SymbolOrdinal::new(ordinal), kind, predicate)
                .with_guard(guard.map(SymbolOrdinal::new))
        };

        let precondition = clause(0, CallableContractClauseKind::Requires, None);
        let constraint = clause(1, CallableContractClauseKind::Static, None);
        let postcondition = clause(2, CallableContractClauseKind::Ensures, None);
        let guard = clause(3, CallableContractClauseKind::Guard, None);
        let guarded = clause(4, CallableContractClauseKind::Ensures, Some(3));

        let promise =
            CallableExecutionGuarantee::new(ExecutionProperty::Pure, Some(guard.ordinal()));

        let mut conditions =
            CallableConditionSet::new([guard, guarded, precondition, postcondition, constraint])
                .with_execution_guarantees([promise, promise]);

        assert_eq!(conditions.invocation_preconditions(), [precondition]);
        assert_eq!(conditions.static_constraints(), [constraint]);

        assert_eq!(
            conditions.normal_completion_postconditions(),
            [postcondition]
        );

        assert_eq!(conditions.entry_guards(), [guard]);
        assert_eq!(conditions.guarded_postconditions(), [guarded]);
        assert_eq!(conditions.execution_guarantees(), [promise]);

        assert_eq!(
            conditions.clauses().copied().collect::<Vec<_>>(),
            [precondition, constraint, postcondition, guard, guarded],
        );

        let original = conditions.clone();

        let failed = conditions.try_map_clauses(|clause| {
            if clause == guarded {
                Err("conversion failed")
            } else {
                Ok(clause.with_guard(None))
            }
        });

        assert_eq!(failed, Err("conversion failed"));
        assert_eq!(conditions, original);

        conditions
            .try_map_clauses::<std::convert::Infallible>(|clause| Ok(clause.with_guard(None)))
            .unwrap();

        assert_eq!(conditions.guarded_postconditions(), []);

        assert_eq!(
            conditions.normal_completion_postconditions(),
            [postcondition, guarded.with_guard(None)],
        );

        assert_eq!(conditions.entry_guards(), [guard]);
        assert_eq!(conditions.execution_guarantees(), [promise]);
    }
}
