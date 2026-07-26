use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_symbols::{
    CallableAbi, CallableContractTemplate, CallableParameterDefaultProviderSymbolId,
    CallableParameterSymbolId, ImplementationInstanceId, ImplementationRequirementKey,
    ReceiverParameterSymbolId,
};

use crate::{BoundCallableTarget, BoundExpressionId, BoundResolvedCall, SelectedConversion};

/// The checked receiver passed to one selected callable.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SelectedReceiver {
    expression: BoundExpressionId,
    parameter: ReceiverParameterSymbolId,
    conversion: SelectedConversion,
}

impl SelectedReceiver {
    /// Creates one receiver-to-parameter mapping.
    pub const fn new(
        expression: BoundExpressionId,
        parameter: ReceiverParameterSymbolId,
        conversion: SelectedConversion,
    ) -> Self {
        Self {
            expression,
            parameter,
            conversion,
        }
    }

    /// Returns the receiver expression occurrence.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the selected callable's receiver parameter.
    pub const fn parameter(&self) -> ReceiverParameterSymbolId {
        self.parameter
    }

    /// Returns the checked receiver conversion.
    pub const fn conversion(&self) -> &SelectedConversion {
        &self.conversion
    }
}

/// One exact implementation requirement and its selected witness.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SelectedImplementationWitness {
    requirement: ImplementationRequirementKey,
    witness: ImplementationInstanceId,
}

impl SelectedImplementationWitness {
    /// Creates an exact requirement-to-witness association.
    pub const fn new(
        requirement: ImplementationRequirementKey,
        witness: ImplementationInstanceId,
    ) -> Self {
        Self {
            requirement,
            witness,
        }
    }

    /// Returns the implementation requirement being satisfied.
    pub const fn requirement(self) -> ImplementationRequirementKey {
        self.requirement
    }

    /// Returns the selected implementation witness.
    pub const fn witness(self) -> ImplementationInstanceId {
        self.witness
    }
}

/// One explicit or defaulted value in call evaluation order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum SelectedArgument {
    /// A source argument mapped to its exact parameter.
    Explicit {
        /// The argument expression occurrence.
        expression: BoundExpressionId,
        /// The exact declaration parameter, when the target is declaration-backed.
        parameter: Option<CallableParameterSymbolId>,
        /// The selected parameter's declaration-order ordinal.
        ordinal: u32,
        /// The checked conversion into the parameter type.
        conversion: SelectedConversion,
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
    contract: Option<Arc<CallableContractTemplate>>,
    receiver: Option<SelectedReceiver>,
    arguments: Arc<[SelectedArgument]>,
    witnesses: Arc<[SelectedImplementationWitness]>,
}

impl SelectedCall {
    /// Creates a complete callable selection.
    pub fn new(
        resolution: BoundResolvedCall,
        abi: CallableAbi,
        receiver: Option<SelectedReceiver>,
        arguments: impl IntoIterator<Item = SelectedArgument>,
        witnesses: impl IntoIterator<Item = SelectedImplementationWitness>,
    ) -> Self {
        Self {
            resolution,
            abi,
            contract: None,
            receiver,
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

    /// Returns the checked receiver mapping for an instance call.
    pub const fn receiver(&self) -> Option<&SelectedReceiver> {
        self.receiver.as_ref()
    }

    /// Retains the selected declaration's source or imported contract clauses.
    pub fn with_contract(mut self, contract: Option<CallableContractTemplate>) -> Self {
        self.contract = contract.map(Arc::new);

        self
    }

    /// Returns contract clauses for a declaration-backed call.
    pub fn contract(&self) -> Option<&CallableContractTemplate> {
        match &self.contract {
            Some(contract) => Some(contract.as_ref()),
            None => None,
        }
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
