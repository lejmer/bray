use bray_bound_tree::{ConstructionTarget, ConversionTarget, SelectedConversion};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::DefaultValueProvider;
use bray_symbols::{CallableAbi, CallableInstanceData, TypeId};

use crate::{
    MirAnonymousCallableReference, MirAsyncOperation, MirCleanupPhase, MirFrameInitializer,
    MirFrameReference, MirGeneratorOperation, MirOperationKind,
};

/// Destruction choice established before a lifecycle helper is realized.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirDestructionDisposition {
    /// An erased callback's caller selects the disposition at invocation.
    Dynamic,
    /// Graceful represented-part obligations remain eligible.
    Ordinary,
    /// Graceful obligations have already been abandoned.
    Abandoned,
}

impl MirDestructionDisposition {
    /// Retains a known MIR disposition before helper realization.
    pub fn from_argument(argument: Option<&crate::MirOperand>) -> Self {
        match argument {
            None
            | Some(crate::MirOperand::Immediate {
                value: crate::MirImmediateValue::Boolean(false),
                ..
            }) => Self::Ordinary,
            Some(crate::MirOperand::Immediate {
                value: crate::MirImmediateValue::Boolean(true),
                ..
            }) => Self::Abandoned,
            Some(_) => Self::Dynamic,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{MirDestructionDisposition, MirHelperReference};
    use crate::{
        MirGeneratedLifecycleKey, MirGeneratedLifecycleRole, MirImmediateValue, MirOperand,
        MirPlace, MirStorageId, MirUnitId,
    };

    #[test]
    fn destruction_identity_retains_known_and_erased_dispositions() {
        let ty = crate::test_support::test_type();

        let dynamic = MirOperand::Copy(MirPlace::new(
            MirStorageId::from_slot(MirUnitId::new(0), 0),
            [],
            ty,
        ));

        let literal = |value| MirOperand::Immediate {
            value: MirImmediateValue::Boolean(value),
            ty,
        };

        assert_eq!(
            MirDestructionDisposition::from_argument(None),
            MirDestructionDisposition::Ordinary
        );

        assert_eq!(
            MirDestructionDisposition::from_argument(Some(&literal(false))),
            MirDestructionDisposition::Ordinary
        );

        assert_eq!(
            MirDestructionDisposition::from_argument(Some(&literal(true))),
            MirDestructionDisposition::Abandoned
        );

        assert_eq!(
            MirDestructionDisposition::from_argument(Some(&dynamic)),
            MirDestructionDisposition::Dynamic
        );

        let keys = [
            MirDestructionDisposition::Dynamic,
            MirDestructionDisposition::Ordinary,
            MirDestructionDisposition::Abandoned,
        ]
        .map(|disposition| {
            let reference = MirHelperReference::Destroy(ty, disposition);
            let role = MirGeneratedLifecycleRole::from_reference(&reference).unwrap();

            MirGeneratedLifecycleKey::new(role, [7; 32])
        });

        assert_eq!(
            keys.into_iter()
                .collect::<std::collections::BTreeSet<_>>()
                .len(),
            3
        );
    }
}

/// Exact semantic role of one callable helper required to realize MIR.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirHelperReference {
    /// Independently lowered capture-free anonymous callable.
    AnonymousCallable(MirAnonymousCallableReference),
    /// Declared callable whose stable function address is materialized as a value.
    DeclaredCallable(crate::MirCallableReference),
    /// Declaration-owned default evaluated before input ownership transfers.
    DefaultValue(DefaultValueProvider),
    /// Evaluate one complete checked predicate over readonly caller inputs.
    Predicate(crate::MirPredicateReference),
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
    /// Resolve child runs while retaining the initialized incident representation.
    Quiesce(TypeId),
    /// Invoke one static finalizer and preserve its completion value.
    StaticFinalize(TypeId),
    /// Destroy a value of the retained type.
    Destroy(TypeId, MirDestructionDisposition),
    /// Resolve buffer contents without ending the borrowed buffer owner's allowance.
    ReleaseBuffer(TypeId),
    /// Run one checked cleanup phase for a value of the retained type.
    Cleanup {
        /// Cleanup phase being executed.
        phase: MirCleanupPhase,
        /// Type whose cleanup implementation is invoked.
        ty: TypeId,
    },
    /// Create an admitted frame for a represented owner's selected cleanup role.
    LifecycleFrame {
        /// Selected lifecycle work performed by the frame.
        role: crate::MirGeneratedLifecycleRole,
        /// Owner retained by the borrowing cleanup frame.
        ty: TypeId,
    },
    /// Create one inactive protected frame.
    CreateFrame(MirFrameReference),
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
            | Self::DefaultValue(_)
            | Self::Predicate(_)
            | Self::TypeForm(_)
            | Self::Conversion(_)
            | Self::BeginGenerator
            | Self::PushGenerator
            | Self::FinishGenerator
            | Self::PanicReport
            | Self::StandardLibrary(_)
            | Self::Quiesce(_)
            | Self::StaticFinalize(_)
            | Self::Destroy(_, _)
            | Self::ReleaseBuffer(_)
            | Self::Cleanup { .. }
            | Self::LifecycleFrame { .. }
            | Self::CreateFrame(_)
            | Self::ComposeAwaitedFrame(_)
            | Self::CommitAwaitedCompletion(_)
            | Self::DestroyTerminalTask => CallableAbi::Bray,
        }
    }

    /// Returns the semantic value type retained by a lifecycle helper.
    pub const fn lifecycle_type(&self) -> Option<TypeId> {
        match self {
            Self::Quiesce(ty)
            | Self::StaticFinalize(ty)
            | Self::Destroy(ty, _)
            | Self::ReleaseBuffer(ty)
            | Self::Cleanup { ty, .. }
            | Self::LifecycleFrame { ty, .. } => Some(*ty),
            Self::AnonymousCallable(_)
            | Self::DeclaredCallable(_)
            | Self::DefaultValue(_)
            | Self::Predicate(_)
            | Self::TypeForm(_)
            | Self::Conversion(_)
            | Self::BeginGenerator
            | Self::PushGenerator
            | Self::FinishGenerator
            | Self::PanicReport
            | Self::StandardLibrary(_)
            | Self::CreateFrame(_)
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
            Self::ComposeAwaitedFrame(_) => Some(RuntimeAbiRole::AwaitedFrameComposition),
            Self::CommitAwaitedCompletion(_) => Some(RuntimeAbiRole::FrameCompletionMove),
            Self::DestroyTerminalTask => Some(RuntimeAbiRole::TaskDestruction),
            Self::AnonymousCallable(_)
            | Self::DeclaredCallable(_)
            | Self::DefaultValue(_)
            | Self::Predicate(_)
            | Self::TypeForm(_)
            | Self::Conversion(_)
            | Self::StandardLibrary(_)
            | Self::Quiesce(_)
            | Self::StaticFinalize(_)
            | Self::Destroy(_, _)
            | Self::ReleaseBuffer(_)
            | Self::Cleanup { .. }
            | Self::LifecycleFrame { .. }
            | Self::CreateFrame(_) => None,
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
                    helpers.push(MirHelperReference::Cleanup {
                        phase: MirCleanupPhase::LifecycleResolution,
                        ty: *element,
                    });

                    helpers.push(MirHelperReference::Destroy(
                        *element,
                        crate::MirDestructionDisposition::Dynamic,
                    ));
                }
            },
            Self::Memory(memory) => collect_memory_helpers(memory, &mut helpers),
            Self::Text(operation) => collect_text_helpers(operation, &mut helpers),
            Self::PanicReport(_) => helpers.push(MirHelperReference::PanicReport),
            Self::Call(call) => {
                if let crate::MirCallTarget::DefaultValue { provider, .. } = call.target() {
                    helpers.push(MirHelperReference::DefaultValue(*provider));
                }

                if let crate::MirCallTarget::Predicate(reference) = call.target() {
                    // Helper identity shares immutable checked input types with the call.
                    helpers.push(MirHelperReference::Predicate(reference.clone()));
                }

                if let crate::MirCallTarget::Lifecycle { role, ty } = call.target() {
                    helpers.push(MirHelperReference::LifecycleFrame {
                        role: *role,
                        ty: *ty,
                    });
                }
            }
            Self::Quiesce { place, .. } => helpers.push(MirHelperReference::Quiesce(place.ty())),
            Self::ReleaseBuffer { place, .. } => {
                helpers.push(MirHelperReference::ReleaseBuffer(place.ty()))
            }
            Self::Destroy {
                place, abandoned, ..
            } => helpers.push(MirHelperReference::Destroy(
                place.ty(),
                MirDestructionDisposition::from_argument(abandoned.as_ref()),
            )),
            Self::Cleanup { phase, place, .. } => helpers.push(MirHelperReference::Cleanup {
                phase: *phase,
                ty: place.ty(),
            }),
            Self::Async(MirAsyncOperation::CreateFrame { frame, initializer }) => {
                if let MirFrameInitializer::Callable(call) = initializer
                    && let crate::MirCallTarget::Lifecycle { role, ty } = call.target()
                {
                    helpers.push(MirHelperReference::LifecycleFrame {
                        role: *role,
                        ty: *ty,
                    });
                }

                if let MirFrameInitializer::TaskObservation { result, .. } = initializer {
                    for phase in [
                        MirCleanupPhase::TaskCancellation,
                        MirCleanupPhase::LifecycleResolution,
                    ] {
                        helpers.push(MirHelperReference::Cleanup {
                            phase,
                            ty: result.completion_type(),
                        });
                    }

                    helpers.push(MirHelperReference::CreateFrame(*frame));
                }
            }
            Self::Async(MirAsyncOperation::MoveInactiveFrame { .. }) => {}
            Self::Async(MirAsyncOperation::ComposeAwaitedFrame { child, .. }) => {
                helpers.push(MirHelperReference::ComposeAwaitedFrame(*child));
            }
            Self::Async(MirAsyncOperation::CommitAwaitedCompletion { child, .. }) => {
                helpers.push(MirHelperReference::CommitAwaitedCompletion(*child));
            }
            Self::Async(MirAsyncOperation::TransferCleanupIncident { incident, .. }) => {
                helpers.push(MirHelperReference::Destroy(
                    incident.ty(),
                    MirDestructionDisposition::Abandoned,
                ));
            }
            Self::Async(MirAsyncOperation::DestroyTerminalTask { .. }) => {
                helpers.push(MirHelperReference::DestroyTerminalTask);
            }
            Self::Host(crate::MirHostOperation::ResolveRootTerminal {
                error: Some(error), ..
            }) => {
                helpers.push(MirHelperReference::Cleanup {
                    phase: MirCleanupPhase::LifecycleResolution,
                    ty: *error,
                });
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
            | Self::AdmitOutgoing { .. }
            | Self::DischargeOutgoing { .. }
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
        | bray_bound_tree::CheckedMemoryOperationKind::Allocate
        | bray_bound_tree::CheckedMemoryOperationKind::RawBufferAllocate => {
            Some(MirStandardLibraryHelper::MemoryAllocate)
        }
        bray_bound_tree::CheckedMemoryOperationKind::RawDeallocate
        | bray_bound_tree::CheckedMemoryOperationKind::Deallocate
        | bray_bound_tree::CheckedMemoryOperationKind::RawBufferRelease { .. } => {
            Some(MirStandardLibraryHelper::MemoryDeallocate)
        }
        _ => None,
    };

    if let Some(standard) = standard {
        helpers.push(MirHelperReference::StandardLibrary(standard));
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
