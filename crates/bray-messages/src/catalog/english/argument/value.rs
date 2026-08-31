use super::callable::{
    format_english_callable_overload_problem, format_english_implementation_overload_problem,
    format_english_trait_fulfillment_mismatch,
};
use super::emission::{
    format_english_artifact_requirement, format_english_assembly_syntax,
    format_english_debug_information_mode, format_english_debug_output_mode,
    format_english_emission_artifact_operation, format_english_emission_failure,
    format_english_link_input_kind, format_english_linked_artifact_kind,
    format_english_linked_product_kind, format_english_product_kind,
};
use super::interface::{
    format_english_interface_limit, format_english_interface_section,
    format_english_interface_semantic_problem, format_english_interface_symbol_graph_problem,
    format_english_interface_symbol_identity, format_english_source_input,
};
use super::native::{
    format_english_dependency_requirement, format_english_dependency_subject,
    format_english_document_parse_kind, format_english_external_tool_exit,
    format_english_external_tool_failure, format_english_external_tool_operation,
    format_english_link_optimization_report_problem, format_english_link_requirement,
    format_english_linker_driver_identity, format_english_native_product_failure,
    format_english_runtime_artifact_problem, format_english_standard_library_manifest_problem,
    format_english_unsupported_emission_reason,
};
use super::project::{
    format_english_project_command_failure, format_english_project_dependency_cycle_member,
    format_english_project_manifest_field, format_english_project_selection_problem,
};
use super::selection::{
    format_english_alignment_kind, format_english_callable_abi,
    format_english_native_link_directive_problem, format_english_native_symbol_directive_problem,
    format_english_platform_service_signature_problem, format_english_selection_candidates,
    format_english_selection_kind, format_english_selection_rejections,
    format_english_target_representation,
};
use super::semantics::{
    format_english_array_generator_problem, format_english_callback_state_problem,
    format_english_constant_operation, format_english_copy_contract_problem,
    format_english_expression_category, format_english_layout_problem,
    format_english_memory_operation, format_english_pattern_coverage,
    format_english_pattern_unreachability, format_english_propagation_problem,
    format_english_refinement_capacity, format_english_storage_access,
    format_english_stored_type_problem, format_english_union_tag_problem,
};
use super::source::{
    format_english_artifact_digest, format_english_artifact_kind, format_english_character,
    format_english_io_error_kind, format_english_module_trust, format_english_name_kind,
    format_english_output_sink, format_english_path, format_english_quoted_text,
    format_english_source_input_kind, format_english_syntax_kind,
    format_english_syntax_kind_without_suffix, format_english_type, format_source_span,
};
use super::target::format_english_target_predicate_value_kind;
use bray_diagnostics::{DiagnosticArgName, DiagnosticArgValue};

pub(crate) fn format_value(name: DiagnosticArgName, value: &DiagnosticArgValue) -> String {
    if let DiagnosticArgValue::SyntaxKind(kind) = value {
        match name {
            DiagnosticArgName::ModifierKind | DiagnosticArgName::ConflictingModifierKind => {
                return format_english_syntax_kind_without_suffix(*kind, "_keyword");
            }
            DiagnosticArgName::DirectiveKind | DiagnosticArgName::ConflictingDirectiveKind => {
                return format_english_syntax_kind_without_suffix(*kind, "_directive");
            }
            DiagnosticArgName::TraitMemberName => {
                return format_english_quoted_text(&format_english_syntax_kind_without_suffix(
                    *kind, "_keyword",
                ));
            }
            _ => {}
        }
    }

    match value {
        DiagnosticArgValue::Count(count) => count.to_string(),
        DiagnosticArgValue::Byte(byte) => format!("0x{byte:02X}"),
        DiagnosticArgValue::ByteCount(byte_count) => byte_count.to_string(),
        DiagnosticArgValue::ArtifactDigest(digest) => format_english_artifact_digest(digest),
        DiagnosticArgValue::ArtifactKind(kind) => format_english_artifact_kind(*kind).to_owned(),
        DiagnosticArgValue::ArtifactOrdinal(ordinal) => ordinal.to_string(),
        DiagnosticArgValue::TargetRepresentation(kind) => {
            format_english_target_representation(*kind).to_owned()
        }
        DiagnosticArgValue::CallableAbi(abi) => format_english_callable_abi(*abi).to_owned(),
        DiagnosticArgValue::AlignmentKind(kind) => format_english_alignment_kind(*kind).to_owned(),
        DiagnosticArgValue::Character(character) => format_english_character(*character),
        DiagnosticArgValue::DeclarationName(name) => format_english_quoted_text(name),
        DiagnosticArgValue::ReferencedName(name) => format_english_quoted_text(name),
        DiagnosticArgValue::CodegenBackendIdentity(identity) => {
            format_english_quoted_text(identity)
        }
        DiagnosticArgValue::CodegenVerificationStage(stage) => match stage {
            bray_diagnostics::DiagnosticCodegenVerificationStage::BeforeOptimization => {
                "before optimization".to_owned()
            }
            bray_diagnostics::DiagnosticCodegenVerificationStage::AfterOptimization => {
                "after optimization".to_owned()
            }
        },
        DiagnosticArgValue::CodegenBackendReport(report) => format_english_quoted_text(report),
        DiagnosticArgValue::LinkerDriverIdentity(identity) => {
            format_english_linker_driver_identity(identity)
        }
        DiagnosticArgValue::PackageIdentity(identity) => format_english_quoted_text(identity),
        DiagnosticArgValue::ProductIdentity(identity) => format_english_quoted_text(identity),
        DiagnosticArgValue::SymbolKind(kind) => format_english_quoted_text(&kind.replace('_', " ")),
        DiagnosticArgValue::TargetTriple(target) => format_english_quoted_text(target),
        DiagnosticArgValue::TargetIdentity(target) => format_english_quoted_text(target),
        DiagnosticArgValue::NativeProductFailureKind(kind) => {
            format_english_native_product_failure(*kind)
        }
        DiagnosticArgValue::EmissionFailure(failure) => format_english_emission_failure(failure),
        DiagnosticArgValue::EmissionArtifactOperation(kind) => {
            format_english_emission_artifact_operation(*kind).to_owned()
        }
        DiagnosticArgValue::LinkInputKind(kind) => format_english_link_input_kind(*kind).to_owned(),
        DiagnosticArgValue::LinkedArtifactKind(kind) => {
            format_english_linked_artifact_kind(*kind).to_owned()
        }
        DiagnosticArgValue::LinkedProductKind(kind) => {
            format_english_linked_product_kind(*kind).to_owned()
        }
        DiagnosticArgValue::ProductKind(kind) => format_english_product_kind(*kind).to_owned(),
        DiagnosticArgValue::DebugInformationMode(kind) => {
            format_english_debug_information_mode(*kind).to_owned()
        }
        DiagnosticArgValue::DebugOutputMode(kind) => {
            format_english_debug_output_mode(*kind).to_owned()
        }
        DiagnosticArgValue::AssemblySyntax(kind) => {
            format_english_assembly_syntax(*kind).to_owned()
        }
        DiagnosticArgValue::ArtifactRequirement(kind) => {
            format_english_artifact_requirement(*kind).to_owned()
        }
        DiagnosticArgValue::NameKind(kind) => format_english_name_kind(*kind).to_owned(),
        DiagnosticArgValue::FilePath(path) => format_english_path(path),
        DiagnosticArgValue::InputIndex(input_index) => input_index.to_string(),
        DiagnosticArgValue::InterfaceLimit(limit) => {
            format_english_interface_limit(*limit).to_owned()
        }
        DiagnosticArgValue::InterfaceSection(section) => {
            format_english_interface_section(*section).to_owned()
        }
        DiagnosticArgValue::InterfaceSymbolIdentity(identity) => {
            format_english_interface_symbol_identity(identity)
        }
        DiagnosticArgValue::InterfaceSymbolGraphProblem(problem) => {
            format_english_interface_symbol_graph_problem(problem)
        }
        DiagnosticArgValue::InterfaceSemanticProblem(problem) => {
            format_english_interface_semantic_problem(problem)
        }
        DiagnosticArgValue::ProjectSelectionProblem(problem) => {
            format_english_project_selection_problem(problem)
        }
        DiagnosticArgValue::ProjectCommandFailure(failure) => {
            format_english_project_command_failure(failure)
        }
        DiagnosticArgValue::ProjectManifestField(field) => {
            format_english_project_manifest_field(*field).to_owned()
        }
        DiagnosticArgValue::ProjectDependencyCycleMember(member) => {
            format_english_project_dependency_cycle_member(member)
        }
        DiagnosticArgValue::TargetPredicateValueKind(kind) => {
            format_english_target_predicate_value_kind(*kind).to_owned()
        }
        DiagnosticArgValue::UnsupportedEmissionReason(reason) => {
            format_english_unsupported_emission_reason(reason)
        }
        DiagnosticArgValue::IoErrorKind(kind) => format_english_io_error_kind(*kind).to_owned(),
        DiagnosticArgValue::ExternalToolOperation(kind) => {
            format_english_external_tool_operation(*kind).to_owned()
        }
        DiagnosticArgValue::ExternalToolFailureKind(kind) => {
            format_english_external_tool_failure(*kind).to_owned()
        }
        DiagnosticArgValue::ExternalToolExit(exit) => format_english_external_tool_exit(exit),
        DiagnosticArgValue::LinkRequirement(requirement) => {
            format_english_link_requirement(requirement)
        }
        DiagnosticArgValue::LinkOptimizationReportProblem(problem) => {
            format_english_link_optimization_report_problem(*problem).to_owned()
        }
        DiagnosticArgValue::DocumentParseKind(kind) => {
            format_english_document_parse_kind(*kind).to_owned()
        }
        DiagnosticArgValue::StandardLibraryManifestProblem(problem) => {
            format_english_standard_library_manifest_problem(*problem).to_owned()
        }
        DiagnosticArgValue::DependencySubjectKind(kind) => {
            format_english_dependency_subject(*kind).to_owned()
        }
        DiagnosticArgValue::DependencyRequirementKind(kind) => {
            format_english_dependency_requirement(*kind).to_owned()
        }
        DiagnosticArgValue::OutputSink(sink) => format_english_output_sink(sink),
        DiagnosticArgValue::Visibility(visibility) => visibility.as_str().to_owned(),
        DiagnosticArgValue::ModuleTrust(trust) => format_english_module_trust(*trust).to_owned(),
        DiagnosticArgValue::SourceName(name) => name.clone(),
        DiagnosticArgValue::SourceCount(source_count) => source_count.to_string(),
        DiagnosticArgValue::SourceInputKind(kind) => {
            format_english_source_input_kind(*kind).to_owned()
        }
        DiagnosticArgValue::SourceInput(input) => format_english_source_input(input),
        DiagnosticArgValue::SyntaxKind(kind) => format_english_syntax_kind(*kind),
        DiagnosticArgValue::TextOffset(offset) => offset.bytes().to_string(),
        DiagnosticArgValue::TokenText(text) => format_english_quoted_text(text),
        DiagnosticArgValue::Uri(uri) => uri.clone(),
        DiagnosticArgValue::SourceSpan(span) => format_source_span(*span),
        DiagnosticArgValue::WorkerCount(worker_count) => worker_count.to_string(),
        DiagnosticArgValue::Revision(revision) => revision.to_string(),
        DiagnosticArgValue::RuntimeAbi(version) => {
            format!("{}.{}", version.major(), version.minor())
        }
        DiagnosticArgValue::RuntimeArtifactProblem(problem) => {
            format_english_runtime_artifact_problem(problem)
        }
        DiagnosticArgValue::Type(ty) => format_english_type(ty),
        DiagnosticArgValue::SelectionKind(kind) => format_english_selection_kind(*kind).to_owned(),
        DiagnosticArgValue::SelectionCandidates(candidates) => {
            format_english_selection_candidates(candidates)
        }
        DiagnosticArgValue::SelectionRejections(rejections) => {
            format_english_selection_rejections(rejections)
        }
        DiagnosticArgValue::NativeLinkDirectiveProblem(problem) => {
            format_english_native_link_directive_problem(problem)
        }
        DiagnosticArgValue::NativeSymbolDirectiveProblem(problem) => {
            format_english_native_symbol_directive_problem(problem)
        }
        DiagnosticArgValue::PlatformServiceSignatureProblem(problem) => {
            format_english_platform_service_signature_problem(problem)
        }
        DiagnosticArgValue::TraitFulfillmentMismatch(mismatch) => {
            format_english_trait_fulfillment_mismatch(mismatch)
        }
        DiagnosticArgValue::ImplementationOverloadProblem(problem) => {
            format_english_implementation_overload_problem(problem)
        }
        DiagnosticArgValue::CallableOverloadProblem(problem) => {
            format_english_callable_overload_problem(problem)
        }
        DiagnosticArgValue::ExpressionCategory(kind) => {
            format_english_expression_category(*kind).to_owned()
        }
        DiagnosticArgValue::ConstantOperation(operation) => {
            format_english_constant_operation(*operation).to_owned()
        }
        DiagnosticArgValue::LayoutProblem(problem) => format_english_layout_problem(problem),
        DiagnosticArgValue::UnionTagProblem(problem) => format_english_union_tag_problem(problem),
        DiagnosticArgValue::CopyContractProblem(problem) => {
            format_english_copy_contract_problem(*problem)
        }
        DiagnosticArgValue::StoredTypeProblem(problem) => {
            format_english_stored_type_problem(*problem)
        }
        DiagnosticArgValue::PropagationProblem(problem) => {
            format_english_propagation_problem(problem)
        }
        DiagnosticArgValue::ArrayGeneratorCardinalityProblem(problem) => {
            format_english_array_generator_problem(problem)
        }
        DiagnosticArgValue::RefinementCapacity(capacity) => {
            format_english_refinement_capacity(*capacity)
        }
        DiagnosticArgValue::MemoryOperation(operation) => {
            format_english_memory_operation(*operation).to_owned()
        }
        DiagnosticArgValue::CallbackStateProblem(problem) => {
            format_english_callback_state_problem(*problem)
        }
        DiagnosticArgValue::StorageAccess(access) => format_english_storage_access(access),
        DiagnosticArgValue::PatternCoverage(coverage) => format_english_pattern_coverage(coverage),
        DiagnosticArgValue::PatternUnreachability(reason) => {
            format_english_pattern_unreachability(*reason).to_owned()
        }
    }
}
