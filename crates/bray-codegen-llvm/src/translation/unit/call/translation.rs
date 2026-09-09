use super::super::core::UnitTranslator;
use super::super::support::pointer_value;
use bray_codegen::{CodegenCallSite, CodegenFailure, CodegenHelperMapping, CodegenTypeKind};
use bray_ir::{MirCall, MirCallArgument, MirCallTarget};
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(in crate::translation::unit) fn translate_call(
        &mut self,
        operation: bray_ir::MirOperationId,
        call: &MirCall,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let checks_call_panic = self.checked_call_operations.contains(&operation);

        let helpers = self.operation_helpers(operation)?;
        let mut helpers = helpers.iter();
        let semantic_arguments = self.evaluate_call_arguments(call)?;
        let semantic_arguments = semantic_arguments.as_slice();

        let default_helper = if let MirCallTarget::ParameterDefault { provider, .. } = call.target()
        {
            Some(super::super::support::next_helper(
                &mut helpers,
                &bray_ir::MirHelperReference::CallableDefault(*provider),
            )?)
        } else {
            None
        };

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
                        .map(MirCallArgument::value)
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
                    self.invoke_checked_function(function, &signature, semantic_arguments, "call")
                } else {
                    self.invoke_function(function, &signature, semantic_arguments, "call")
                }
            }
            MirCallTarget::ParameterDefault { .. } => {
                let helper = default_helper.ok_or(CodegenFailure::GeneratedModuleInvariant)?;

                if checks_call_panic {
                    let context = self.checked_call_panic_report_context()?;
                    self.retain_checked_call_context(context)?;

                    self.invoke_helper_with_panic_report_context(
                        helper,
                        semantic_arguments,
                        context,
                    )
                } else {
                    self.invoke_helper(helper, semantic_arguments)
                }
            }
            MirCallTarget::Runtime(runtime) => self.invoke_runtime(*runtime, semantic_arguments),
            MirCallTarget::Indirect { callee, .. } => self.invoke_indirect_callable(
                callee,
                semantic_arguments,
                call.may_propagate_panic() && checks_call_panic,
            ),
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

    pub(in crate::translation::unit) fn invoke_indirect_callable(
        &mut self,
        callee: &bray_ir::MirOperand,
        arguments: &[BasicValueEnum<'context>],
        checked: bool,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let callee_type = self.operand_type(callee)?;

        let mapping = self
            .type_mapping(callee_type)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Callable(signature) = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        // Translation mutates its value cache after releasing the borrowed mapping.
        let signature = signature.clone();

        let pointer =
            pointer_value(self.operand(callee)?).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let function_type = self.types.function_type(&signature)?;

        if checked {
            self.invoke_checked_indirect(
                function_type,
                pointer,
                &signature,
                arguments,
                "call.indirect",
            )
        } else {
            self.invoke_indirect(
                function_type,
                pointer,
                &signature,
                arguments,
                "call.indirect",
            )
        }
    }

    pub(in crate::translation::unit) fn operation_helpers(
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
