use bray_codegen::{CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping};
use bray_compiler_known::RepresentationRole;
use bray_ir::MirHelperReference;
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{
    BorrowKind, CallableAbi, CallableExecution, TypeAssociatedLifecycleSlot, TypeData,
};

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use super::support::{is_void_result, void_signature};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn generated_lifecycle_signature(
        &self,
        reference: &MirHelperReference,
        cancellation: &CancellationToken,
    ) -> Result<CodegenCallableSignature, CodegenPreparationError> {
        let ty = reference
            .lifecycle_type()
            .ok_or_else(|| CodegenPreparationError::MissingHelperInstance(reference.clone()))?;

        let pointer = self
            .semantic_value_store()?
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: ty,
            })
            .map_err(FactQueryError::SemanticValueStore)?;

        let result = if matches!(reference, MirHelperReference::StaticFinalize(_)) {
            let Some((_, _, result, execution)) =
                self.lifecycle_callable(ty, TypeAssociatedLifecycleSlot::Finalizer, cancellation)?
            else {
                return Ok(CodegenCallableSignature::new(
                    [CodegenParameterMapping::direct(pointer, None, [])],
                    CodegenResultMapping::Void,
                    CallableAbi::Bray,
                    false,
                ));
            };

            let result = if execution == CallableExecution::Asynchronous {
                self.available_compiler_known_symbols()
                    .unary_representation_type(
                        self.semantic_value_store()?,
                        RepresentationRole::Future,
                        result,
                    )
                    .map_err(FactQueryError::SemanticValueStore)?
                    .ok_or_else(|| {
                        ProductQueryFailure::missing(
                            ProductQueryContext::UnaryRepresentation {
                                role: RepresentationRole::Future,
                                argument: result,
                            },
                            ProductDataKind::CompilerKnownRepresentation,
                        )
                    })?
            } else {
                result
            };

            if is_void_result(self, result)? {
                CodegenResultMapping::Void
            } else {
                CodegenResultMapping::direct(result, None, [])
            }
        } else {
            CodegenResultMapping::Void
        };

        Ok(CodegenCallableSignature::new(
            [CodegenParameterMapping::direct(pointer, None, [])],
            result,
            CallableAbi::Bray,
            false,
        ))
    }

    pub(super) fn codegen_runtime_signature(
        &self,
        role: RuntimeAbiRole,
    ) -> Result<CodegenCallableSignature, CodegenPreparationError> {
        if let Some(signature) = self.codegen_runtime_source_signature(role)? {
            return Ok(signature);
        }

        if role == RuntimeAbiRole::CurrentRunCancellationObservation {
            let boolean = self.codegen_representation_type(RepresentationRole::ScalarBool)?;

            return Ok(CodegenCallableSignature::new(
                [],
                CodegenResultMapping::direct(boolean, None, []),
                CallableAbi::Bray,
                false,
            ));
        }

        if role == RuntimeAbiRole::NativeThreadExecution {
            let pointer = self.codegen_opaque_pointer_type()?;
            let address = self.codegen_representation_type(RepresentationRole::ScalarUsize)?;
            let status = self.codegen_representation_type(RepresentationRole::ScalarU32)?;

            return Ok(CodegenCallableSignature::new(
                [
                    CodegenParameterMapping::direct(pointer, None, []),
                    CodegenParameterMapping::direct(address, None, []),
                    CodegenParameterMapping::direct(pointer, None, []),
                    CodegenParameterMapping::direct(address, None, []),
                    CodegenParameterMapping::direct(pointer, None, []),
                ],
                CodegenResultMapping::direct(status, None, []),
                CallableAbi::C,
                false,
            ));
        }

        if matches!(
            role,
            RuntimeAbiRole::CurrentNativeThreadIdentity | RuntimeAbiRole::MainNativeThreadIdentity
        ) {
            let identity = self.codegen_representation_type(RepresentationRole::ScalarU64)?;

            return Ok(CodegenCallableSignature::new(
                [],
                CodegenResultMapping::direct(identity, None, []),
                CallableAbi::Bray,
                false,
            ));
        }

        if role == RuntimeAbiRole::NativeThreadPanicReportRecovery {
            let address = self.codegen_representation_type(RepresentationRole::ScalarUsize)?;
            let report = self.codegen_representation_type(RepresentationRole::PanicReport)?;

            return Ok(CodegenCallableSignature::new(
                [CodegenParameterMapping::direct(address, None, [])],
                CodegenResultMapping::direct(report, None, []),
                CallableAbi::Bray,
                false,
            ));
        }

        if role == RuntimeAbiRole::PanicPropagation {
            let report = self.codegen_representation_type(RepresentationRole::PanicReport)?;

            return Ok(CodegenCallableSignature::new(
                [CodegenParameterMapping::direct(report, None, [])],
                CodegenResultMapping::Void,
                CallableAbi::Bray,
                false,
            ));
        }

        if matches!(
            role,
            RuntimeAbiRole::PanicReporting | RuntimeAbiRole::PanicReportDestruction
        ) {
            let report = self.codegen_representation_type(RepresentationRole::PanicReport)?;
            let status = self.codegen_representation_type(RepresentationRole::ScalarU32)?;

            return Ok(CodegenCallableSignature::new(
                [CodegenParameterMapping::direct(report, None, [])],
                CodegenResultMapping::direct(status, None, []),
                CallableAbi::Bray,
                false,
            ));
        }

        if matches!(
            role,
            RuntimeAbiRole::TaskCancellationRequest | RuntimeAbiRole::TaskDestruction
        ) {
            let task = self.codegen_representation_type(RepresentationRole::ScalarU64)?;
            let status = self.codegen_representation_type(RepresentationRole::ScalarU32)?;

            return Ok(CodegenCallableSignature::new(
                [CodegenParameterMapping::direct(task, None, [])],
                CodegenResultMapping::direct(status, None, []),
                CallableAbi::Bray,
                false,
            ));
        }

        if role == RuntimeAbiRole::TaskEventCreation {
            let event = self.codegen_representation_type(RepresentationRole::ScalarUsize)?;

            return Ok(CodegenCallableSignature::new(
                [],
                CodegenResultMapping::direct(event, None, []),
                CallableAbi::Bray,
                false,
            ));
        }

        if matches!(
            role,
            RuntimeAbiRole::TaskEventSignal | RuntimeAbiRole::TaskEventDestruction
        ) {
            let event = self.codegen_representation_type(RepresentationRole::ScalarUsize)?;
            let status = self.codegen_representation_type(RepresentationRole::ScalarU32)?;

            return Ok(CodegenCallableSignature::new(
                [CodegenParameterMapping::direct(event, None, [])],
                CodegenResultMapping::direct(status, None, []),
                CallableAbi::Bray,
                false,
            ));
        }

        match role {
            RuntimeAbiRole::GeneratorBegin
            | RuntimeAbiRole::GeneratorPush
            | RuntimeAbiRole::GeneratorFinish
            | RuntimeAbiRole::GeneratorCleanupBroadcast
            | RuntimeAbiRole::GeneratorDestruction => {}
            RuntimeAbiRole::RuntimeInitialization
            | RuntimeAbiRole::RootExecution
            | RuntimeAbiRole::SynchronousRootExecution
            | RuntimeAbiRole::ForeignCallbackExecution
            | RuntimeAbiRole::NativeThreadExecution
            | RuntimeAbiRole::CurrentNativeThreadIdentity
            | RuntimeAbiRole::MainNativeThreadIdentity
            | RuntimeAbiRole::NativeThreadPanicReportRecovery
            | RuntimeAbiRole::TaskEventCreation
            | RuntimeAbiRole::TaskEventSignal
            | RuntimeAbiRole::TaskEventDestruction
            | RuntimeAbiRole::ThreadAttachmentIdentity
            | RuntimeAbiRole::ThreadStaticCleanupRegistration
            | RuntimeAbiRole::ProductHostControl
            | RuntimeAbiRole::RootCancellationRequest
            | RuntimeAbiRole::TaskAllocation
            | RuntimeAbiRole::TaskStart
            | RuntimeAbiRole::FrameResume
            | RuntimeAbiRole::SuspensionRegistration
            | RuntimeAbiRole::Wake
            | RuntimeAbiRole::TaskCancellationRequest
            | RuntimeAbiRole::CurrentRunCancellationObservation
            | RuntimeAbiRole::CurrentRunCancellationPropagation
            | RuntimeAbiRole::JoinRegistration
            | RuntimeAbiRole::TaskObservationCreation
            | RuntimeAbiRole::TaskResolution
            | RuntimeAbiRole::TerminalPublication
            | RuntimeAbiRole::RuntimeEvent
            | RuntimeAbiRole::CompatibleLaneSelection
            | RuntimeAbiRole::CleanupIncidentTransfer
            | RuntimeAbiRole::CleanupIncidentReporting
            | RuntimeAbiRole::MainThreadLaneStartup
            | RuntimeAbiRole::MainThreadLaneDrive
            | RuntimeAbiRole::RootTerminalObservation
            | RuntimeAbiRole::RootCompletionResolution
            | RuntimeAbiRole::PanicReporting
            | RuntimeAbiRole::PanicReportDestruction
            | RuntimeAbiRole::EntryFailureReporting
            | RuntimeAbiRole::TestEntrySelection
            | RuntimeAbiRole::StructuredShutdown
            | RuntimeAbiRole::FrameTaskBroadcast
            | RuntimeAbiRole::FrameLifecycleResolution
            | RuntimeAbiRole::FrameCompletionMove
            | RuntimeAbiRole::FrameDestruction
            | RuntimeAbiRole::PanicReportConstruction
            | RuntimeAbiRole::PanicPropagation
            | RuntimeAbiRole::FrameCreation
            | RuntimeAbiRole::InactiveFrameMove
            | RuntimeAbiRole::AwaitedFrameComposition
            | RuntimeAbiRole::TaskDestruction => return Ok(void_signature(CallableAbi::Bray)),
        }

        let pointer = self.codegen_opaque_pointer_type()?;
        let usize = self.codegen_representation_type(RepresentationRole::ScalarUsize)?;
        let boolean = self.codegen_representation_type(RepresentationRole::ScalarBool)?;
        let parameter = |ty| CodegenParameterMapping::direct(ty, None, []);

        let (parameters, result) = match role {
            RuntimeAbiRole::GeneratorBegin => (
                vec![
                    parameter(pointer),
                    parameter(usize),
                    parameter(usize),
                    parameter(usize),
                    parameter(usize),
                    parameter(boolean),
                ],
                CodegenResultMapping::Void,
            ),
            RuntimeAbiRole::GeneratorPush | RuntimeAbiRole::GeneratorCleanupBroadcast => (
                vec![parameter(pointer), parameter(pointer)],
                CodegenResultMapping::Void,
            ),
            RuntimeAbiRole::GeneratorFinish => {
                (vec![parameter(pointer)], CodegenResultMapping::Void)
            }
            RuntimeAbiRole::GeneratorDestruction => (
                vec![parameter(pointer), parameter(pointer), parameter(pointer)],
                CodegenResultMapping::Void,
            ),
            _ => return Err(ProductQueryFailure::UnsupportedRuntimeRole { role }.into()),
        };

        Ok(CodegenCallableSignature::new(
            parameters,
            result,
            CallableAbi::Bray,
            false,
        ))
    }
}
