use bray_codegen::CodegenFailure;
use inkwell::types::BasicTypeEnum;
use inkwell::values::PointerValue;

use super::core::UnitTranslator;
use super::support::{integer_constant, llvm};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn store_union_tag(
        &mut self,
        storage: PointerValue<'context>,
        tag: Option<bray_symbols::TypeId>,
        value: Option<&bray_symbols::IntegerConstant>,
    ) -> Result<(), CodegenFailure> {
        let (Some(tag), Some(value)) = (tag, value) else {
            return Ok(());
        };

        let BasicTypeEnum::IntType(tag_type) = self.types.map(tag)? else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        llvm(
            self.builder
                .build_store(storage, integer_constant(tag_type, value)),
        )?;

        Ok(())
    }
}
