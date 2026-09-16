use super::core::UnitTranslator;
use super::support::llvm;
use bray_codegen::{CodegenFailure, CodegenHelperMapping};
use bray_ir::{MirGeneratorKind, MirGeneratorOperation, MirHelperReference, MirPlace};
use inkwell::values::BasicValueEnum;

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_generator(
        &mut self,
        operation: bray_ir::MirOperationId,
        generator: &MirGeneratorOperation,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let helpers = self.operation_helpers(operation)?;

        match generator {
            MirGeneratorOperation::Begin {
                kind,
                destination,
                element,
                exact_count,
            } => {
                self.translate_generator_begin(&helpers, *kind, destination, *element, *exact_count)
            }
            MirGeneratorOperation::Push { destination, value } => {
                self.translate_generator_push(&helpers, destination, value)
            }
            MirGeneratorOperation::Finish { destination } => {
                self.translate_generator_finish(&helpers, destination)
            }
            MirGeneratorOperation::CleanupBroadcast {
                destination,
                element,
                runtime,
            } => self.translate_generator_cleanup(&helpers, destination, *element, *runtime),
            MirGeneratorOperation::Destroy {
                destination,
                element,
                runtime,
            } => self.translate_generator_destroy(&helpers, destination, *element, *runtime),
        }
    }

    fn translate_generator_begin(
        &mut self,
        helpers: &[CodegenHelperMapping],
        kind: MirGeneratorKind,
        destination: &MirPlace,
        element: bray_symbols::TypeId,
        exact_count: Option<bray_symbols::ConstantTermId>,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let [helper] = helpers else {
            panic!("checked MIR translation violated an established compiler contract");
        };

        if helper.reference() != &MirHelperReference::BeginGenerator {
            panic!("checked MIR translation violated an established compiler contract");
        }

        let layout = self
            .type_mapping(element)
            .expect("checked MIR translation requires an established mapping or value")
            .layout()
            .expect("checked MIR translation requires an established mapping or value");

        let kind = match kind {
            MirGeneratorKind::Array => 0,
            MirGeneratorKind::General => 1,
        };

        let mut arguments = vec![
            self.place(destination)?.into(),
            self.helper_integer_argument(helper, 1, kind)?,
            self.helper_integer_argument(helper, 2, layout.size())?,
            self.helper_integer_argument(helper, 3, layout.alignment().get())?,
        ];

        if let Some(term) = exact_count {
            let value = self
                .request
                .mappings()
                .constant_term(self.instance.key(), term)
                .expect("checked MIR translation requires an established mapping or value");

            arguments.push(self.constant(value)?);
        } else {
            arguments.push(self.helper_integer_argument(helper, 4, 0)?);
        }

        arguments.push(self.helper_boolean_argument(helper, 5, exact_count.is_some())?);

        if self.invoke_helper(helper, &arguments)?.is_some() {
            panic!("checked MIR translation violated an established compiler contract");
        }

        Ok(None)
    }

    fn translate_generator_push(
        &mut self,
        helpers: &[CodegenHelperMapping],
        destination: &MirPlace,
        value: &bray_ir::MirOperand,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let [helper] = helpers else {
            panic!("checked MIR translation violated an established compiler contract");
        };

        if helper.reference() != &MirHelperReference::PushGenerator {
            panic!("checked MIR translation violated an established compiler contract");
        }

        let element = self.operand_type(value);

        let layout = self
            .type_mapping(element)
            .expect("checked MIR translation requires an established mapping or value")
            .layout()
            .expect("checked MIR translation requires an established mapping or value");

        let storage =
            self.aligned_alloca(element, layout.alignment().get(), "generator.element")?;

        let value = self.operand(value)?;

        llvm(self.builder.build_store(storage, value))?;

        let destination = self.place(destination)?.into();

        if self
            .invoke_helper(helper, &[destination, storage.into()])?
            .is_some()
        {
            panic!("checked MIR translation violated an established compiler contract");
        }

        Ok(None)
    }

    fn translate_generator_finish(
        &mut self,
        helpers: &[CodegenHelperMapping],
        destination: &MirPlace,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let [helper] = helpers else {
            panic!("checked MIR translation violated an established compiler contract");
        };

        if helper.reference() != &MirHelperReference::FinishGenerator {
            panic!("checked MIR translation violated an established compiler contract");
        }

        let destination_pointer = self.place(destination)?;

        if self
            .invoke_helper(helper, &[destination_pointer.into()])?
            .is_some()
        {
            panic!("checked MIR translation violated an established compiler contract");
        }

        let value = llvm(self.builder.build_load(
            self.types.map(destination.ty())?,
            destination_pointer,
            "generator.result",
        ))?;

        Ok(Some(value))
    }

    fn translate_generator_cleanup(
        &mut self,
        helpers: &[CodegenHelperMapping],
        destination: &MirPlace,
        element: bray_symbols::TypeId,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let [helper] = helpers else {
            panic!("checked MIR translation violated an established compiler contract");
        };

        let expected = MirHelperReference::Cleanup {
            phase: bray_ir::MirCleanupPhase::TaskCancellation,
            ty: element,
        };

        if helper.reference() != &expected {
            panic!("checked MIR translation violated an established compiler contract");
        }

        let callback = self.generator_callback_argument(helper, runtime, 1)?;
        let destination = self.place(destination)?.into();

        if self
            .invoke_runtime(runtime, &[destination, callback])?
            .is_some()
        {
            panic!("checked MIR translation violated an established compiler contract");
        }

        Ok(None)
    }

    fn translate_generator_destroy(
        &mut self,
        helpers: &[CodegenHelperMapping],
        destination: &MirPlace,
        element: bray_symbols::TypeId,
        runtime: bray_ir::MirRuntimeReference,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let [finalize, destroy] = helpers else {
            panic!("checked MIR translation violated an established compiler contract");
        };

        if finalize.reference() != &MirHelperReference::Finalize(element)
            || destroy.reference() != &MirHelperReference::Destroy(element)
        {
            panic!("checked MIR translation violated an established compiler contract");
        }

        let finalize = self.generator_callback_argument(finalize, runtime, 1)?;
        let destroy = self.generator_callback_argument(destroy, runtime, 2)?;
        let destination = self.place(destination)?.into();

        if self
            .invoke_runtime(runtime, &[destination, finalize, destroy])?
            .is_some()
        {
            panic!("checked MIR translation violated an established compiler contract");
        }

        Ok(None)
    }

    fn generator_callback_argument(
        &mut self,
        helper: &CodegenHelperMapping,
        runtime: bray_ir::MirRuntimeReference,
        parameter: usize,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        self.helper_address(helper)?.map_or_else(
            || self.runtime_null_pointer_argument(runtime, parameter),
            Ok,
        )
    }
}
