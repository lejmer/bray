use super::core::UnitTranslator;
use super::support::{llvm, parameter_type, pointer_value};
use bray_codegen::{
    CodegenCallSite, CodegenCallableSignature, CodegenFailure, CodegenHelperMapping,
    CodegenParameterMapping, CodegenResultMapping, CodegenSymbolKey, CodegenTypeKind,
};
use bray_ir::{MirCall, MirCallArgument, MirCallTarget, MirTaskTerminalState};
use inkwell::types::BasicTypeEnum;
use inkwell::values::{
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, FunctionValue, PointerValue,
};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_call(
        &mut self,
        operation: bray_ir::MirOperationId,
        call: &MirCall,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let checks_call_panic = self.checked_call_operations.contains(&operation);

        let checked_default_context = if checks_call_panic
            && call
                .arguments()
                .iter()
                .any(|argument| matches!(argument, MirCallArgument::Default { .. }))
        {
            Some(self.checked_call_panic_report_context()?)
        } else {
            None
        };

        let helpers = self.operation_helpers(operation)?;
        let mut helpers = helpers.iter();

        let evaluated =
            self.evaluate_call_arguments(call, &mut helpers, checked_default_context)?;

        let (semantic_arguments, checked_defaults) = evaluated.into_parts();

        let semantic_arguments = semantic_arguments.as_slice();

        if helpers.next().is_some() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let result = match call.target() {
            MirCallTarget::Direct(_) => {
                let mapping = self
                    .request
                    .mappings()
                    .callable(self.instance.key(), CodegenCallSite::Operation(operation))
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                if let Some(intrinsic) = mapping.intrinsic_operation() {
                    let operand_type = call
                        .arguments()
                        .first()
                        .and_then(MirCallArgument::value)
                        .map(|operand| self.operand_type(operand))
                        .transpose()?
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    return self
                        .translate_intrinsic_call(
                            intrinsic,
                            operand_type,
                            call.result().ty(),
                            semantic_arguments,
                        )
                        .map(Some);
                }

                let instance = mapping
                    .instance()
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let symbol = self
                    .request
                    .mappings()
                    .instance_symbol(instance)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let function = self
                    .module
                    .get_function(symbol.name().as_str())
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                // Owning the signature releases the immutable mapping borrow before invocation
                // mutates translation state.
                let signature = symbol.signature().clone();

                if call.may_propagate_panic() && checks_call_panic {
                    if let Some(context) = checked_default_context {
                        self.invoke_function_with_panic_report_context(
                            function,
                            &signature,
                            semantic_arguments,
                            "call",
                            Some(context),
                        )
                    } else {
                        self.invoke_checked_function(
                            function,
                            &signature,
                            semantic_arguments,
                            "call",
                        )
                    }
                } else {
                    self.invoke_function(function, &signature, semantic_arguments, "call")
                }
            }
            MirCallTarget::Runtime(runtime) => self.invoke_runtime(*runtime, semantic_arguments),
            MirCallTarget::Indirect { callee, .. } => {
                let callee_type = self.operand_type(callee)?;

                let mapping = self
                    .type_mapping(callee_type)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let CodegenTypeKind::Callable(signature) = mapping.kind() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                // Translation mutates its value cache after releasing the borrowed mapping.
                let signature = signature.clone();

                let pointer = pointer_value(self.operand(callee)?)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let function_type = self.types.function_type(&signature)?;

                if call.may_propagate_panic() && checks_call_panic {
                    if let Some(context) = checked_default_context {
                        self.invoke_indirect_with_panic_report_context(
                            function_type,
                            pointer,
                            &signature,
                            semantic_arguments,
                            "call.indirect",
                            Some(context),
                        )
                    } else {
                        self.invoke_checked_indirect(
                            function_type,
                            pointer,
                            &signature,
                            semantic_arguments,
                            "call.indirect",
                        )
                    }
                } else {
                    self.invoke_indirect(
                        function_type,
                        pointer,
                        &signature,
                        semantic_arguments,
                        "call.indirect",
                    )
                }
            }
        }?;

        let result = if let Some(defaults) = checked_defaults {
            self.finish_checked_default_evaluation(defaults, result)?
        } else {
            result
        };

        if result.is_some() {
            return Ok(result);
        }

        let Some(result) = self
            .unit
            .operation(operation)
            .and_then(bray_ir::MirOperation::result)
        else {
            return Ok(None);
        };

        let ty = self
            .unit
            .value(result)
            .map(bray_ir::MirValue::ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let mapping = self
            .type_mapping(ty)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        if mapping.layout().is_none_or(|layout| layout.size() != 0) {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let ty = self.types.map(ty)?;

        Ok(Some(ty.const_zero()))
    }

    pub(super) fn invoke_runtime(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
        arguments: &[BasicValueEnum<'context>],
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let key = CodegenSymbolKey::Runtime(runtime);

        let symbol = self
            .request
            .mappings()
            .symbol(&key)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let function = self
            .module
            .get_function(symbol.name().as_str())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.invoke_function(
            function,
            symbol.signature(),
            arguments,
            runtime.role().as_str(),
        )
    }

    pub(super) fn runtime_integer_argument(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
        parameter: usize,
        value: u64,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let ty = self.runtime_parameter_type(runtime, parameter)?;

        let BasicTypeEnum::IntType(ty) = self.types.map(ty)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(ty.const_int(value, false).into())
    }

    pub(super) fn helper_boolean_argument(
        &mut self,
        helper: &CodegenHelperMapping,
        parameter: usize,
        value: bool,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        self.helper_integer_argument(helper, parameter, u64::from(value))
    }

    pub(super) fn helper_integer_argument(
        &mut self,
        helper: &CodegenHelperMapping,
        parameter: usize,
        value: u64,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let key = helper
            .symbol()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let symbol = self
            .request
            .mappings()
            .symbol(key)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let ty = parameter_type(symbol.signature(), parameter)?;

        let BasicTypeEnum::IntType(ty) = self.types.map(ty)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(ty.const_int(value, false).into())
    }

    pub(super) fn helper_address(
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
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let function = self
            .module
            .get_function(symbol.callable_address_name().as_str())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        Ok(Some(function.as_global_value().as_pointer_value().into()))
    }

    pub(super) fn runtime_null_pointer_argument(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
        parameter: usize,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let ty = self.runtime_parameter_type(runtime, parameter)?;

        let BasicTypeEnum::PointerType(ty) = self.types.map(ty)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(ty.const_null().into())
    }

    fn runtime_parameter_type(
        &self,
        runtime: bray_ir::MirRuntimeReference,
        parameter: usize,
    ) -> Result<bray_symbols::TypeId, CodegenFailure> {
        let key = CodegenSymbolKey::Runtime(runtime);

        let symbol = self
            .request
            .mappings()
            .symbol(&key)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        parameter_type(symbol.signature(), parameter)
    }

    pub(super) fn terminal_state_arguments(
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

    pub(super) fn invoke_helper(
        &mut self,
        helper: &CodegenHelperMapping,
        arguments: &[BasicValueEnum<'context>],
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let key = helper
            .symbol()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let symbol = self
            .request
            .mappings()
            .symbol(key)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let function = self
            .module
            .get_function(symbol.name().as_str())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        self.invoke_function(function, symbol.signature(), arguments, "helper")
    }

    pub(super) fn invoke_function(
        &mut self,
        function: FunctionValue<'context>,
        signature: &CodegenCallableSignature,
        semantic_arguments: &[BasicValueEnum<'context>],
        name: &str,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let panic_report_context = self.boundary_panic_report_context(signature)?;

        let result = self.invoke_function_with_panic_report_context(
            function,
            signature,
            semantic_arguments,
            name,
            panic_report_context,
        )?;

        if let Some(context) = panic_report_context {
            self.propagate_boundary_call_panic(context)?;
        }

        Ok(result)
    }

    fn invoke_checked_function(
        &mut self,
        function: FunctionValue<'context>,
        signature: &CodegenCallableSignature,
        semantic_arguments: &[BasicValueEnum<'context>],
        name: &str,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let panic_report_context = self.checked_panic_report_context(signature)?;

        let result = self.invoke_function_with_panic_report_context(
            function,
            signature,
            semantic_arguments,
            name,
            Some(panic_report_context),
        )?;

        if self
            .pending_call_panic_report_context
            .replace(panic_report_context)
            .is_some()
        {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(result)
    }

    pub(super) fn invoke_function_with_panic_report_context(
        &mut self,
        function: FunctionValue<'context>,
        signature: &CodegenCallableSignature,
        semantic_arguments: &[BasicValueEnum<'context>],
        name: &str,
        panic_report_context: Option<PointerValue<'context>>,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let mut arguments = Vec::new();

        if !call_argument_count_is_valid(signature, semantic_arguments.len()) {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let result_storage = self.prepare_call_arguments(
            signature,
            semantic_arguments,
            &mut arguments,
            panic_report_context,
        )?;

        let call = llvm(self.builder.build_call(function, &arguments, name))?;

        call.set_call_convention(function.get_call_conventions());
        crate::mapping::apply_signature_call_attributes(call, signature, self.types)?;

        self.finish_call(call, signature, result_storage)
    }

    pub(super) fn invoke_indirect(
        &mut self,
        function_type: inkwell::types::FunctionType<'context>,
        function: PointerValue<'context>,
        signature: &CodegenCallableSignature,
        semantic_arguments: &[BasicValueEnum<'context>],
        name: &str,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let panic_report_context = self.boundary_panic_report_context(signature)?;

        let result = self.invoke_indirect_with_panic_report_context(
            function_type,
            function,
            signature,
            semantic_arguments,
            name,
            panic_report_context,
        )?;

        if let Some(context) = panic_report_context {
            self.propagate_boundary_call_panic(context)?;
        }

        Ok(result)
    }

    fn invoke_checked_indirect(
        &mut self,
        function_type: inkwell::types::FunctionType<'context>,
        function: PointerValue<'context>,
        signature: &CodegenCallableSignature,
        semantic_arguments: &[BasicValueEnum<'context>],
        name: &str,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let panic_report_context = self.checked_panic_report_context(signature)?;

        let result = self.invoke_indirect_with_panic_report_context(
            function_type,
            function,
            signature,
            semantic_arguments,
            name,
            Some(panic_report_context),
        )?;

        if self
            .pending_call_panic_report_context
            .replace(panic_report_context)
            .is_some()
        {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(result)
    }

    fn invoke_indirect_with_panic_report_context(
        &mut self,
        function_type: inkwell::types::FunctionType<'context>,
        function: PointerValue<'context>,
        signature: &CodegenCallableSignature,
        semantic_arguments: &[BasicValueEnum<'context>],
        name: &str,
        panic_report_context: Option<PointerValue<'context>>,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let mut arguments = Vec::new();

        if !call_argument_count_is_valid(signature, semantic_arguments.len()) {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let result_storage = self.prepare_call_arguments(
            signature,
            semantic_arguments,
            &mut arguments,
            panic_report_context,
        )?;

        let call =
            llvm(
                self.builder
                    .build_indirect_call(function_type, function, &arguments, name),
            )?;

        call.set_call_convention(crate::mapping::call_convention(
            signature,
            self.request.target(),
        )?);

        crate::mapping::apply_signature_call_attributes(call, signature, self.types)?;

        self.finish_call(call, signature, result_storage)
    }

    pub(super) fn prepare_call_arguments(
        &mut self,
        signature: &CodegenCallableSignature,
        semantic_arguments: &[BasicValueEnum<'context>],
        arguments: &mut Vec<BasicMetadataValueEnum<'context>>,
        panic_report_context: Option<PointerValue<'context>>,
    ) -> Result<Option<(PointerValue<'context>, bray_symbols::TypeId)>, CodegenFailure> {
        let result_storage = match signature.result() {
            CodegenResultMapping::Indirect {
                pointee, alignment, ..
            } => {
                let storage = self.aligned_alloca(*pointee, alignment.get(), "call.result")?;

                if signature.has_panic_report_context() {
                    llvm(
                        self.builder
                            .build_store(storage, self.types.map(*pointee)?.const_zero()),
                    )?;
                }

                arguments.push(storage.into());

                Some((storage, *pointee))
            }
            CodegenResultMapping::Void | CodegenResultMapping::Direct { .. } => None,
        };

        for (parameter, argument) in signature.parameters().iter().zip(semantic_arguments) {
            match parameter {
                CodegenParameterMapping::Ignore => {}
                CodegenParameterMapping::Direct { .. } => arguments.push((*argument).into()),
                CodegenParameterMapping::Indirect {
                    pointee, alignment, ..
                } => {
                    let storage =
                        self.aligned_alloca(*pointee, alignment.get(), "call.argument")?;

                    llvm(self.builder.build_store(storage, *argument))?;
                    arguments.push(storage.into());
                }
            }
        }

        for argument in semantic_arguments.iter().skip(signature.parameters().len()) {
            arguments.push((*argument).into());
        }

        match (signature.has_panic_report_context(), panic_report_context) {
            (true, Some(context)) => arguments.push(context.into()),
            (true, None) => {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }
            (false, Some(_)) => return Err(CodegenFailure::GeneratedModuleInvariant),
            (false, None) => {}
        }

        Ok(result_storage)
    }

    fn boundary_panic_report_context(
        &mut self,
        signature: &CodegenCallableSignature,
    ) -> Result<Option<PointerValue<'context>>, CodegenFailure> {
        if !signature.has_panic_report_context() {
            return Ok(None);
        }

        self.allocate_panic_report_context().map(Some)
    }

    fn checked_panic_report_context(
        &mut self,
        signature: &CodegenCallableSignature,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        if !signature.has_panic_report_context() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        match self.panic_report_context {
            Some(context) => Ok(context),
            None => self.allocate_panic_report_context(),
        }
    }

    pub(super) fn allocate_panic_report_context(
        &mut self,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let ty = crate::native::pointer_integer_type(self.types.context(), self.request.target());
        let context = self.allocate_temporary(ty, "call.panic.report.context")?;

        llvm(self.builder.build_store(context, ty.const_zero()))?;

        Ok(context)
    }

    fn propagate_boundary_call_panic(
        &mut self,
        context: PointerValue<'context>,
    ) -> Result<(), CodegenFailure> {
        let ty = crate::native::pointer_integer_type(self.types.context(), self.request.target());

        let (report, continued) = crate::translation::branch_on_pending_panic(
            self.types.context(),
            &self.builder,
            ty,
            context,
        )?;

        let runtime = bray_ir::MirRuntimeReference::new(
            bray_runtime_interface::RuntimeAbiRole::PanicPropagation,
            self.unit.target().runtime_abi(),
        );

        self.invoke_runtime(runtime, &[report.into()])?;
        llvm(self.builder.build_unreachable())?;

        self.builder.position_at_end(continued);

        Ok(())
    }

    pub(super) fn aligned_alloca(
        &mut self,
        pointee: bray_symbols::TypeId,
        alignment: u64,
        name: &str,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let pointee = self.types.map(pointee)?;
        let storage = self.allocate_temporary(pointee, name)?;

        let alignment = u32::try_from(alignment).map_err(|_| CodegenFailure::UnsupportedTarget)?;

        storage
            .as_instruction_value()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .set_alignment(alignment)
            .map_err(|_| CodegenFailure::UnsupportedTarget)?;

        Ok(storage)
    }

    pub(super) fn finish_call(
        &mut self,
        call: inkwell::values::CallSiteValue<'context>,
        signature: &CodegenCallableSignature,
        result_storage: Option<(PointerValue<'context>, bray_symbols::TypeId)>,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        match signature.result() {
            CodegenResultMapping::Void => Ok(None),
            CodegenResultMapping::Direct { .. } => call
                .try_as_basic_value()
                .basic()
                .map(Some)
                .ok_or(CodegenFailure::GeneratedModuleInvariant),
            CodegenResultMapping::Indirect { .. } => {
                let Some((storage, pointee)) = result_storage else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                llvm(self.builder.build_load(
                    self.types.map(pointee)?,
                    storage,
                    "call.result.value",
                ))
                .map(Some)
            }
        }
    }

    pub(super) fn operation_helpers(
        &self,
        operation: bray_ir::MirOperationId,
    ) -> Result<Vec<CodegenHelperMapping>, CodegenFailure> {
        if let Some(mapping) = self
            .request
            .mappings()
            .operation(self.instance.key(), operation)
        {
            return Ok(mapping.helpers().to_vec());
        }

        let has_helpers = self
            .unit
            .operation(operation)
            .map(|operation| !operation.kind().helper_references().is_empty())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        if has_helpers {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(Vec::new())
    }
}

fn call_argument_count_is_valid(signature: &CodegenCallableSignature, actual: usize) -> bool {
    let fixed = signature.parameters().len();

    if signature.is_variadic() {
        actual >= fixed
    } else {
        actual == fixed
    }
}
