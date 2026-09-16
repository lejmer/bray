use bray_diagnostics::DiagnosticFailureField;

use super::diagnostic_context::{
    boolean_field, count_field, identity_field, identity_list_field, product_kind, text_field,
    text_list_field,
};

pub(super) fn push_codegen_artifact_fact_context(
    fields: &mut Vec<DiagnosticFailureField>,
    key: &crate::fact::CodegenArtifactQueryKey,
) {
    let backend = key.backend();
    let options = key.options();

    fields.extend([
        identity_field("codegen_unit", key.unit()),
        identity_field("codegen_mappings", key.mappings()),
        identity_field("codegen_target_contract", key.target()),
        text_field("target_triple", key.target().triple()),
        text_field("backend_name", backend.name()),
        text_field("backend_revision", backend.revision()),
        text_field("backend_toolchain_revision", backend.toolchain_revision()),
        count_field(
            "backend_capability_revision",
            u64::from(key.capability_revision().get()),
        ),
        text_field("product_kind", product_kind(key.product())),
        text_field(
            "codegen_optimization",
            codegen_optimization(options.optimization()),
        ),
        text_field(
            "codegen_size_preference",
            codegen_size_preference(options.size_preference()),
        ),
        text_field(
            "codegen_debug_information",
            codegen_debug_information(options.debug_information()),
        ),
        text_field(
            "codegen_reproducibility",
            codegen_reproducibility(options.reproducibility()),
        ),
    ]);

    push_codegen_mappings(fields, key.mappings());
    push_runtime_observation(fields, options.runtime_observations());
    push_backend_artifact_request(fields, key.artifacts());
    super::target_diagnostic::push_target_profile(fields, key.target().profile());
}

fn push_codegen_mappings(
    fields: &mut Vec<DiagnosticFailureField>,
    mappings: &bray_codegen::CodegenMappings,
) {
    fields.extend([
        identity_field("codegen_mapping_unit", mappings.unit()),
        identity_field("codegen_mapping_target", mappings.target()),
        identity_list_field("codegen_type_mappings", mappings.types()),
        identity_list_field("codegen_instance_type_mappings", mappings.instance_types()),
        identity_list_field("codegen_symbol_mappings", mappings.symbols()),
        identity_list_field("codegen_constant_mappings", mappings.constants()),
        identity_list_field("codegen_constant_term_mappings", mappings.constant_terms()),
        identity_list_field("codegen_callable_mappings", mappings.callables()),
        identity_list_field("codegen_operation_mappings", mappings.operations()),
        identity_list_field(
            "codegen_static_storage_mappings",
            mappings.static_storages(),
        ),
        identity_list_field(
            "codegen_native_storage_mappings",
            mappings.native_storages(),
        ),
        identity_list_field("codegen_terminator_mappings", mappings.terminators()),
        identity_list_field(
            "codegen_debug_location_mappings",
            mappings.debug_locations(),
        ),
        boolean_field(
            "codegen_product_host_present",
            mappings.product_host().is_some(),
        ),
    ]);

    if let Some(product_host) = mappings.product_host() {
        fields.push(identity_field("codegen_product_host_mapping", product_host));
    }
}

fn push_backend_artifact_request(
    fields: &mut Vec<DiagnosticFailureField>,
    request: &bray_codegen::BackendArtifactRequest,
) {
    let entries = request.entries();
    let serialization = request.serialization();

    fields.extend([
        identity_field("backend_artifact_request", request),
        identity_field("backend_artifact_unit", request.unit()),
        identity_list_field("backend_artifact_entries", entries),
        text_list_field(
            "backend_artifact_kinds",
            entries.iter().map(|entry| entry.id().kind().as_str()),
        ),
        text_list_field(
            "backend_artifact_ordinals",
            entries.iter().map(|entry| entry.id().ordinal().to_string()),
        ),
        text_list_field(
            "backend_artifact_requirements",
            entries
                .iter()
                .map(|entry| backend_artifact_requirement(entry.requirement())),
        ),
        text_field(
            "backend_artifact_debug_information",
            debug_information_output(request.debug_information()),
        ),
        boolean_field(
            "backend_linkable_artifact_present",
            request.linkable_artifact().is_some(),
        ),
        text_field(
            "backend_assembly_syntax",
            assembly_syntax_kind(serialization.assembly_syntax_kind()),
        ),
        text_field(
            "backend_bitcode_semantics",
            backend_bitcode_semantics(serialization.bitcode_semantics()),
        ),
    ]);

    if let Some(linkable) = request.linkable_artifact() {
        fields.extend([
            text_field(
                "backend_linkable_artifact_kind",
                linkable.artifact_kind().as_str(),
            ),
            text_field(
                "backend_linkable_artifact_requirement",
                backend_artifact_requirement(linkable.requirement()),
            ),
        ]);
    }
}

const fn codegen_optimization(value: bray_codegen::OptimizationLevel) -> &'static str {
    match value {
        bray_codegen::OptimizationLevel::None => "none",
        bray_codegen::OptimizationLevel::Basic => "basic",
        bray_codegen::OptimizationLevel::Full => "full",
    }
}

const fn codegen_size_preference(value: bray_codegen::SizePreference) -> &'static str {
    match value {
        bray_codegen::SizePreference::None => "none",
        bray_codegen::SizePreference::Size => "size",
        bray_codegen::SizePreference::MinimumSize => "minimum_size",
    }
}

const fn codegen_debug_information(value: bray_codegen::DebugInformationMode) -> &'static str {
    match value {
        bray_codegen::DebugInformationMode::None => "none",
        bray_codegen::DebugInformationMode::LineTables => "line_tables",
        bray_codegen::DebugInformationMode::Full => "full",
    }
}

const fn codegen_reproducibility(value: bray_codegen::ReproducibilityLevel) -> &'static str {
    match value {
        bray_codegen::ReproducibilityLevel::Semantic => "semantic",
        bray_codegen::ReproducibilityLevel::ByteForByte => "byte_for_byte",
    }
}

fn push_runtime_observation(
    fields: &mut Vec<DiagnosticFailureField>,
    observation: bray_codegen::RuntimeObservationMode,
) {
    use bray_codegen::RuntimeObservationMode as Observation;

    let kind = match observation {
        Observation::None => "none",
        Observation::Memory => "memory",
        Observation::PerformanceInterval { inner_iterations } => {
            fields.push(count_field(
                "codegen_observation_inner_iterations",
                inner_iterations.get(),
            ));

            "performance_interval"
        }
    };

    fields.push(text_field("codegen_runtime_observation", kind));
}

const fn backend_artifact_requirement(
    value: bray_codegen::BackendArtifactRequirement,
) -> &'static str {
    match value {
        bray_codegen::BackendArtifactRequirement::Required => "required",
        bray_codegen::BackendArtifactRequirement::Optional => "optional",
    }
}

const fn debug_information_output(value: bray_codegen::DebugInformationOutputMode) -> &'static str {
    match value {
        bray_codegen::DebugInformationOutputMode::Omit => "omit",
        bray_codegen::DebugInformationOutputMode::Embedded => "embedded",
        bray_codegen::DebugInformationOutputMode::Separate => "separate",
    }
}

const fn assembly_syntax_kind(value: bray_codegen::AssemblySyntaxKind) -> &'static str {
    match value {
        bray_codegen::AssemblySyntaxKind::TargetDefault => "target_default",
        bray_codegen::AssemblySyntaxKind::Intel => "intel",
        bray_codegen::AssemblySyntaxKind::Att => "att",
    }
}

const fn backend_bitcode_semantics(value: bray_codegen::BackendBitcodeSemantics) -> &'static str {
    match value {
        bray_codegen::BackendBitcodeSemantics::Plain => "plain",
        bray_codegen::BackendBitcodeSemantics::ThinLto => "thin_lto",
    }
}
