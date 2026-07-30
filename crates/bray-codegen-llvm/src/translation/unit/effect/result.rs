use super::super::core::UnitTranslator;
use super::super::support::{int_value, llvm};
use bray_codegen::CodegenFailure;
use bray_runtime_interface::{ExecutableEntryResult, RootExecution};
use inkwell::IntPredicate;
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn host_exit_status(
        &mut self,
        integer: inkwell::types::IntType<'context>,
    ) -> Result<inkwell::values::IntValue<'context>, CodegenFailure> {
        let bray_ir::MirUnitKind::ExecutableHost(host) = self.unit.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let entry_result = host.entry_result();
        let asynchronous = matches!(host.root(), RootExecution::Asynchronous { .. });

        let (completed, result) = if asynchronous {
            let outcome = self
                .host_result
                .take()
                .and_then(|value| match value {
                    BasicValueEnum::StructValue(value) => Some(value),
                    _ => None,
                })
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let state = super::super::support::extract_value(
                &self.builder,
                outcome.into(),
                0,
            )
            .and_then(|value| {
                int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant)
            })?;

            let payload = super::super::support::extract_value(
                &self.builder,
                outcome.into(),
                1,
            )
            .and_then(|value| {
                int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant)
            })?;

            let completed = llvm(self.builder.build_int_compare(
                IntPredicate::EQ,
                state,
                state.get_type().const_zero(),
                "root.completed",
            ))?;

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
                    let pointer = llvm(self.builder.build_int_to_ptr(
                        payload,
                        self.types
                            .context()
                            .ptr_type(inkwell::AddressSpace::default()),
                        "root.completion",
                    ))?;

                    Some(llvm(self.builder.build_load(
                        self.types.context().i32_type(),
                        pointer,
                        "root.result",
                    ))?)
                }
                ExecutableEntryResult::Fallible { ty, .. } => {
                    let pointer = llvm(self.builder.build_int_to_ptr(
                        payload,
                        self.types
                            .context()
                            .ptr_type(inkwell::AddressSpace::default()),
                        "root.completion",
                    ))?;

                    Some(llvm(self.builder.build_load(
                        self.types.map(ty)?,
                        pointer,
                        "root.result",
                    ))?)
                }
            };

            (Some(completed), result)
        } else {
            (None, self.host_result.take())
        };

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
                success_variant,
            } => {
                let result =
                    result.ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let tag = self.union_tag(result, ty)?;
                let success = self.union_variant_tag(ty, success_variant, tag.get_type())?;

                let succeeded =
                    llvm(self.builder.build_int_compare(
                        IntPredicate::EQ,
                        tag,
                        success,
                        "host.succeeded",
                    ))?;

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

        llvm(self.builder.build_select(
            completed,
            status,
            integer.const_int(1, false),
            "root.exit",
        ))
        .and_then(|value| {
            int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant)
        })
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

        llvm(
            self.builder
                .build_store(fallback, result_type.const_zero()),
        )?;

        let fallback = llvm(self.builder.build_ptr_to_int(
            fallback,
            payload.get_type(),
            "root.completion.fallback.handle",
        ))?;

        llvm(self.builder.build_select(
            completed,
            payload,
            fallback,
            "root.completion.handle",
        ))
        .and_then(|value| {
            int_value(value).ok_or(CodegenFailure::GeneratedModuleInvariant)
        })
    }

}
