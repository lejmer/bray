use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;

use super::CallableExecutionGuarantee;
use crate::SymbolOrdinal;

/// A declaration-level promise that callers can require as implementation evidence.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableContractObligation {
    /// An execution property on its declared entry domain.
    Execution(CallableExecutionGuarantee),
    /// A normal-completion predicate identified within the callable's contract.
    Postcondition(SymbolOrdinal),
}

/// The authority that establishes a callable promise.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CallableEvidenceOrigin {
    /// The provider verified the Bray body and its implementation dependencies.
    CheckedBody,
    /// A validated trusted foreign declaration asserts the promise at its ABI boundary.
    ForeignAssertion,
}

/// A callable promise, its proof authority, and its implementation dependencies.
///
/// Symbol identities use the owning representation: stable references in an interface and resolved
/// callable identities after import. Artifact validation checks these references before use.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CallableContractEvidence<S> {
    origin: CallableEvidenceOrigin,
    obligation: CallableContractObligation,
    dependencies: Arc<[(S, CallableContractObligation)]>,
}

impl<S> CallableContractEvidence<S> {
    /// Records a checked promise with its canonical dependency set.
    pub fn new(
        obligation: CallableContractObligation,
        dependencies: impl IntoIterator<Item = (S, CallableContractObligation)>,
    ) -> Self
    where
        S: Ord,
    {
        Self {
            origin: CallableEvidenceOrigin::CheckedBody,
            obligation,
            dependencies: sorted_unique_shared_slice(dependencies),
        }
    }

    /// Records the assertion of a validated trusted foreign declaration.
    pub fn foreign_assertion(obligation: CallableContractObligation) -> Self {
        Self {
            origin: CallableEvidenceOrigin::ForeignAssertion,
            obligation,
            dependencies: Arc::from([]),
        }
    }

    /// Returns whether this promise comes from a checked body or a foreign assertion.
    pub const fn origin(&self) -> CallableEvidenceOrigin {
        self.origin
    }

    /// Resolves dependency identities while preserving their obligations and proof authority.
    pub fn try_map_symbols<T: Ord, E>(
        &self,
        mut map: impl FnMut(&S) -> Result<T, E>,
    ) -> Result<CallableContractEvidence<T>, E> {
        let dependencies = self
            .dependencies
            .iter()
            .map(|(symbol, obligation)| map(symbol).map(|symbol| (symbol, *obligation)))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(CallableContractEvidence {
            origin: self.origin,
            obligation: self.obligation,
            dependencies: sorted_unique_shared_slice(dependencies),
        })
    }

    /// Returns the promise supplied by this evidence.
    pub const fn obligation(&self) -> CallableContractObligation {
        self.obligation
    }

    /// Returns the exact implementation promises required by this proof.
    pub fn dependencies(&self) -> &[(S, CallableContractObligation)] {
        &self.dependencies
    }
}

#[cfg(test)]
mod tests {
    use super::{CallableContractEvidence, CallableContractObligation};
    use crate::SymbolOrdinal;

    #[test]
    fn evidence_canonicalizes_dependencies_without_losing_the_required_clause() {
        let first = CallableContractObligation::Postcondition(SymbolOrdinal::new(1));
        let second = CallableContractObligation::Postcondition(SymbolOrdinal::new(2));
        let proof = CallableContractEvidence::new(first, [(2, second), (1, first), (2, second)]);

        assert_eq!(proof.obligation(), first);
        assert_eq!(proof.dependencies(), [(1, first), (2, second)]);
    }

    #[test]
    fn symbol_mapping_retains_body_and_foreign_authority_and_failures() {
        let obligation = CallableContractObligation::Postcondition(SymbolOrdinal::new(1));
        let checked = CallableContractEvidence::new(obligation, [(2, obligation)]);

        let mapped = checked
            .try_map_symbols(|symbol| Ok::<_, ()>(symbol + 1))
            .unwrap();

        assert_eq!(mapped.origin(), super::CallableEvidenceOrigin::CheckedBody);
        assert_eq!(mapped.dependencies(), [(3, obligation)]);
        assert_eq!(checked.try_map_symbols(|_| Err::<u32, _>(7)), Err(7));

        let foreign = CallableContractEvidence::<u32>::foreign_assertion(obligation);
        let mapped = foreign.try_map_symbols(|_| Err::<u64, _>(7)).unwrap();

        assert_eq!(
            mapped.origin(),
            super::CallableEvidenceOrigin::ForeignAssertion
        );

        assert_eq!(mapped.obligation(), obligation);
        assert!(mapped.dependencies().is_empty());
    }
}
