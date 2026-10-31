use super::super::core::UnitTranslator;
use super::super::support::pointer_value;
use bray_codegen::{
    CodegenCallSite, CodegenCallableSignature, CodegenFailure, CodegenHelperMapping,
    CodegenTypeKind,
};
use bray_ir::{MirCall, MirCallArgument, MirCallTarget};
use inkwell::values::{BasicValueEnum, FunctionValue};

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
            MirCallTarget::Lifecycle { role, ty } => Some(super::super::support::next_helper(
                &mut helpers,
                &bray_ir::MirHelperReference::LifecycleFrame {
                    role: *role,
                    ty: *ty,
                },
            )),
            MirCallTarget::Predicate(reference) => Some(super::super::support::next_helper(
                &mut helpers,
                &bray_ir::MirHelperReference::Predicate(reference.clone()),
            )),
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

        // An async cleanup call constructs its already-admitted inactive frame.
        // Its lifecycle starts when the frame is activated, not at construction.
        let outgoing = if call.is_cleanup()
            && matches!(
                call.result(),
                bray_bound_tree::BoundCallResult::Immediate(_)
            ) {
            self.activate_outgoing_call(operation)?
        } else {
            None
        };

        let retains_call_outcome = checks_call_panic || outgoing.is_some();

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

                    if retains_call_outcome {
                        let context = self.checked_call_panic_report_context()?;

                        self.set_pending_call_context(context)?;
                    }

                    if let Some(record) = outgoing {
                        self.retire_outgoing_call(record, checks_call_panic)?;
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

                self.invoke_call_function(
                    call,
                    retains_call_outcome,
                    function,
                    &signature,
                    semantic_arguments,
                )
            }
            MirCallTarget::DefaultValue { .. }
            | MirCallTarget::Predicate(_)
            | MirCallTarget::Lifecycle { .. } => {
                let helper = default_helper.expect(
                    "checked MIR call translation requires an established mapping or value",
                );

                if let Some(key) = helper.symbol() {
                    let symbol = self.request.mappings().symbol(key).expect(
                        "checked MIR call translation requires an established mapping or value",
                    );

                    let function = self.module.get_function(symbol.name().as_str()).expect(
                        "checked MIR call translation requires an established mapping or value",
                    );

                    let signature = symbol.signature().clone();

                    self.invoke_call_function(
                        call,
                        retains_call_outcome,
                        function,
                        &signature,
                        semantic_arguments,
                    )
                } else {
                    self.invoke_operation_helper(operation, helper, semantic_arguments)
                }
            }
            MirCallTarget::Runtime(runtime) => self.invoke_runtime(*runtime, semantic_arguments),
            MirCallTarget::Indirect { callee, .. } => {
                self.invoke_call_indirect(call, retains_call_outcome, callee, semantic_arguments)
            }
        }?;

        if let Some(record) = outgoing {
            self.retire_outgoing_call(record, checks_call_panic)?;
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

        self.materialize_call_result(None, ty).map(Some)
    }

    fn invoke_call_function(
        &mut self,
        call: &MirCall,
        retains_call_outcome: bool,
        function: FunctionValue<'context>,
        signature: &CodegenCallableSignature,
        arguments: &[BasicValueEnum<'context>],
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        if retains_call_outcome && signature.has_panic_report_context() {
            self.invoke_checked_function(function, signature, arguments, "call")
        } else if retains_call_outcome {
            let context = self.checked_call_panic_report_context()?;

            self.set_pending_call_context(context)?;

            self.invoke_function(function, signature, arguments, "call")
        } else if call.has_non_unwinding_invocation() {
            let context = signature
                .has_panic_report_context()
                .then(|| self.checked_call_panic_report_context())
                .transpose()?;

            self.invoke_function_with_panic_report_context(
                function, signature, arguments, "call", context,
            )
        } else {
            self.invoke_function(function, signature, arguments, "call")
        }
    }

    fn invoke_call_indirect(
        &mut self,
        call: &MirCall,
        retains_call_outcome: bool,
        callee: &bray_ir::MirOperand,
        semantic_arguments: &[BasicValueEnum<'context>],
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let callee_type = self.operand_type(callee);

        let mapping = self
            .type_mapping(callee_type)
            .expect("checked MIR call translation requires an established mapping or value");

        let CodegenTypeKind::Callable(signature) = mapping.kind() else {
            panic!("checked MIR call translation violated an established compiler contract");
        };

        // Translation mutates its value cache after releasing the borrowed mapping.
        let signature = signature.clone();

        let pointer = pointer_value(self.operand(callee)?)
            .expect("checked MIR call translation requires an established mapping or value");

        let pointer = if matches!(
            call.result(),
            bray_bound_tree::BoundCallResult::LazyFuture(_)
        ) {
            crate::mapping::frame::factory_field(self.types.context(), &self.builder, pointer, 0)?
        } else {
            pointer
        };

        let function_type = self.types.function_type(&signature)?;

        if retains_call_outcome && signature.has_panic_report_context() {
            self.invoke_checked_indirect(
                function_type,
                pointer,
                &signature,
                semantic_arguments,
                "call.indirect",
            )
        } else if retains_call_outcome {
            let context = self.checked_call_panic_report_context()?;

            self.set_pending_call_context(context)?;

            self.invoke_indirect(
                function_type,
                pointer,
                &signature,
                semantic_arguments,
                "call.indirect",
            )
        } else if call.has_non_unwinding_invocation() {
            let context = signature
                .has_panic_report_context()
                .then(|| self.checked_call_panic_report_context())
                .transpose()?;

            self.invoke_indirect_with_panic_report_context(
                function_type,
                pointer,
                &signature,
                semantic_arguments,
                "call.indirect",
                context,
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

    pub(in crate::translation::unit) fn materialize_call_result(
        &mut self,
        value: Option<BasicValueEnum<'context>>,
        ty: bray_symbols::TypeId,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        if let Some(value) = value {
            return Ok(value);
        }

        let mapping = self
            .type_mapping(ty)
            .expect("call result retains its semantic type mapping");

        assert!(
            mapping.layout().is_some_and(|layout| layout.size() == 0),
            "an erased call result must have zero-sized representation: {ty:?}"
        );

        Ok(self.types.map(ty)?.const_zero())
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
