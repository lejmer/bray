use std::collections::BTreeMap;

use bray_codegen::{CodegenFailure, CodegenHelperMapping};
use bray_ir::{MirCall, MirCallArgument, MirHelperReference};
use inkwell::IntPredicate;
use inkwell::basic_block::BasicBlock;
use inkwell::values::{BasicValueEnum, PointerValue};

use super::core::UnitTranslator;
use super::support::{llvm, next_helper};

pub(super) struct EvaluatedCallArguments<'context> {
    values: Vec<BasicValueEnum<'context>>,
    checked_defaults: Option<CheckedDefaultEvaluation<'context>>,
}

impl<'context> EvaluatedCallArguments<'context> {
    pub(super) fn values(&self) -> &[BasicValueEnum<'context>] {
        &self.values
    }

    pub(super) fn into_parts(
        self,
    ) -> (
        Vec<BasicValueEnum<'context>>,
        Option<CheckedDefaultEvaluation<'context>>,
    ) {
        (self.values, self.checked_defaults)
    }
}

pub(super) struct CheckedDefaultEvaluation<'context> {
    context: PointerValue<'context>,
    panicked: BasicBlock<'context>,
}

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn evaluate_call_arguments<'mapping>(
        &mut self,
        call: &MirCall,
        helpers: &mut impl Iterator<Item = &'mapping CodegenHelperMapping>,
        checked_default_context: Option<PointerValue<'context>>,
    ) -> Result<EvaluatedCallArguments<'context>, CodegenFailure> {
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

        let checked_defaults = checked_default_context.map(|context| CheckedDefaultEvaluation {
            context,
            panicked: self
                .types
                .context()
                .append_basic_block(self.function, "call.default.panicked"),
        });

        let receiver_storage = if defaults.is_empty() {
            None
        } else {
            receiver
                .map(|value| self.store_default_input(value))
                .transpose()?
        };

        let mut parameter_storage = BTreeMap::new();

        if !defaults.is_empty() {
            for (ordinal, value) in &parameters {
                parameter_storage.insert(*ordinal, self.store_default_input(*value)?);
            }
        }

        for (ordinal, provider) in defaults {
            let helper = next_helper(helpers, &MirHelperReference::CallableDefault(provider))?;

            let mut preceding =
                Vec::with_capacity(parameters.len() + usize::from(receiver.is_some()));

            preceding.extend(receiver_storage.map(BasicValueEnum::PointerValue));

            preceding.extend(
                parameter_storage
                    .range(..ordinal)
                    .map(|(_, pointer)| BasicValueEnum::PointerValue(*pointer)),
            );

            let value = match checked_defaults.as_ref() {
                Some(defaults) => {
                    let value = self.invoke_helper_with_panic_report_context(
                        helper,
                        &preceding,
                        defaults.context,
                    )?;

                    self.branch_on_checked_default(defaults)?;

                    value
                }
                None => self.invoke_helper(helper, &preceding)?,
            }
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            parameter_storage.insert(ordinal, self.store_default_input(value)?);

            if parameters.insert(ordinal, value).is_some() {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }
        }

        let mut arguments = Vec::with_capacity(parameters.len() + usize::from(receiver.is_some()));

        if let Some(value) = receiver {
            arguments.push(match receiver_storage {
                Some(pointer) => llvm(self.builder.build_load(
                    value.get_type(),
                    pointer,
                    "call.receiver.value",
                ))?,
                None => value,
            });
        }

        for (expected, (ordinal, value)) in (0_u32..).zip(parameters) {
            if ordinal != expected {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }

            arguments.push(match parameter_storage.get(&ordinal) {
                Some(pointer) => llvm(self.builder.build_load(
                    value.get_type(),
                    *pointer,
                    "call.argument.value",
                ))?,
                None => value,
            });
        }

        if arguments.len() != call.arguments().len() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(EvaluatedCallArguments {
            values: arguments,
            checked_defaults,
        })
    }

    fn store_default_input(
        &self,
        value: BasicValueEnum<'context>,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let storage = self.allocate_temporary(value.get_type(), "call.default.input")?;
        llvm(self.builder.build_store(storage, value))?;

        Ok(storage)
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

    pub(super) fn checked_call_panic_report_context(
        &mut self,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        match self.panic_report_context {
            Some(context) => Ok(context),
            None => self.allocate_panic_report_context(),
        }
    }

    fn branch_on_checked_default(
        &mut self,
        defaults: &CheckedDefaultEvaluation<'context>,
    ) -> Result<(), CodegenFailure> {
        let ty = crate::native::pointer_integer_type(self.types.context(), self.request.target());

        let report = llvm(self.builder.build_load(
            ty,
            defaults.context,
            "call.default.panic.report",
        ))?
        .into_int_value();

        let pending = llvm(self.builder.build_int_compare(
            IntPredicate::NE,
            report,
            ty.const_zero(),
            "call.default.panic.pending",
        ))?;

        let continued = self
            .types
            .context()
            .append_basic_block(self.function, "call.default.continue");

        llvm(
            self.builder
                .build_conditional_branch(pending, defaults.panicked, continued),
        )?;

        self.builder.position_at_end(continued);

        Ok(())
    }

    pub(super) fn finish_checked_default_evaluation(
        &mut self,
        defaults: CheckedDefaultEvaluation<'context>,
        result: Option<BasicValueEnum<'context>>,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let completed = self
            .builder
            .get_insert_block()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let finished = self
            .types
            .context()
            .append_basic_block(self.function, "call.default.finished");

        llvm(self.builder.build_unconditional_branch(finished))?;

        self.builder.position_at_end(defaults.panicked);
        llvm(self.builder.build_unconditional_branch(finished))?;

        self.builder.position_at_end(finished);

        if self
            .pending_call_panic_report_context
            .replace(defaults.context)
            .is_some()
        {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let Some(result) = result else {
            return Ok(None);
        };

        let fallback = result.get_type().const_zero();

        let phi = llvm(
            self.builder
                .build_phi(result.get_type(), "call.default.result"),
        )?;

        phi.add_incoming(&[(&result, completed), (&fallback, defaults.panicked)]);

        Ok(Some(phi.as_basic_value()))
    }
}
