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

        let default_helper = match call.target() {
            MirCallTarget::DefaultValue { provider, .. } => {
                Some(super::super::support::next_helper(
                    &mut helpers,
                    &bray_ir::MirHelperReference::DefaultValue(*provider),
                ))
            }
            _ => None,
        };

        if helpers.next().is_some() {
            panic!("checked MIR call translation violated an established compiler contract");
        }

        let outgoing = if call.is_cleanup() {
            self.activate_outgoing_call(operation)?
        } else {
            None
        };

        let result = match call.target() {
            MirCallTarget::Direct(_) => {
                let mapping = self
                    .request
                    .mappings()
                    .callable(self.instance.key(), CodegenCallSite::Operation(operation))
                    .expect(
                        "checked MIR call translation requires an established mapping or value",
                    );

                if let Some(intrinsic) = mapping.intrinsic_operation() {
                    let operand_type = call
                        .arguments()
                        .first()
                        .map(MirCallArgument::value)
                        .map(|operand| self.operand_type(operand))
                        .expect(
                            "checked MIR call translation requires an established mapping or value",
                        );

                    let result = self.translate_intrinsic_call(
                        intrinsic,
                        operand_type,
                        call.result().ty(),
                        semantic_arguments,
                    )?;

                    if checks_call_panic {
                        let context = self.checked_call_panic_report_context()?;

                        self.set_pending_call_context(context)?;
                    }

                    if let Some(record) = outgoing {
                        self.retire_outgoing_call(record)?;
                    }

                    return Ok(Some(result));
                }

                let instance = mapping.instance().expect(
                    "checked MIR call translation requires an established mapping or value",
                );

                let symbol = self.request.mappings().instance_symbol(instance).expect(
                    "checked MIR call translation requires an established mapping or value",
                );

                let function = self.module.get_function(symbol.name().as_str()).expect(
                    "checked MIR call translation requires an established mapping or value",
                );

                // Owning the signature releases the immutable mapping borrow before invocation
                // mutates translation state.
                let signature = symbol.signature().clone();

                if call.may_propagate_panic() && checks_call_panic {
                    self.invoke_checked_function(function, &signature, semantic_arguments, "call")
                } else {
                    self.invoke_function(function, &signature, semantic_arguments, "call")
                }
            }
            MirCallTarget::DefaultValue { .. } => {
                let helper = default_helper.expect(
                    "checked MIR call translation requires an established mapping or value",
                );

                self.invoke_operation_helper(operation, helper, semantic_arguments)
            }
            MirCallTarget::Runtime(runtime) => self.invoke_runtime(*runtime, semantic_arguments),
            MirCallTarget::Indirect { callee, .. } => {
                let callee_type = self.operand_type(callee);

                let mapping = self.type_mapping(callee_type).expect(
                    "checked MIR call translation requires an established mapping or value",
                );

                let CodegenTypeKind::Callable(signature) = mapping.kind() else {
                    panic!(
                        "checked MIR call translation violated an established compiler contract"
                    );
                };

                // Translation mutates its value cache after releasing the borrowed mapping.
                let signature = signature.clone();

                let pointer = pointer_value(self.operand(callee)?).expect(
                    "checked MIR call translation requires an established mapping or value",
                );

                let function_type = self.types.function_type(&signature)?;

                if call.may_propagate_panic() && checks_call_panic {
                    self.invoke_checked_indirect(
                        function_type,
                        pointer,
                        &signature,
                        semantic_arguments,
                        "call.indirect",
                    )
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

        if let Some(record) = outgoing {
            self.retire_outgoing_call(record)?;
        }

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
            .expect("checked MIR call translation requires an established mapping or value");

        let mapping = self
            .type_mapping(ty)
            .expect("checked MIR call translation requires an established mapping or value");

        if mapping.layout().is_none_or(|layout| layout.size() != 0) {
            panic!("checked MIR call translation violated an established compiler contract");
        }

        let ty = self.types.map(ty)?;

        Ok(Some(ty.const_zero()))
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
            .expect("checked MIR call translation requires an established mapping or value");

        if has_helpers {
            panic!("checked MIR call translation violated an established compiler contract");
        }

        Ok(Vec::new())
    }
}
