use super::super::{
    DiagnosticInterfaceSymbolIdentityJson, DiagnosticOutputSinkJson,
    interface_symbol_graph_problem_json,
};
use super::failure::{
    DiagnosticEmissionFieldJson, DiagnosticEmissionFieldValueJson, artifact_field, count_field,
    count_u64_field, digest_field, field, text_field,
};
use super::foreign_query::foreign_query_failure_context;
use super::product_query::product_query_failure_context;
use super::{checker_failure_context, lowering_failure_context, lowering_input_failure_context};

pub(super) fn planning_failure_context(
    failure: &bray_diagnostics::DiagnosticEmissionPlanningFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticEmissionPlanningFailure as Failure;

    match failure {
        Failure::ProductArtifactMismatch { product, artifact } => vec![
            field(
                "product_kind",
                DiagnosticEmissionFieldValueJson::ProductKind(product.as_str()),
            ),
            field(
                "artifact_kind",
                DiagnosticEmissionFieldValueJson::ArtifactKind(artifact.as_str()),
            ),
        ],
        Failure::PackageInterfaceProductMismatch { expected, actual } => vec![
            text_field("expected_product", expected),
            text_field("actual_product", actual),
        ],
        Failure::MissingRootFrame(identity) => vec![digest_field("root_frame", identity)],
        Failure::UnsupportedBackendTarget(target) => vec![text_field("target", target)],
        Failure::UnsupportedBackendArtifact(artifact)
        | Failure::MissingSerializationArtifact(artifact)
        | Failure::MissingOutputName(artifact)
        | Failure::InvalidGeneratedFileName(artifact)
        | Failure::ExplicitOutputSuffixMismatch(artifact)
        | Failure::ArtifactOrdinalOverflow(artifact) => vec![field(
            "artifact_kind",
            DiagnosticEmissionFieldValueJson::ArtifactKind(artifact.as_str()),
        )],
        Failure::UnsupportedDebugInformation(mode) => vec![field(
            "debug_information",
            DiagnosticEmissionFieldValueJson::DebugInformationMode(mode.as_str()),
        )],
        Failure::UnsupportedDebugOutput(mode) => vec![field(
            "debug_output",
            DiagnosticEmissionFieldValueJson::DebugOutputMode(mode.as_str()),
        )],
        Failure::UnsupportedAssemblySyntax(syntax) => vec![field(
            "assembly_syntax",
            DiagnosticEmissionFieldValueJson::AssemblySyntax(syntax.as_str()),
        )],
        Failure::InvalidDebugOutput {
            information,
            output,
        } => vec![
            field(
                "debug_information",
                DiagnosticEmissionFieldValueJson::DebugInformationMode(information.as_str()),
            ),
            field(
                "debug_output",
                DiagnosticEmissionFieldValueJson::DebugOutputMode(output.as_str()),
            ),
        ],
        Failure::OutputCollision(sink) => vec![field(
            "output_sink",
            DiagnosticEmissionFieldValueJson::OutputSink(DiagnosticOutputSinkJson::from_sink(sink)),
        )],
        Failure::BackendRequestForeignUnit(artifact)
        | Failure::BackendRequestDuplicateIdentity(artifact) => {
            vec![artifact_field("artifact", *artifact)]
        }
        Failure::BackendRequestMissingLinkableArtifact { kind, requirement } => vec![
            field(
                "artifact_kind",
                DiagnosticEmissionFieldValueJson::ArtifactKind(kind.as_str()),
            ),
            field(
                "requirement",
                DiagnosticEmissionFieldValueJson::ArtifactRequirement(requirement.as_str()),
            ),
        ],
        Failure::Incomplete
        | Failure::MissingPackageInterfaceArtifact
        | Failure::UnexpectedPackageInterfaceArtifact
        | Failure::MissingLinkedProduct
        | Failure::MultipleLinkedProducts
        | Failure::LinkedCompanionRequirementMismatch
        | Failure::MissingBackend
        | Failure::MissingCodegenUnits
        | Failure::MissingExecutableHost
        | Failure::MissingRequiredDebugCompanion
        | Failure::UnexpectedDebugCompanion
        | Failure::MissingLinkableArtifact
        | Failure::InvalidProductName
        | Failure::MultipleArtifactsForSingleSink
        | Failure::MissingExplicitFileName
        | Failure::InvalidExplicitFileName
        | Failure::ManagedProductDestinationRequired
        | Failure::BackendRequestEmpty
        | Failure::BackendRequestMissingRequiredDebugCompanion
        | Failure::BackendRequestUnexpectedDebugCompanion
        | Failure::BackendRequestUnexpectedAssemblySyntax
        | Failure::BackendRequestUnexpectedBitcodeSemantics
        | Failure::InconsistentPlan => Vec::new(),
    }
}

pub(super) fn package_interface_failure_context(
    failure: &bray_diagnostics::DiagnosticPackageInterfaceFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticPackageInterfaceFailure as Failure;

    match failure {
        Failure::InvalidCompilationCause { context, .. } => diagnostic_failure_context(context),
        Failure::SemanticValueStoreCreate => Vec::new(),
        Failure::SemanticValue(failure) => semantic_value_failure_context(*failure),
        Failure::RecoveredPublicSymbol(kind)
        | Failure::IncompletePublicDeclaration(kind)
        | Failure::DuplicateSymbol(kind)
        | Failure::MissingSymbol(kind) => vec![text_field("symbol_kind", kind)],
        Failure::MissingDeclarationData(declaration) => {
            vec![interface_symbol_identity_field("declaration", declaration)]
        }
        Failure::ConstantCallableEvaluation { declaration, cause }
        | Failure::ExecutableTemplateEvaluation { declaration, cause } => {
            let mut context = vec![interface_symbol_identity_field("declaration", declaration)];

            context.extend(evaluation_failure_context(cause));

            context
        }
        Failure::LostDeclarationReference {
            declaration,
            table,
            reference,
        } => declaration_reference_fields(declaration.as_ref(), table, *reference),
        Failure::MissingPackageReference { table, reference } => vec![
            text_field("semantic_table", table),
            count_field("reference", *reference),
        ],
        Failure::DuplicateDeclarationIdentity {
            first,
            second,
            identity,
        } => vec![
            interface_symbol_identity_field("first_declaration", first),
            interface_symbol_identity_field("second_declaration", second),
            interface_symbol_identity_field("stable_identity", identity),
        ],
        Failure::RecursiveDeclarationData {
            declaration,
            table,
            reference,
        } => declaration_reference_fields(declaration.as_ref(), table, *reference),
        Failure::ConflictingDeclarationRecord { kind, owner } => {
            let mut context = vec![text_field("record_kind", kind)];

            context.extend(interface_symbol_reference_fields(owner));

            context
        }
        Failure::RecursiveDeclarationReference(table) => {
            vec![text_field("semantic_table", table)]
        }
        Failure::SemanticTableOverflow { table, maximum } => vec![
            text_field("semantic_table", table),
            count_field("maximum_records", *maximum),
        ],
        Failure::SymbolGraph(problem) => vec![field(
            "symbol_graph",
            DiagnosticEmissionFieldValueJson::Problem(interface_symbol_graph_problem_json(problem)),
        )],
        Failure::DuplicateConstantCallableBody(record)
        | Failure::DuplicateExecutableTemplate(record)
        | Failure::InvalidExecutableTemplateFamily(record)
        | Failure::DuplicateNativeBoundary(record)
        | Failure::ImplementationDuplicateCallableBody(record)
        | Failure::ImplementationDuplicateExecutableTemplate(record)
        | Failure::ImplementationInvalidExecutableTemplateFamily(record)
        | Failure::ImplementationDuplicateNativeBoundary(record)
        | Failure::ImplementationInvalidExecutableOwner(record)
        | Failure::ImplementationInvalidNativeBoundaryOwner(record)
        | Failure::ImplementationInvalidCallableOwner(record) => {
            vec![count_field("record", *record)]
        }
        Failure::DeclarationDiscoveryFailure { cause, cycle } => {
            let mut context = evaluation_failure_context(cause);

            if !cycle.is_empty() {
                context.push(text_field("query_cycle", cycle.join(" -> ")));
            }

            context
        }
        Failure::MisassignedPackageRecord(table) => {
            vec![text_field("semantic_table", table)]
        }
        Failure::Unavailable
        | Failure::InvalidCompilation
        | Failure::SymbolCountOverflow
        | Failure::ImplementationContentTooLarge
        | Failure::ImplementationDuplicateSpecialization
        | Failure::ImplementationSpecializationIdentityMismatch => Vec::new(),
    }
}

pub(super) fn evaluation_failure_context(
    failure: &bray_diagnostics::DiagnosticEmissionEvaluationFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticEmissionEvaluationFailure as Failure;

    match failure {
        Failure::Cycle(failure) | Failure::SemanticContext(failure) => {
            let mut context = vec![text_field("cause", failure.reason())];
            context.extend(diagnostic_failure_context(failure.context()));

            context
        }
        Failure::Runtime(failure) => fact_runtime_failure_context(failure),
        Failure::SemanticQuery(failure) => {
            let mut context = vec![text_field("category", failure.category())];
            context.extend(diagnostic_failure_context(failure.context()));

            context
        }
        Failure::SemanticValue(failure) => semantic_value_failure_context(*failure),
        Failure::Checker(failure) => checker_failure_context(*failure),
        Failure::Binding(failure) => match failure.semantic_value_failure() {
            Some(failure) => semantic_value_failure_context(failure),
            None => diagnostic_failure_context(failure.context()),
        },
        Failure::LoweringInput(failure) => lowering_input_failure_context(*failure),
        Failure::Lowering(failure) => lowering_failure_context(*failure),
        Failure::Product(failure) => product_query_failure_context(failure),
        Failure::Foreign(failure) => foreign_query_failure_context(failure),
        Failure::Cancelled
        | Failure::SemanticValueStoreCreate
        | Failure::ConstantCallableBodyUnavailable
        | Failure::ConstantCallableRootUnavailable
        | Failure::AtomicRepresentationTypeUnavailable
        | Failure::AtomicRepresentationArgumentsUnavailable
        | Failure::AtomicInitializerArgumentUnavailable
        | Failure::AtomicInitializerResultUnavailable
        | Failure::UninitInitializerResultUnavailable
        | Failure::ImportedExecutableTemplateMismatch => {
            vec![text_field("cause", failure.as_str())]
        }
    }
}

pub(in crate::output::diagnostic::json) fn fact_runtime_failure_context(
    failure: &bray_diagnostics::DiagnosticFactRuntimeFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    diagnostic_failure_context(failure.context())
}

pub(in crate::output::diagnostic::json) fn diagnostic_failure_context(
    context: &[bray_diagnostics::DiagnosticFailureField],
) -> Vec<DiagnosticEmissionFieldJson> {
    context
        .iter()
        .map(|diagnostic_field| {
            let value = match diagnostic_field.value() {
                bray_diagnostics::DiagnosticFailureValue::ArtifactDigest(value) => {
                    DiagnosticEmissionFieldValueJson::ArtifactDigest(
                        super::super::DiagnosticArtifactDigestJson::from_digest(value),
                    )
                }
                bray_diagnostics::DiagnosticFailureValue::Boolean(value) => {
                    DiagnosticEmissionFieldValueJson::Boolean(*value)
                }
                bray_diagnostics::DiagnosticFailureValue::Count(value) => {
                    DiagnosticEmissionFieldValueJson::Count(*value)
                }
                bray_diagnostics::DiagnosticFailureValue::Identity(value) => {
                    DiagnosticEmissionFieldValueJson::Identity(bray_base::lowercase_hex(value))
                }
                bray_diagnostics::DiagnosticFailureValue::IdentityList(values) => {
                    DiagnosticEmissionFieldValueJson::IdentityList(
                        values
                            .iter()
                            .map(|value| bray_base::lowercase_hex(value))
                            .collect(),
                    )
                }
                bray_diagnostics::DiagnosticFailureValue::Evaluation(value) => {
                    DiagnosticEmissionFieldValueJson::Evaluation(Box::new(
                        super::failure::DiagnosticEmissionFailureJson::from_failure(
                            &bray_diagnostics::DiagnosticEmissionFailure::Evaluation(
                                (**value).clone(),
                            ),
                        ),
                    ))
                }
                bray_diagnostics::DiagnosticFailureValue::ExternalToolExit(value) => {
                    DiagnosticEmissionFieldValueJson::ExternalToolExit(
                        super::super::DiagnosticExternalToolExitJson::from_exit(value),
                    )
                }
                bray_diagnostics::DiagnosticFailureValue::InterfaceSymbolIdentity(value) => {
                    DiagnosticEmissionFieldValueJson::InterfaceSymbolIdentity(
                        DiagnosticInterfaceSymbolIdentityJson::from_identity(value),
                    )
                }
                bray_diagnostics::DiagnosticFailureValue::InterfaceSymbolGraphProblem(value) => {
                    DiagnosticEmissionFieldValueJson::InterfaceSymbolGraphProblem(
                        super::super::interface_symbol_graph_problem_json(value),
                    )
                }
                bray_diagnostics::DiagnosticFailureValue::InterfaceValidationFailure(value) => {
                    DiagnosticEmissionFieldValueJson::InterfaceValidationFailure(
                        super::super::interface_validation_failure_json(value),
                    )
                }
                bray_diagnostics::DiagnosticFailureValue::IoErrorKind(value) => {
                    DiagnosticEmissionFieldValueJson::IoErrorKind(value.as_str())
                }
                bray_diagnostics::DiagnosticFailureValue::Natural(value) => {
                    DiagnosticEmissionFieldValueJson::Natural(value.clone())
                }
                bray_diagnostics::DiagnosticFailureValue::Path(value) => {
                    DiagnosticEmissionFieldValueJson::Path(
                        super::super::DiagnosticPathJson::from_path(value),
                    )
                }
                bray_diagnostics::DiagnosticFailureValue::Signed(value) => {
                    DiagnosticEmissionFieldValueJson::Signed(*value)
                }
                bray_diagnostics::DiagnosticFailureValue::Text(value) => {
                    DiagnosticEmissionFieldValueJson::Text(value.clone())
                }
                bray_diagnostics::DiagnosticFailureValue::TextList(values) => {
                    DiagnosticEmissionFieldValueJson::TextList(values.to_vec())
                }
            };

            field(diagnostic_field.name(), value)
        })
        .collect()
}

pub(in crate::output::diagnostic::json) fn semantic_value_failure_context(
    failure: bray_diagnostics::DiagnosticSemanticValueFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticSemanticValueFailure as Failure;

    let mut context = vec![text_field("cause", failure.as_str())];

    match failure {
        Failure::ForeignId {
            expected_store,
            actual_store,
        } => context.extend([
            count_u64_field("expected_store", expected_store),
            count_u64_field("actual_store", actual_store),
        ]),
        Failure::UnknownId { kind } | Failure::CapacityExhausted { kind } => {
            context.push(text_field("semantic_value_kind", kind));
        }
        Failure::GenericOwnerMismatch {
            expected_kind,
            expected,
            actual_kind,
            actual,
        } => context.extend([
            text_field("expected_owner_kind", expected_kind),
            count_field("expected_owner", expected),
            text_field("actual_owner_kind", actual_kind),
            count_field("actual_owner", actual),
        ]),
        Failure::OpenSubstitution => {}
    }

    context
}

fn interface_symbol_identity_field(
    name: &'static str,
    identity: &bray_diagnostics::DiagnosticInterfaceSymbolIdentity,
) -> DiagnosticEmissionFieldJson {
    field(
        name,
        DiagnosticEmissionFieldValueJson::InterfaceSymbolIdentity(
            DiagnosticInterfaceSymbolIdentityJson::from_identity(identity),
        ),
    )
}

fn declaration_reference_fields(
    declaration: Option<&bray_diagnostics::DiagnosticInterfaceSymbolIdentity>,
    table: &str,
    reference: u32,
) -> Vec<DiagnosticEmissionFieldJson> {
    let mut context = vec![
        text_field("semantic_table", table),
        count_field("reference", reference),
    ];

    if let Some(declaration) = declaration {
        context.push(interface_symbol_identity_field("declaration", declaration));
    }

    context
}

fn interface_symbol_reference_fields(
    reference: &bray_diagnostics::DiagnosticInterfaceSymbolReference,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticInterfaceSymbolReference as Reference;

    match reference {
        Reference::Local(symbol) => vec![
            text_field("owner_reference_kind", "local"),
            count_field("owner_symbol", *symbol),
        ],
        Reference::Dependency {
            dependency,
            identity,
        } => vec![
            text_field("owner_reference_kind", "dependency"),
            count_field("owner_dependency", *dependency),
            field(
                "owner_identity",
                DiagnosticEmissionFieldValueJson::InterfaceSymbolIdentity(
                    DiagnosticInterfaceSymbolIdentityJson::from_identity(identity),
                ),
            ),
        ],
        Reference::CompilerKnown(identity) => vec![
            text_field("owner_reference_kind", "compiler_known"),
            field(
                "owner_identity",
                DiagnosticEmissionFieldValueJson::InterfaceSymbolIdentity(
                    DiagnosticInterfaceSymbolIdentityJson::from_identity(identity),
                ),
            ),
        ],
    }
}

pub(super) fn codegen_failure_context(
    failure: &bray_diagnostics::DiagnosticEmissionCodegenFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticEmissionCodegenFailure as Failure;

    match failure {
        Failure::DuplicateUnit(unit)
        | Failure::DuplicateMappings(unit)
        | Failure::MissingUnit(unit)
        | Failure::MissingMappings(unit)
        | Failure::Request(unit)
        | Failure::Generation(unit)
        | Failure::MergeDuplicateUnit(unit)
        | Failure::MergeMissingUnit(unit)
        | Failure::MergeUnrequestedUnit(unit)
        | Failure::MergeBackendMismatch(unit)
        | Failure::MergeCapabilityMismatch(unit)
        | Failure::MergeTargetMismatch(unit) => vec![digest_field("codegen_unit", unit)],
        Failure::MergeMissingArtifact(artifact)
        | Failure::MergeUnrequestedArtifact(artifact)
        | Failure::MergeArtifactKindMismatch(artifact)
        | Failure::MergeInvalidContent(artifact) => vec![artifact_field("artifact", *artifact)],
        Failure::MergeReadFailed { artifact, error } => vec![
            artifact_field("artifact", *artifact),
            field(
                "io_error",
                DiagnosticEmissionFieldValueJson::IoErrorKind(error.as_str()),
            ),
        ],
        Failure::MergeLengthMismatch {
            artifact,
            expected,
            actual,
        } => vec![
            artifact_field("artifact", *artifact),
            field(
                "expected_bytes",
                DiagnosticEmissionFieldValueJson::Count(*expected),
            ),
            field(
                "actual_bytes",
                DiagnosticEmissionFieldValueJson::Count(*actual),
            ),
        ],
        Failure::MergeDigestMismatch {
            artifact,
            expected,
            actual,
        } => vec![
            artifact_field("artifact", *artifact),
            digest_field("expected_digest", expected),
            digest_field("actual_digest", actual),
        ],
        Failure::Incomplete => Vec::new(),
    }
}

pub(super) fn staging_failure_context(
    failure: &bray_diagnostics::DiagnosticEmissionStagingFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticEmissionStagingFailure as Failure;

    match failure {
        Failure::DuplicateContribution(artifact)
        | Failure::MissingContribution(artifact)
        | Failure::InvalidContribution(artifact)
        | Failure::UnexpectedContribution(artifact)
        | Failure::UnsupportedOutput(artifact)
        | Failure::InvalidPath(artifact) => vec![artifact_field("artifact", *artifact)],
        Failure::Incomplete => Vec::new(),
    }
}

pub(super) fn link_plan_failure_context(
    failure: &bray_diagnostics::DiagnosticEmissionLinkPlanFailure,
) -> Vec<DiagnosticEmissionFieldJson> {
    use bray_diagnostics::DiagnosticEmissionLinkPlanFailure as Failure;

    match failure {
        Failure::DuplicateStagedArtifact(artifact)
        | Failure::MissingStagedArtifact(artifact)
        | Failure::UnexpectedStagedArtifact(artifact)
        | Failure::UnsupportedStagedArtifact(artifact)
        | Failure::DuplicateOutputStaging(artifact)
        | Failure::MissingOutputStaging(artifact)
        | Failure::UnexpectedOutputStaging(artifact) => {
            vec![artifact_field("artifact", *artifact)]
        }
        Failure::OutputKindMismatch { artifact, actual } => vec![
            artifact_field("artifact", *artifact),
            field(
                "actual_output_kind",
                DiagnosticEmissionFieldValueJson::LinkedArtifactKind(actual.as_str()),
            ),
        ],
        Failure::InvalidStartupInputKind(kind)
        | Failure::InvalidNativeInputKind(kind)
        | Failure::InvalidTerminationInputKind(kind) => vec![field(
            "input_kind",
            DiagnosticEmissionFieldValueJson::LinkInputKind(kind.as_str()),
        )],
        Failure::DuplicateInput(ordinal) | Failure::UnexpectedStartupInput(ordinal) => {
            vec![count_field("input_ordinal", *ordinal)]
        }
        Failure::DuplicateOutput(ordinal) => vec![count_field("output_ordinal", *ordinal)],
        Failure::OutputPathCollision { first, second } => vec![
            count_field("first_output", *first),
            count_field("second_output", *second),
        ],
        Failure::IncompatibleOutputKind { product, artifact } => vec![
            field(
                "product_kind",
                DiagnosticEmissionFieldValueJson::LinkedProductKind(product.as_str()),
            ),
            field(
                "artifact_kind",
                DiagnosticEmissionFieldValueJson::LinkedArtifactKind(artifact.as_str()),
            ),
        ],
        Failure::InvalidOutputPath(path) => vec![field(
            "path",
            DiagnosticEmissionFieldValueJson::Path(super::super::DiagnosticPathJson::from_path(
                path,
            )),
        )],
        Failure::Incomplete
        | Failure::MissingLinkedProduct
        | Failure::MissingLinker
        | Failure::UnexpectedLinker
        | Failure::RuntimeContractMismatch
        | Failure::InputOrdinalOverflow
        | Failure::OutputOrdinalOverflow
        | Failure::InputEmptyPath
        | Failure::InputSourceKindMismatch
        | Failure::WholeArchiveRequiresArchive
        | Failure::OutputEmptyPath
        | Failure::MissingInputs
        | Failure::DuplicateSearchPath
        | Failure::MissingRuntimeComponent
        | Failure::UnexpectedRuntimeComponent
        | Failure::RuntimeArtifactMismatch
        | Failure::MissingPrimaryOutput
        | Failure::MultiplePrimaryOutputs
        | Failure::OptionalPrimaryOutput
        | Failure::MissingExecutableHost
        | Failure::UnexpectedExecutableHost
        | Failure::ExecutableHostProductMismatch
        | Failure::ExecutableHostTargetMismatch
        | Failure::UnexpectedEntryPoint
        | Failure::MissingStartupMode
        | Failure::UnexpectedStartupMode
        | Failure::MissingStartupInput
        | Failure::MissingDebugCompanion
        | Failure::UnexpectedDebugCompanion => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticEmissionEvaluationFailure, DiagnosticEvaluationFailureDetail,
        DiagnosticFailureField, DiagnosticFailureValue, DiagnosticInterfaceSymbolIdentity,
        DiagnosticPackageInterfaceFailure, DiagnosticSemanticValueFailure,
    };

    use super::super::lowering_input_failure_context;
    use super::{
        diagnostic_failure_context, package_interface_failure_context,
        semantic_value_failure_context,
    };

    fn package(name: &str) -> DiagnosticInterfaceSymbolIdentity {
        DiagnosticInterfaceSymbolIdentity::Package(name.to_owned())
    }

    #[test]
    fn identity_conflicts_preserve_every_declaration() {
        let context = package_interface_failure_context(
            &DiagnosticPackageInterfaceFailure::DuplicateDeclarationIdentity {
                first: package("example.first"),
                second: package("example.second"),
                identity: package("example.shared"),
            },
        );

        let context = serde_json::to_value(context)
            .unwrap_or_else(|error| panic!("diagnostic context should serialize: {error:?}"));

        assert_eq!(context[0]["name"], "first_declaration");
        assert_eq!(context[0]["value"]["value"]["package"], "example.first");
        assert_eq!(context[1]["name"], "second_declaration");
        assert_eq!(context[1]["value"]["value"]["package"], "example.second");
        assert_eq!(context[2]["name"], "stable_identity");
        assert_eq!(context[2]["value"]["value"]["package"], "example.shared");
    }

    #[test]
    fn nested_evaluation_failures_serialize_their_exact_context() {
        let context = diagnostic_failure_context(&[DiagnosticFailureField::new(
            "evaluation_cause",
            DiagnosticFailureValue::Evaluation(Box::new(
                DiagnosticEmissionEvaluationFailure::SemanticContext(
                    DiagnosticEvaluationFailureDetail::new(
                        "missing_owner",
                        [DiagnosticFailureField::new(
                            "owner",
                            DiagnosticFailureValue::Count(29),
                        )],
                    ),
                ),
            )),
        )]);

        let context = serde_json::to_value(context).unwrap_or_else(|error| {
            panic!("nested evaluation context should serialize: {error:?}")
        });

        assert_eq!(context[0]["value"]["value"]["category"], "evaluation");
        assert_eq!(context[0]["value"]["value"]["reason"], "missing_owner");

        assert_eq!(context[0]["value"]["value"]["context"][0]["name"], "cause");

        assert_eq!(context[0]["value"]["value"]["context"][1]["name"], "owner");

        assert_eq!(
            context[0]["value"]["value"]["context"][1]["value"]["value"],
            29
        );
    }

    #[test]
    fn lowering_count_mismatches_serialize_expected_and_actual_values() {
        let context = lowering_input_failure_context(
            bray_diagnostics::DiagnosticLoweringInputFailure::new(
            bray_diagnostics::DiagnosticLoweringInputFailureKind::StorageOperationCountMismatch {
                expected: 5,
                actual: 8,
            },
                bray_source::SourceSpan::new(
                    bray_source::SourceId::new(2),
                    bray_source::TextRange::new(
                        bray_source::TextSize::new(3),
                        bray_source::TextSize::new(4),
                    ),
                ),
            ),
        );

        let context = serde_json::to_value(context)
            .unwrap_or_else(|error| panic!("lowering context should serialize: {error:?}"));

        assert_eq!(context[1]["name"], "expected");
        assert_eq!(context[1]["value"]["value"], 5);
        assert_eq!(context[2]["name"], "actual");
        assert_eq!(context[2]["value"]["value"], 8);
    }

    #[test]
    fn nested_interface_validation_failures_serialize_their_exact_payload() {
        let context = diagnostic_failure_context(&[DiagnosticFailureField::new(
            "interface_validation_cause",
            DiagnosticFailureValue::InterfaceValidationFailure(
                bray_diagnostics::DiagnosticInterfaceValidationFailure::Truncated {
                    context: bray_diagnostics::DiagnosticInterfaceValidationContext::Header,
                    field: bray_diagnostics::DiagnosticInterfaceValidationField::RecordCount,
                    offset: 13,
                    expected_length: 8,
                    actual_length: 3,
                },
            ),
        )]);

        let context = serde_json::to_value(context).unwrap_or_else(|error| {
            panic!("nested validation context should serialize: {error:?}")
        });

        assert_eq!(context[0]["value"]["value"]["reason"], "truncated");

        assert_eq!(
            context[0]["value"]["value"]["context"][0]["value"]["value"],
            "header"
        );

        assert_eq!(
            context[0]["value"]["value"]["context"][2]["value"]["value"],
            13
        );
    }

    #[test]
    fn executable_template_failures_preserve_declaration_and_cause() {
        let context = package_interface_failure_context(
            &DiagnosticPackageInterfaceFailure::ExecutableTemplateEvaluation {
                declaration: package("example.template"),
                cause: DiagnosticEmissionEvaluationFailure::Cycle(
                    bray_diagnostics::DiagnosticEvaluationFailureDetail::new("cycle", []),
                ),
            },
        );

        let context = serde_json::to_value(context)
            .unwrap_or_else(|error| panic!("diagnostic context should serialize: {error:?}"));

        assert_eq!(context[0]["name"], "declaration");
        assert_eq!(context[0]["value"]["value"]["package"], "example.template");
        assert_eq!(context[1]["name"], "cause");
        assert_eq!(context[1]["value"]["value"], "cycle");
    }

    #[test]
    fn semantic_value_failures_preserve_every_leaf_payload() {
        let foreign = serde_json::to_value(semantic_value_failure_context(
            DiagnosticSemanticValueFailure::ForeignId {
                expected_store: 41,
                actual_store: 73,
            },
        ))
        .unwrap_or_else(|error| panic!("foreign-id context should serialize: {error:?}"));

        assert_eq!(foreign[1]["name"], "expected_store");
        assert_eq!(foreign[1]["value"]["value"], 41);
        assert_eq!(foreign[2]["name"], "actual_store");
        assert_eq!(foreign[2]["value"]["value"], 73);

        for (failure, expected_cause) in [
            (
                DiagnosticSemanticValueFailure::UnknownId { kind: "type" },
                "binding_semantic_value_unknown_id",
            ),
            (
                DiagnosticSemanticValueFailure::CapacityExhausted {
                    kind: "constant_term",
                },
                "binding_semantic_value_capacity_exhausted",
            ),
        ] {
            let context = serde_json::to_value(semantic_value_failure_context(failure))
                .unwrap_or_else(|error| panic!("value-kind context should serialize: {error:?}"));

            assert_eq!(context[0]["value"]["value"], expected_cause);
            assert_eq!(context[1]["name"], "semantic_value_kind");
        }

        let owner = serde_json::to_value(semantic_value_failure_context(
            DiagnosticSemanticValueFailure::GenericOwnerMismatch {
                expected_kind: "function",
                expected: 5,
                actual_kind: "trait",
                actual: 8,
            },
        ))
        .unwrap_or_else(|error| panic!("owner context should serialize: {error:?}"));

        assert_eq!(owner[1]["value"]["value"], "function");
        assert_eq!(owner[2]["value"]["value"], 5);
        assert_eq!(owner[3]["value"]["value"], "trait");
        assert_eq!(owner[4]["value"]["value"], 8);

        let open = serde_json::to_value(semantic_value_failure_context(
            DiagnosticSemanticValueFailure::OpenSubstitution,
        ))
        .unwrap_or_else(|error| panic!("open-substitution context should serialize: {error:?}"));

        assert_eq!(open.as_array().map(Vec::len), Some(1));
    }
}
