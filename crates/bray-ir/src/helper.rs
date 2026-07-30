use bray_bound_tree::{
    BoundUnitKey, ConstructionDefaultProvider, ConstructionTarget, ConversionTarget,
    SelectedConversion,
};
use bray_symbols::{
    CallableAbi, CallableInstanceData, CallableParameterDefaultProviderSymbolId, TypeId,
};

use crate::{
    MirAsyncOperation, MirCall, MirCallArgument, MirCleanupPhase, MirFrameInitializer,
    MirFrameReference, MirGeneratorOperation, MirOperationKind,
};

/// Exact semantic role of one callable helper required to realize MIR.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirHelperReference {
    /// Independently lowered capture-free anonymous callable.
    AnonymousCallable(BoundUnitKey),
    /// Declaration-owned default for an omitted call argument.
    CallableDefault(CallableParameterDefaultProviderSymbolId),
    /// Declaration-owned default for a construction input.
    ConstructionDefault(ConstructionDefaultProvider),
    /// Selected type-form construction callable.
    TypeForm(CallableInstanceData),
    /// Selected implementation callable for a semantic conversion.
    Conversion(CallableInstanceData),
    /// Initialize generator accumulation.
    BeginGenerator,
    /// Append one yielded generator element.
    PushGenerator,
    /// Finish generator accumulation.
    FinishGenerator,
    /// Construct one owned panic report from a checked failure cause.
    PanicReport,
    /// Run checked finalization for a value of the retained type.
    Finalize(TypeId),
    /// Destroy a value of the retained type.
    Destroy(TypeId),
    /// Run one checked cleanup phase for a value of the retained type.
    Cleanup {
        /// Cleanup phase being executed.
        phase: MirCleanupPhase,
        /// Type whose cleanup implementation is invoked.
        ty: TypeId,
    },
    /// Create one inactive protected frame.
    CreateFrame(MirFrameReference),
    /// Move one inactive protected frame before its first resume.
    MoveInactiveFrame(MirFrameReference),
    /// Compose one directly awaited child frame.
    ComposeAwaitedFrame(MirFrameReference),
    /// Move one awaited completion result.
    CommitAwaitedCompletion(MirFrameReference),
    /// Destroy one terminal task control record.
    DestroyTerminalTask,
}

impl MirHelperReference {
    /// Returns the calling convention required by this helper role.
    pub const fn abi(&self) -> CallableAbi {
        CallableAbi::Bray
    }

    /// Returns the semantic value type retained by a lifecycle helper.
    pub const fn lifecycle_type(&self) -> Option<TypeId> {
        match self {
            Self::Finalize(ty) | Self::Destroy(ty) | Self::Cleanup { ty, .. } => Some(*ty),
            Self::AnonymousCallable(_)
            | Self::CallableDefault(_)
            | Self::ConstructionDefault(_)
            | Self::TypeForm(_)
            | Self::Conversion(_)
            | Self::BeginGenerator
            | Self::PushGenerator
            | Self::FinishGenerator
            | Self::PanicReport
            | Self::CreateFrame(_)
            | Self::MoveInactiveFrame(_)
            | Self::ComposeAwaitedFrame(_)
            | Self::CommitAwaitedCompletion(_)
            | Self::DestroyTerminalTask => None,
        }
    }
}

impl MirOperationKind {
    /// Returns callable helpers required to realize this operation in execution order.
    pub fn helper_references(&self) -> Vec<MirHelperReference> {
        let mut helpers = Vec::new();

        match self {
            Self::AnonymousCallable(unit) => {
                helpers.push(MirHelperReference::AnonymousCallable(unit.clone()));
            }
            Self::Construct(construction) => {
                for input in construction.inputs() {
                    if let crate::MirConstructionInput::Default { provider, .. } = input {
                        helpers.push(MirHelperReference::ConstructionDefault(*provider));
                    }
                }

                if let ConstructionTarget::TypeForm { callable, .. } = construction.target() {
                    helpers.push(MirHelperReference::TypeForm(callable));
                }
            }
            Self::Convert { conversion, .. } => {
                collect_conversion_helpers(conversion, &mut helpers);
            }
            Self::Generator(operation) => {
                helpers.push(match operation {
                    MirGeneratorOperation::Begin { .. } => MirHelperReference::BeginGenerator,
                    MirGeneratorOperation::Push { .. } => MirHelperReference::PushGenerator,
                    MirGeneratorOperation::Finish { .. } => MirHelperReference::FinishGenerator,
                });
            }
            Self::PanicReport(_) => helpers.push(MirHelperReference::PanicReport),
            Self::Call(call) => collect_call_defaults(call, &mut helpers),
            Self::Finalize(place) => helpers.push(MirHelperReference::Finalize(place.ty())),
            Self::Destroy(place) => helpers.push(MirHelperReference::Destroy(place.ty())),
            Self::Cleanup { phase, place } => helpers.push(MirHelperReference::Cleanup {
                phase: *phase,
                ty: place.ty(),
            }),
            Self::Async(MirAsyncOperation::CreateFrame { frame, initializer }) => {
                if let MirFrameInitializer::Callable(call) = initializer {
                    collect_call_defaults(call, &mut helpers);
                }

                helpers.push(MirHelperReference::CreateFrame(*frame));
            }
            Self::Async(MirAsyncOperation::MoveInactiveFrame { frame, .. }) => {
                helpers.push(MirHelperReference::MoveInactiveFrame(*frame));
            }
            Self::Async(MirAsyncOperation::ComposeAwaitedFrame { child, .. }) => {
                helpers.push(MirHelperReference::ComposeAwaitedFrame(*child));
            }
            Self::Async(MirAsyncOperation::CommitAwaitedCompletion { child }) => {
                helpers.push(MirHelperReference::CommitAwaitedCompletion(*child));
            }
            Self::Async(MirAsyncOperation::DestroyTerminalTask { .. }) => {
                helpers.push(MirHelperReference::DestroyTerminalTask);
            }
            Self::Store { .. }
            | Self::Borrow { .. }
            | Self::Unary { .. }
            | Self::Binary { .. }
            | Self::Aggregate(_)
            | Self::PatternProjection { .. }
            | Self::Async(_)
            | Self::Host(_) => {}
        }

        helpers
    }
}

fn collect_conversion_helpers(
    conversion: &SelectedConversion,
    helpers: &mut Vec<MirHelperReference>,
) {
    match conversion.target() {
        ConversionTarget::Composite(conversions) => {
            for conversion in conversions.iter() {
                collect_conversion_helpers(conversion, helpers);
            }
        }
        ConversionTarget::Trait { fulfillment, .. } => {
            helpers.push(MirHelperReference::Conversion(*fulfillment));
        }
        ConversionTarget::Identity | ConversionTarget::BuiltInScalar => {}
    }
}

fn collect_call_defaults(call: &MirCall, helpers: &mut Vec<MirHelperReference>) {
    helpers.extend(call.arguments().iter().filter_map(|argument| {
        let MirCallArgument::Default { provider, .. } = argument else {
            return None;
        };

        Some(MirHelperReference::CallableDefault(*provider))
    }));
}
