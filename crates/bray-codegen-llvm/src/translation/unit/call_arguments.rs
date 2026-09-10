use std::collections::BTreeMap;

use bray_codegen::{CodegenFailure, CodegenHelperMapping};
use bray_ir::{MirCall, MirCallArgument};
use inkwell::values::{BasicValueEnum, PointerValue};

use super::core::UnitTranslator;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn evaluate_call_arguments(
        &mut self,
        call: &MirCall,
    ) -> Result<Vec<BasicValueEnum<'context>>, CodegenFailure> {
        let mut receiver = None;
        let mut parameters = BTreeMap::new();

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

        Ok(arguments)
    }

    pub(super) fn invoke_operation_helper(
        &mut self,
        operation: bray_ir::MirOperationId,
        helper: &CodegenHelperMapping,
        arguments: &[BasicValueEnum<'context>],
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        if let Some(context) = self.operation_panic_report_context(operation)? {
            self.invoke_helper_with_panic_report_context(helper, arguments, context)
        } else {
            self.invoke_helper(helper, arguments)
        }
    }

    pub(super) fn invoke_helper_with_panic_report_context(
        &mut self,
        helper: &CodegenHelperMapping,
        arguments: &[BasicValueEnum<'context>],
        context: PointerValue<'context>,
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

        // Owning the signature releases the immutable mapping borrow before invocation mutates
        // translation state.
        let signature = symbol.signature().clone();

        self.invoke_function_with_panic_report_context(
            function,
            &signature,
            arguments,
            "call.default",
            Some(context),
        )
    }
}
