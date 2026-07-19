use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_symbols::{
    CallableAbi, CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId,
    ImplementationInstanceId, ImplementationSelectionKey,
};

use crate::{BoundCallableTarget, BoundExpressionId, BoundResolvedCall};

/// One exact implementation requirement and its selected witness.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SelectedImplementationWitness {
    requirement: ImplementationSelectionKey,
    witness: ImplementationInstanceId,
}

impl SelectedImplementationWitness {
    /// Creates an exact requirement-to-witness association.
    pub const fn new(
        requirement: ImplementationSelectionKey,
        witness: ImplementationInstanceId,
    ) -> Self {
        Self {
            requirement,
            witness,
        }
    }

    /// Returns the implementation requirement being satisfied.
    pub const fn requirement(self) -> ImplementationSelectionKey {
        self.requirement
    }

    /// Returns the selected implementation witness.
    pub const fn witness(self) -> ImplementationInstanceId {
        self.witness
    }
}

/// One explicit or defaulted value in call evaluation order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SelectedArgument {
    /// A source argument mapped to its exact parameter.
    Explicit {
        /// The argument expression occurrence.
        expression: BoundExpressionId,
        /// The exact selected parameter.
        parameter: CallableParameterSymbolId,
    },
    /// An omitted parameter supplied by its declaration-owned default provider.
    Default {
        /// The exact omitted parameter.
        parameter: CallableParameterSymbolId,
        /// The declaration-owned default provider evaluated by the call.
        provider: CallableParameterDefaultProviderSymbolId,
    },
}

/// One exact callable, ABI, implementation witnesses, and normalized argument mapping.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedCall {
    resolution: BoundResolvedCall,
    abi: CallableAbi,
    arguments: Arc<[SelectedArgument]>,
    witnesses: Arc<[SelectedImplementationWitness]>,
}

impl SelectedCall {
    /// Creates a complete callable selection.
    pub fn new(
        resolution: BoundResolvedCall,
        abi: CallableAbi,
        arguments: impl IntoIterator<Item = SelectedArgument>,
        witnesses: impl IntoIterator<Item = SelectedImplementationWitness>,
    ) -> Self {
        Self {
            resolution,
            abi,
            arguments: shared_slice(arguments),
            witnesses: sorted_unique_shared_slice(witnesses),
        }
    }

    /// Returns the exact selected callable target and result behavior.
    pub const fn resolution(&self) -> &BoundResolvedCall {
        &self.resolution
    }

    /// Returns the callable ABI participating in the selected call contract.
    pub const fn abi(&self) -> CallableAbi {
        self.abi
    }

    /// Returns explicit arguments in source order followed by defaults in parameter order.
    pub fn arguments(&self) -> &[SelectedArgument] {
        &self.arguments
    }

    /// Returns exact implementation requirements and witnesses in canonical order.
    pub fn witnesses(&self) -> &[SelectedImplementationWitness] {
        &self.witnesses
    }

    /// Returns the exact declared, anonymous, or indirect call target.
    pub const fn target(&self) -> BoundCallableTarget {
        self.resolution.target()
    }
}
