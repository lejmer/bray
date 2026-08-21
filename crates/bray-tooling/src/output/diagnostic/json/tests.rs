use std::fs;
use std::path::{Path, PathBuf};

use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArgName, DiagnosticArgValue,
    DiagnosticArrayGeneratorCardinalityProblem, DiagnosticArrayLength, DiagnosticArtifactDigest,
    DiagnosticArtifactDigestAlgorithm, DiagnosticBag, DiagnosticCallableOverloadArm,
    DiagnosticCallableOverloadProblem, DiagnosticCallbackStateProblem,
    DiagnosticConstructionInputRejection, DiagnosticDependencySubjectKind,
    DiagnosticEmissionEvaluationFailure, DiagnosticEmissionFailure,
    DiagnosticGenericParameterCategory, DiagnosticId, DiagnosticInterfaceDeclarationIdentity,
    DiagnosticInterfaceLimit, DiagnosticInterfaceSection, DiagnosticInterfaceSymbolIdentity,
    DiagnosticInterfaceSymbolKind, DiagnosticInterfaceSynthesizedIdentity, DiagnosticKind,
    DiagnosticLayoutOption, DiagnosticLayoutProblem, DiagnosticMemoryOperation,
    DiagnosticModuleTrust, DiagnosticNameKind, DiagnosticNamedType,
    DiagnosticNativeProductFailureKind, DiagnosticNote, DiagnosticNoteKind, DiagnosticOutputSink,
    DiagnosticPatternCoverage, DiagnosticPatternMissingCase, DiagnosticProjectManifestField,
    DiagnosticPropagationProblem, DiagnosticRefinementCapacity,
    DiagnosticRefinementCapacitySurface, DiagnosticRejectedSelectionCandidate,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, DiagnosticRuntimeAbiVersion,
    DiagnosticRuntimeArtifactProblem, DiagnosticSelectionCandidate,
    DiagnosticSelectionCandidateIdentity, DiagnosticSelectionCandidateSignature,
    DiagnosticSelectionCandidates, DiagnosticSelectionKind, DiagnosticSelectionRejectionReason,
    DiagnosticSelectionRejections, DiagnosticSourceEdit, DiagnosticStorageAccess,
    DiagnosticStorageAccessPurpose, DiagnosticStorageProjection, DiagnosticStorageRoot,
    DiagnosticSuggestion, DiagnosticSuggestionApplicability, DiagnosticSuggestionKind,
    DiagnosticTargetPredicateValueKind, DiagnosticTraitFulfillmentMismatch, DiagnosticType,
    DiagnosticTypeArgument, DiagnosticVisibility, DiagnosticYieldCardinality, SeverityKind,
};
use bray_source::{SourceSpan, TextRange, TextSize};
use bray_syntax::SyntaxKind;

use super::{DiagnosticArtifactDigestJson, DiagnosticOutputSinkJson, write_json_diagnostics};
use crate::output::diagnostic::test_support::file_source_store;
use crate::product::NativeLinkerBuildError;
use crate::toolchain::LlvmToolPathError;

const JSON_SOURCE_INVENTORY: &[&str] = &[
    "output/diagnostic/json.rs",
    "output/diagnostic/json/argument.rs",
    "output/diagnostic/json/checking.rs",
    "output/diagnostic/json/checking/analysis.rs",
    "output/diagnostic/json/checking/candidate.rs",
    "output/diagnostic/json/checking/contract_mismatch.rs",
    "output/diagnostic/json/checking/overload.rs",
    "output/diagnostic/json/checking/selection.rs",
    "output/diagnostic/json/checking/trait_mismatch.rs",
    "output/diagnostic/json/emission.rs",
    "output/diagnostic/json/emission/context.rs",
    "output/diagnostic/json/emission/failure.rs",
    "output/diagnostic/json/foreign.rs",
    "output/diagnostic/json/interface.rs",
    "output/diagnostic/json/interface/identity.rs",
    "output/diagnostic/json/interface/validation.rs",
    "output/diagnostic/json/project.rs",
    "output/diagnostic/json/project/command.rs",
    "output/diagnostic/json/project/execution.rs",
    "output/diagnostic/json/project/inspection.rs",
    "output/diagnostic/json/report.rs",
    "output/diagnostic/json/runtime.rs",
    "output/diagnostic/json/tests.rs",
    "output/diagnostic/json/value.rs",
];
const JSON_SOURCE_INVENTORY_TEST: &str = "output/diagnostic/json/tests.rs";

#[test]
fn json_source_inventory_is_complete_and_production_sources_have_no_recovery_sentinels() {
    let source_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    let mut actual = vec!["output/diagnostic/json.rs".to_owned()];

    collect_rust_sources(
        &source_root.join("output/diagnostic/json"),
        &source_root,
        &mut actual,
    );

    actual.sort();

    assert_eq!(actual, JSON_SOURCE_INVENTORY);

    for relative in JSON_SOURCE_INVENTORY
        .iter()
        .copied()
        .filter(|relative| *relative != JSON_SOURCE_INVENTORY_TEST)
    {
        let source = fs::read_to_string(source_root.join(relative))
            .unwrap_or_else(|error| panic!("JSON source {relative} must be readable: {error:?}"));

        for forbidden in ["tokens truncated", "…", "43404"] {
            assert!(
                !source.contains(forbidden),
                "JSON source {relative} contains recovery sentinel {forbidden:?}"
            );
        }
    }
}

fn collect_rust_sources(directory: &Path, source_root: &Path, sources: &mut Vec<String>) {
    let entries = fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("JSON source directory must be readable: {error:?}"));

    for entry in entries {
        let entry =
            entry.unwrap_or_else(|error| panic!("JSON source entry must be valid: {error:?}"));

        let path = entry.path();

        if path.is_dir() {
            collect_rust_sources(&path, source_root, sources);
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            let relative = path.strip_prefix(source_root).unwrap_or_else(|error| {
                panic!("JSON source must be beneath crate source: {error:?}")
            });

            sources.push(relative.to_string_lossy().replace('\\', "/"));
        }
    }
}

#[test]
fn json_output_serializes_runtime_artifact_problems_with_typed_details() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::RuntimeArtifactMetadataInvalid,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::artifact_path("runtime/bray-runtime.brayrt"))
    .with_arg(DiagnosticArg::runtime_artifact_problem(
        DiagnosticRuntimeArtifactProblem::InvalidComponentDependency {
            component: "runtime.scheduler".to_owned(),
            dependency: "runtime.reactor".to_owned(),
        },
    ));

    let mut output = Vec::new();

    write_json_diagnostics(&DiagnosticBag::single(diagnostic), None, &mut output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should write: {error:?}"));

    let output: serde_json::Value = serde_json::from_slice(&output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should parse: {error:?}"));

    let argument = &output["diagnostics"][0]["args"][1];

    assert_eq!(argument["name"], "runtime_artifact_problem");
    assert_eq!(argument["value"]["kind"], "runtime_artifact_problem");

    assert_eq!(
        argument["value"]["value"]["category"],
        "invalid_component_dependency"
    );

    assert_eq!(argument["value"]["value"]["component"], "runtime.scheduler");

    assert_eq!(argument["value"]["value"]["dependency"], "runtime.reactor");
}

#[test]
fn unsupported_emission_reasons_keep_exact_json_payloads() {
    let failures = [
        (
            NativeLinkerBuildError::Tool(LlvmToolPathError::Unavailable {
                tool: bray_diagnostics::DiagnosticLlvmToolRole::Archiver,
                configured: Some(PathBuf::from("toolchain/bin/llvm-ar")),
            }),
            "tool_unavailable",
        ),
        (
            NativeLinkerBuildError::Tool(LlvmToolPathError::CandidateInspection {
                tool: bray_diagnostics::DiagnosticLlvmToolRole::CompilerDriver,
                path: PathBuf::from("toolchain/bin/clang"),
                error: std::io::ErrorKind::PermissionDenied,
            }),
            "tool_inspection_failed",
        ),
        (
            NativeLinkerBuildError::Tool(LlvmToolPathError::InvalidCandidate {
                tool: bray_diagnostics::DiagnosticLlvmToolRole::SymbolInspector,
                path: PathBuf::from("toolchain/bin/llvm-nm"),
            }),
            "invalid_tool_file",
        ),
        (
            NativeLinkerBuildError::MissingEnvironment(
                bray_diagnostics::DiagnosticHostEnvironmentVariable::WindowsSystemRoot,
            ),
            "missing_host_environment",
        ),
    ];

    for (failure, expected_reason) in failures {
        let diagnostic = failure.diagnostic("x86_64-pc-windows-msvc");
        let mut output = Vec::new();

        write_json_diagnostics(&DiagnosticBag::single(diagnostic), None, &mut output)
            .unwrap_or_else(|error| panic!("JSON diagnostics should write: {error:?}"));

        let output: serde_json::Value = serde_json::from_slice(&output)
            .unwrap_or_else(|error| panic!("JSON diagnostics should parse: {error:?}"));

        let reason = &output["diagnostics"][0]["args"][1]["value"]["value"]["reason"];

        assert_eq!(reason, expected_reason);
    }
}

#[test]
fn emission_evaluation_failures_use_domain_named_json_categories() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::EmissionFailed,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::emission_failure(
        DiagnosticEmissionFailure::Evaluation(DiagnosticEmissionEvaluationFailure::Cycle),
    ));

    let mut output = Vec::new();

    write_json_diagnostics(&DiagnosticBag::single(diagnostic), None, &mut output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should write: {error:?}"));

    let output: serde_json::Value = serde_json::from_slice(&output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should parse: {error:?}"));

    let failure = &output["diagnostics"][0]["args"][0]["value"]["value"];

    assert_eq!(failure["category"], "evaluation");
    assert_eq!(failure["reason"], "cycle");
}

#[test]
fn json_output_serializes_structured_diagnostics() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(3),
        DiagnosticKind::SourceInvalidUtf8,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::text_offset(TextSize::new(5)))
    .with_note(DiagnosticNote::new(DiagnosticNoteKind::SourceMustBeUtf8));

    let bag = DiagnosticBag::single(diagnostic);

    let mut output = Vec::new();

    match write_json_diagnostics(&bag, None, &mut output) {
        Ok(()) => {}
        Err(error) => panic!("JSON diagnostics should write: {error:?}"),
    }

    let output = match String::from_utf8(output) {
        Ok(output) => output,
        Err(error) => panic!("JSON diagnostics should be UTF-8: {error:?}"),
    };

    let output_json: serde_json::Value = match serde_json::from_str(&output) {
        Ok(value) => value,
        Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
    };

    assert_eq!(output_json["has_errors"], true);

    let diagnostic_json = &output_json["diagnostics"][0];

    assert_eq!(diagnostic_json["id"], 3);
    assert_eq!(diagnostic_json["code"], 1002);
    assert_eq!(diagnostic_json["kind"], "source_invalid_utf8");
    assert_eq!(diagnostic_json["severity"], "error");

    assert!(diagnostic_json.get("message").is_none());

    let arg_json = &diagnostic_json["args"][0];

    assert_eq!(arg_json["name"], "text_offset");
    assert_eq!(arg_json["value"]["kind"], "text_offset");
    assert_eq!(arg_json["value"]["value"], 5);

    assert!(!output.contains("source input contains invalid UTF-8 at byte offset"));
}

#[test]
fn target_predicate_value_kind_mismatch_keeps_typed_json_context() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::ProjectManifestTargetPredicateValueKindMismatch,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::file_path("bray-package.json"))
    .with_arg(DiagnosticArg::project_manifest_field(
        DiagnosticProjectManifestField::TargetPredicate,
    ))
    .with_arg(DiagnosticArg::referenced_name("target.pointer.BITS"))
    .with_arg(DiagnosticArg::expected_target_predicate_value_kind(
        DiagnosticTargetPredicateValueKind::UnsignedInteger,
    ))
    .with_arg(DiagnosticArg::actual_target_predicate_value_kind(
        DiagnosticTargetPredicateValueKind::String,
    ));

    let mut output = Vec::new();

    write_json_diagnostics(&DiagnosticBag::single(diagnostic), None, &mut output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should write: {error:?}"));

    let output: serde_json::Value = serde_json::from_slice(&output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should parse: {error:?}"));

    let arguments = output["diagnostics"][0]["args"]
        .as_array()
        .unwrap_or_else(|| panic!("diagnostic arguments should be an array"));

    assert!(arguments.iter().any(|argument| {
        argument["name"] == "expected_target_predicate_value_kind"
            && argument["value"]["value"] == "unsigned_integer"
    }));

    assert!(arguments.iter().any(|argument| {
        argument["name"] == "actual_target_predicate_value_kind"
            && argument["value"]["value"] == "string"
    }));
}

#[test]
fn json_output_preserves_related_locations_and_suggestions() {
    let sources = file_source_store("let first = 1;\nlet first = 2;");

    let first = SourceSpan::new(
        bray_source::SourceId::new(0),
        TextRange::new(TextSize::new(4), TextSize::new(9)),
    );

    let duplicate = SourceSpan::new(
        bray_source::SourceId::new(0),
        TextRange::new(TextSize::new(19), TextSize::new(24)),
    );

    let suggestion = DiagnosticSuggestion::try_edits(
        DiagnosticSuggestionKind::InsertExpectedSyntax,
        DiagnosticSuggestionApplicability::MachineApplicable,
        [DiagnosticSourceEdit::new(
            SourceSpan::empty(bray_source::SourceId::new(0), TextSize::new(31)),
            ";",
        )],
    )
    .unwrap_or_else(|error| panic!("test suggestion should be valid: {error:?}"));

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::DeclarationDuplicateName,
        SeverityKind::Error,
    )
    .with_primary_span(duplicate)
    .with_arg(DiagnosticArg::declaration_name("first"))
    .with_related_location(DiagnosticRelatedLocation::new(
        DiagnosticRelatedLocationKind::FirstDeclaration,
        first,
    ))
    .with_suggestion(suggestion);

    let mut output = Vec::new();

    write_json_diagnostics(
        &DiagnosticBag::single(diagnostic),
        Some(&sources),
        &mut output,
    )
    .unwrap_or_else(|error| panic!("JSON diagnostics should write: {error:?}"));

    let output: serde_json::Value = serde_json::from_slice(&output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should parse: {error:?}"));

    let diagnostic = &output["diagnostics"][0];

    assert_eq!(
        diagnostic["related_locations"][0]["kind"],
        "first_declaration"
    );

    assert_eq!(
        diagnostic["related_locations"][0]["span"]["location"]["start"]["line"],
        1
    );

    assert_eq!(
        diagnostic["suggestions"][0]["applicability"],
        "machine_applicable"
    );

    assert_eq!(diagnostic["suggestions"][0]["edits"][0]["replacement"], ";");
}

#[test]
fn json_output_preserves_typed_binding_arguments() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::BindingWrongNameKind,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::referenced_name("Size"))
    .with_arg(DiagnosticArg::expected_name_kind(DiagnosticNameKind::Type));

    let mut output = Vec::new();

    match write_json_diagnostics(&DiagnosticBag::single(diagnostic), None, &mut output) {
        Ok(()) => {}
        Err(error) => panic!("JSON diagnostics should write: {error:?}"),
    }

    let output: serde_json::Value = match serde_json::from_slice(&output) {
        Ok(value) => value,
        Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
    };

    let arguments = &output["diagnostics"][0]["args"];

    assert_eq!(arguments[0]["name"], "referenced_name");
    assert_eq!(arguments[0]["value"]["kind"], "referenced_name");
    assert_eq!(arguments[0]["value"]["value"], "Size");
    assert_eq!(arguments[1]["name"], "expected_name_kind");
    assert_eq!(arguments[1]["value"]["kind"], "name_kind");
    assert_eq!(arguments[1]["value"]["value"], "type");
}

#[test]
fn json_output_preserves_typed_checker_arguments() {
    let type_diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingIncompatibleExpressionType,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::expected_type(DiagnosticType::Named(
        DiagnosticNamedType::new(
            [String::from("collection"), String::from("MoveCursor")],
            [DiagnosticTypeArgument::Type(DiagnosticType::I32)],
        ),
    )))
    .with_arg(DiagnosticArg::actual_type(DiagnosticType::Named(
        DiagnosticNamedType::new(
            [String::from("collection"), String::from("ReadCursor")],
            [DiagnosticTypeArgument::Type(DiagnosticType::I32)],
        ),
    )));

    let selection_diagnostic = Diagnostic::new(
        DiagnosticId::new(1),
        DiagnosticKind::CheckingAmbiguousCandidate,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::selection_kind(
        DiagnosticSelectionKind::Operator,
    ))
    .with_arg(DiagnosticArg::selection_candidates(
        DiagnosticSelectionCandidates::new([
            DiagnosticSelectionCandidate::new(
                DiagnosticSelectionCandidateIdentity::NamedDeclaration {
                    identity: DiagnosticInterfaceSymbolIdentity::SourceDeclaration {
                        owner: Box::new(DiagnosticInterfaceSymbolIdentity::Module {
                            owner: Box::new(DiagnosticInterfaceSymbolIdentity::Package(
                                String::from("example.package"),
                            )),
                            path: Box::new([String::from("math")]),
                        }),
                        kind: DiagnosticInterfaceSymbolKind::Function,
                        declaration: 3,
                    },
                    name: String::from("combine"),
                },
                DiagnosticSelectionCandidateSignature::Callable {
                    parameter_types: Box::new([DiagnosticType::I32, DiagnosticType::I32]),
                    result_type: DiagnosticType::I32,
                },
            ),
            DiagnosticSelectionCandidate::new(
                DiagnosticSelectionCandidateIdentity::BuiltIn,
                DiagnosticSelectionCandidateSignature::Operation {
                    operand_types: Box::new([DiagnosticType::I32, DiagnosticType::I32]),
                    result_type: Some(DiagnosticType::I32),
                },
            ),
        ]),
    ));

    let trait_diagnostic = Diagnostic::new(
        DiagnosticId::new(2),
        DiagnosticKind::CheckingIncompatibleTraitFulfillment,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::trait_fulfillment_mismatch(
        DiagnosticTraitFulfillmentMismatch::GenericParameterCategory {
            ordinal: 1,
            required: DiagnosticGenericParameterCategory::Type,
            provided: DiagnosticGenericParameterCategory::Constant,
        },
    ));

    let overload_diagnostic = Diagnostic::new(
        DiagnosticId::new(3),
        DiagnosticKind::CheckingConflictingCallableOverloadSignature,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::callable_overload_problem(
        DiagnosticCallableOverloadProblem::ConflictingSignatures {
            arm: DiagnosticCallableOverloadArm::new(
                DiagnosticInterfaceSymbolIdentity::Package(String::from("example.first")),
                false,
                vec![DiagnosticType::Boolean],
                DiagnosticType::Unit,
            ),
            conflicting: DiagnosticCallableOverloadArm::new(
                DiagnosticInterfaceSymbolIdentity::Package(String::from("example.second")),
                false,
                vec![DiagnosticType::Boolean],
                DiagnosticType::Unit,
            ),
        },
    ));

    let rejection_diagnostic = Diagnostic::new(
        DiagnosticId::new(4),
        DiagnosticKind::CheckingIncompatibleCandidate,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::selection_kind(
        DiagnosticSelectionKind::Construction,
    ))
    .with_arg(DiagnosticArg::selection_rejections(
        DiagnosticSelectionRejections::try_from_prefix(
            [DiagnosticRejectedSelectionCandidate::new(
                DiagnosticSelectionCandidate::new(
                    DiagnosticSelectionCandidateIdentity::BuiltIn,
                    DiagnosticSelectionCandidateSignature::Operation {
                        operand_types: Box::new([DiagnosticType::I32]),
                        result_type: Some(DiagnosticType::I32),
                    },
                ),
                DiagnosticSelectionRejectionReason::ConstructionInput(
                    DiagnosticConstructionInputRejection::UnknownName {
                        provided: String::from("y"),
                        accepted: Box::new([String::from("x")]),
                    },
                ),
            )],
            1,
        )
        .unwrap_or_else(|_| panic!("one rejection must fit the diagnostic bound")),
    ));

    let mut output = Vec::new();

    match write_json_diagnostics(
        &DiagnosticBag::from(vec![
            type_diagnostic,
            selection_diagnostic,
            trait_diagnostic,
            overload_diagnostic,
            rejection_diagnostic,
        ]),
        None,
        &mut output,
    ) {
        Ok(()) => {}
        Err(error) => panic!("JSON diagnostics should write: {error:?}"),
    }

    let output: serde_json::Value = match serde_json::from_slice(&output) {
        Ok(value) => value,
        Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
    };

    let arguments = &output["diagnostics"][0]["args"];

    assert_eq!(arguments[0]["name"], "expected_type");
    assert_eq!(arguments[0]["value"]["kind"], "type");
    assert_eq!(arguments[0]["value"]["value"]["kind"], "named");

    assert_eq!(
        arguments[0]["value"]["value"]["path"],
        serde_json::json!(["collection", "MoveCursor"])
    );

    assert_eq!(
        arguments[0]["value"]["value"]["arguments"][0]["kind"],
        "type"
    );

    assert_eq!(
        arguments[0]["value"]["value"]["arguments"][0]["type"]["kind"],
        "i32"
    );

    assert_eq!(arguments[1]["name"], "actual_type");

    assert_eq!(
        arguments[1]["value"]["value"]["path"],
        serde_json::json!(["collection", "ReadCursor"])
    );

    let selection = &output["diagnostics"][1]["args"][0];

    assert_eq!(selection["name"], "selection_kind");
    assert_eq!(selection["value"]["kind"], "selection_kind");
    assert_eq!(selection["value"]["value"], "operator");

    let candidates = &output["diagnostics"][1]["args"][1]["value"]["value"];
    assert_eq!(candidates["omitted_count"], 0);

    assert_eq!(
        candidates["candidates"][0]["identity"]["kind"],
        "named_declaration"
    );

    assert_eq!(candidates["candidates"][0]["identity"]["name"], "combine");

    assert_eq!(
        candidates["candidates"][0]["identity"]["identity"]["kind"],
        "source_declaration"
    );

    assert_eq!(
        candidates["candidates"][0]["signature"]["parameter_types"][0]["kind"],
        "i32"
    );

    assert_eq!(candidates["candidates"][1]["identity"]["kind"], "built_in");

    let mismatch = &output["diagnostics"][2]["args"][0]["value"]["value"];
    assert_eq!(mismatch["reason"], "generic_parameter_category");
    assert_eq!(mismatch["ordinal"], 1);
    assert_eq!(mismatch["required"], "type");
    assert_eq!(mismatch["provided"], "constant");

    let overload = &output["diagnostics"][3]["args"][0]["value"]["value"];
    assert_eq!(overload["reason"], "conflicting_signatures");
    assert_eq!(overload["arm"]["identity"]["kind"], "package");
    assert_eq!(overload["arm"]["parameter_types"][0]["kind"], "boolean");
    assert_eq!(overload["arm"]["result_type"]["kind"], "unit");

    assert_eq!(
        overload["conflicting"]["identity"]["package"],
        "example.second"
    );

    let rejections = &output["diagnostics"][4]["args"][1]["value"]["value"];
    assert_eq!(rejections["omitted_count"], 0);

    assert_eq!(
        rejections["rejections"][0]["candidate"]["identity"]["kind"],
        "built_in"
    );

    assert_eq!(
        rejections["rejections"][0]["mismatch"]["reason"],
        "construction_input"
    );

    assert_eq!(
        rejections["rejections"][0]["mismatch"]["mismatch"]["reason"],
        "unknown_name"
    );

    assert_eq!(
        rejections["rejections"][0]["mismatch"]["mismatch"]["provided"],
        "y"
    );

    assert_eq!(
        rejections["rejections"][0]["mismatch"]["mismatch"]["accepted"],
        serde_json::json!(["x"])
    );
}

#[test]
fn json_output_preserves_bounded_pattern_missing_cases() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingNonExhaustiveMatch,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::pattern_coverage(
        DiagnosticPatternCoverage::new(
            DiagnosticType::Named(DiagnosticNamedType::new([String::from("Choice")], [])),
            [
                DiagnosticPatternMissingCase::UnionVariant(String::from("Second")),
                DiagnosticPatternMissingCase::RemainingValues,
            ],
            3,
        ),
    ));

    let mut output = Vec::new();

    write_json_diagnostics(&DiagnosticBag::from(vec![diagnostic]), None, &mut output)
        .unwrap_or_else(|error| panic!("pattern JSON diagnostics should write: {error:?}"));

    let output: serde_json::Value = serde_json::from_slice(&output)
        .unwrap_or_else(|error| panic!("pattern JSON diagnostics should parse: {error:?}"));

    let coverage = &output["diagnostics"][0]["args"][0]["value"]["value"];

    assert_eq!(coverage["subject_type"]["kind"], "named");
    assert_eq!(coverage["missing_cases"][0]["kind"], "union_variant");
    assert_eq!(coverage["missing_cases"][0]["name"], "Second");
    assert_eq!(coverage["missing_cases"][1]["kind"], "remaining_values");
    assert_eq!(coverage["omitted_count"], 3);
}

#[test]
fn json_output_preserves_propagation_and_array_cardinality_causes() {
    let diagnostics = DiagnosticBag::from(vec![
        Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::propagation_problem(
            DiagnosticPropagationProblem::NullableBoundaryUnavailable {
                operand: DiagnosticType::Nullable,
                available_boundaries: vec![DiagnosticType::I32].into_boxed_slice(),
            },
        )),
        Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::CheckingNoCompatiblePropagationBoundary,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::propagation_problem(
            DiagnosticPropagationProblem::ResultBoundaryUnavailable {
                source_error: DiagnosticType::I32,
                available_errors: vec![DiagnosticType::I64].into_boxed_slice(),
            },
        )),
        Diagnostic::new(
            DiagnosticId::new(2),
            DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::array_generator_cardinality_problem(
            DiagnosticArrayGeneratorCardinalityProblem::SourceCountUnavailable {
                source: DiagnosticType::Unknown,
                element: DiagnosticType::Boolean,
                required: DiagnosticArrayLength::Exact(2),
            },
        )),
        Diagnostic::new(
            DiagnosticId::new(3),
            DiagnosticKind::CheckingArrayGeneratorCardinalityNotProvable,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::array_generator_cardinality_problem(
            DiagnosticArrayGeneratorCardinalityProblem::YieldCountNotExact {
                element: DiagnosticType::Boolean,
                source_length: DiagnosticArrayLength::Symbolic,
                required: DiagnosticArrayLength::Exact(2),
                actual: DiagnosticYieldCardinality::Multiple,
            },
        )),
    ]);

    let mut output = Vec::new();

    write_json_diagnostics(&diagnostics, None, &mut output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should write: {error:?}"));

    let output: serde_json::Value = serde_json::from_slice(&output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should parse: {error:?}"));

    let values = output["diagnostics"]
        .as_array()
        .unwrap_or_else(|| panic!("diagnostics must be a JSON array: {output:?}"));

    assert_eq!(
        values[0]["args"][0]["value"]["value"]["reason"],
        "nullable_boundary_unavailable"
    );

    assert_eq!(
        values[1]["args"][0]["value"]["value"]["reason"],
        "result_boundary_unavailable"
    );

    assert_eq!(
        values[2]["args"][0]["value"]["value"]["reason"],
        "source_count_unavailable"
    );

    assert_eq!(
        values[2]["args"][0]["value"]["value"]["context"][2]["value"],
        serde_json::json!({
            "kind": "array_length",
            "value": { "kind": "exact", "value": 2 }
        })
    );

    assert_eq!(
        values[3]["args"][0]["value"]["value"]["reason"],
        "yield_count_not_exact"
    );

    assert_eq!(
        values[3]["args"][0]["value"]["value"]["context"][3]["value"],
        serde_json::json!({ "kind": "text", "value": "multiple" })
    );
}

#[test]
fn json_output_preserves_exact_refinement_capacity() {
    let capacity = DiagnosticRefinementCapacity::try_new(
        DiagnosticRefinementCapacitySurface::RetainedStateCells,
        16_777_217,
        16_777_216,
    )
    .unwrap_or_else(|| panic!("limit plus one must be a capacity violation"));

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingRefinementCapacityExceeded,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::refinement_capacity(capacity));

    let mut output = Vec::new();

    write_json_diagnostics(&DiagnosticBag::single(diagnostic), None, &mut output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should write: {error:?}"));

    let output: serde_json::Value = serde_json::from_slice(&output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should parse: {error:?}"));

    let value = &output["diagnostics"][0]["args"][0]["value"];

    assert_eq!(value["kind"], "refinement_capacity");
    assert_eq!(value["value"]["reason"], "capacity_exceeded");

    assert_eq!(
        value["value"]["context"][0]["value"],
        serde_json::json!({ "kind": "text", "value": "retained_state_cells" })
    );

    assert_eq!(
        value["value"]["context"][1]["value"],
        serde_json::json!({ "kind": "count", "value": 16_777_217 })
    );

    assert_eq!(
        value["value"]["context"][2]["value"],
        serde_json::json!({ "kind": "count", "value": 16_777_216 })
    );
}

#[test]
fn json_output_preserves_memory_operation_and_callback_causes() {
    let memory = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingTargetMemoryOperationUnavailable,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::target_triple("wasm32-unknown-unknown"))
    .with_arg(DiagnosticArg::memory_operation(
        DiagnosticMemoryOperation::SizeDetermination,
    ))
    .with_arg(DiagnosticArg::native_product_failure_kind(
        DiagnosticNativeProductFailureKind::CodegenBackendNotSelected,
    ))
    .with_arg(DiagnosticArg::dependency_subject_kind(
        DiagnosticDependencySubjectKind::SelectedImplementation,
    ));

    let callback = Diagnostic::new(
        DiagnosticId::new(1),
        DiagnosticKind::CheckingInvalidCallbackStateContext,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::callback_state_problem(
        DiagnosticCallbackStateProblem::ContextParameterNotFirst { actual_ordinal: 2 },
    ));

    let diagnostics = DiagnosticBag::single(memory).merged(&DiagnosticBag::single(callback));
    let mut output = Vec::new();

    write_json_diagnostics(&diagnostics, None, &mut output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should write: {error:?}"));

    let output: serde_json::Value = serde_json::from_slice(&output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should parse: {error:?}"));

    let memory = &output["diagnostics"][0]["args"][1]["value"];
    let native_product = &output["diagnostics"][0]["args"][2]["value"];
    let dependency = &output["diagnostics"][0]["args"][3]["value"];
    let callback = &output["diagnostics"][1]["args"][0]["value"];

    assert_eq!(memory["kind"], "memory_operation");
    assert_eq!(memory["value"], "size_determination");
    assert_eq!(native_product["kind"], "native_product_failure_kind");
    assert_eq!(native_product["value"], "codegen_backend_not_selected");
    assert_eq!(dependency["kind"], "dependency_subject_kind");
    assert_eq!(dependency["value"], "selected_implementation");
    assert_eq!(callback["kind"], "callback_state_problem");
    assert_eq!(callback["value"]["reason"], "context_parameter_not_first");
    assert_eq!(callback["value"]["context"][0]["name"], "actual_ordinal");
    assert_eq!(callback["value"]["context"][0]["value"]["value"], 2);
}

#[test]
fn json_output_preserves_exact_storage_access_path() {
    let access = DiagnosticStorageAccess::new(
        DiagnosticStorageAccessPurpose::IndexSelection,
        DiagnosticStorageRoot::Local,
        [
            DiagnosticStorageProjection::ProductField(String::from("items")),
            DiagnosticStorageProjection::IndexedElement,
        ],
        DiagnosticType::Boolean,
    );

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingMissingMutationAuthority,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::storage_access(access));

    let mut output = Vec::new();

    write_json_diagnostics(&DiagnosticBag::single(diagnostic), None, &mut output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should write: {error:?}"));

    let output: serde_json::Value = serde_json::from_slice(&output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should parse: {error:?}"));

    let value = &output["diagnostics"][0]["args"][0]["value"];

    assert_eq!(value["kind"], "storage_access");
    assert_eq!(value["value"]["purpose"], "index_selection");
    assert_eq!(value["value"]["root"], "local");
    assert_eq!(value["value"]["projections"][0]["reason"], "product_field");

    assert_eq!(
        value["value"]["projections"][0]["context"][0]["value"]["value"],
        "items"
    );

    assert_eq!(
        value["value"]["projections"][1]["reason"],
        "indexed_element"
    );

    assert_eq!(value["value"]["reached_type"]["kind"], "boolean");
}

#[test]
fn json_output_preserves_type_representation_causes() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::CheckingInvalidLayoutDirective,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::layout_problem(
        DiagnosticLayoutProblem::OptionNotPowerOfTwo {
            option: DiagnosticLayoutOption::Packing,
            value: 6,
        },
    ));

    let mut output = Vec::new();

    write_json_diagnostics(&DiagnosticBag::single(diagnostic), None, &mut output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should write: {error:?}"));

    let output: serde_json::Value = serde_json::from_slice(&output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should parse: {error:?}"));

    let value = &output["diagnostics"][0]["args"][0]["value"];

    assert_eq!(value["kind"], "layout_problem");
    assert_eq!(value["value"]["reason"], "option_not_power_of_two");
    assert_eq!(value["value"]["context"][0]["name"], "option");
    assert_eq!(value["value"]["context"][0]["value"]["kind"], "text");
    assert_eq!(value["value"]["context"][0]["value"]["value"], "packing");
    assert_eq!(value["value"]["context"][1]["name"], "value");
    assert_eq!(value["value"]["context"][1]["value"]["kind"], "count");
    assert_eq!(value["value"]["context"][1]["value"]["value"], 6);
}

#[test]
fn json_output_serializes_package_interface_diagnostic_args() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::InterfaceResourceLimitExceeded,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::new(
        DiagnosticArgName::InterfaceLimit,
        DiagnosticArgValue::InterfaceLimit(DiagnosticInterfaceLimit::ImplementationEntryCount),
    ))
    .with_arg(DiagnosticArg::new(
        DiagnosticArgName::InterfaceSection,
        DiagnosticArgValue::InterfaceSection(DiagnosticInterfaceSection::Contracts),
    ))
    .with_arg(DiagnosticArg::new(
        DiagnosticArgName::ActualCount,
        DiagnosticArgValue::Count(12),
    ))
    .with_arg(DiagnosticArg::new(
        DiagnosticArgName::ExpectedRevision,
        DiagnosticArgValue::Revision(1),
    ))
    .with_note(
        DiagnosticNote::new(DiagnosticNoteKind::InterfaceDependencyContext)
            .with_arg(DiagnosticArg::expected_package_identity(
                "example.dependency",
            ))
            .with_arg(DiagnosticArg::expected_product_identity("library"))
            .with_arg(DiagnosticArg::artifact_path("dependency.brayi")),
    );

    let bag = DiagnosticBag::single(diagnostic);

    let mut output = Vec::new();

    match write_json_diagnostics(&bag, None, &mut output) {
        Ok(()) => {}
        Err(error) => panic!("JSON diagnostics should write: {error:?}"),
    }

    let output: serde_json::Value = match serde_json::from_slice(&output) {
        Ok(value) => value,
        Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
    };

    let args = &output["diagnostics"][0]["args"];

    assert_eq!(args[0]["value"]["kind"], "interface_limit");
    assert_eq!(args[0]["value"]["value"], "implementation_entry_count");
    assert_eq!(args[1]["value"]["kind"], "interface_section");
    assert_eq!(args[1]["value"]["value"], "contracts");
    assert_eq!(args[2]["value"]["kind"], "count");
    assert_eq!(args[2]["value"]["value"], 12);
    assert_eq!(args[3]["value"]["kind"], "revision");
    assert_eq!(args[3]["value"]["value"], 1);

    let note_args = &output["diagnostics"][0]["notes"][0]["args"];

    assert_eq!(note_args[0]["value"]["kind"], "package_identity");
    assert_eq!(note_args[0]["value"]["value"], "example.dependency");
    assert_eq!(note_args[1]["value"]["kind"], "product_identity");
    assert_eq!(note_args[1]["value"]["value"], "library");
    assert_eq!(note_args[2]["value"]["kind"], "file_path");
    assert_eq!(note_args[2]["value"]["value"], "dependency.brayi");
}

#[test]
fn json_output_preserves_nested_package_interface_symbol_identities() {
    let package = DiagnosticInterfaceSymbolIdentity::Package("example.math".to_owned());

    let declaration = DiagnosticInterfaceSymbolIdentity::Declaration {
        owner: Box::new(package),
        kind: DiagnosticInterfaceSymbolKind::Function,
        identity: DiagnosticInterfaceDeclarationIdentity::Name("sum".to_owned()),
    };

    let identity = DiagnosticInterfaceSymbolIdentity::Synthesized {
        owner: Box::new(declaration),
        identity: DiagnosticInterfaceSynthesizedIdentity::CallableParameter(2),
    };

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::InterfaceCompilerDeclarationExported,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::interface_symbol_identity(identity));

    let mut output = Vec::new();

    write_json_diagnostics(&DiagnosticBag::single(diagnostic), None, &mut output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should write: {error:?}"));

    let output: serde_json::Value = serde_json::from_slice(&output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should parse: {error:?}"));

    let identity = &output["diagnostics"][0]["args"][0]["value"]["value"];

    assert_eq!(identity["kind"], "synthesized");
    assert_eq!(identity["identity"]["kind"], "callable_parameter");
    assert_eq!(identity["identity"]["ordinal"], 2);
    assert_eq!(identity["owner"]["kind"], "named_declaration");
    assert_eq!(identity["owner"]["name"], "sum");
    assert_eq!(identity["owner"]["symbol_kind"], "function");
    assert_eq!(identity["owner"]["owner"]["kind"], "package");
    assert_eq!(identity["owner"]["owner"]["package"], "example.math");
}

#[test]
fn json_output_serializes_runtime_abi_arguments_as_typed_versions() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::StandardLibraryRuntimeAbiMismatch,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::expected_runtime_abi(
        DiagnosticRuntimeAbiVersion::new(2, 1),
    ))
    .with_arg(DiagnosticArg::actual_runtime_abi(
        DiagnosticRuntimeAbiVersion::new(1, 4),
    ));

    let mut output = Vec::new();

    write_json_diagnostics(&DiagnosticBag::single(diagnostic), None, &mut output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should write: {error:?}"));

    let output: serde_json::Value = serde_json::from_slice(&output)
        .unwrap_or_else(|error| panic!("JSON diagnostics should parse: {error:?}"));

    let args = &output["diagnostics"][0]["args"];

    assert_eq!(args[0]["name"], "expected_runtime_abi");
    assert_eq!(args[0]["value"]["kind"], "runtime_abi");
    assert_eq!(args[0]["value"]["value"]["major"], 2);
    assert_eq!(args[0]["value"]["value"]["minor"], 1);
    assert_eq!(args[1]["name"], "actual_runtime_abi");
    assert_eq!(args[1]["value"]["value"]["major"], 1);
    assert_eq!(args[1]["value"]["value"]["minor"], 4);
}

#[test]
fn json_output_serializes_syntax_diagnostic_args() {
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::SyntaxExpectedToken,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::expected_syntax_kind(SyntaxKind::FuncKeyword))
    .with_arg(DiagnosticArg::actual_syntax_kind(
        SyntaxKind::IdentifierToken,
    ))
    .with_arg(DiagnosticArg::token_text("main"));

    let bag = DiagnosticBag::single(diagnostic);

    let mut output = Vec::new();

    match write_json_diagnostics(&bag, None, &mut output) {
        Ok(()) => {}
        Err(error) => panic!("JSON diagnostics should write: {error:?}"),
    }

    let output = match String::from_utf8(output) {
        Ok(output) => output,
        Err(error) => panic!("JSON diagnostics should be UTF-8: {error:?}"),
    };

    let output_json: serde_json::Value = match serde_json::from_str(&output) {
        Ok(value) => value,
        Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
    };

    let args = &output_json["diagnostics"][0]["args"];

    assert_eq!(
        output_json["diagnostics"][0]["kind"],
        "syntax_expected_token"
    );

    assert_eq!(output_json["diagnostics"][0]["severity"], "error");

    assert_eq!(args[0]["name"], "expected_syntax_kind");
    assert_eq!(args[0]["value"]["kind"], "syntax_kind");
    assert_eq!(args[0]["value"]["value"], "func_keyword");

    assert_eq!(args[1]["name"], "actual_syntax_kind");
    assert_eq!(args[1]["value"]["value"], "identifier_token");

    assert_eq!(args[2]["name"], "token_text");
    assert_eq!(args[2]["value"]["kind"], "token_text");
    assert_eq!(args[2]["value"]["value"], "main");
}

#[test]
fn json_output_serializes_declaration_diagnostic_args() {
    let visibility = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::DeclarationConflictingModuleVisibility,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::declaration_name("core"))
    .with_arg(DiagnosticArg::expected_visibility(
        DiagnosticVisibility::Public,
    ))
    .with_arg(DiagnosticArg::actual_visibility(
        DiagnosticVisibility::Internal,
    ));

    let trust = Diagnostic::new(
        DiagnosticId::new(1),
        DiagnosticKind::DeclarationConflictingModuleTrust,
        SeverityKind::Error,
    )
    .with_arg(DiagnosticArg::expected_module_trust(
        DiagnosticModuleTrust::Trusted,
    ))
    .with_arg(DiagnosticArg::actual_module_trust(
        DiagnosticModuleTrust::Ordinary,
    ));

    let bag = DiagnosticBag::from(vec![visibility, trust]);
    let mut output = Vec::new();

    match write_json_diagnostics(&bag, None, &mut output) {
        Ok(()) => {}
        Err(error) => panic!("JSON diagnostics should write: {error:?}"),
    }

    let output = match String::from_utf8(output) {
        Ok(output) => output,
        Err(error) => panic!("JSON diagnostics should be UTF-8: {error:?}"),
    };

    let output_json: serde_json::Value = match serde_json::from_str(&output) {
        Ok(value) => value,
        Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
    };

    let visibility_args = &output_json["diagnostics"][0]["args"];

    assert_eq!(visibility_args[0]["name"], "declaration_name");
    assert_eq!(visibility_args[0]["value"]["kind"], "declaration_name");
    assert_eq!(visibility_args[0]["value"]["value"], "core");
    assert_eq!(visibility_args[1]["value"]["kind"], "visibility");
    assert_eq!(visibility_args[1]["value"]["value"], "public");
    assert_eq!(visibility_args[2]["value"]["value"], "internal");

    let trust_args = &output_json["diagnostics"][1]["args"];

    assert_eq!(trust_args[0]["value"]["kind"], "module_trust");
    assert_eq!(trust_args[0]["value"]["value"], "trusted");
    assert_eq!(trust_args[1]["value"]["value"], "ordinary");
}

#[test]
fn json_output_serializes_typed_artifact_details() {
    let sink = DiagnosticOutputSink::Memory("host.output".to_owned());
    let sink = DiagnosticOutputSinkJson::from_sink(&sink);

    let Ok(sink) = serde_json::to_value(sink) else {
        panic!("diagnostic output sink must serialize");
    };

    assert_eq!(sink["kind"], "memory");
    assert_eq!(sink["value"], "host.output");

    let digest =
        DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Blake3, [3_u8; 32]);

    let digest = DiagnosticArtifactDigestJson::from_digest(&digest);

    let Ok(digest) = serde_json::to_value(digest) else {
        panic!("diagnostic artifact digest must serialize");
    };

    assert_eq!(digest["algorithm"], "blake3");
    assert_eq!(digest["bytes"].as_array().map(Vec::len), Some(32));
    assert_eq!(digest["bytes"][0], 3);
    assert_eq!(digest["bytes"][31], 3);
}

#[test]
fn json_output_serializes_resolved_source_locations() {
    let sources = file_source_store("ok\n$");

    let span = SourceSpan::new(
        bray_source::SourceId::new(0),
        TextRange::new(TextSize::new(3), TextSize::new(4)),
    );

    let diagnostic = Diagnostic::new(
        DiagnosticId::new(0),
        DiagnosticKind::LexicalInvalidCharacter,
        SeverityKind::Error,
    )
    .with_primary_span(span);

    let bag = DiagnosticBag::single(diagnostic);

    let mut output = Vec::new();

    match write_json_diagnostics(&bag, Some(&sources), &mut output) {
        Ok(()) => {}
        Err(error) => panic!("JSON diagnostics should write: {error:?}"),
    }

    let output = match String::from_utf8(output) {
        Ok(output) => output,
        Err(error) => panic!("JSON diagnostics should be UTF-8: {error:?}"),
    };

    let output_json: serde_json::Value = match serde_json::from_str(&output) {
        Ok(value) => value,
        Err(error) => panic!("JSON diagnostics should parse: {error:?}"),
    };

    let location = &output_json["diagnostics"][0]["primary_span"]["location"];

    assert_eq!(
        output_json["diagnostics"][0]["primary_span"]["source_origin"]["kind"],
        "file"
    );

    assert_eq!(
        output_json["diagnostics"][0]["primary_span"]["source_origin"]["file_path"],
        "main.bray"
    );

    assert_eq!(location["start"]["line"], 2);
    assert_eq!(location["start"]["column"], 1);

    assert_eq!(location["end"]["line"], 2);
    assert_eq!(location["end"]["column"], 1);

    assert_eq!(location["lsp_start"]["line"], 1);
    assert_eq!(location["lsp_start"]["character"], 0);

    assert_eq!(location["lsp_end"]["line"], 1);
    assert_eq!(location["lsp_end"]["character"], 1);
}
