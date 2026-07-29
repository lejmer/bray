use crate::mapping::type_attribute;
use super::core::UnitTranslator;
use super::support::{llvm, next_helper, parameter_type, pointer_value};
use bray_codegen::{
    CodegenCallableSignature, CodegenFailure, CodegenHelperMapping,
    CodegenIndirectParameterKind, CodegenParameterMapping, CodegenResultMapping,
    CodegenSymbolKey, CodegenTypeKind,
};
use bray_ir::{MirCall, MirCallArgument, MirCallTarget, MirHelperReference, MirTaskTerminalState};
use inkwell::attributes::AttributeLoc;
use inkwell::types::BasicTypeEnum;
use inkwell::values::{
    BasicMetadataValueEnum, BasicValue, BasicValueEnum, CallSiteValue, FunctionValue,
    PointerValue,
};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_call(
        &mut self,
        operation: bray_ir::MirOperationId,
        call: &MirCall,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let helpers = self.operation_helpers(operation)?;
        let mut helpers = helpers.iter();
        let semantic_arguments = self.evaluate_call_arguments(call, &mut helpers)?;

        if helpers.next().is_some() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let (function, signature) = match call.target() {
            MirCallTarget::Direct(reference) => {
                let mapping = self
                    .request
                    .mappings()
                    .callable(self.instance.key(), *reference)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let symbol = self
                    .request
                    .mappings()
                    .instance_symbol(mapping.instance())
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let function = self
                    .module
                    .get_function(symbol.name().as_str())
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                (function, symbol.signature())
            }
            MirCallTarget::Indirect { callee, .. } => {
                let callee_type = self.operand_type(callee)?;

                let mapping = self
                    .request
                    .mappings()
                    .ty(callee_type)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let CodegenTypeKind::Callable(signature) = mapping.kind() else {
                    return Err(CodegenFailure::GeneratedModuleInvariant);
                };

                // Translation mutates its value cache after releasing the borrowed mapping.
                let signature = signature.clone();

                let pointer = pointer_value(self.operand(callee)?)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                let function_type = self.types.function_type(&signature)?;

                return self.invoke_indirect(
                    function_type,
                    pointer,
                    &signature,
                    &semantic_arguments,
                    "call.indirect",
                );
            }
        };

        self.invoke_function(function, signature, &semantic_arguments, "call")
    }

    pub(super) fn evaluate_call_arguments<'mapping>(
        &mut self,
        call: &MirCall,
        helpers: &mut impl Iterator<Item = &'mapping CodegenHelperMapping>,
    ) -> Result<Vec<BasicValueEnum<'context>>, CodegenFailure> {
        let mut arguments = Vec::with_capacity(call.arguments().len());

        for argument in call.arguments() {
            let value = match argument {
                MirCallArgument::Receiver { value, .. }
                | MirCallArgument::Explicit { value, .. } => self.operand(value)?,
                MirCallArgument::Default { provider, .. } => {
                    let helper =
                        next_helper(helpers, &MirHelperReference::CallableDefault(*provider))?;

                    self.invoke_helper(helper, &arguments)?
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?
                }
            };

            arguments.push(value);
        }

        Ok(arguments)
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
        let key = CodegenSymbolKey::Runtime(runtime);

        let symbol = self
            .request
            .mappings()
            .symbol(&key)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let ty = parameter_type(symbol.signature(), parameter)?;

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
        let symbol = self
            .request
            .mappings()
            .symbol(helper.symbol())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let ty = parameter_type(symbol.signature(), parameter)?;

        let BasicTypeEnum::IntType(ty) = self.types.map(ty)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(ty.const_int(u64::from(value), false).into())
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
        let symbol = self
            .request
            .mappings()
            .symbol(helper.symbol())
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
        let mut arguments = Vec::new();

        if semantic_arguments.len() != signature.parameters().len() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let result_storage =
            self.prepare_call_arguments(signature, semantic_arguments, &mut arguments)?;

        let call = llvm(self.builder.build_call(function, &arguments, name))?;

        self.apply_call_attributes(call, signature)?;

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
        let mut arguments = Vec::new();

        if semantic_arguments.len() != signature.parameters().len() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let result_storage =
            self.prepare_call_arguments(signature, semantic_arguments, &mut arguments)?;

        let call =
            llvm(
                self.builder
                    .build_indirect_call(function_type, function, &arguments, name),
            )?;

        self.apply_call_attributes(call, signature)?;

        self.finish_call(call, signature, result_storage)
    }

    pub(super) fn prepare_call_arguments(
        &mut self,
        signature: &CodegenCallableSignature,
        semantic_arguments: &[BasicValueEnum<'context>],
        arguments: &mut Vec<BasicMetadataValueEnum<'context>>,
    ) -> Result<Option<(PointerValue<'context>, bray_symbols::TypeId)>, CodegenFailure> {
        let result_storage = match signature.result() {
            CodegenResultMapping::Indirect {
                pointee,
                alignment,
                ..
            } => {
                let storage =
                    self.aligned_alloca(*pointee, alignment.get(), "call.result")?;

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
                    pointee,
                    alignment,
                    ..
                } => {
                    let storage =
                        self.aligned_alloca(*pointee, alignment.get(), "call.argument")?;

                    llvm(self.builder.build_store(storage, *argument))?;
                    arguments.push(storage.into());
                }
            }
        }

        Ok(result_storage)
    }

    fn aligned_alloca(
        &mut self,
        pointee: bray_symbols::TypeId,
        alignment: u64,
        name: &str,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let storage = llvm(self.builder.build_alloca(self.types.map(pointee)?, name))?;

        let alignment =
            u32::try_from(alignment).map_err(|_| CodegenFailure::UnsupportedTarget)?;

        storage
            .as_instruction_value()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .set_alignment(alignment)
            .map_err(|_| CodegenFailure::UnsupportedTarget)?;

        Ok(storage)
    }

    fn apply_call_attributes(
        &mut self,
        call: CallSiteValue<'context>,
        signature: &CodegenCallableSignature,
    ) -> Result<(), CodegenFailure> {
        let mut parameter_index = 0_u32;

        if let CodegenResultMapping::Indirect {
            pointee,
            alignment,
            ..
        } = signature.result()
        {
            self.apply_call_type_attribute(call, parameter_index, "sret", *pointee)?;
            self.apply_call_alignment(call, parameter_index, alignment.get())?;
            parameter_index = 1;
        }

        for parameter in signature.parameters() {
            let CodegenParameterMapping::Indirect {
                pointee,
                kind,
                alignment,
                ..
            } = parameter
            else {
                if !matches!(parameter, CodegenParameterMapping::Ignore) {
                    parameter_index = parameter_index
                        .checked_add(1)
                        .ok_or(CodegenFailure::UnsupportedTarget)?;
                }

                continue;
            };

            if *kind == CodegenIndirectParameterKind::ByValue {
                self.apply_call_type_attribute(
                    call,
                    parameter_index,
                    "byval",
                    *pointee,
                )?;
            }

            self.apply_call_alignment(call, parameter_index, alignment.get())?;

            parameter_index = parameter_index
                .checked_add(1)
                .ok_or(CodegenFailure::UnsupportedTarget)?;
        }

        Ok(())
    }

    fn apply_call_alignment(
        &self,
        call: CallSiteValue<'context>,
        parameter: u32,
        alignment: u64,
    ) -> Result<(), CodegenFailure> {
        let alignment =
            u32::try_from(alignment).map_err(|_| CodegenFailure::UnsupportedTarget)?;

        call.set_alignment_attribute(AttributeLoc::Param(parameter), alignment);

        Ok(())
    }

    fn apply_call_type_attribute(
        &mut self,
        call: CallSiteValue<'context>,
        parameter: u32,
        name: &str,
        pointee: bray_symbols::TypeId,
    ) -> Result<(), CodegenFailure> {
        call.add_attribute(
            AttributeLoc::Param(parameter),
            type_attribute(name, pointee, self.types)?,
        );

        Ok(())
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
        self.request
            .mappings()
            .operation(self.instance.key(), operation)
            .map(|mapping| mapping.helpers().to_vec())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)
    }
}
