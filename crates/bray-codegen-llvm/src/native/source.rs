use bray_runtime_interface::NATIVE_PANIC_REPORT_FIELDS;
use inkwell::context::Context;
use inkwell::types::StructType;
use inkwell::values::StructValue;

use super::core::panic_field_type;

pub(crate) fn source_anchor_type(context: &Context) -> StructType<'_> {
    context.struct_type(
        &std::array::from_fn::<_, 9, _>(|index| {
            panic_field_type(context, NATIVE_PANIC_REPORT_FIELDS[index])
        }),
        false,
    )
}

fn source_anchor_value(
    context: &Context,
    source: bray_runtime_abi::NativeSourceAnchor,
) -> StructValue<'_> {
    let namespace = source.namespace();

    let [word0, word1, word2, word3] = [0, 8, 16, 24].map(|offset| {
        let bytes = namespace[offset..offset + 8]
            .try_into()
            .expect("source namespace has four words");

        context
            .i64_type()
            .const_int(u64::from_le_bytes(bytes), false)
            .into()
    });

    source_anchor_type(context).const_named_struct(&[
        context
            .i32_type()
            .const_int(u64::from(source.is_available()), false)
            .into(),
        context
            .i32_type()
            .const_int(u64::from(source.source()), false)
            .into(),
        context
            .i32_type()
            .const_int(u64::from(source.start()), false)
            .into(),
        context
            .i32_type()
            .const_int(u64::from(source.end()), false)
            .into(),
        context.i64_type().const_int(source.version(), false).into(),
        word0,
        word1,
        word2,
        word3,
    ])
}

pub(crate) fn mir_source_anchor_value<'context>(
    context: &'context inkwell::context::Context,
    namespace: [u8; 32],
    source: Option<&bray_ir::MirSourceAnchor>,
) -> StructValue<'context> {
    let source = match source {
        Some(bray_ir::MirSourceAnchor::Source(origin)) => {
            let anchor = origin.source_anchor();
            let syntax = anchor.syntax();
            let range = syntax.full_range();

            bray_runtime_abi::NativeSourceAnchor::new(
                namespace,
                syntax.source_id().raw(),
                range.start().bytes(),
                range.end().bytes(),
                anchor.source_version().raw(),
            )
        }
        Some(bray_ir::MirSourceAnchor::ImportedSource {
            namespace,
            span,
            version,
            ..
        }) => bray_runtime_abi::NativeSourceAnchor::new(
            *namespace,
            span.source_id().raw(),
            span.start().bytes(),
            span.end().bytes(),
            version.raw(),
        ),
        Some(
            bray_ir::MirSourceAnchor::ExecutableHost(_)
            | bray_ir::MirSourceAnchor::GeneratedLifecycle(_)
            | bray_ir::MirSourceAnchor::CompilerProvidedCallable(_)
            | bray_ir::MirSourceAnchor::ImportedExecutable(_),
        )
        | None => bray_runtime_abi::NativeSourceAnchor::unavailable(),
    };

    source_anchor_value(context, source)
}
