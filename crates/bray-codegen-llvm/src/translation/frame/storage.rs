use bray_codegen::CodegenFailure;
use inkwell::AddressSpace;
use inkwell::builder::Builder;
use inkwell::module::Module;
use inkwell::types::IntType;
use inkwell::values::PointerValue;

/// Allocates zeroed frame bytes with the alignment required by their LLVM representation.
pub(super) fn allocate<'context>(
    module: &Module<'context>,
    builder: &Builder<'context>,
    integer: IntType<'context>,
    size: u64,
    alignment: u32,
    pointer_alignment: u32,
) -> Result<PointerValue<'context>, CodegenFailure> {
    let context = module.get_context();
    let pointer = context.ptr_type(AddressSpace::default());
    let pointer_bytes = integer.get_bit_width() / 8;

    let Some(bytes) = allocation_size(size, alignment, pointer_bytes, pointer_alignment) else {
        return Ok(pointer.const_null());
    };

    let calloc = module.get_function("calloc").unwrap_or_else(|| {
        module.add_function(
            "calloc",
            pointer.fn_type(&[integer.into(), integer.into()], false),
            None,
        )
    });

    let raw = builder
        .build_call(
            calloc,
            &[
                integer.const_int(1, false).into(),
                integer.const_int(bytes, false).into(),
            ],
            "frame.allocation",
        )
        .map_err(CodegenFailure::backend_library)?
        .try_as_basic_value()
        .basic()
        .and_then(crate::translation::unit::pointer_value)
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    if alignment <= pointer_alignment {
        return Ok(raw);
    }

    let entry = builder
        .get_insert_block()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let function = entry
        .get_parent()
        .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

    let allocated = context.append_basic_block(function, "frame.align");
    let finished = context.append_basic_block(function, "frame.aligned");

    let available = builder
        .build_is_not_null(raw, "frame.allocation.available")
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_conditional_branch(available, allocated, finished)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(allocated);

    let address = builder
        .build_ptr_to_int(raw, integer, "frame.allocation.address")
        .map_err(CodegenFailure::backend_library)?;

    let prefix = u64::from(pointer_bytes) + u64::from(alignment) - 1;

    let rounded = builder
        .build_int_add(
            address,
            integer.const_int(prefix, false),
            "frame.alignment.rounded",
        )
        .map_err(CodegenFailure::backend_library)?;

    let aligned = builder
        .build_and(
            rounded,
            integer.const_int(!(u64::from(alignment) - 1), false),
            "frame.alignment.address",
        )
        .map_err(CodegenFailure::backend_library)?;

    let storage = builder
        .build_int_to_ptr(aligned, pointer, "frame.storage")
        .map_err(CodegenFailure::backend_library)?;

    // Keep the allocator's exact pointer immediately before the aligned frame. The checked
    // allocation size includes both this header and the maximum alignment padding.
    let header = header_pointer(builder, integer, storage)?;

    builder
        .build_store(header, raw)
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_unconditional_branch(finished)
        .map_err(CodegenFailure::backend_library)?;

    builder.position_at_end(finished);

    let result = builder
        .build_phi(pointer, "frame.allocation.result")
        .map_err(CodegenFailure::backend_library)?;

    result.add_incoming(&[(&pointer.const_null(), entry), (&storage, allocated)]);

    Ok(result.as_basic_value().into_pointer_value())
}

/// Releases the exact allocation underlying an aligned or ordinary frame pointer.
pub(super) fn release<'context>(
    module: &Module<'context>,
    builder: &Builder<'context>,
    integer: IntType<'context>,
    storage: PointerValue<'context>,
    alignment: u32,
    pointer_alignment: u32,
) -> Result<(), CodegenFailure> {
    let context = module.get_context();
    let pointer = context.ptr_type(AddressSpace::default());

    let free = module.get_function("free").unwrap_or_else(|| {
        module.add_function(
            "free",
            context.void_type().fn_type(&[pointer.into()], false),
            None,
        )
    });

    let (allocation, finished) = if alignment > pointer_alignment {
        let function = builder
            .get_insert_block()
            .and_then(|block| block.get_parent())
            .ok_or(CodegenFailure::GeneratedModuleInvariant)?;

        let allocated = context.append_basic_block(function, "frame.release");
        let finished = context.append_basic_block(function, "frame.released");

        let available = builder
            .build_is_not_null(storage, "frame.storage.available")
            .map_err(CodegenFailure::backend_library)?;

        builder
            .build_conditional_branch(available, allocated, finished)
            .map_err(CodegenFailure::backend_library)?;

        builder.position_at_end(allocated);
        let header = header_pointer(builder, integer, storage)?;

        let allocation = builder
            .build_load(pointer, header, "frame.allocation")
            .map_err(CodegenFailure::backend_library)?
            .into_pointer_value();

        (allocation, Some(finished))
    } else {
        (storage, None)
    };

    builder
        .build_call(free, &[allocation.into()], "")
        .map_err(CodegenFailure::backend_library)?;

    if let Some(finished) = finished {
        builder
            .build_unconditional_branch(finished)
            .map_err(CodegenFailure::backend_library)?;

        builder.position_at_end(finished);
    }

    Ok(())
}

fn header_pointer<'context>(
    builder: &Builder<'context>,
    integer: IntType<'context>,
    storage: PointerValue<'context>,
) -> Result<PointerValue<'context>, CodegenFailure> {
    let address = builder
        .build_ptr_to_int(storage, integer, "frame.storage.address")
        .map_err(CodegenFailure::backend_library)?;

    let header = builder
        .build_int_sub(
            address,
            integer.const_int(u64::from(integer.get_bit_width() / 8), false),
            "frame.allocation.header.address",
        )
        .map_err(CodegenFailure::backend_library)?;

    builder
        .build_int_to_ptr(header, storage.get_type(), "frame.allocation.header")
        .map_err(CodegenFailure::backend_library)
}

fn allocation_size(
    size: u64,
    alignment: u32,
    pointer_bytes: u32,
    pointer_alignment: u32,
) -> Option<u64> {
    if !alignment.is_power_of_two()
        || !pointer_alignment.is_power_of_two()
        || !(1..=8).contains(&pointer_bytes)
    {
        return None;
    }

    let prefix = if alignment > pointer_alignment {
        u64::from(pointer_bytes) + u64::from(alignment) - 1
    } else {
        0
    };

    let maximum = u64::MAX >> (64 - pointer_bytes * 8);

    size.checked_add(prefix).filter(|size| *size <= maximum)
}

#[cfg(test)]
mod tests {
    use super::allocation_size;

    #[test]
    fn allocation_and_release_emit_valid_null_safe_ir_for_both_pointer_widths() {
        for bits in [32, 64] {
            for alignment in [4, 8, 32, 64] {
                let context = inkwell::context::Context::create();
                let module = context.create_module("frame_storage");
                let builder = context.create_builder();

                let integer = context
                    .custom_width_int_type(std::num::NonZeroU32::new(bits).unwrap())
                    .unwrap();

                let pointer = context.ptr_type(inkwell::AddressSpace::default());
                let create = module.add_function("create", pointer.fn_type(&[], false), None);
                let entry = context.append_basic_block(create, "entry");
                builder.position_at_end(entry);

                let storage =
                    super::allocate(&module, &builder, integer, 256, alignment, bits / 8).unwrap();

                builder.build_return(Some(&storage)).unwrap();

                let destroy = module.add_function(
                    "destroy",
                    context.void_type().fn_type(&[pointer.into()], false),
                    None,
                );

                let entry = context.append_basic_block(destroy, "entry");
                builder.position_at_end(entry);
                let storage = destroy.get_first_param().unwrap().into_pointer_value();
                super::release(&module, &builder, integer, storage, alignment, bits / 8).unwrap();
                builder.build_return(None).unwrap();

                module.verify().unwrap();
                let ir = module.print_to_string().to_string();
                assert_eq!(ir.contains("frame.allocation.header"), alignment > bits / 8);
                assert_eq!(ir.contains("frame.released"), alignment > bits / 8);
                assert!(module.get_function("calloc").is_some());
                assert!(module.get_function("free").is_some());
            }
        }
    }

    #[test]
    fn aligned_layout_reserves_every_possible_header_and_padding_offset() {
        for pointer_bytes in [4, 8] {
            for alignment in [1, 2, 4, 8, 16, 32, 64, 4096] {
                for size in [1, 7, 128, 4096] {
                    let bytes =
                        allocation_size(size, alignment, pointer_bytes, pointer_bytes).unwrap();

                    if alignment <= pointer_bytes {
                        assert_eq!(bytes, size);
                        continue;
                    }

                    for raw in 1..=u64::from(alignment) {
                        let storage = (raw + u64::from(pointer_bytes + alignment - 1))
                            & !(u64::from(alignment) - 1);

                        assert_eq!(storage % u64::from(alignment), 0);
                        assert!(storage - u64::from(pointer_bytes) >= raw);
                        assert!(storage + size <= raw + bytes);
                    }
                }
            }
        }
    }

    #[test]
    fn impossible_allocation_sizes_reject_instead_of_truncating_to_the_target_width() {
        assert_eq!(
            allocation_size(u64::from(u32::MAX), 4, 4, 4),
            Some(u64::from(u32::MAX))
        );

        assert_eq!(allocation_size(u64::from(u32::MAX), 64, 4, 4), None);
        assert_eq!(allocation_size(u64::MAX, 64, 8, 8), None);
        assert_eq!(allocation_size(8, 3, 8, 8), None);
    }
}
