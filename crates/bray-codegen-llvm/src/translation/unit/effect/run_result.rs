use bray_codegen::{CodegenFailure, CodegenTypeKind};
use bray_ir::MirRunResultVariants;
use bray_runtime_abi::{NativeBrayCallOutcome, NativeRunState, NativeRuntimeStatus};
use bray_symbols::TypeId;
use inkwell::builder::Builder;
use inkwell::module::Linkage;
use inkwell::values::{FunctionValue, IntValue, PointerValue};
use inkwell::{AddressSpace, IntPredicate};

use super::super::core::UnitTranslator;
use super::super::support::{extract_value, llvm, offset_pointer};

impl<'context, 'module, 'request, 'types> UnitTranslator<'context, 'module, 'request, 'types> {
    pub(super) fn run_result_layout(
        &mut self,
        result: TypeId,
        variants: MirRunResultVariants,
    ) -> Result<PointerValue<'context>, CodegenFailure> {
        let represented = self
            .type_mapping(result)
            .and_then(|mapping| mapping.layout())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let transfer = self.run_result_transfer(result, variants)?;
        let context = self.types.context();
        let usize = crate::native::pointer_integer_type(context, self.request.target());
        let layout = crate::native::run_result_layout_type(context, self.request.target());

        let value = layout.const_named_struct(&[
            usize.const_int(represented.size(), false).into(),
            usize.const_int(represented.alignment().get(), false).into(),
            transfer.as_global_value().as_pointer_value().into(),
        ]);

        let storage = self.allocate_temporary(layout, "task.result.layout")?;

        llvm(self.builder.build_store(storage, value))?;

        Ok(storage)
    }

    fn run_result_transfer(
        &mut self,
        result: TypeId,
        selected: MirRunResultVariants,
    ) -> Result<FunctionValue<'context>, CodegenFailure> {
        if let Some((variants, function)) = self.run_result_transfers.get(&result) {
            if *variants != selected {
                return Err(CodegenFailure::GeneratedModuleInvariant);
            }

            return Ok(*function);
        }

        // First-demand order follows MIR traversal and does not expose semantic interning order.
        let name = format!(
            "{}.run_result.transfer.{}",
            self.function.get_name().to_string_lossy(),
            self.run_result_transfers.len()
        );

        let context = self.types.context();
        let usize = crate::native::pointer_integer_type(context, self.request.target());
        let pointer = context.ptr_type(AddressSpace::default());

        let signature = context
            .i32_type()
            .fn_type(&[usize.into(), pointer.into()], false);

        let function = self
            .module
            .add_function(&name, signature, Some(Linkage::Internal));

        let builder = context.create_builder();
        let entry = context.append_basic_block(function, "entry");
        let invalid = context.append_basic_block(function, "invalid");

        builder.position_at_end(entry);

        let destination = function
            .get_nth_param(0)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .into_int_value();

        let destination = llvm(builder.build_int_to_ptr(destination, pointer, "result"))?;

        let outcome = function
            .get_nth_param(1)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .into_pointer_value();

        let outcome = llvm(builder.build_load(
            crate::native::run_outcome_type(context, self.request.target()),
            outcome,
            "outcome",
        ))?;

        let state = extract_value(&builder, outcome, 0)?.into_int_value();
        let payload = extract_value(&builder, outcome, 1)?.into_int_value();

        let cases = [
            (NativeRunState::COMPLETED, selected.completed()),
            (NativeRunState::PANICKED, selected.panicked()),
            (NativeRunState::CANCELLED, selected.cancelled()),
        ];

        let blocks = cases.map(|_| context.append_basic_block(function, "variant"));

        let switch = cases
            .iter()
            .zip(blocks)
            .map(|((state, _), block)| {
                (
                    context.i32_type().const_int(u64::from(state.code()), false),
                    block,
                )
            })
            .collect::<Vec<_>>();

        llvm(builder.build_switch(state, invalid, &switch))?;
        builder.position_at_end(invalid);

        llvm(builder.build_return(Some(&context.i32_type().const_int(
            u64::from(NativeRuntimeStatus::INVALID_ARGUMENT.code()),
            false,
        ))))?;

        for ((state, variant), block) in cases.into_iter().zip(blocks) {
            builder.position_at_end(block);

            self.write_run_result_variant(
                &builder,
                result,
                variant,
                state,
                destination,
                payload,
                invalid,
            )?;
        }

        self.run_result_transfers
            .insert(result, (selected, function));

        Ok(function)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "the variant transfer borrows one LLVM callback and its concrete source and destination"
    )]
    fn write_run_result_variant(
        &self,
        builder: &Builder<'context>,
        result: TypeId,
        variant: bray_symbols::UnionVariantSymbolId,
        state: NativeRunState,
        destination: PointerValue<'context>,
        payload: IntValue<'context>,
        invalid: inkwell::basic_block::BasicBlock<'context>,
    ) -> Result<(), CodegenFailure> {
        let mapping = self
            .type_mapping(result)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let represented = mapping
            .layout()
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let CodegenTypeKind::Union { tag: Some(tag), .. } = mapping.kind() else {
            return Err(CodegenFailure::GeneratedModuleInvariant);
        };

        let tag_size = self
            .type_mapping(*tag)
            .and_then(|mapping| mapping.layout())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?
            .size();

        let tag_bits = crate::conversion::target_value(
            tag_size
                .checked_mul(8)
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?,
            "run_result_tag_bits",
        )?;

        let tag_bits =
            std::num::NonZeroU32::new(tag_bits).ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let variant = mapping
            .kind()
            .union_variant(variant)
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let tag = variant
            .tag()
            .and_then(|tag| tag.to_u64())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let field = match (state, variant.fields()) {
            (NativeRunState::COMPLETED | NativeRunState::PANICKED, [field]) => Some(field),
            (NativeRunState::CANCELLED, []) => None,
            _ => return Err(CodegenFailure::GeneratedModuleInvariant),
        };

        let field_layout = field
            .map(|field| {
                self.type_mapping(field.ty())
                    .and_then(|mapping| mapping.layout())
                    .ok_or(CodegenFailure::GeneratedModuleInvariant)
            })
            .transpose()?;

        let context = self.types.context();
        let usize = payload.get_type();

        if state == NativeRunState::PANICKED
            || field_layout.is_some_and(|layout| layout.size() != 0)
        {
            let minimum = if state == NativeRunState::PANICKED {
                u64::try_from(NativeBrayCallOutcome::cancelled().raw())
                    .map_err(|_| CodegenFailure::GeneratedModuleInvariant)?
            } else {
                0
            };

            let valid = llvm(builder.build_int_compare(
                IntPredicate::UGT,
                payload,
                usize.const_int(minimum, false),
                "payload.valid",
            ))?;

            let function = invalid
                .get_parent()
                .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

            let write = context.append_basic_block(function, "variant.write");

            llvm(builder.build_conditional_branch(valid, write, invalid))?;
            builder.position_at_end(write);
        }

        llvm(builder.build_memset(
            destination,
            1,
            context.i8_type().const_zero(),
            usize.const_int(represented.size(), false),
        ))?;

        let tag_type = context
            .custom_width_int_type(tag_bits)
            .map_err(CodegenFailure::backend_library)?;

        llvm(builder.build_store(destination, tag_type.const_int(tag, false)))?;

        if let (Some(field), Some(layout)) = (field, field_layout) {
            let target = offset_pointer(
                builder,
                destination,
                usize.const_int(field.offset_bytes(), false),
            )?;

            if state == NativeRunState::PANICKED {
                llvm(builder.build_store(target, payload))?;
            } else if layout.size() != 0 {
                let source = llvm(builder.build_int_to_ptr(
                    payload,
                    destination.get_type(),
                    "completed.payload",
                ))?;

                let alignment = crate::conversion::target_value(
                    layout.alignment().get(),
                    "run_result_payload_alignment",
                )?;

                llvm(builder.build_memcpy(
                    target,
                    alignment,
                    source,
                    alignment,
                    usize.const_int(layout.size(), false),
                ))?;
            }
        }

        llvm(
            builder.build_return(Some(
                &context
                    .i32_type()
                    .const_int(u64::from(NativeRuntimeStatus::SUCCESS.code()), false),
            )),
        )?;

        Ok(())
    }
}
