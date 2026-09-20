use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_runtime_interface::{RuntimeAbiRole, RuntimeAbiVersion};
use bray_symbols::{
    CallableAbi, CallableContractTemplate, CallableInstanceData, CallableParameterSymbolId,
    CallablePhaseBehaviors, ImplementationInstanceId, ReceiverParameterSymbolId,
    StructFieldSymbolId, UnionPayloadFieldSymbolId,
};

use bray_bound_tree::{BoundCallResult, SelectedImplementationWitness};

use crate::{MirImportedExecutableKey, MirOperand};

/// Stable reference to one source-backed or imported anonymous callable body.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirAnonymousCallableReference {
    /// A nested source semantic unit.
    Bound(bray_bound_tree::BoundUnitKey),
    /// A nested executable template reconstructed from a package implementation artifact.
    Imported(MirImportedExecutableKey),
}

impl MirAnonymousCallableReference {
    /// Creates a reference to one nested source semantic unit.
    pub const fn bound(unit: bray_bound_tree::BoundUnitKey) -> Self {
        Self::Bound(unit)
    }

    /// Creates a reference to one nested imported executable template.
    pub const fn imported(key: MirImportedExecutableKey) -> Self {
        Self::Imported(key)
    }
}

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

/// Declaration context selecting a default provider's specialization and witnesses.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum MirDefaultOwner {
    /// A callable parameter default, including type-form parameters.
    Callable(MirCallableReference),
    /// A field default of a constructed named type.
    Type {
        /// Selected field-bearing construction.
        target: bray_bound_tree::ConstructionTarget,
        /// Constructed type carrying its specialization.
        ty: bray_symbols::TypeId,
    },
}

/// Exact callable mechanism selected for one MIR call.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirCallTarget {
    /// A concrete Bray callable instance.
    Direct(MirCallableReference),
    /// A declaration-owned default evaluated before its caller transfers inputs.
    DefaultValue {
        /// Exact declaration context selecting the provider specialization.
        owner: MirDefaultOwner,
        /// Selected declaration-owned provider.
        provider: bray_bound_tree::DefaultValueProvider,
    },
    /// One private runtime ABI operation.
    Runtime(MirRuntimeReference),
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
            Self::DefaultValue { .. } => CallableAbi::Bray,
            Self::Runtime(_) => CallableAbi::Bray,
            Self::Indirect { abi, .. } => *abi,
        }
    }
}

/// One evaluated receiver or argument.
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

    /// Returns the evaluated input operand.
    pub const fn value(&self) -> &MirOperand {
        match self {
            Self::Receiver { value, .. } | Self::Explicit { value, .. } => value,
        }
    }
}

/// One explicit call with ordered inputs and retained checked behavior.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirCall {
    cleanup: bool,
    target: MirCallTarget,
    result: BoundCallResult,
    arguments: Arc<[MirCallArgument]>,
    phase_behaviors: Option<Arc<CallablePhaseBehaviors>>,
    contract: Option<Arc<CallableContractTemplate>>,
    dispatch_witnesses: Arc<[ImplementationInstanceId]>,
    trait_dispatch: Option<bray_symbols::TraitConstraintDispatch>,
    intrinsic: Option<MirCallIntrinsic>,
    witnesses: Arc<[SelectedImplementationWitness]>,
}

/// A compiler-defined operation that may realize a generic protocol call after specialization.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirCallIntrinsic {
    /// A scalar unary operation.
    Unary(crate::MirUnaryOperator),
    /// A scalar binary operation.
    Binary(crate::MirBinaryOperator),
    /// A scalar three-way comparison producing the compiler-known ordering representation.
    Comparison {
        /// Variant returned when the left operand is less than the right operand.
        less: bray_symbols::UnionVariantSymbolId,
        /// Variant returned when both operands compare equal.
        equal: bray_symbols::UnionVariantSymbolId,
        /// Variant returned when the left operand is greater than the right operand.
        greater: bray_symbols::UnionVariantSymbolId,
    },
    /// A compiler-defined conversion to the retained target type.
    Conversion(bray_symbols::TypeId),
}

impl MirCall {
    pub(crate) fn remap_local_ids(
        &mut self,
        mappings: &impl crate::unit::local_id_remap::MirLocalIdMapping,
    ) {
        if let MirCallTarget::Indirect { callee, .. } = &mut self.target {
            callee.remap_local_ids(mappings);
        }

        for argument in Arc::make_mut(&mut self.arguments) {
            match argument {
                MirCallArgument::Receiver { value, .. }
                | MirCallArgument::Explicit { value, .. } => value.remap_local_ids(mappings),
            }
        }
    }

    /// Marks an invocation of a mandatory local action backed by its owner's admission.
    pub const fn with_cleanup(mut self, cleanup: bool) -> Self {
        self.cleanup = cleanup;

        self
    }

    /// Returns whether this invocation consumes a local action's outgoing allowance.
    pub const fn is_cleanup(&self) -> bool {
        self.cleanup
    }

    /// Returns whether this call can propagate a panic through the synchronous Bray ABI.
    pub const fn may_propagate_panic(&self) -> bool {
        matches!(
            &self.target,
            MirCallTarget::Direct(MirCallableReference {
                abi: CallableAbi::Bray,
                ..
            }) | MirCallTarget::DefaultValue { .. }
                | MirCallTarget::Indirect {
                    abi: CallableAbi::Bray,
                    ..
                }
        ) && matches!(self.result, BoundCallResult::Immediate(_))
            && self.intrinsic.is_none()
    }

    /// Creates a checked call reconstructed from a compiled dependency.
    #[expect(
        clippy::too_many_arguments,
        reason = "a MIR call retains each checked call component explicitly"
    )]
    pub fn imported(
        target: MirCallTarget,
        result: BoundCallResult,
        arguments: impl IntoIterator<Item = MirCallArgument>,
        phase_behaviors: Option<CallablePhaseBehaviors>,
        dispatch_witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
        trait_dispatch: Option<bray_symbols::TraitConstraintDispatch>,
        intrinsic: Option<MirCallIntrinsic>,
        witnesses: impl IntoIterator<Item = SelectedImplementationWitness>,
    ) -> Self {
        Self {
            cleanup: false,
            target,
            result,
            arguments: shared_slice(arguments),
            phase_behaviors: phase_behaviors.map(Arc::new),
            contract: None,
            dispatch_witnesses: sorted_unique_shared_slice(dispatch_witnesses),
            trait_dispatch,
            intrinsic,
            witnesses: sorted_unique_shared_slice(witnesses),
        }
    }

    /// Creates a compiler-selected protocol call with positional arguments.
    pub fn protocol(
        target: MirCallTarget,
        result: BoundCallResult,
        arguments: impl IntoIterator<Item = MirOperand>,
        witnesses: impl IntoIterator<Item = SelectedImplementationWitness>,
    ) -> Self {
        Self {
            cleanup: false,
            target,
            result,
            arguments: shared_slice(arguments.into_iter().enumerate().map(|(ordinal, value)| {
                MirCallArgument::positional(u32::try_from(ordinal).unwrap_or(u32::MAX), value)
            })),
            phase_behaviors: None,
            contract: None,
            dispatch_witnesses: Arc::from([]),
            trait_dispatch: None,
            intrinsic: None,
            witnesses: sorted_unique_shared_slice(witnesses),
        }
    }

    /// Returns a protocol call dispatched through one surrounding generic constraint.
    pub const fn with_trait_dispatch(
        mut self,
        dispatch: bray_symbols::TraitConstraintDispatch,
    ) -> Self {
        self.trait_dispatch = Some(dispatch);

        self
    }

    /// Returns a protocol call with its compiler-defined concrete realization retained.
    pub const fn with_intrinsic(mut self, intrinsic: MirCallIntrinsic) -> Self {
        self.intrinsic = Some(intrinsic);

        self
    }

    /// Creates a source call from its complete checked selection.
    #[expect(
        clippy::too_many_arguments,
        reason = "a MIR call retains each checked call component explicitly"
    )]
    pub fn selected(
        target: MirCallTarget,
        result: BoundCallResult,
        arguments: impl IntoIterator<Item = MirCallArgument>,
        phase_behaviors: CallablePhaseBehaviors,
        contract: Option<CallableContractTemplate>,
        dispatch_witnesses: impl IntoIterator<Item = ImplementationInstanceId>,
        trait_dispatch: Option<bray_symbols::TraitConstraintDispatch>,
        witnesses: impl IntoIterator<Item = SelectedImplementationWitness>,
    ) -> Self {
        Self {
            cleanup: false,
            target,
            result,
            arguments: shared_slice(arguments),
            phase_behaviors: Some(Arc::new(phase_behaviors)),
            contract: contract.map(Arc::new),
            dispatch_witnesses: sorted_unique_shared_slice(dispatch_witnesses),
            trait_dispatch,
            intrinsic: None,
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

    /// Returns the generic constraint supplying callable dispatch.
    pub const fn trait_dispatch(&self) -> Option<bray_symbols::TraitConstraintDispatch> {
        self.trait_dispatch
    }

    /// Returns the compiler-defined operation available for concrete protocol realization.
    pub const fn intrinsic(&self) -> Option<MirCallIntrinsic> {
        self.intrinsic
    }

    /// Returns exact implementation requirements and witnesses in canonical order.
    pub fn witnesses(&self) -> &[SelectedImplementationWitness] {
        &self.witnesses
    }
}
