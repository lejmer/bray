use super::super::core::UnitTranslator;
use super::super::support::{extract_value, int_value, llvm};
use bray_codegen::{CodegenFailure, CodegenFieldLayout, CodegenTypeKind};
use bray_ir::{MirCleanupPhase, MirHelperReference, MirOperationId, MirRuntimeReference};
use bray_runtime_interface::{ExecutableEntryResult, ExecutableHostEntryId};
use inkwell::IntPredicate;
use inkwell::basic_block::BasicBlock;
use inkwell::values::{IntValue, PointerValue};

pub(in crate::translation::unit) struct HostReturnedValue<'context> {
    pub(in crate::translation::unit) destination: PointerValue<'context>,
    pub(in crate::translation::unit) handle: IntValue<'context>,
    admission_failure: BasicBlock<'context>,
    continuation: BasicBlock<'context>,
}

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn entry_cleanup_helpers(
        &self,
        operation: MirOperationId,
        error: bray_symbols::TypeId,
    ) -> Result<[bray_codegen::CodegenHelperMapping; 2], CodegenFailure> {
        let helpers: [bray_codegen::CodegenHelperMapping; 2] = self
            .operation_helpers(operation)?
            .try_into()
            .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?;

        for (helper, phase) in helpers.iter().zip([
            MirCleanupPhase::TaskCancellation,
            MirCleanupPhase::LifecycleResolution,
        ]) {
            if helper.reference() != &(MirHelperReference::Cleanup { phase, ty: error }) {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }
        }

        Ok(helpers)
    }

    pub(super) fn resolve_returned_value(
        &mut self,
        runtime: MirRuntimeReference,
        skip_error: IntValue<'context>,
        succeeded: IntValue<'context>,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        let admission = self
            .host_returned_value
            .as_ref()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let handle = admission.handle;
        let error = llvm(self.builder.build_not(skip_error, "entry.returned_error"))?;

        let error = llvm(self.builder.build_int_z_extend(
            error,
            self.types.context().i8_type(),
            "entry.returned_error.flag",
        ))?;

        let status = self
            .invoke_native_runtime(runtime, &[handle.into(), error.into()])?
            .and_then(int_value)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let resolved = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_zero(),
            "entry.result.resolution.success",
        ))?;

        llvm(
            self.builder
                .build_and(succeeded, resolved, "entry.result.success"),
        )
    }

    pub(super) fn entry_error_field(
        &self,
        result: ExecutableEntryResult,
    ) -> Result<&'request CodegenFieldLayout, CodegenFailure> {
        let ExecutableEntryResult::Fallible {
            ty,
            error,
            success_variant,
        } = result
        else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let mapping = self
            .type_mapping(ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Union { variants, .. } = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let variant = variants
            .iter()
            .find(|variant| variant.variant() != success_variant)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let [field] = variant.fields() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if field.ty() != error
            || !matches!(
                field.reference(),
                Some(bray_ir::MirFieldReference::UnionPayload(_))
            )
        {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(field)
    }

    pub(super) fn prepare_returned_value(
        &mut self,
        operation: MirOperationId,
        entry: ExecutableHostEntryId,
        runtime: MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let entry = host
            .entry(entry)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let ExecutableEntryResult::Fallible { ty, error, .. } = entry.result() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let offset = self
            .entry_error_field(entry.result())
            .map_err(|cause| {
                CodegenFailure::generated_module_invariant(("entry_result_field", cause))
            })?
            .offset_bytes();

        let layout = self
            .type_mapping(ty)
            .and_then(|mapping| mapping.layout())
            .ok_or_else(|| {
                CodegenFailure::generated_module_invariant(("entry_result_layout", ty))
            })?;

        let [broadcast, lifecycle] =
            self.entry_cleanup_helpers(operation, error)
                .map_err(|cause| {
                    CodegenFailure::generated_module_invariant(("entry_cleanup_helpers", cause))
                })?;

        if lifecycle.frame().is_none() || lifecycle.frame() != entry.returned_value_cleanup() {
            return Err(CodegenFailure::generated_module_invariant((
                "entry_cleanup_frame",
                lifecycle.frame(),
                entry.returned_value_cleanup(),
            )));
        }

        let broadcast = self.value_cleanup_descriptor(&broadcast)?;
        let lifecycle = self.value_cleanup_descriptor(&lifecycle)?;
        let context = self.types.context();
        let word = crate::native::pointer_integer_type(context, self.request.target());
        let address = self.allocate_temporary(word, "entry.result.address")?;

        let product = self.product_descriptor().map_err(|cause| {
            CodegenFailure::generated_module_invariant(("entry_product", cause))
        })?;

        let allocation = self
            .invoke_native_runtime(
                runtime,
                &[
                    product.into(),
                    word.const_int(layout.size(), false).into(),
                    word.const_int(layout.alignment().get(), false).into(),
                    word.const_int(offset, false).into(),
                    broadcast,
                    lifecycle,
                    address.into(),
                ],
            )
            .map_err(|cause| {
                CodegenFailure::generated_module_invariant(("entry_result_admission", cause))
            })?
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let status = extract_value(&self.builder, allocation, 0)
            .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        let handle = extract_value(&self.builder, allocation, 1)
            .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        let failure = self
            .builder
            .get_insert_block()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let function = failure
            .get_parent()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let admitted = context.append_basic_block(function, "entry.result.admitted");
        let continuation = context.append_basic_block(function, "entry.result.resolved");

        let succeeded = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            status,
            status.get_type().const_zero(),
            "entry.result.admission.success",
        ))?;

        llvm(
            self.builder
                .build_conditional_branch(succeeded, admitted, continuation),
        )?;

        self.builder.position_at_end(admitted);

        let address = llvm(
            self.builder
                .build_load(word, address, "entry.result.address"),
        )?
        .into_int_value();

        let destination = llvm(self.builder.build_int_to_ptr(
            address,
            context.ptr_type(inkwell::AddressSpace::default()),
            "entry.result.destination",
        ))?;

        self.host_returned_value = Some(HostReturnedValue {
            destination,
            handle,
            admission_failure: failure,
            continuation,
        });

        Ok(())
    }

    pub(super) fn finish_returned_value_admission(
        &mut self,
        status: IntValue<'context>,
    ) -> Result<IntValue<'context>, CodegenFailure> {
        let Some(admission) = self.host_returned_value.take() else {
            return Ok(status);
        };

        let completed = self
            .builder
            .get_insert_block()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        llvm(
            self.builder
                .build_unconditional_branch(admission.continuation),
        )?;

        self.builder.position_at_end(admission.continuation);

        let joined = llvm(
            self.builder
                .build_phi(status.get_type(), "entry.result.status"),
        )?;

        joined.add_incoming(&[
            (&status, completed),
            (
                &status.get_type().const_int(1, false),
                admission.admission_failure,
            ),
        ]);

        Ok(joined.as_basic_value().into_int_value())
    }
}
