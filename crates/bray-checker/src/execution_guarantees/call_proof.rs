use std::borrow::Cow;
use std::collections::BTreeMap;
use std::sync::Arc;

use super::{
    ExecutionAssumptions, ExecutionCallEvidence, ExecutionCondition, ExecutionPlace,
    UnprovenExecutionCondition,
};

/// The reached logical inputs used to select an execution obligation.
#[derive(Default)]
pub struct ExecutionCallProof<'entry> {
    arguments: Cow<'entry, BTreeMap<ExecutionPlace, ExecutionCondition>>,
    assumptions: Cow<'entry, ExecutionAssumptions>,
    trusted_assumptions: Cow<'entry, ExecutionAssumptions>,
    implementation_witnesses: &'entry [bray_symbols::ImplementationInstanceId],
}

impl<'entry> ExecutionCallProof<'entry> {
    pub(super) fn new(entry: &'entry ExecutionCallEvidence) -> Self {
        Self {
            arguments: Cow::Borrowed(&entry.arguments),
            assumptions: Cow::Borrowed(&entry.assumptions),
            trusted_assumptions: Cow::Borrowed(&entry.trusted_assumptions),
            implementation_witnesses: entry.implementation_witnesses(),
        }
    }

    /// The checked implementation choices of this invocation.
    pub fn implementation_witnesses(&self) -> &[bray_symbols::ImplementationInstanceId] {
        self.implementation_witnesses
    }

    /// Maps the logical inputs without remapping the entry's storage transitions.
    pub fn try_map_conditions<E>(
        self,
        mut map: impl FnMut(&ExecutionCondition) -> Result<ExecutionCondition, E>,
    ) -> Result<Self, E> {
        Ok(Self {
            arguments: Cow::Owned(map_arguments(&self.arguments, &mut map)?),
            assumptions: Cow::Owned(map_assumptions(&self.assumptions, &mut map)?),
            trusted_assumptions: Cow::Owned(map_assumptions(&self.trusted_assumptions, &mut map)?),
            implementation_witnesses: self.implementation_witnesses,
        })
    }

    /// Proves every entry condition, retaining the first condition and its actual arguments on failure.
    pub(super) fn prove_requirements<'a>(
        &self,
        conditions: impl IntoIterator<Item = &'a super::ExecutionCondition>,
    ) -> Result<(), UnprovenExecutionCondition> {
        self.prove_from(conditions, &self.assumptions, &[])
    }

    /// Proves every trusted obligation from the captured live evidence.
    pub fn prove_trusted_requirements(&self, conditions: &[super::ExecutionCondition]) -> Result<(), UnprovenExecutionCondition> {
        self.prove_from(conditions, &self.trusted_assumptions, &[])
    }

    /// Checks a declaration's own inputs against captured receiver-first arguments.
    pub fn prove_trusted_with_inputs(
        &self,
        conditions: &[super::ExecutionCondition],
        inputs: &[bray_bound_tree::BoundReferenceTarget],
    ) -> Result<(), UnprovenExecutionCondition> {
        self.prove_from(conditions, &self.trusted_assumptions, inputs)
    }

    fn prove_from<'a>(
        &self,
        conditions: impl IntoIterator<Item = &'a super::ExecutionCondition>,
        assumptions: &std::collections::BTreeSet<(super::ExecutionCondition, bool)>,
        inputs: &[bray_bound_tree::BoundReferenceTarget],
    ) -> Result<(), UnprovenExecutionCondition> {
        let mut conditions = conditions.into_iter().peekable();

        if conditions.peek().is_none() {
            return Ok(());
        }

        let equalities = super::ExecutionCondition::equalities(&self.assumptions, None);

        let known = ExecutionCallEvidence::proof_facts(assumptions)
            .map(|(condition, value)| (condition.with_equalities(&equalities), value))
            .collect();

        let known = super::trusted_condition_closure(known, &self.assumptions);
        let proof = super::ExecutionProof::new(&known, &self.assumptions);

        for required in conditions {
            let condition = required.substitute(
                &|input| {
                    input
                        .value_in(self.arguments.as_ref())
                        .or_else(|| {
                            let ordinal = inputs
                                .iter()
                                .position(|reference| Some(*reference) == input.reference())?;

                            let mut argument =
                                super::ExecutionPlace::argument(bray_symbols::SymbolOrdinal::new(
                                    u32::try_from(ordinal)
                                        .expect("callable parameter ordinal fits"),
                                ));

                            // The captured argument retains this immutable component path.
                            argument.projections = Arc::clone(&input.projections);

                            argument.value_in(self.arguments.as_ref())
                        })
                        .or_else(|| {
                            input
                                .constant_term()
                                .map(|_| super::ExecutionCondition::new(crate::ExecutionConditionKind::Input(input.clone())))
                        })
                        .or_else(|| match input.reference() {
                            Some(bray_bound_tree::BoundReferenceTarget::Surface(
                                bray_symbols::AnySymbolId::GenericConstParameter(_),
                            )) => {
                                // Generic substitution has already mapped this immutable compile-time input into the caller.
                                Some(super::ExecutionCondition::new(crate::ExecutionConditionKind::Input(input.clone())))
                            }
                            _ => None,
                        })
                        .unwrap_or_else(super::ExecutionCondition::unknown)
                },
                &super::ExecutionCondition::unknown(),
                &mut { crate::ExecutionCondition::WORK_LIMIT },
            );

            let condition = condition.with_equalities(&equalities);

            if proof.prove_mixed(&condition, &mut {
                super::ExecutionCondition::WORK_LIMIT
            }) != Some(true) {
                return Err(UnprovenExecutionCondition::new(required.clone(), condition));
            }
        }

        Ok(())
    }
}

pub(super) fn map_arguments<E>(
    arguments: &BTreeMap<ExecutionPlace, ExecutionCondition>,
    map: &mut impl FnMut(&ExecutionCondition) -> Result<ExecutionCondition, E>,
) -> Result<BTreeMap<ExecutionPlace, ExecutionCondition>, E> {
    arguments.iter().map(|(place, value)| Ok((place.clone(), map(value)?))).collect()
}

pub(super) fn map_assumptions<E>(
    assumptions: &ExecutionAssumptions,
    map: &mut impl FnMut(&ExecutionCondition) -> Result<ExecutionCondition, E>,
) -> Result<ExecutionAssumptions, E> {
    assumptions.iter().map(|(condition, holds)| Ok((map(condition)?, *holds))).collect()
}
