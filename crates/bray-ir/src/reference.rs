use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeAbiVersion};
use bray_symbols::{
    CallableAbi, CallableContractTemplate, CallableInstanceData,
    CallableParameterDefaultProviderSymbolId, CallableParameterSymbolId, CallablePhaseBehaviors,
    ImplementationInstanceId, ReceiverParameterSymbolId, StructFieldSymbolId,
    UnionPayloadFieldSymbolId,
};

use bray_bound_tree::{BoundCallResult, SelectedImplementationWitness};

use crate::MirOperand;

/// Exact declared field selected by a MIR projection.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirFieldReference {
    /// A field declared by a struct.
    Struct(StructFieldSymbolId),
    /// A field in one union variant payload.
    UnionPayload(UnionPayloadFieldSymbolId),
}

/// One exact substituted callable selected before MIR construction.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MirCallableReference {
    instance: CallableInstanceData,
    abi: CallableAbi,
}

impl MirCallableReference {
    /// Creates an exact callable reference and its checked calling convention.
    pub const fn new(instance: CallableInstanceData, abi: CallableAbi) -> Self {
        Self { instance, abi }
    }

    /// Returns the selected callable instance.
    pub const fn instance(self) -> CallableInstanceData {
        self.instance
    }

    /// Returns the selected calling convention.
    pub const fn abi(self) -> CallableAbi {
        self.abi
    }
}

/// One exact private runtime ABI requirement selected during lowering.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MirRuntimeReference {
    role: RuntimeAbiRole,
    abi_version: RuntimeAbiVersion,
}

impl MirRuntimeReference {
    /// Creates a runtime reference from its closed role and ABI version.
    pub const fn new(role: RuntimeAbiRole, abi_version: RuntimeAbiVersion) -> Self {
        Self { role, abi_version }
    }

    /// Returns the private runtime ABI role.
    pub const fn role(self) -> RuntimeAbiRole {
        self.role
    }

    /// Returns the required private runtime ABI version.
    pub const fn abi_version(self) -> RuntimeAbiVersion {
        self.abi_version
    }
}

/// Exact callable mechanism selected for one MIR call.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirCallTarget {
    /// A concrete Bray callable instance.
    Direct(MirCallableReference),
    /// A checked callable value.
    Indirect {
        /// Evaluated callable value.
        callee: MirOperand,
        /// Calling convention carried by the callable type.
        abi: CallableAbi,
    },
}

impl MirCallTarget {
    /// Returns the checked calling convention used by this target.
    pub const fn abi(&self) -> CallableAbi {
        match self {
            Self::Direct(reference) => reference.abi(),
            Self::Indirect { abi, .. } => *abi,
        }
    }
}

/// One checked receiver, explicit value, or declaration-owned runtime default.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirCallArgument {
    /// An evaluated instance receiver.
    Receiver {
        /// Exact receiver parameter.
        parameter: ReceiverParameterSymbolId,
        /// Evaluated and converted receiver value.
        value: MirOperand,
    },
    /// An evaluated source argument.
    Explicit {
        /// Exact declaration parameter when the call is declaration-backed.
        parameter: Option<CallableParameterSymbolId>,
        /// Declaration-order parameter position.
        ordinal: u32,
        /// Evaluated and converted argument value.
        value: MirOperand,
    },
    /// An omitted parameter supplied by its declaration-owned runtime default.
    Default {
        /// Exact omitted parameter.
        parameter: CallableParameterSymbolId,
        /// Declaration-order parameter position.
        ordinal: u32,
        /// Exact runtime default provider.
        provider: CallableParameterDefaultProviderSymbolId,
    },
}

impl MirCallArgument {
    /// Creates one positional argument for a compiler-selected call.
    pub const fn positional(ordinal: u32, value: MirOperand) -> Self {
        Self::Explicit {
            parameter: None,
            ordinal,
            value,
        }
    }

    /// Returns the evaluated operand when this input is supplied directly.
    pub const fn value(&self) -> Option<&MirOperand> {
        match self {
            Self::Receiver { value, .. } | Self::Explicit { value, .. } => Some(value),
            Self::Default { .. } => None,
        }
    }
}

/// One explicit call with ordered inputs and retained checked behavior.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirCall {
    target: MirCallTarget,
    result: BoundCallResult,
    arguments: Arc<[MirCallArgument]>,
    phase_behaviors: Option<Arc<CallablePhaseBehaviors>>,
    contract: Option<Arc<CallableContractTemplate>>,
    dispatch_witnesses: Arc<[ImplementationInstanceId]>,
    witnesses: Arc<[SelectedImplementationWitness]>,
}

impl MirCall {
    /// Creates a compiler-selected protocol call with positional arguments.
    pub fn protocol(
        target: MirCallTarget,
        result: BoundCallResult,
        arguments: impl IntoIterator<Item = MirOperand>,
        witnesses: impl IntoIterator<Item = SelectedImplementationWitness>,
    ) -> Self {
        Self {
            target,
            result,
            arguments: shared_slice(
                arguments
                    .into_iter()
                    .enumerate()
                    .map(|(ordinal, value)| {
                        MirCallArgument::positional(
                            u32::try_from(ordinal).unwrap_or(u32::MAX),
                            value,
                        )
                    }),
            ),
            phase_behaviors: None,
            contract: None,
            dispatch_witnesses: Arc::from([]),
            witnesses: sorted_unique_shared_slice(witnesses),
        }
    }

    /// Creates a source call from its complete checked selection.
    pub fn selected(
        target: MirCallTarget,
        result: BoundCallResult,
        arguments: impl IntoIterator<Item = MirCallArgument>,
        phase_behaviors: CallablePhaseBehaviors,
        contract: Option<CallableContractTemplate>,
        dispatch_witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
        witnesses: impl IntoIterator<Item = SelectedImplementationWitness>,
    ) -> Self {
        Self {
            target,
            result,
            arguments: shared_slice(arguments),
            phase_behaviors: Some(Arc::new(phase_behaviors)),
            contract: contract.map(Arc::new),
            dispatch_witnesses: sorted_unique_shared_slice(dispatch_witnesses),
            witnesses: sorted_unique_shared_slice(witnesses),
        }
    }

    /// Returns the exact selected call target.
    pub const fn target(&self) -> &MirCallTarget {
        &self.target
    }

    /// Returns whether invocation executes immediately or creates a lazy future.
    pub const fn result(&self) -> BoundCallResult {
        self.result
    }

    /// Returns checked inputs in evaluation order.
    pub fn arguments(&self) -> &[MirCallArgument] {
        &self.arguments
    }

    /// Returns the checked invocation and deferred-execution behavior when source selection supplied it.
    pub fn phase_behaviors(&self) -> Option<&CallablePhaseBehaviors> {
        self.phase_behaviors.as_deref()
    }

    /// Returns source or imported callable contract clauses when declaration-backed.
    pub fn contract(&self) -> Option<&CallableContractTemplate> {
        self.contract.as_deref()
    }

    /// Returns dispatch witnesses retained by the selected callable target.
    pub fn dispatch_witnesses(&self) -> &[ImplementationInstanceId] {
        &self.dispatch_witnesses
    }

    /// Returns exact implementation requirements and witnesses in canonical order.
    pub fn witnesses(&self) -> &[SelectedImplementationWitness] {
        &self.witnesses
    }
}
