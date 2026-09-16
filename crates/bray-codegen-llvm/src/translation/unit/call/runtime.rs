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
