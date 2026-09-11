use super::super::core::UnitTranslator;
use super::super::support::llvm;
use bray_codegen::{
    CodegenCallableSignature, CodegenFailure, CodegenParameterMapping, CodegenResultMapping,
};
use inkwell::values::{BasicMetadataValueEnum, BasicValueEnum, FunctionValue, PointerValue};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(in crate::translation::unit) fn invoke_function(
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

    pub(super) fn invoke_checked_function(
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

    pub(in crate::translation::unit) fn invoke_function_with_panic_report_context(
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

    pub(in crate::translation::unit) fn invoke_indirect(
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

    pub(super) fn invoke_checked_indirect(
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

    pub(super) fn invoke_indirect_with_panic_report_context(
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

    pub(in crate::translation::unit) fn prepare_call_arguments(
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

    pub(in crate::translation::unit) fn finish_call(
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
}

fn call_argument_count_is_valid(signature: &CodegenCallableSignature, actual: usize) -> bool {
    let fixed = signature.parameters().len();

    if signature.is_variadic() {
        actual >= fixed
    } else {
        actual == fixed
    }
}
