use std::sync::Arc;

use bray_base::shared_slice;
use bray_runtime_interface::{RuntimeAbiRole, RuntimeAbiVersion};
use bray_symbols::{
    CallableAbi, CallableInstanceId, StructFieldSymbolId, UnionPayloadFieldSymbolId,
};

use crate::{MirOperand, MirValueId};

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
    instance: CallableInstanceId,
    abi: CallableAbi,
}

impl MirCallableReference {
    /// Creates an exact callable reference and its checked calling convention.
    pub const fn new(instance: CallableInstanceId, abi: CallableAbi) -> Self {
        Self { instance, abi }
    }

    /// Returns the selected callable instance.
    pub const fn instance(self) -> CallableInstanceId {
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
    Indirect(MirValueId),
}

/// One explicit call with arguments in evaluation order.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirCall {
    target: MirCallTarget,
    arguments: Arc<[MirOperand]>,
}

impl MirCall {
    /// Creates a call from its selected target and ordered arguments.
    pub fn new(target: MirCallTarget, arguments: impl IntoIterator<Item = MirOperand>) -> Self {
        Self {
            target,
            arguments: shared_slice(arguments),
        }
    }

    /// Returns the exact selected call target.
    pub const fn target(&self) -> &MirCallTarget {
        &self.target
    }

    /// Returns arguments in evaluation order.
    pub fn arguments(&self) -> &[MirOperand] {
        &self.arguments
    }
}
