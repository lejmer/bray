use bray_codegen::{CodegenCallableSignature, CodegenParameterMapping, CodegenResultMapping};
use bray_compiler_known::RepresentationRole;
use bray_ir::MirHelperReference;
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::{
    BorrowKind, CallableAbi, CallableExecution, TypeAssociatedLifecycleSlot, TypeData,
};

use super::super::super::{CodegenPreparationError, Compilation};
use super::super::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use super::support::is_void_result;
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
            let Some(bray_bound_tree::LifecycleCallable {
                result, execution, ..
            }) =
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

        let signature = CodegenCallableSignature::new(
            [CodegenParameterMapping::direct(pointer, None, [])],
            result,
            CallableAbi::Bray,
            false,
        );

        Ok(signature.with_panic_report_context())
    }
}

macro_rules! define_runtime_signatures {
    ($( $role:ident {
        $documentation:literal, $name:literal,
        native: ($($symbol:ident = $native:literal, [$($native_parameter:ident),*] -> $native_result:ident)?),
        $(resident: ($resident_service:ident $resident_field:ident: $resident_callback:ty),)?
        call_hook: ($($hook:ident)?),
        compiler: $abi:ident [$($parameter:ident),*] -> $result:ident,
        owner: $owner:ident, availability: $availability:ident,
        bootstrap: ($($bootstrap:literal)?), host_control: $host_control:literal,
        capabilities: [$($capability:ident),*],
        effects: [$($effect:ident),*]
    })+) => {
        impl Compilation {
            pub(super) fn codegen_runtime_signature(
                &self,
                role: RuntimeAbiRole,
            ) -> Result<CodegenCallableSignature, CodegenPreparationError> {
                match role {
                    $(RuntimeAbiRole::$role => Ok(CodegenCallableSignature::new(
                        [$(CodegenParameterMapping::direct(
                            define_runtime_signatures!(@type self, $parameter)?, None, [],
                        ),)*],
                        define_runtime_signatures!(@result self, $result),
                        CallableAbi::$abi,
                        false,
                    )),)+
                }
            }
        }
    };
    (@type $compilation:ident, Pointer) => { $compilation.codegen_opaque_pointer_type() };
    (@type $compilation:ident, Usize) => { $compilation.codegen_representation_type(RepresentationRole::ScalarUsize) };
    (@type $compilation:ident, U32) => { $compilation.codegen_representation_type(RepresentationRole::ScalarU32) };
    (@type $compilation:ident, U64) => { $compilation.codegen_representation_type(RepresentationRole::ScalarU64) };
    (@type $compilation:ident, Bool) => { $compilation.codegen_representation_type(RepresentationRole::ScalarBool) };
    (@type $compilation:ident, PanicReport) => { $compilation.codegen_representation_type(RepresentationRole::PanicReport) };
    (@result $compilation:ident, Void) => { CodegenResultMapping::Void };
    (@result $compilation:ident, $kind:ident) => {
        CodegenResultMapping::direct(define_runtime_signatures!(@type $compilation, $kind)?, None, [])
    };
}

bray_runtime_abi::runtime_role_catalog!(define_runtime_signatures);
