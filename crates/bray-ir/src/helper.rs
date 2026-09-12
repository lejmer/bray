use bray_bound_tree::{
    ConstructionDefaultProvider, ConstructionTarget, ConversionTarget, SelectedConversion,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{
    CallableAbi, CallableInstanceData, CallableParameterDefaultProviderSymbolId, TypeId,
};

use crate::{
    MirAnonymousCallableReference, MirAsyncOperation, MirCall, MirCallArgument, MirCleanupPhase,
    MirFrameInitializer, MirFrameReference, MirGeneratorOperation, MirOperationKind,
};

/// Exact semantic role of one callable helper required to realize MIR.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirHelperReference {
    /// Independently lowered capture-free anonymous callable.
    AnonymousCallable(MirAnonymousCallableReference),
    /// Declared callable whose stable function address is materialized as a value.
    DeclaredCallable(crate::MirCallableReference),
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
    /// Invoke one compiler-recognized standard-library implementation.
    StandardLibrary(MirStandardLibraryHelper),
    /// Run checked finalization for a value of the retained type.
    Finalize(TypeId),
    /// Invoke one static finalizer and preserve its completion value.
    StaticFinalize(TypeId),
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
        match self {
            Self::DeclaredCallable(callable) => callable.abi(),
            Self::AnonymousCallable(_)
            | Self::CallableDefault(_)
            | Self::ConstructionDefault(_)
            | Self::TypeForm(_)
            | Self::Conversion(_)
            | Self::BeginGenerator
            | Self::PushGenerator
            | Self::FinishGenerator
            | Self::PanicReport
            | Self::StandardLibrary(_)
            | Self::Finalize(_)
            | Self::StaticFinalize(_)
            | Self::Destroy(_)
            | Self::Cleanup { .. }
            | Self::CreateFrame(_)
            | Self::MoveInactiveFrame(_)
            | Self::ComposeAwaitedFrame(_)
            | Self::CommitAwaitedCompletion(_)
            | Self::DestroyTerminalTask => CallableAbi::Bray,
        }
    }

    /// Returns the semantic value type retained by a lifecycle helper.
    pub const fn lifecycle_type(&self) -> Option<TypeId> {
        match self {
            Self::Finalize(ty)
            | Self::StaticFinalize(ty)
            | Self::Destroy(ty)
            | Self::Cleanup { ty, .. } => Some(*ty),
            Self::AnonymousCallable(_)
            | Self::DeclaredCallable(_)
            | Self::CallableDefault(_)
            | Self::ConstructionDefault(_)
            | Self::TypeForm(_)
            | Self::Conversion(_)
            | Self::BeginGenerator
            | Self::PushGenerator
            | Self::FinishGenerator
            | Self::PanicReport
            | Self::StandardLibrary(_)
            | Self::CreateFrame(_)
            | Self::MoveInactiveFrame(_)
            | Self::ComposeAwaitedFrame(_)
            | Self::CommitAwaitedCompletion(_)
            | Self::DestroyTerminalTask => None,
        }
    }

    /// Returns the runtime role that directly realizes this helper when one is required.
    pub const fn runtime_role(&self) -> Option<RuntimeAbiRole> {
        match self {
            Self::BeginGenerator => Some(RuntimeAbiRole::GeneratorBegin),
            Self::PushGenerator => Some(RuntimeAbiRole::GeneratorPush),
            Self::FinishGenerator => Some(RuntimeAbiRole::GeneratorFinish),
            Self::PanicReport => Some(RuntimeAbiRole::PanicReportConstruction),
            Self::MoveInactiveFrame(MirFrameReference::Erased) => {
                Some(RuntimeAbiRole::InactiveFrameMove)
            }
            Self::ComposeAwaitedFrame(_) => Some(RuntimeAbiRole::AwaitedFrameComposition),
            Self::CommitAwaitedCompletion(MirFrameReference::Erased) => {
                Some(RuntimeAbiRole::FrameCompletionMove)
            }
            Self::DestroyTerminalTask => Some(RuntimeAbiRole::TaskDestruction),
            Self::AnonymousCallable(_)
            | Self::DeclaredCallable(_)
            | Self::CallableDefault(_)
            | Self::ConstructionDefault(_)
            | Self::TypeForm(_)
            | Self::Conversion(_)
            | Self::StandardLibrary(_)
            | Self::Finalize(_)
            | Self::StaticFinalize(_)
            | Self::Destroy(_)
            | Self::Cleanup { .. }
            | Self::CreateFrame(_)
            | Self::MoveInactiveFrame(MirFrameReference::Known(_))
            | Self::CommitAwaitedCompletion(MirFrameReference::Known(_)) => None,
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
            Self::DeclaredCallable(callable) => {
                helpers.push(MirHelperReference::DeclaredCallable(*callable));
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
            Self::Generator(operation) => match operation {
                MirGeneratorOperation::Begin { .. } => {
                    helpers.push(MirHelperReference::BeginGenerator);
                }
                MirGeneratorOperation::Push { .. } => {
                    helpers.push(MirHelperReference::PushGenerator);
                }
                MirGeneratorOperation::Finish { .. } => {
                    helpers.push(MirHelperReference::FinishGenerator);
                }
                MirGeneratorOperation::CleanupBroadcast { element, .. } => {
                    helpers.push(MirHelperReference::Cleanup {
                        phase: MirCleanupPhase::TaskCancellation,
                        ty: *element,
                    });
                }
                MirGeneratorOperation::Destroy { element, .. } => {
                    helpers.push(MirHelperReference::Finalize(*element));
                    helpers.push(MirHelperReference::Destroy(*element));
                }
            },
            Self::Memory(memory) => collect_memory_helpers(memory, &mut helpers),
            Self::Text(operation) => collect_text_helpers(operation, &mut helpers),
            Self::PanicReport(_) => helpers.push(MirHelperReference::PanicReport),
            Self::Call(call) => collect_call_defaults(call, &mut helpers),
            Self::Finalize(place) => helpers.push(MirHelperReference::Finalize(place.ty())),
            Self::Destroy(place) => helpers.push(MirHelperReference::Destroy(place.ty())),
            Self::Cleanup { phase, place } => helpers.push(MirHelperReference::Cleanup {
                phase: *phase,
                ty: place.ty(),
            }),
            Self::Async(MirAsyncOperation::CreateFrame { frame, initializer }) => {
                match initializer {
                    MirFrameInitializer::Callable(call) => {
                        collect_call_defaults(call, &mut helpers);
                    }
                    MirFrameInitializer::TaskObservation { result, .. } => {
                        for phase in [
                            MirCleanupPhase::TaskCancellation,
                            MirCleanupPhase::LifecycleResolution,
                        ] {
                            helpers.push(MirHelperReference::Cleanup {
                                phase,
                                ty: result.completion_type(),
                            });
                        }
                    }
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
            Self::Host(crate::MirHostOperation::ResolveRootTerminal {
                error: Some(error), ..
            }) => {
                helpers.push(MirHelperReference::Finalize(*error));
                helpers.push(MirHelperReference::Destroy(*error));
            }
            Self::Store { .. }
            | Self::Borrow { .. }
            | Self::Unary { .. }
            | Self::Binary { .. }
            | Self::NumericConversion { .. }
            | Self::NullableQuery(_)
            | Self::Aggregate(_)
            | Self::PatternProjection { .. }
            | Self::Async(_)
            | Self::Host(_) => {}
        }

        helpers
    }
}

fn collect_memory_helpers(
    memory: &crate::MirMemoryOperation,
    helpers: &mut Vec<MirHelperReference>,
) {
    let standard = match memory.kind() {
        bray_bound_tree::CheckedMemoryOperationKind::RawAllocate
        | bray_bound_tree::CheckedMemoryOperationKind::Allocate => {
            Some(MirStandardLibraryHelper::MemoryAllocate)
        }
        bray_bound_tree::CheckedMemoryOperationKind::RawDeallocate
        | bray_bound_tree::CheckedMemoryOperationKind::Deallocate => {
            Some(MirStandardLibraryHelper::MemoryDeallocate)
        }
        _ => None,
    };

    if let Some(standard) = standard {
        helpers.push(MirHelperReference::StandardLibrary(standard));
    }

    if let bray_bound_tree::CheckedMemoryOperationKind::RawBufferRelease { element }
    | bray_bound_tree::CheckedMemoryOperationKind::RawBufferReplace { element } = memory.kind()
    {
        helpers.push(MirHelperReference::Cleanup {
            phase: MirCleanupPhase::LifecycleResolution,
            ty: element,
        });

        helpers.push(MirHelperReference::StandardLibrary(
            MirStandardLibraryHelper::MemoryDeallocate,
        ));
    }
}

fn collect_text_helpers(
    operation: &crate::MirTextOperation,
    helpers: &mut Vec<MirHelperReference>,
) {
    let standard = match operation.kind() {
        crate::MirTextOperationKind::ScalarCount => {
            Some(MirStandardLibraryHelper::StringScalarCount)
        }
        crate::MirTextOperationKind::Equals => Some(MirStandardLibraryHelper::StringEquals),
        crate::MirTextOperationKind::ScalarAt => Some(MirStandardLibraryHelper::StringScalarAt),
        crate::MirTextOperationKind::ScalarSlice => {
            Some(MirStandardLibraryHelper::StringScalarSlice)
        }
        crate::MirTextOperationKind::FromUtf8 => Some(MirStandardLibraryHelper::StringFromUtf8),
        crate::MirTextOperationKind::CharacterScalarValue => {
            Some(MirStandardLibraryHelper::CharacterScalarValue)
        }
        crate::MirTextOperationKind::CharacterFromScalarValue => {
            Some(MirStandardLibraryHelper::CharacterFromScalarValue)
        }
        crate::MirTextOperationKind::CharacterUtf8Length => {
            Some(MirStandardLibraryHelper::CharacterUtf8Length)
        }
        crate::MirTextOperationKind::CharacterUtf8Byte => {
            Some(MirStandardLibraryHelper::CharacterUtf8Byte)
        }
        crate::MirTextOperationKind::CharacterIsAlphabetic => {
            Some(MirStandardLibraryHelper::CharacterIsAlphabetic)
        }
        crate::MirTextOperationKind::CharacterIsNumeric => {
            Some(MirStandardLibraryHelper::CharacterIsNumeric)
        }
        crate::MirTextOperationKind::CharacterIsWhitespace => {
            Some(MirStandardLibraryHelper::CharacterIsWhitespace)
        }
        crate::MirTextOperationKind::Release => Some(MirStandardLibraryHelper::MemoryDeallocate),
        crate::MirTextOperationKind::IsEmpty | crate::MirTextOperationKind::Utf8 => None,
    };

    if let Some(standard) = standard {
        helpers.push(MirHelperReference::StandardLibrary(standard));
    }
}

/// Compiler-recognized standard-library implementation required by one MIR operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirStandardLibraryHelper {
    /// Allocate manually managed storage.
    MemoryAllocate,
    /// Release manually managed storage.
    MemoryDeallocate,
    /// Count Unicode scalar values in UTF-8 text.
    StringScalarCount,
    /// Compare two UTF-8 byte sequences.
    StringEquals,
    /// Select one Unicode scalar from UTF-8 text.
    StringScalarAt,
    /// Copy one Unicode scalar range into owned text.
    StringScalarSlice,
    /// Validate and copy one UTF-8 byte sequence.
    StringFromUtf8,
    /// Return one character's Unicode scalar value.
    CharacterScalarValue,
    /// Validate one Unicode scalar value.
    CharacterFromScalarValue,
    /// Return one scalar's UTF-8 width.
    CharacterUtf8Length,
    /// Encode one byte of a Unicode scalar.
    CharacterUtf8Byte,
    /// Test the Unicode Alphabetic property.
    CharacterIsAlphabetic,
    /// Test whether a scalar has a Unicode numeric type.
    CharacterIsNumeric,
    /// Test the Unicode White_Space property.
    CharacterIsWhitespace,
}

impl MirStandardLibraryHelper {
    /// Every standard-library helper in stable order.
    pub const ALL: [Self; 14] = [
        Self::MemoryAllocate,
        Self::MemoryDeallocate,
        Self::StringScalarCount,
        Self::StringEquals,
        Self::StringScalarAt,
        Self::StringScalarSlice,
        Self::StringFromUtf8,
        Self::CharacterScalarValue,
        Self::CharacterFromScalarValue,
        Self::CharacterUtf8Length,
        Self::CharacterUtf8Byte,
        Self::CharacterIsAlphabetic,
        Self::CharacterIsNumeric,
        Self::CharacterIsWhitespace,
    ];
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
        ConversionTarget::TraitConstraint { member, .. } => {
            helpers.push(MirHelperReference::Conversion(*member));
        }
        ConversionTarget::Identity
        | ConversionTarget::CallableContract
        | ConversionTarget::NullablePresent
        | ConversionTarget::BuiltInScalar
        | ConversionTarget::CVariadicPromotion => {}
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
