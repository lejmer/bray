use super::super::DiagnosticInterfaceSymbolIdentityJson;
use super::super::DiagnosticOutputSinkJson;
use super::failure::{
    DiagnosticEmissionFieldJson, DiagnosticEmissionFieldValueJson, artifact_field, count_field,
    count_u64_field, digest_field, field, text_field,
};
use crate::output::path_to_output_string;

use super::foreign_query::foreign_query_failure_context;
use super::product_query::product_query_failure_context;

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
        Failure::DuplicateDependencyPackage(package) => {
            vec![text_field("package", package)]
        }
        Failure::NonCanonicalSymbolOrder { previous, current } => vec![
            count_field("previous_record", *previous),
            count_field("current_record", *current),
        ],
        Failure::IdentityNonCanonicalSymbolId { expected, actual } => vec![
            count_field("expected_record", *expected),
            count_field("actual_record", *actual),
        ],
        Failure::IdentityMissingPackageRoot { actual } => {
            vec![text_field("actual_symbol_kind", actual)]
        }
        Failure::IdentityPackageRootHasContainer(container) => {
            vec![count_field("container_record", *container)]
        }
        Failure::IdentityPackageMismatch(record)
        | Failure::IdentityMissingContainer(record)
        | Failure::ExportOwnerOutOfBounds(record)
        | Failure::InvalidExportOwner(record)
        | Failure::ExportTargetOutOfBounds(record)
        | Failure::InvalidDirectExportTarget(record)
        | Failure::DuplicateConstantCallableBody(record)
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
        Failure::IdentitySymbolKindMismatch {
            record,
            declared,
            keyed,
        } => vec![
            count_field("record", *record),
            text_field("declared_symbol_kind", declared),
            text_field("keyed_symbol_kind", keyed),
        ],
        Failure::IdentityDuplicateExternalKey { first, duplicate } => vec![
            count_field("first_record", *first),
            count_field("duplicate_record", *duplicate),
        ],
        Failure::IdentityInvalidContainer { record, container }
        | Failure::IdentityContainerKeyMismatch { record, container } => vec![
            count_field("record", *record),
            count_field("container_record", *container),
        ],
        Failure::IdentityUnexpectedRoot { record, kind } => vec![
            count_field("record", *record),
            text_field("symbol_kind", kind),
        ],
        Failure::RelationshipSymbolOutOfBounds {
            owner,
            member,
            ordinal,
        }
        | Failure::InvalidRelationship {
            owner,
            member,
            ordinal,
        }
        | Failure::DuplicateRelationshipPosition {
            owner,
            member,
            ordinal,
        } => vec![
            count_field("owner_record", *owner),
            count_field("member_record", *member),
            count_field("ordinal", *ordinal),
        ],
        Failure::DependencyOutOfBounds(index) | Failure::DependencyKeyPackageMismatch(index) => {
            vec![count_field("dependency_index", *index)]
        }
        Failure::DuplicateExportName { owner, name } => vec![
            count_field("owner_record", *owner),
            text_field("name", name),
        ],
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
        | Failure::NonLibraryProduct
        | Failure::DependencyCountOverflow
        | Failure::IdentityEmpty
        | Failure::IdentitySymbolCountOverflow
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
        Failure::SemanticValue(failure)
        | Failure::Binding(bray_diagnostics::DiagnosticBindingFailure::SemanticValue(failure))
        | Failure::Checker(bray_diagnostics::DiagnosticCheckerFailure::SemanticValue(failure)) => {
            semantic_value_failure_context(*failure)
        }
        Failure::LoweringInput(failure) => match failure.kind() {
            bray_diagnostics::DiagnosticLoweringInputFailureKind::SemanticValue(failure) => {
                semantic_value_failure_context(failure)
            }
            _ => vec![text_field("cause", failure.as_str())],
        },
        Failure::Lowering(failure) => match failure.kind() {
            bray_diagnostics::DiagnosticLoweringFailureKind::SemanticValue(failure) => {
                semantic_value_failure_context(failure)
            }
            _ => vec![text_field("cause", failure.as_str())],
        },
        Failure::Product(failure) => product_query_failure_context(failure),
        Failure::Foreign(failure) => foreign_query_failure_context(failure),
        _ => vec![text_field("cause", failure.as_str())],
    }
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
        | Failure::InvalidPath(artifact)
        | Failure::InvalidContent(artifact) => vec![artifact_field("artifact", *artifact)],
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
            DiagnosticEmissionFieldValueJson::Path(path_to_output_string(path)),
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
        DiagnosticEmissionEvaluationFailure, DiagnosticInterfaceSymbolIdentity,
        DiagnosticPackageInterfaceFailure, DiagnosticSemanticValueFailure,
    };

    use super::{package_interface_failure_context, semantic_value_failure_context};

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
    fn executable_template_failures_preserve_declaration_and_cause() {
        let context = package_interface_failure_context(
            &DiagnosticPackageInterfaceFailure::ExecutableTemplateEvaluation {
                declaration: package("example.template"),
                cause: DiagnosticEmissionEvaluationFailure::Cycle,
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
