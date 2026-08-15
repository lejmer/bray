use std::ffi::CString;

use inkwell::basic_block::BasicBlock;
use inkwell::attributes::Attribute;
use inkwell::builder::Builder;
use inkwell::context::ContextRef;
use inkwell::llvm_sys::core::{LLVMAddCallSiteAttribute, LLVMBuildCallBr};
use inkwell::types::{AsTypeRef, BasicMetadataTypeEnum, FunctionType};
use inkwell::values::{
    AsValueRef, BasicMetadataValueEnum, BasicValueEnum, InstructionOpcode, PointerValue,
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum CallBrError {
    ContextMismatch,
    DestinationOutsideFunction,
    InvalidArguments,
    InvalidName,
    MissingInstruction,
    ResourceExhausted,
    UnexpectedInstruction,
    UnexpectedResultType,
    UnsetPosition,
}

#[expect(
    unsafe_code,
    reason = "Inkwell 0.9.0 exposes neither callbr construction nor callbr as an attributed call site"
)]
pub(crate) fn build_callbr<'context>(
    builder: &Builder<'context>,
    function_type: FunctionType<'context>,
    function_pointer: PointerValue<'context>,
    default_destination: BasicBlock<'context>,
    indirect_destinations: &[BasicBlock<'context>],
    arguments: &[BasicMetadataValueEnum<'context>],
    parameter_attributes: &[(u32, Attribute)],
    name: &str,
) -> Result<Option<BasicValueEnum<'context>>, CallBrError> {
    let (insertion_block, destination_count, argument_count) = validate_callbr(
        builder,
        function_type,
        function_pointer,
        default_destination,
        indirect_destinations,
        arguments,
        parameter_attributes,
    )?;

    let name = if function_type.get_return_type().is_some() {
        name
    } else {
        ""
    };

    let name = CString::new(name).map_err(|_| CallBrError::InvalidName)?;

    let mut destinations = indirect_destinations
        .iter()
        .map(BasicBlock::as_mut_ptr)
        .collect::<Vec<_>>();

    let mut arguments = arguments
        .iter()
        .map(AsValueRef::as_value_ref)
        .collect::<Vec<_>>();

    // SAFETY: All handles share the validated insertion function and Inkwell context. The
    // destination and argument arrays remain borrowed for this call, and every attribute index
    // was validated against that argument array before being attached to the new instruction.
    let built_instruction = unsafe {
        let instruction = LLVMBuildCallBr(
            builder.as_mut_ptr(),
            function_type.as_type_ref(),
            function_pointer.as_value_ref(),
            default_destination.as_mut_ptr(),
            destinations.as_mut_ptr(),
            destination_count,
            arguments.as_mut_ptr(),
            argument_count,
            std::ptr::null_mut(),
            0,
            name.as_ptr(),
        );

        for &(index, attribute) in parameter_attributes {
            LLVMAddCallSiteAttribute(instruction, index + 1, attribute.as_mut_ptr());
        }

        instruction
    };

    let instruction = insertion_block
        .get_last_instruction()
        .ok_or(CallBrError::MissingInstruction)?;

    if instruction.get_opcode() != InstructionOpcode::CallBr
        || instruction.as_value_ref() != built_instruction
    {
        return Err(CallBrError::UnexpectedInstruction);
    }

    let Some(result_type) = function_type.get_return_type() else {
        return Ok(None);
    };

    if instruction.get_type().as_type_ref() != result_type.as_type_ref() {
        return Err(CallBrError::UnexpectedResultType);
    }

    // SAFETY: The recovered instruction is a non-void callbr and its LLVM type was checked
    // against the basic result type supplied to LLVMBuildCallBr.
    Ok(Some(unsafe {
        BasicValueEnum::new(instruction.as_value_ref())
    }))
}

fn validate_callbr<'context>(
    builder: &Builder<'context>,
    function_type: FunctionType<'context>,
    function_pointer: PointerValue<'context>,
    default_destination: BasicBlock<'context>,
    indirect_destinations: &[BasicBlock<'context>],
    arguments: &[BasicMetadataValueEnum<'context>],
    parameter_attributes: &[(u32, Attribute)],
) -> Result<(BasicBlock<'context>, u32, u32), CallBrError> {
    let insertion_block = builder
        .get_insert_block()
        .ok_or(CallBrError::UnsetPosition)?;

    let function = insertion_block
        .get_parent()
        .ok_or(CallBrError::DestinationOutsideFunction)?;

    let context = function_type.get_context();

    if function.get_type().get_context() != context
        || function_pointer.get_type().get_context() != context
    {
        return Err(CallBrError::ContextMismatch);
    }

    if default_destination.get_parent() != Some(function)
        || indirect_destinations
            .iter()
            .any(|destination| destination.get_parent() != Some(function))
    {
        return Err(CallBrError::DestinationOutsideFunction);
    }

    let parameter_types = function_type.get_param_types();

    if arguments.len() < parameter_types.len()
        || (!function_type.is_var_arg() && arguments.len() != parameter_types.len())
        || arguments
            .iter()
            .zip(&parameter_types)
            .any(|(argument, parameter)| argument_type(*argument) != Some(*parameter))
    {
        return Err(CallBrError::InvalidArguments);
    }

    if arguments
        .iter()
        .any(|argument| argument_context(*argument) != Some(context))
    {
        return Err(CallBrError::ContextMismatch);
    }

    if parameter_attributes
        .iter()
        .any(|(index, _)| usize::try_from(*index).map_or(true, |index| index >= arguments.len()))
    {
        return Err(CallBrError::InvalidArguments);
    }

    let destination_count = u32::try_from(indirect_destinations.len())
        .map_err(|_| CallBrError::ResourceExhausted)?;

    let argument_count =
        u32::try_from(arguments.len()).map_err(|_| CallBrError::ResourceExhausted)?;

    Ok((insertion_block, destination_count, argument_count))
}

fn argument_type(
    argument: BasicMetadataValueEnum<'_>,
) -> Option<BasicMetadataTypeEnum<'_>> {
    match argument {
        BasicMetadataValueEnum::ArrayValue(value) => Some(value.get_type().into()),
        BasicMetadataValueEnum::IntValue(value) => Some(value.get_type().into()),
        BasicMetadataValueEnum::FloatValue(value) => Some(value.get_type().into()),
        BasicMetadataValueEnum::PointerValue(value) => Some(value.get_type().into()),
        BasicMetadataValueEnum::StructValue(value) => Some(value.get_type().into()),
        BasicMetadataValueEnum::VectorValue(value) => Some(value.get_type().into()),
        BasicMetadataValueEnum::ScalableVectorValue(value) => Some(value.get_type().into()),
        BasicMetadataValueEnum::MetadataValue(_) => None,
    }
}

fn argument_context(argument: BasicMetadataValueEnum<'_>) -> Option<ContextRef<'_>> {
    match argument {
        BasicMetadataValueEnum::ArrayValue(value) => Some(value.get_type().get_context()),
        BasicMetadataValueEnum::IntValue(value) => Some(value.get_type().get_context()),
        BasicMetadataValueEnum::FloatValue(value) => Some(value.get_type().get_context()),
        BasicMetadataValueEnum::PointerValue(value) => Some(value.get_type().get_context()),
        BasicMetadataValueEnum::StructValue(value) => Some(value.get_type().get_context()),
        BasicMetadataValueEnum::VectorValue(value) => Some(value.get_type().get_context()),
        BasicMetadataValueEnum::ScalableVectorValue(value) => {
            Some(value.get_type().get_context())
        }
        BasicMetadataValueEnum::MetadataValue(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use inkwell::attributes::Attribute;
    use inkwell::context::Context;
    use inkwell::types::AnyType;
    use inkwell::values::{BasicValueEnum, InstructionOpcode};

    use super::{CallBrError, build_callbr};

    #[test]
    fn callbr_requires_destinations_in_the_insertion_function() {
        let context = Context::create();
        let module = context.create_module("callbr.destination.test");
        let builder = context.create_builder();
        let function_type = context.void_type().fn_type(&[], false);
        let function = module.add_function("test", function_type, None);
        let other = module.add_function("other", function_type, None);
        let entry = context.append_basic_block(function, "entry");
        let default = context.append_basic_block(other, "default");

        let assembly = context.create_inline_asm(
            function_type,
            String::new(),
            "!i".to_owned(),
            false,
            false,
            None,
            false,
        );

        builder.position_at_end(entry);

        assert_eq!(
            build_callbr(
                &builder,
                function_type,
                assembly,
                default,
                &[default],
                &[],
                &[],
                "assembly"
            ),
            Err(CallBrError::DestinationOutsideFunction)
        );
    }

    #[test]
    fn callbr_recovers_a_checked_aggregate_result() {
        let context = Context::create();
        let module = context.create_module("callbr.result.test");
        let builder = context.create_builder();
        let integer = context.i32_type();
        let result = context.struct_type(&[integer.into(), integer.into()], false);
        let function_type = result.fn_type(&[integer.into()], false);
        let function = module.add_function("test", function_type, None);
        let entry = context.append_basic_block(function, "entry");
        let default = context.append_basic_block(function, "default");
        let alternate = context.append_basic_block(function, "alternate");

        let assembly = context.create_inline_asm(
            function_type,
            String::new(),
            "=r,=r,0,!i".to_owned(),
            false,
            false,
            None,
            false,
        );

        let input = function
            .get_first_param()
            .unwrap_or_else(|| panic!("test function must have an input"));

        builder.position_at_end(entry);

        let output = build_callbr(
            &builder,
            function_type,
            assembly,
            default,
            &[alternate],
            &[input.into()],
            &[],
            "assembly",
        )
        .unwrap_or_else(|error| panic!("callbr must build: {error:?}"))
        .unwrap_or_else(|| panic!("callbr must retain its aggregate output"));

        assert!(matches!(output, BasicValueEnum::StructValue(_)));

        assert_eq!(
            entry
                .get_last_instruction()
                .map(|instruction| instruction.get_opcode()),
            Some(InstructionOpcode::CallBr)
        );
    }

    #[test]
    fn callbr_attaches_device_element_types_to_indirect_memory_parameters() {
        let context = Context::create();
        let module = context.create_module("callbr.memory.test");
        let builder = context.create_builder();
        let pointer = context.ptr_type(inkwell::AddressSpace::from(1_u16));
        let function_type = context.void_type().fn_type(&[pointer.into()], false);
        let function = module.add_function("test", function_type, None);
        let entry = context.append_basic_block(function, "entry");
        let default = context.append_basic_block(function, "default");
        let alternate = context.append_basic_block(function, "alternate");

        let assembly = context.create_inline_asm(
            function_type,
            String::new(),
            "*m,!i".to_owned(),
            true,
            false,
            None,
            false,
        );

        let input = function
            .get_first_param()
            .unwrap_or_else(|| panic!("test function must have an input"));

        let kind = Attribute::get_named_enum_kind_id("elementtype");
        let attribute = context.create_type_attribute(kind, context.i16_type().as_any_type_enum());

        builder.position_at_end(entry);

        build_callbr(
            &builder,
            function_type,
            assembly,
            default,
            &[alternate],
            &[input.into()],
            &[(0, attribute)],
            "assembly",
        )
        .unwrap_or_else(|error| panic!("memory callbr must build: {error:?}"));

        builder.position_at_end(default);

        builder
            .build_unreachable()
            .unwrap_or_else(|error| panic!("default block must terminate: {error:?}"));

        builder.position_at_end(alternate);

        builder
            .build_unreachable()
            .unwrap_or_else(|error| panic!("alternate block must terminate: {error:?}"));

        assert!(module.verify().is_ok());

        assert!(
            module
                .print_to_string()
                .to_string()
                .contains("ptr addrspace(1) elementtype(i16)")
        );
    }
}
