use bray_codegen::{CodegenMappings, DebugInformationMode};
use inkwell::context::Context;
use inkwell::debug_info::{DWARFEmissionKind, DWARFSourceLanguage};
use inkwell::module::{FlagBehavior, Module};

pub(crate) fn add_debug_metadata<'context>(
    context: &'context Context,
    module: &Module<'context>,
    mappings: &CodegenMappings,
    mode: DebugInformationMode,
) {
    if mode == DebugInformationMode::None {
        return;
    }

    let source_file = mappings
        .debug_locations()
        .first()
        .map(|location| location.file().path())
        .unwrap_or("generated.bray");

    let emission = match mode {
        DebugInformationMode::None => return,
        DebugInformationMode::LineTables => DWARFEmissionKind::LineTablesOnly,
        DebugInformationMode::Full => DWARFEmissionKind::Full,
    };

    module.add_basic_value_flag(
        "Debug Info Version",
        FlagBehavior::Warning,
        context.i32_type().const_int(3, false),
    );

    let (builder, _) = module.create_debug_info_builder(
        true,
        DWARFSourceLanguage::C,
        source_file,
        "",
        "Bray compiler",
        false,
        "",
        0,
        "",
        emission,
        0,
        false,
        false,
        "",
        "",
    );

    for location in mappings.debug_locations() {
        builder.create_file(location.file().path(), "");
    }

    builder.finalize();
}

#[cfg(test)]
mod tests {
    use bray_codegen::DebugInformationMode;
    use bray_codegen::test_support::codegen_request;
    use inkwell::context::Context;

    use super::add_debug_metadata;

    #[test]
    fn debug_metadata_uses_exact_mapped_source_files() {
        let fixture = codegen_request();
        let context = Context::create();
        let module = context.create_module("debug");

        add_debug_metadata(
            &context,
            &module,
            fixture.request().mappings(),
            DebugInformationMode::Full,
        );

        let text = module.print_to_string().to_string();

        assert!(text.contains("!DICompileUnit"));
        assert!(text.contains("test.bray"));
    }
}
