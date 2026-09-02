use bray_codegen::{CodegenFailure, CodegenTypeKind};
use bray_ir::{MirBlockId, MirEdge, MirPlace};
use inkwell::IntPredicate;
use inkwell::values::BasicValueEnum;

use super::core::UnitTranslator;
use super::support::{extract_value, insert_value, llvm};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn translate_range_iteration(
        &mut self,
        cursor: &MirPlace,
        element_type: bray_symbols::TypeId,
        item: MirBlockId,
        exhausted: &MirEdge,
    ) -> Result<(), CodegenFailure> {
        let pointer = self.place(cursor)?;
        let range_type = self.types.map(cursor.ty())?;
        let range = llvm(self.builder.build_load(range_type, pointer, "range.cursor"))?;

        let fields = match self
            .type_mapping(cursor.ty())
            .map(|mapping| mapping.kind().clone())
        {
            Some(CodegenTypeKind::Aggregate(fields)) => fields,
            _ => return Err(CodegenFailure::GeneratedModuleInvariant),
        };

        let start_index = self.aggregate_element(&fields, 0)?;
        let end_index = self.aggregate_element(&fields, 1)?;

        let start = extract_value(&self.builder, range, start_index)?.into_int_value();
        let end = extract_value(&self.builder, range, end_index)?.into_int_value();

        let predicate = match self
            .type_mapping(element_type)
            .map(|mapping| mapping.kind())
        {
            Some(CodegenTypeKind::SignedInteger(_)) => IntPredicate::SLT,
            Some(CodegenTypeKind::UnsignedInteger(_)) => IntPredicate::ULT,
            _ => return Err(CodegenFailure::GeneratedModuleInvariant),
        };

        let present = llvm(
            self.builder
                .build_int_compare(predicate, start, end, "range.present"),
        )?;

        let incremented = llvm(self.builder.build_int_add(
            start,
            start.get_type().const_int(1, false),
            "range.incremented",
        ))?;

        let next = llvm(
            self.builder
                .build_select(present, incremented, start, "range.next"),
        )?;

        let range = insert_value(
            &self.builder,
            range,
            next,
            crate::conversion::resource_limit(start_index, "range_start_index")?,
        )?;

        llvm(self.builder.build_store(pointer, range))?;

        let item_block = self
            .unit
            .block(item)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let [parameter] = item_block.parameters() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let phi = self
            .phis
            .get(parameter)
            .copied()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let element: BasicValueEnum<'context> = start.into();

        let (source, pending_moves) = self.take_control_source()?;

        let item_route = self.route_target(item, "range.item", &pending_moves)?;
        let exhausted_route = self.route_edge(exhausted, "range.exhausted", &pending_moves)?;

        phi.add_incoming(&[(&element, item_route)]);
        self.builder.position_at_end(source);

        llvm(
            self.builder
                .build_conditional_branch(present, item_route, exhausted_route),
        )?;

        Ok(())
    }
}
