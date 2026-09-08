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
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if helper.reference() != &MirHelperReference::BeginGenerator {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let layout = self
            .type_mapping(element)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .layout()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

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
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            arguments.push(self.constant(value)?);
        } else {
            arguments.push(self.helper_integer_argument(helper, 4, 0)?);
        }

        arguments.push(self.helper_boolean_argument(helper, 5, exact_count.is_some())?);

        if self.invoke_helper(helper, &arguments)?.is_some() {
            return Err(CodegenFailure::GeneratedModuleInvariant);
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
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if helper.reference() != &MirHelperReference::PushGenerator {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let element = self.operand_type(value)?;

        let layout = self
            .type_mapping(element)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .layout()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let storage =
            self.aligned_alloca(element, layout.alignment().get(), "generator.element")?;

        let value = self.operand(value)?;

        llvm(self.builder.build_store(storage, value))?;

        let destination = self.place(destination)?.into();

        if self
            .invoke_helper(helper, &[destination, storage.into()])?
            .is_some()
        {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        Ok(None)
    }

    fn translate_generator_finish(
        &mut self,
        helpers: &[CodegenHelperMapping],
        destination: &MirPlace,
    ) -> Result<Option<BasicValueEnum<'context>>, CodegenFailure> {
        let [helper] = helpers else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        if helper.reference() != &MirHelperReference::FinishGenerator {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let destination_pointer = self.place(destination)?;

        if self
            .invoke_helper(helper, &[destination_pointer.into()])?
            .is_some()
        {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        }

        let value = llvm(self.builder.build_load(
            self.types.map(destination.ty())?,
            destination_pointer,
            "generator.result",
        ))?;

        Ok(Some(value))
    }
}
