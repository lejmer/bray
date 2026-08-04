use std::collections::BTreeMap;

use bray_codegen::{CodegenMappings, DebugInformationMode};
use bray_ir::MirSourceAnchor;
use bray_target::ObjectFormat;
use inkwell::builder::Builder;
use inkwell::context::Context;
use inkwell::debug_info::{
    AsDIScope, DICompileUnit, DIFile, DIFlags, DIFlagsConstants, DISubprogram, DWARFEmissionKind,
    DWARFSourceLanguage, DebugInfoBuilder,
};
use inkwell::module::{FlagBehavior, Module};
use inkwell::values::FunctionValue;

#[derive(Clone, Copy)]
struct LlvmDebugLocation<'context> {
    file: DIFile<'context>,
    line: u32,
    column: u32,
}

pub(crate) struct LlvmDebugInfo<'context> {
    context: &'context Context,
    builder: DebugInfoBuilder<'context>,
    compile_unit: DICompileUnit<'context>,
    locations: BTreeMap<MirSourceAnchor, LlvmDebugLocation<'context>>,
    optimized: bool,
}

impl<'context> LlvmDebugInfo<'context> {
    pub(crate) fn attach_function(
        &self,
        function: FunctionValue<'context>,
        display_name: &str,
        linkage_name: &str,
        source: &MirSourceAnchor,
    ) -> Option<DISubprogram<'context>> {
        let location = self.locations.get(source).copied()?;

        let function_type =
            self.builder
                .create_subroutine_type(location.file, None, &[], DIFlags::ZERO);

        let subprogram = self.builder.create_function(
            self.compile_unit.as_debug_info_scope(),
            display_name,
            Some(linkage_name),
            location.file,
            location.line,
            function_type,
            false,
            true,
            location.line,
            DIFlags::ZERO,
            self.optimized,
        );

        function.set_subprogram(subprogram);

        Some(subprogram)
    }

    pub(crate) fn set_location(
        &self,
        builder: &Builder<'context>,
        scope: DISubprogram<'context>,
        source: &MirSourceAnchor,
    ) {
        let Some(location) = self.locations.get(source) else {
            builder.unset_current_debug_location();

            return;
        };

        let location = self.builder.create_debug_location(
            self.context,
            location.line,
            location.column,
            scope.as_debug_info_scope(),
            None,
        );

        builder.set_current_debug_location(location);
    }

    pub(crate) fn finalize(self) {
        self.builder.finalize();
    }
}

pub(crate) fn create_debug_metadata<'context>(
    context: &'context Context,
    module: &Module<'context>,
    mappings: &CodegenMappings,
    mode: DebugInformationMode,
    optimized: bool,
    object_format: ObjectFormat,
) -> Option<LlvmDebugInfo<'context>> {
    if mode == DebugInformationMode::None {
        return None;
    }

    let source_file = mappings
        .debug_locations()
        .first()
        .map(|location| location.file().path())
        .unwrap_or("generated.bray");

    let emission = match mode {
        DebugInformationMode::None => return None,
        DebugInformationMode::LineTables => DWARFEmissionKind::LineTablesOnly,
        DebugInformationMode::Full => DWARFEmissionKind::Full,
    };

    module.add_basic_value_flag(
        "Debug Info Version",
        FlagBehavior::Warning,
        context.i32_type().const_int(3, false),
    );

    if object_format == ObjectFormat::Coff {
        module.add_basic_value_flag(
            "CodeView",
            FlagBehavior::Warning,
            context.i32_type().const_int(1, false),
        );
    }

    let (builder, compile_unit) = module.create_debug_info_builder(
        true,
        DWARFSourceLanguage::C,
        source_file,
        "",
        "Bray compiler",
        optimized,
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

    let mut files = BTreeMap::new();
    let mut locations = BTreeMap::new();

    for location in mappings.debug_locations() {
        let path = location.file().path();

        let file = *files
            .entry(path.to_owned())
            .or_insert_with(|| builder.create_file(path, ""));

        locations.insert(
            location.anchor().clone(),
            LlvmDebugLocation {
                file,
                line: location.line().get(),
                column: location.column().get(),
            },
        );
    }

    Some(LlvmDebugInfo {
        context,
        builder,
        compile_unit,
        locations,
        optimized,
    })
}

#[cfg(test)]
mod tests {
    use bray_codegen::test_support::codegen_request;
    use bray_codegen::{DebugInformationMode, OptimizationLevel};
    use inkwell::context::Context;

    use super::create_debug_metadata;

    #[test]
    fn debug_metadata_uses_exact_mapped_source_files() {
        let fixture = codegen_request();
        let context = Context::create();
        let module = context.create_module("debug");

        let Some(debug) = create_debug_metadata(
            &context,
            &module,
            fixture.request().mappings(),
            DebugInformationMode::LineTables,
            fixture.request().options().optimization() != OptimizationLevel::None,
            bray_target::ObjectFormat::Coff,
        ) else {
            panic!("line-table debug information must create metadata");
        };

        let function_type = context.void_type().fn_type(&[], false);
        let function = module.add_function("main", function_type, None);
        let block = context.append_basic_block(function, "entry");
        let builder = context.create_builder();

        builder.position_at_end(block);

        let source = fixture.request().mappings().debug_locations()[0].anchor();

        let Some(subprogram) = debug.attach_function(function, "display", "linkage", source) else {
            panic!("mapped source must attach a function subprogram");
        };

        debug.set_location(&builder, subprogram, source);

        builder
            .build_return(None)
            .unwrap_or_else(|error| panic!("debug test return must build: {error}"));

        debug.finalize();

        let text = module.print_to_string().to_string();

        assert!(text.contains("!DICompileUnit"));
        assert!(text.contains("!DISubprogram"));
        assert!(text.contains("!DILocation"));
        assert!(text.contains("!dbg"));
        assert!(text.contains("test.bray"));
        assert!(text.contains("!\"CodeView\""));
        assert!(text.contains("name: \"display\", linkageName: \"linkage\""));
    }
}
