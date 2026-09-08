use super::super::core::UnitTranslator;
use super::super::support::parameter_type;
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
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        if runtime.role().native_signature().is_some() {
            let result = self.invoke_native_runtime(runtime, arguments)?;

            return if matches!(symbol.signature().result(), CodegenResultMapping::Void) {
                Ok(None)
            } else {
                result
                    .map(Some)
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)
            };
        }

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
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let native_arguments = arguments
            .iter()
            .copied()
            .map(Into::into)
            .collect::<Vec<_>>();

        crate::native::invoke_function(
            self.types.context(),
            &self.builder,
            self.request.target(),
            &key,
            function,
            &native_arguments,
            runtime.role().as_str(),
        )
    }

    pub(in crate::translation::unit) fn runtime_integer_argument(
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
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let symbol = self
            .request
            .mappings()
            .symbol(key)
            .ok_or_else(|| CodegenFailure::generated_module_invariant(key))?;

        let ty = parameter_type(symbol.signature(), parameter)?;

        let BasicTypeEnum::IntType(ty) = self.types.map(ty)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        Ok(ty.const_int(value, false).into())
    }

    pub(in crate::translation::unit) fn value_cleanup_descriptor(
        &mut self,
        helper: &CodegenHelperMapping,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let Some(symbol) = self.helper_symbol(helper)? else {
            return Ok(self
                .types
                .context()
                .ptr_type(inkwell::AddressSpace::default())
                .const_null()
                .into());
        };

        let value = crate::mapping::value_cleanup_descriptor(self.module, symbol, self.types)?;
        let name = format!("{}.value_cleanup.descriptor", symbol.name().as_str());

        let descriptor = crate::mapping::publish_immutable_global(
            self.module,
            self.types.target(),
            &name,
            value.into(),
        );

        Ok(descriptor.as_pointer_value().into())
    }

    pub(in crate::translation::unit) fn task_terminal_cleanup_descriptor(
        &mut self,
        helper: &CodegenHelperMapping,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let name = self.helper_symbol(helper)?.map_or_else(
            || "bray.trivial_task_terminal_cleanup".to_owned(),
            |symbol| format!("{}.task_terminal_cleanup", symbol.name().as_str()),
        );

        let value = self.value_cleanup_descriptor(helper)?;

        let value =
            crate::mapping::task_terminal_cleanup_descriptor(self.module, value, self.types)?;

        let descriptor = crate::mapping::publish_immutable_global(
            self.module,
            self.types.target(),
            &name,
            value.into(),
        );

        Ok(descriptor.as_pointer_value().into())
    }

    pub(in crate::translation::unit) fn helper_address(
        &self,
        helper: &CodegenHelperMapping,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let Some(symbol) = self.helper_symbol(helper)? else {
            return Ok(None);
        };

        let function = self
            .module
            .get_function(symbol.callable_address_name().as_str())
            .ok_or_else(|| {
                CodegenFailure::generated_module_invariant((
                    symbol.key(),
                    symbol.callable_address_name(),
                ))
            })?;

        Ok(Some(function.as_global_value().as_pointer_value().into()))
    }

    fn helper_symbol(
        &self,
        helper: &CodegenHelperMapping,
    ) -> Result<Option<&'request bray_codegen::CodegenSymbolMapping>, CodegenFailure> {
        helper
            .symbol()
            .map(|key| {
                self.request
                    .mappings()
                    .symbol(key)
                    .ok_or_else(|| CodegenFailure::generated_module_invariant(key))
            })
            .transpose()
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

    pub(in crate::translation::unit) fn terminal_state_arguments(
        &mut self,
        runtime: bray_ir::MirRuntimeReference,
        state: &MirTaskTerminalState,
    ) -> Result<Vec<BasicValueEnum<'context>>, CodegenFailure> {
        let tag = match state {
            MirTaskTerminalState::CapturesCompleted => {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }
            MirTaskTerminalState::Completed(_) => 0,
            MirTaskTerminalState::Cancelled => 1,
            MirTaskTerminalState::Panicked(_) => 2,
        };

        let mut arguments = vec![self.runtime_integer_argument(runtime, 0, tag)?];

        match state {
            MirTaskTerminalState::Completed(value) | MirTaskTerminalState::Panicked(value) => {
                arguments.push(self.operand(value)?);
            }
            MirTaskTerminalState::Cancelled | MirTaskTerminalState::CapturesCompleted => {}
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
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        if let CodegenSymbolKey::Runtime(runtime) = key {
            return self.invoke_runtime(*runtime, arguments);
        }

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
}
