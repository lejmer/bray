use super::super::core::UnitTranslator;
use super::super::support::{extract_value, insert_value, llvm, pointer_value};
use bray_codegen::CodegenFailure;
use bray_ir::{MirOperand, MirOperation, MirOperationId};
use inkwell::AddressSpace;
use inkwell::module::Linkage;
use inkwell::values::{BasicValueEnum, UnnamedAddress};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_trait_view(
        &mut self,
        id: MirOperationId,
        operation: &MirOperation,
        operand: &MirOperand,
        entries: &[Option<bray_ir::MirTraitViewEntry>],
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let pointer = self.types.context().ptr_type(AddressSpace::default());
        let helpers = self.operation_helpers(id)?;
        let mut helpers = helpers.iter();

        let entries = entries.iter().map(|entry| match entry {
            None => Ok(pointer.const_null()),
            Some(entry) => {
                let helper = helpers.next().expect("view table retains each selected callable helper");

                assert!(matches!(helper.reference(), bray_ir::MirHelperReference::TraitViewEntry(retained) if retained.as_ref() == entry), "view table entry retains its selected helper: {id:?}");

                Ok(pointer_value(self.helper_address(helper)?.expect("view callable has its selected address")).expect("view callable address is a pointer"))
            }
        }).collect::<Result<Vec<_>, CodegenFailure>>()?;

        assert!(
            helpers.next().is_none(),
            "view table consumes each selected callable helper"
        );

        let initializer = pointer.const_array(&entries);

        let table = self
            .module
            .add_global(initializer.get_type(), None, "view.witness");

        table.set_linkage(Linkage::Private);
        table.set_constant(true);
        table.set_unnamed_address(UnnamedAddress::Global);
        table.set_initializer(&initializer);

        let result = self
            .types
            .map(self.operation_result_type(operation))?
            .into_struct_type()
            .get_undef();

        let data = self.operand(operand)?;
        let result = insert_value(&self.builder, result.into(), data, 0)?;

        insert_value(&self.builder, result, table.as_pointer_value().into(), 1)
    }

    pub(super) fn translate_trait_view_method(
        &mut self,
        operation: &MirOperation,
        operand: &MirOperand,
        slot: bray_symbols::SymbolOrdinal,
    ) -> Result<BasicValueEnum<'context>, CodegenFailure> {
        let view = self.operand(operand)?;
        let data = extract_value(&self.builder, view, 0)?;

        let table = pointer_value(extract_value(&self.builder, view, 1)?)
            .expect("view witness is a table pointer");

        let pointer = self.types.context().ptr_type(AddressSpace::default());

        let index = self
            .types
            .context()
            .i64_type()
            .const_int(u64::from(slot.raw()), false);

        let stride = self.types.target_data().get_pointer_byte_size(None);
        let entry = self.dynamic_offset_pointer(table, index, u64::from(stride))?;
        let method = llvm(self.builder.build_load(pointer, entry, "view.method"))?;

        let result = self
            .types
            .map(self.operation_result_type(operation))?
            .into_struct_type()
            .get_undef();

        let result = insert_value(&self.builder, result.into(), method, 0)?;

        insert_value(&self.builder, result, data, 1)
    }
}
