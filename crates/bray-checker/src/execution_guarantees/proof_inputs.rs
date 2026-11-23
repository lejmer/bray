use std::collections::{BTreeSet, HashMap};
use std::sync::{Arc, Mutex};

use super::condition::ProofMemo;
use super::{ExecutionAssumptions, ExecutionCondition, trusted_condition_closure};

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
pub(crate) enum ExecutionProofInputDomain {
    Call,
    Postcondition,
    TrustedCall,
}

pub(crate) struct NormalizedProofInputs {
    pub(crate) trusted: ExecutionAssumptions,
    pub(crate) ordinary: ExecutionAssumptions,
}

impl NormalizedProofInputs {
    pub(crate) fn prepare(
        trusted: BTreeSet<(ExecutionCondition, bool)>,
        ordinary: ExecutionAssumptions,
    ) -> Self {
        let trusted = trusted_condition_closure(trusted, &ordinary).into();

        Self { trusted, ordinary }
    }
}

#[derive(Default)]
pub(super) struct NormalizedProofInputCache {
    inputs: Mutex<HashMap<(ExecutionProofInputDomain, [usize; 2]), Arc<NormalizedProofInputs>>>,
}

impl NormalizedProofInputCache {
    pub(super) fn get_or_insert(
        &self,
        domain: ExecutionProofInputDomain,
        sources: &[Arc<ProofMemo>; 2],
        normalize: impl FnOnce() -> NormalizedProofInputs,
    ) -> Arc<NormalizedProofInputs> {
        let key = (domain, sources.each_ref().map(|source| Arc::as_ptr(source).addr()));

        if let Some(inputs) = self
            .inputs
            .lock()
            .expect("normalized proof input cache lock is not poisoned")
            .get(&key)
        {
            return Arc::clone(inputs);
        }

        let normalized = Arc::new(normalize());

        let mut inputs = self
            .inputs
            .lock()
            .expect("normalized proof input cache lock is not poisoned");

        Arc::clone(inputs.entry(key).or_insert(normalized))
    }
}
