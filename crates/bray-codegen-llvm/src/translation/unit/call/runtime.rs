use super::super::core::UnitTranslator;
use super::super::support::{llvm, parameter_type};
use bray_codegen::{CodegenFailure, CodegenHelperMapping, CodegenResultMapping, CodegenSymbolKey};
use bray_ir::MirTaskTerminalState;
use inkwell::types::BasicTypeEnum;
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(in crate::translation::unit) fn invoke_runtime(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
        arguments: &[BasicValueEnum<'context>],
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let key = CodegenSymbolKey::Runtime(runtime);

        let symbol = self
            .request
            .mappings()
            .symbol(&key)
            .expect("checked MIR call translation requires an established mapping or value");

        if runtime.role().native_signature().is_some() {
            let result = self.invoke_native_runtime(runtime, arguments)?;

            return if matches!(symbol.signature().result(), CodegenResultMapping::Void) {
                Ok(None)
            } else {
                Ok(result
                    .map(Some)
                    .expect("non-void runtime calls must return a value"))
            };
        }

        let function = self
            .module
            .get_function(symbol.name().as_str())
            .expect("checked MIR call translation requires an established mapping or value");

        self.invoke_function(
            function,
            symbol.signature(),
            arguments,
            runtime.role().as_str(),
        )
    }

    pub(in crate::translation::unit) fn invoke_native_runtime(
        &self,
        runtime: bray_ir::MirRuntimeReference,
        arguments: &[BasicValueEnum<'context>],
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let key = bray_codegen::CodegenSymbolKey::Runtime(runtime);

        let function = self
            .request
            .mappings()
            .symbol(&key)
            .and_then(|symbol| self.module.get_function(symbol.name().as_str()))
            .expect("checked MIR call translation requires an established mapping or value");

        let native_arguments = arguments
            .iter()
            .copied()
            .map(Into::into)
            .collect::<Vec<_>>();

        let result = crate::native::invoke_function(
            self.types.context(),
            &self.builder,
            self.request.target(),
            &key,
            function,
            &native_arguments,
            runtime.role().as_str(),
        )?;

        // These native operations publish protected failure into their borrowed outcome slot.
        if matches!(
            runtime.role(),
            bray_runtime_interface::RuntimeAbiRole::OutgoingAdmission
                | bray_runtime_interface::RuntimeAbiRole::OutgoingRetirement
        ) {
            let destination = arguments[1].into_pointer_value();
            let ty = crate::native::run_outcome_type(self.types.context(), self.request.target());

            let outcome = self
                .builder
                .build_load(ty, destination, "outgoing.provider.outcome")
                .map_err(CodegenFailure::backend_library)?;

            let retained = self.retain_runtime_report(
                bray_runtime_interface::RuntimeAbiType::RunOutcome,
                outcome,
            )?;

            self.builder
                .build_store(destination, retained)
                .map_err(CodegenFailure::backend_library)?;
        }

        result
            .map(|result| {
                self.retain_runtime_report(
                    runtime
                        .role()
                        .native_signature()
                        .expect("native runtime calls have an established signature")
                        .result(),
                    result,
                )
            })
            .transpose()
    }

    pub(in crate::translation::unit) fn form_product_host(&mut self) -> Result<(), CodegenFailure> {
        let Some(host) = self.request.mappings().product_host() else {
            return Ok(());
        };

        let context = self.types.context();

        let observation = crate::native::invoke_product_control(
            context,
            self.module,
            &self.builder,
            self.request.target(),
            host,
            self.unit.target().runtime_abi(),
            bray_runtime_abi::NativeProductHostOperation::FORM,
        )?;

        let status =
            super::super::support::extract_value(&self.builder, observation, 0)?.into_int_value();

        let admitted = llvm(self.builder.build_int_compare(
            inkwell::IntPredicate::EQ,
            status,
            status.get_type().const_zero(),
            "product.formed",
        ))?;

        let continued = context.append_basic_block(self.function, "product.formed");
        let failed = context.append_basic_block(self.function, "product.formation.failed");

        llvm(
            self.builder
                .build_conditional_branch(admitted, continued, failed),
        )?;

        self.builder.position_at_end(failed);
        self.translate_compiler_shutdown(context.i64_type().const_int(1, false))?;
        llvm(self.builder.build_unreachable())?;
        self.builder.position_at_end(continued);

        Ok(())
    }

    fn retain_runtime_report(
        &self,
        result_kind: bray_runtime_interface::RuntimeAbiType,
        result: BasicValueEnum<'context>,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        use bray_runtime_interface::RuntimeAbiType;

        let Some(host) = self.request.mappings().product_host() else {
            return Ok(result);
        };

        let (report_index, state_index) = match result_kind {
            RuntimeAbiType::PanicReport => (None, None),
            RuntimeAbiType::RunOutcome => (Some(2), Some(0)),
            RuntimeAbiType::FrameProgress => (Some(3), Some(1)),
            _ => return Ok(result),
        };

        let storage = self.allocate_temporary(result.get_type(), "runtime.provider.owner")?;

        self.builder
            .build_store(storage, result)
            .map_err(CodegenFailure::backend_library)?;

        let report = if let Some(index) = report_index {
            self.builder
                .build_struct_gep(
                    result.into_struct_value().get_type(),
                    storage,
                    index,
                    "runtime.report",
                )
                .map_err(CodegenFailure::backend_library)?
        } else {
            storage
        };

        let report_type = crate::native::panic_report_type(self.types.context());

        let retained = self
            .builder
            .build_struct_gep(report_type, report, 15, "runtime.provider.retain")
            .map_err(CodegenFailure::backend_library)?;

        let retain = self
            .builder
            .build_load(
                self.types
                    .context()
                    .ptr_type(inkwell::AddressSpace::default()),
                retained,
                "runtime.provider.callback",
            )
            .map_err(CodegenFailure::backend_library)?
            .into_pointer_value();

        let mut needs_owner = self
            .builder
            .build_is_null(retain, "runtime.provider.absent")
            .map_err(CodegenFailure::backend_library)?;

        if let Some(index) = state_index {
            let state = self
                .builder
                .build_extract_value(result.into_struct_value(), index, "runtime.state")
                .map_err(CodegenFailure::backend_library)?
                .into_int_value();

            let panicked = self
                .builder
                .build_int_compare(
                    inkwell::IntPredicate::EQ,
                    state,
                    state.get_type().const_int(
                        u64::from(bray_runtime_abi::NativeRunState::PANICKED.code()),
                        false,
                    ),
                    "runtime.panicked",
                )
                .map_err(CodegenFailure::backend_library)?;

            needs_owner = self
                .builder
                .build_and(needs_owner, panicked, "runtime.report.unretained")
                .map_err(CodegenFailure::backend_library)?;
        }

        let parent = self
            .builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .expect("runtime reports are constructed inside a function");

        let retain_block = self
            .types
            .context()
            .append_basic_block(parent, "runtime.provider.acquire");

        let continued = self
            .types
            .context()
            .append_basic_block(parent, "runtime.provider.continue");

        self.builder
            .build_conditional_branch(needs_owner, retain_block, continued)
            .map_err(CodegenFailure::backend_library)?;

        self.builder.position_at_end(retain_block);

        let owner = self
            .builder
            .build_struct_gep(report_type, report, 14, "runtime.provider.reference")
            .map_err(CodegenFailure::backend_library)?;

        crate::native::retain_product_provider(
            self.types.context(),
            self.module,
            &self.builder,
            self.request.target(),
            host,
            self.unit.target().runtime_abi(),
            owner,
        )?;

        self.builder
            .build_unconditional_branch(continued)
            .map_err(CodegenFailure::backend_library)?;

        self.builder.position_at_end(continued);

        self.builder
            .build_load(result.get_type(), storage, "runtime.retained")
            .map_err(CodegenFailure::backend_library)
    }

    pub(in crate::translation::unit) fn runtime_integer_argument(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
        parameter: usize,
        value: u64,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let ty = self.runtime_parameter_type(runtime, parameter);

        let BasicTypeEnum::IntType(ty) = self.types.map(ty)? else {
            panic!("checked MIR call translation violated an established compiler contract");
        };

        Ok(ty.const_int(value, false).into())
    }

    pub(in crate::translation::unit) fn helper_boolean_argument(
        &mut self,
        helper: &CodegenHelperMapping,
        parameter: usize,
        value: bool,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        self.helper_integer_argument(helper, parameter, u64::from(value))
    }

    pub(in crate::translation::unit) fn helper_integer_argument(
        &mut self,
        helper: &CodegenHelperMapping,
        parameter: usize,
        value: u64,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let key = helper
            .symbol()
            .expect("checked MIR call translation requires an established mapping or value");

        let symbol = self
            .request
            .mappings()
            .symbol(key)
            .expect("checked MIR call translation requires an established mapping or value");

        let ty = parameter_type(symbol.signature(), parameter);

        let BasicTypeEnum::IntType(ty) = self.types.map(ty)? else {
            panic!("checked MIR call translation violated an established compiler contract");
        };

        Ok(ty.const_int(value, false).into())
    }

    pub(in crate::translation::unit) fn helper_address(
        &self,
        helper: &CodegenHelperMapping,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let Some(key) = helper.symbol() else {
            return Ok(None);
        };

        let symbol = self
            .request
            .mappings()
            .symbol(key)
            .expect("checked MIR call translation requires an established mapping or value");

        let function = self
            .module
            .get_function(symbol.callable_address_name().as_str())
            .expect("checked MIR call translation requires an established mapping or value");

        Ok(Some(function.as_global_value().as_pointer_value().into()))
    }

    pub(in crate::translation::unit) fn runtime_null_pointer_argument(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
        parameter: usize,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let ty = self.runtime_parameter_type(runtime, parameter);

        let BasicTypeEnum::PointerType(ty) = self.types.map(ty)? else {
            panic!("checked MIR call translation violated an established compiler contract");
        };

        Ok(ty.const_null().into())
    }

    fn runtime_parameter_type(
        &self,
        runtime: bray_ir::MirRuntimeReference,
        parameter: usize,
    ) -> bray_symbols::TypeId {
        let key = CodegenSymbolKey::Runtime(runtime);

        let symbol = self
            .request
            .mappings()
            .symbol(&key)
            .expect("checked MIR call translation requires an established mapping or value");

        parameter_type(symbol.signature(), parameter)
    }

    pub(in crate::translation::unit) fn terminal_state_arguments(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
        state: &MirTaskTerminalState,
    ) -> Result<Vec<BasicValueEnum<'context>>, CodegenFailure> {
        let tag = match state {
            MirTaskTerminalState::Completed(_) => 0,
            MirTaskTerminalState::Cancelled => 1,
            MirTaskTerminalState::Panicked(_) => 2,
        };

        let mut arguments = vec![self.runtime_integer_argument(runtime, 0, tag)?];

        match state {
            MirTaskTerminalState::Completed(value) | MirTaskTerminalState::Panicked(value) => {
                arguments.push(self.operand(value)?);
            }
            MirTaskTerminalState::Cancelled => {}
        }

        Ok(arguments)
    }

    pub(in crate::translation::unit) fn invoke_helper(
        &mut self,
        helper: &CodegenHelperMapping,
        arguments: &[BasicValueEnum<'context>],
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let key = helper
            .symbol()
            .expect("checked MIR call translation requires an established mapping or value");

        if let CodegenSymbolKey::Runtime(runtime) = key {
            return self.invoke_runtime(*runtime, arguments);
        }

        let symbol = self
            .request
            .mappings()
            .symbol(key)
            .expect("checked MIR call translation requires an established mapping or value");

        let function = self
            .module
            .get_function(symbol.name().as_str())
            .expect("checked MIR call translation requires an established mapping or value");

        self.invoke_function(function, symbol.signature(), arguments, "helper")
    }
}
