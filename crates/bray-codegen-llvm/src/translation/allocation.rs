use bray_codegen::CodegenFailure;
use inkwell::basic_block::BasicBlock;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::types::{BasicType, BasicTypeEnum};
use inkwell::values::{BasicValueEnum, InstructionOpcode, PointerValue};

pub(crate) fn reinterpret_value<'context>(
    context: &'context Context,
    builder: &Builder<'context>,
    value: BasicValueEnum<'context>,
    result: BasicTypeEnum<'context>,
    storage_type: BasicTypeEnum<'context>,
    name: &str,
) -> Result<BasicValueEnum<'context>, CodegenFailure> {
    if value.get_type() == result {
        return Ok(value);
    }

    // The caller supplies storage large and aligned enough for both representations.
    let storage = allocate_temporary(context, builder, storage_type, &format!("{name}.storage"))?;

    builder
        .build_store(storage, storage_type.const_zero())
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_store(storage, value)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_load(result, storage, name)
        .map_err(CodegenFailure::backend_library)
}

pub(crate) fn allocate_temporary<'context>(
    context: &'context Context,
    source: &Builder<'context>,
    ty: impl BasicType<'context>,
    name: &str,
) -> Result<PointerValue<'context>, CodegenFailure> {
    let function = source
        .get_insert_block()
        .and_then(BasicBlock::get_parent)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let entry = function
        .get_first_basic_block()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let builder = context.create_builder();
    let mut instruction = entry.get_first_instruction();

    while instruction.is_some_and(|instruction| {
        matches!(
            instruction.get_opcode(),
            InstructionOpcode::Alloca | InstructionOpcode::Phi
        )
    }) {
        instruction = instruction.and_then(|instruction| instruction.get_next_instruction());
    }

    match instruction {
        Some(instruction) => builder.position_before(&instruction),
        None => builder.position_at_end(entry),
    }

    builder
        .build_alloca(ty, name)
        .map_err(CodegenFailure::backend_library)
}

#[cfg(test)]
mod tests {
    use inkwell::context::Context;
    use inkwell::values::BasicValue;

    #[test]
    fn temporary_uses_the_entry_of_the_active_function() {
        let context = Context::create();
        let module = context.create_module("temporary");
        let function_type = context.void_type().fn_type(&[], false);
        let inactive = module.add_function("inactive", function_type, None);
        let inactive_entry = context.append_basic_block(inactive, "entry");
        let active = module.add_function("active", function_type, None);
        let active_entry = context.append_basic_block(active, "entry");
        let active_body = context.append_basic_block(active, "body");
        let builder = context.create_builder();

        builder.position_at_end(inactive_entry);
        assert!(builder.build_return(None).is_ok());

        builder.position_at_end(active_entry);
        assert!(builder.build_unconditional_branch(active_body).is_ok());

        builder.position_at_end(active_body);
        assert!(builder.build_return(None).is_ok());

        let Ok(temporary) =
            super::allocate_temporary(&context, &builder, context.i32_type(), "result")
        else {
            panic!("temporary allocation must succeed");
        };

        assert_eq!(
            temporary
                .as_instruction_value()
                .and_then(|value| value.get_parent()),
            Some(active_entry)
        );

        assert_eq!(builder.get_insert_block(), Some(active_body));
    }
}
