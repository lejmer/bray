use super::super::core::UnitTranslator;
use super::super::support::{int_value, llvm};
use bray_codegen::CodegenFailure;
use bray_runtime_interface::{ExecutableEntryResult, RootExecution, RuntimeRoleImplementation};
use inkwell::IntPredicate;
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn resolve_host_result(
        &mut self,
        operation_id: bray_ir::MirOperationId,
        entry: bray_runtime_interface::ExecutableHostEntryId,
        integer: inkwell::types::IntType<'context>,
        completion: bray_ir::MirRuntimeReference,
        panic: bray_ir::MirRuntimeReference,
        entry_failure: bray_ir::MirRuntimeReference,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let entry = host
            .entry(entry)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let entry_result = entry.result();
        let asynchronous = matches!(entry.root(), RootExecution::Asynchronous { .. });

        let native_boundary = asynchronous
            || host
                .requirements()
                .roles()
                .contains(&bray_runtime_interface::RuntimeAbiRole::SynchronousRootExecution);

        let (completed, result) = self.take_host_result(entry_result, native_boundary, panic)?;

        let status = match entry_result {
            ExecutableEntryResult::Unit => {
                if result.is_some() {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                integer.const_zero()
            }
            ExecutableEntryResult::I32 => {
                let result = result
                    .and_then(int_value)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                llvm(
                    self.builder
                        .build_int_s_extend(result, integer, "host.exit"),
                )?
            }
            ExecutableEntryResult::Fallible {
                ty,
                error: error_type,
                success_variant,
            } => {
                let result = result.ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let tag = self.union_tag(result, ty)?;
                let success = self.union_variant_tag(ty, success_variant, tag.get_type())?;

                let succeeded = llvm(self.builder.build_int_compare(
                    IntPredicate::EQ,
                    tag,
                    success,
                    "host.succeeded",
                ))?;

                let mapping = self
                    .request
                    .mappings()
                    .ty(ty)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let bray_codegen::CodegenTypeKind::Union { variants, .. } = mapping.kind() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let error_variant = variants
                    .iter()
                    .find(|variant| variant.variant() != success_variant)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let [error_field] = error_variant.fields() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                if error_field.ty() != error_type {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                }

                let Some(bray_ir::MirFieldReference::UnionPayload(error_field)) =
                    error_field.reference()
                else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                let storage = llvm(
                    self.builder
                        .build_alloca(self.types.map(ty)?, "entry.result"),
                )?;

                llvm(self.builder.build_store(storage, result))?;

                let error = self.union_field_pointer(
                    storage,
                    mapping.kind(),
                    error_variant.variant(),
                    error_field,
                )?;

                let error_handle = llvm(self.builder.build_ptr_to_int(
                    error,
                    crate::native::pointer_integer_type(
                        self.types.context(),
                        self.request.target(),
                    ),
                    "entry.error",
                ))?;

                self.resolve_entry_failure(
                    operation_id,
                    succeeded,
                    error,
                    error_handle,
                    error_type,
                    entry_failure,
                )?;

                llvm(self.builder.build_select(
                    succeeded,
                    integer.const_zero(),
                    integer.const_int(1, false),
                    "host.exit",
                ))
                .and_then(|value| {
                    int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant)
                })?
            }
        };

        let Some(completed) = completed else {
            return Ok(status);
        };

        let status = llvm(self.builder.build_select(
            completed,
            status,
            integer.const_int(1, false),
            "root.exit",
        ))
        .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        if asynchronous {
            let root = self
                .host_root
                .take()
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            if self.host_role_implementation(completion)?
                != RuntimeRoleImplementation::CompilerLowering
            {
                self.invoke_native_runtime(completion, &[root])?;
            }
        }

        Ok(status)
    }

    fn take_host_result(
        &mut self,
        entry_result: ExecutableEntryResult,
        native_boundary: bool,
        panic: bray_ir::MirRuntimeReference,
    ) -> Result<
        (
            Option<inkwell::values::IntValue<'context>>,
            Option<BasicValueEnum<'context>>,
        ),
        CodegenFailure,
    > {
        if !native_boundary {
            return Ok((None, self.host_result.take()));
        }

        let outcome = self
            .host_result
            .take()
            .and_then(|value| match value {
                BasicValueEnum::StructValue(value) => Some(value),
                _ => None,
            })
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let state = super::super::support::extract_value(&self.builder, outcome.into(), 0)
            .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        let payload = super::super::support::extract_value(&self.builder, outcome.into(), 1)
            .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))?;

        let completed = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            state,
            state.get_type().const_zero(),
            "root.completed",
        ))?;

        let panicked = llvm(self.builder.build_int_compare(
            IntPredicate::EQ,
            state,
            state.get_type().const_int(2, false),
            "root.panicked",
        ))?;

        if self.host_role_implementation(panic)? != RuntimeRoleImplementation::CompilerLowering {
            self.invoke_native_runtime_if(panicked, panic, &[payload.into()])?;
        }

        let payload = match entry_result {
            ExecutableEntryResult::Unit => payload,
            ExecutableEntryResult::I32 => self.safe_completion_payload(
                payload,
                completed,
                self.types.context().i32_type().into(),
            )?,
            ExecutableEntryResult::Fallible { ty, .. } => {
                let result_type = self.types.map(ty)?;

                self.safe_completion_payload(payload, completed, result_type)?
            }
        };

        let result = match entry_result {
            ExecutableEntryResult::Unit => None,
            ExecutableEntryResult::I32 => {
                let pointer = llvm(
                    self.builder.build_int_to_ptr(
                        payload,
                        self.types
                            .context()
                            .ptr_type(inkwell::AddressSpace::default()),
                        "root.completion",
                    ),
                )?;

                Some(llvm(self.builder.build_load(
                    self.types.context().i32_type(),
                    pointer,
                    "root.result",
                ))?)
            }
            ExecutableEntryResult::Fallible { ty, .. } => {
                let pointer = llvm(
                    self.builder.build_int_to_ptr(
                        payload,
                        self.types
                            .context()
                            .ptr_type(inkwell::AddressSpace::default()),
                        "root.completion",
                    ),
                )?;

                Some(llvm(self.builder.build_load(
                    self.types.map(ty)?,
                    pointer,
                    "root.result",
                ))?)
            }
        };

        Ok((Some(completed), result))
    }

    fn safe_completion_payload(
        &self,
        payload: inkwell::values::IntValue<'context>,
        completed: inkwell::values::IntValue<'context>,
        result_type: inkwell::types::BasicTypeEnum<'context>,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        let fallback = llvm(
            self.builder
                .build_alloca(result_type, "root.completion.fallback"),
        )?;

        llvm(self.builder.build_store(fallback, result_type.const_zero()))?;

        let fallback = llvm(self.builder.build_ptr_to_int(
            fallback,
            payload.get_type(),
            "root.completion.fallback.handle",
        ))?;

        llvm(
            self.builder
                .build_select(completed, payload, fallback, "root.completion.handle"),
        )
        .and_then(|value| int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant))
    }

    fn resolve_entry_failure(
        &mut self,
        operation: bray_ir::MirOperationId,
        succeeded: inkwell::values::IntValue<'context>,
        error: inkwell::values::PointerValue<'context>,
        error_handle: inkwell::values::IntValue<'context>,
        error_type: bray_symbols::TypeId,
        reporter: bray_ir::MirRuntimeReference,
    ) -> Result<(), CodegenFailure> {
        let helpers = self.operation_helpers(operation)?;

        let [finalize, destroy] = helpers.as_slice() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if finalize.reference() != &bray_ir::MirHelperReference::Finalize(error_type)
            || destroy.reference() != &bray_ir::MirHelperReference::Destroy(error_type)
        {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let function = self
            .builder
            .get_insert_block()
            .and_then(inkwell::basic_block::BasicBlock::get_parent)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let failed = self
            .types
            .context()
            .append_basic_block(function, "entry.failure");

        let resolved = self
            .types
            .context()
            .append_basic_block(function, "entry.failure.resolved");

        llvm(
            self.builder
                .build_conditional_branch(succeeded, resolved, failed),
        )?;

        self.builder.position_at_end(failed);

        if self.host_role_implementation(reporter)? != RuntimeRoleImplementation::CompilerLowering {
            let size =
                crate::native::pointer_integer_type(self.types.context(), self.request.target())
                    .const_int(
                        self.types
                            .target_data()
                            .get_store_size(&self.types.map(error_type)?),
                        false,
                    );

            self.invoke_native_runtime(reporter, &[error_handle.into(), size.into()])?;
        }

        if finalize.symbol().is_some() {
            self.invoke_helper(finalize, &[error.into()])?;
        }

        if destroy.symbol().is_some() && self.invoke_helper(destroy, &[error.into()])?.is_some() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        llvm(self.builder.build_unconditional_branch(resolved))?;
        self.builder.position_at_end(resolved);

        Ok(())
    }

    fn invoke_native_runtime_if(
        &self,
        condition: inkwell::values::IntValue<'context>,
        runtime: bray_ir::MirRuntimeReference,
        arguments: &[BasicValueEnum<'context>],
    ) -> Result<(), CodegenFailure> {
        let function = self
            .builder
            .get_insert_block()
            .and_then(inkwell::basic_block::BasicBlock::get_parent)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let invoke = self
            .types
            .context()
            .append_basic_block(function, "host.report");

        let continued = self
            .types
            .context()
            .append_basic_block(function, "host.reported");

        llvm(
            self.builder
                .build_conditional_branch(condition, invoke, continued),
        )?;

        self.builder.position_at_end(invoke);
        self.invoke_native_runtime(runtime, arguments)?;
        llvm(self.builder.build_unconditional_branch(continued))?;
        self.builder.position_at_end(continued);

        Ok(())
    }
}
