use std::collections::BTreeMap;

use super::core::UnitTranslator;
use super::support::{llvm, next_helper, parameter_type, pointer_value};
use bray_codegen::{
    CodegenCallSite, CodegenCallableSignature, CodegenFailure, CodegenHelperMapping,
    CodegenParameterMapping, CodegenResultMapping, CodegenSymbolKey, CodegenTypeKind,
};
use bray_ir::{MirCall, MirCallArgument, MirCallTarget, MirHelperReference, MirTaskTerminalState};
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
        let helpers = self.operation_helpers(operation)?;
        let mut helpers = helpers.iter();
        let semantic_arguments = self.evaluate_call_arguments(call, &mut helpers)?;

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
                        .and_then(bray_ir::MirCallArgument::value)
                        .map(|operand| self.operand_type(operand))
                        .transpose()?
                        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                    return self
                        .translate_intrinsic_call(intrinsic, operand_type, &semantic_arguments)
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

                self.invoke_function(function, &signature, &semantic_arguments, "call")
            }
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

                self.invoke_indirect(
                    function_type,
                    pointer,
                    &signature,
                    &semantic_arguments,
                    "call.indirect",
                )
            }
        }?;

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

    pub(super) fn evaluate_call_arguments<'mapping>(
        &mut self,
        call: &MirCall,
        helpers: &mut impl Iterator<Item = &'mapping CodegenHelperMapping>,
    ) -> Result<Vec<BasicValueEnum<'context>>, CodegenFailure> {
        let mut receiver = None;
        let mut parameters = BTreeMap::new();
        let mut defaults = Vec::new();

        for argument in call.arguments() {
            match argument {
                MirCallArgument::Receiver { value, .. } => {
                    let value = self.operand(value)?;

                    if receiver.replace(value).is_some() {
                        return Err(CodegenFailure::GeneratedModuleInvariant);
                    }
                }
                MirCallArgument::Explicit { ordinal, value, .. } => {
                    let value = self.operand(value)?;

                    if parameters.insert(*ordinal, value).is_some() {
                        return Err(CodegenFailure::GeneratedModuleInvariant);
                    }
                }
                MirCallArgument::Default {
                    ordinal, provider, ..
                } => defaults.push((*ordinal, *provider)),
            }
        }

        for (ordinal, provider) in defaults {
            let helper = next_helper(helpers, &MirHelperReference::CallableDefault(provider))?;

            let mut preceding =
                Vec::with_capacity(parameters.len() + usize::from(receiver.is_some()));

            preceding.extend(receiver);

            preceding.extend(parameters.range(..ordinal).map(|(_, value)| *value));

            let value = self
                .invoke_helper(helper, &preceding)?
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            if parameters.insert(ordinal, value).is_some() {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }
        }

        let mut arguments = Vec::with_capacity(parameters.len() + usize::from(receiver.is_some()));

        arguments.extend(receiver);

        for (expected, (ordinal, value)) in (0_u32..).zip(parameters) {
            if ordinal != expected {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }

            arguments.push(value);
        }

        if arguments.len() != call.arguments().len() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
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
            .get_function(symbol.name().as_str())
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
        let mut arguments = Vec::new();

        if semantic_arguments.len() != signature.parameters().len() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let result_storage =
            self.prepare_call_arguments(signature, semantic_arguments, &mut arguments)?;

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
    ) -> Result<Option<(PointerValue<'context>, bray_symbols::TypeId)>, CodegenFailure> {
        let result_storage = match signature.result() {
            CodegenResultMapping::Indirect {
                pointee, alignment, ..
            } => {
                let storage = self.aligned_alloca(*pointee, alignment.get(), "call.result")?;

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

        Ok(result_storage)
    }

    pub(super) fn aligned_alloca(
        &mut self,
        pointee: bray_symbols::TypeId,
        alignment: u64,
        name: &str,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let storage = llvm(self.builder.build_alloca(self.types.map(pointee)?, name))?;

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
