use bray_diagnostics::{
    Diagnostic, DiagnosticBag, DiagnosticEmissionCodegenFailure,
    DiagnosticEmissionEvaluationFailure, DiagnosticEmissionFailure, DiagnosticIoErrorKind,
    DiagnosticLabel, DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind,
    DiagnosticPackageInterfaceFailure, DiagnosticRelatedLocation, DiagnosticRelatedLocationKind,
};
use bray_emitter::BackendContributionMergeErrorKind;
use bray_package_interface::{
    InterfaceSemanticCommitError, PackageImplementationArtifactBuildError,
    PackageInterfaceExportBuildError, PackageInterfaceExportSurfaceError,
    PackageInterfaceSurfaceBuildError,
};
use bray_symbols::{
    ImportedIdentitySurfaceError, ProductIdentity, diagnostic_external_symbol_identity,
};
use bray_target::TargetIdentity;

use super::common::{
    codegen_unit_identity, diagnostic_backend_artifact, emission_failure_diagnostic,
    emission_failure_diagnostics,
};
use super::evaluation::{diagnostic_evaluation_failure, query_failure_diagnostics};
use super::model::ProductEmissionErrorKind;
use crate::compilation::diagnostics::diagnostic_interface_symbol_reference;
use crate::compilation::product::{
    codegen_preparation_failure_kind, native_product_preparation_diagnostic,
};
use crate::compilation::{
    CodegenPreparationError, EmissionCodegenError, EmissionCodegenErrorKind,
    PackageInterfaceExportError,
};
use crate::fact::FactQueryError;

pub(super) fn codegen_failure_diagnostics(
    error: &EmissionCodegenError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    let failure = match error.kind() {
        EmissionCodegenErrorKind::DuplicateUnit(unit) => {
            DiagnosticEmissionCodegenFailure::DuplicateUnit(codegen_unit_identity(unit))
        }
        EmissionCodegenErrorKind::DuplicateMappings(unit) => {
            DiagnosticEmissionCodegenFailure::DuplicateMappings(codegen_unit_identity(unit))
        }
        EmissionCodegenErrorKind::MissingUnit(unit) => {
            DiagnosticEmissionCodegenFailure::MissingUnit(codegen_unit_identity(unit))
        }
        EmissionCodegenErrorKind::MissingMappings(unit) => {
            DiagnosticEmissionCodegenFailure::MissingMappings(codegen_unit_identity(unit))
        }
        EmissionCodegenErrorKind::Request { unit: _, error } => {
            match error.as_ref() {
                CodegenPreparationError::Diagnostics(diagnostics) => return diagnostics.clone(),
                CodegenPreparationError::Query(error) => {
                    return query_failure_diagnostics(error, product, target);
                }
                _ => {}
            }

            let failure = codegen_preparation_failure_kind(error)
                .unwrap_or_else(|| unreachable!("query and diagnostic failures return above"));

            return DiagnosticBag::single(native_product_preparation_diagnostic(
                failure,
                product,
                target.as_str(),
            ));
        }
        EmissionCodegenErrorKind::Generation { unit, .. } => {
            DiagnosticEmissionCodegenFailure::Generation(codegen_unit_identity(unit))
        }
        EmissionCodegenErrorKind::Cancelled(_) => return DiagnosticBag::new(),
        EmissionCodegenErrorKind::Query(error) => {
            return query_failure_diagnostics(error, product, target);
        }
        EmissionCodegenErrorKind::InvalidContributions(error) => {
            return contribution_merge_failure_diagnostics(error.kind(), product, target);
        }
    };

    emission_failure_diagnostics(DiagnosticEmissionFailure::Codegen(failure), product, target)
}

fn contribution_merge_failure_diagnostics(
    error: &BackendContributionMergeErrorKind,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> DiagnosticBag {
    let failure = match error {
        BackendContributionMergeErrorKind::Cancelled => return DiagnosticBag::new(),
        BackendContributionMergeErrorKind::DuplicateUnit(unit) => {
            DiagnosticEmissionCodegenFailure::MergeDuplicateUnit(codegen_unit_identity(unit))
        }
        BackendContributionMergeErrorKind::MissingUnit(unit) => {
            DiagnosticEmissionCodegenFailure::MergeMissingUnit(codegen_unit_identity(unit))
        }
        BackendContributionMergeErrorKind::UnrequestedUnit(unit) => {
            DiagnosticEmissionCodegenFailure::MergeUnrequestedUnit(codegen_unit_identity(unit))
        }
        BackendContributionMergeErrorKind::BackendMismatch(unit) => {
            DiagnosticEmissionCodegenFailure::MergeBackendMismatch(codegen_unit_identity(unit))
        }
        BackendContributionMergeErrorKind::CapabilityRevisionMismatch(unit) => {
            DiagnosticEmissionCodegenFailure::MergeCapabilityMismatch(codegen_unit_identity(unit))
        }
        BackendContributionMergeErrorKind::TargetMismatch(unit) => {
            DiagnosticEmissionCodegenFailure::MergeTargetMismatch(codegen_unit_identity(unit))
        }
        BackendContributionMergeErrorKind::MissingArtifact(artifact) => {
            DiagnosticEmissionCodegenFailure::MergeMissingArtifact(diagnostic_backend_artifact(
                artifact,
            ))
        }
        BackendContributionMergeErrorKind::UnrequestedArtifact(artifact) => {
            DiagnosticEmissionCodegenFailure::MergeUnrequestedArtifact(diagnostic_backend_artifact(
                artifact,
            ))
        }
        BackendContributionMergeErrorKind::ArtifactKindMismatch(artifact) => {
            DiagnosticEmissionCodegenFailure::MergeArtifactKindMismatch(
                diagnostic_backend_artifact(artifact),
            )
        }
        BackendContributionMergeErrorKind::Read { artifact, kind } => {
            DiagnosticEmissionCodegenFailure::MergeReadFailed {
                artifact: diagnostic_backend_artifact(artifact),
                error: DiagnosticIoErrorKind::from(*kind),
            }
        }
        BackendContributionMergeErrorKind::LengthMismatch {
            artifact,
            expected,
            actual,
        } => DiagnosticEmissionCodegenFailure::MergeLengthMismatch {
            artifact: diagnostic_backend_artifact(artifact),
            expected: *expected,
            actual: *actual,
        },
        BackendContributionMergeErrorKind::DigestMismatch {
            artifact,
            expected,
            actual,
        } => DiagnosticEmissionCodegenFailure::MergeDigestMismatch {
            artifact: diagnostic_backend_artifact(artifact),
            expected: expected.diagnostic_digest(),
            actual: actual.diagnostic_digest(),
        },
        BackendContributionMergeErrorKind::InvalidContent(artifact) => {
            DiagnosticEmissionCodegenFailure::MergeInvalidContent(diagnostic_backend_artifact(
                artifact,
            ))
        }
    };

    emission_failure_diagnostics(DiagnosticEmissionFailure::Codegen(failure), product, target)
}

pub(super) fn package_interface_failure_diagnostics(
    kind: &ProductEmissionErrorKind,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Option<DiagnosticBag> {
    let diagnostic = match kind {
        ProductEmissionErrorKind::PackageInterfaceUnavailable => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::Unavailable,
            product,
            target,
        ),
        ProductEmissionErrorKind::PackageInterface(error) => {
            package_interface_export_failure_diagnostic(error, product, target)?
        }
        ProductEmissionErrorKind::PackageImplementation(error) => {
            package_implementation_failure_diagnostic(error, product, target)?
        }
        ProductEmissionErrorKind::PackageImplementationContent(_) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::ImplementationContentTooLarge,
            product,
            target,
        ),
        ProductEmissionErrorKind::PackageInterfaceEncoding(_) => return None,
        _ => return None,
    };

    Some(DiagnosticBag::single(diagnostic))
}

fn package_failure_diagnostic(
    failure: DiagnosticPackageInterfaceFailure,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    emission_failure_diagnostic(
        DiagnosticEmissionFailure::PackageInterface(failure),
        product,
        target,
    )
}

fn package_interface_export_failure_diagnostic(
    error: &PackageInterfaceExportError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Option<Diagnostic> {
    let diagnostic = match error {
        PackageInterfaceExportError::Cancelled => return None,
        PackageInterfaceExportError::InvalidCompilation => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::InvalidCompilation,
            product,
            target,
        ),
        PackageInterfaceExportError::SemanticValueStoreCreate(_) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::SemanticValueStoreCreate,
            product,
            target,
        ),
        PackageInterfaceExportError::SemanticValueStore(error) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::SemanticValue(
                crate::fact::diagnostic_semantic_value_failure(*error),
            ),
            product,
            target,
        ),
        PackageInterfaceExportError::RecoveredPublicSymbol(kind) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::RecoveredPublicSymbol(kind.as_str().to_owned()),
            product,
            target,
        ),
        PackageInterfaceExportError::ConstantCallableEvaluation { declaration, cause } => {
            let cause = diagnostic_evaluation_failure(cause);

            package_evaluation_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::ConstantCallableEvaluation {
                    declaration: declaration.clone(),
                    cause,
                },
                cause,
                product,
                target,
            )
        }
        PackageInterfaceExportError::ExecutableTemplateEvaluation { declaration, cause } => {
            let cause = diagnostic_evaluation_failure(cause);

            package_evaluation_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::ExecutableTemplateEvaluation {
                    declaration: declaration.clone(),
                    cause,
                },
                cause,
                product,
                target,
            )
        }
        PackageInterfaceExportError::IncompletePublicDeclarationSemantics(kind) => {
            package_compiler_defect_diagnostic(
                DiagnosticPackageInterfaceFailure::IncompletePublicDeclaration(
                    kind.as_str().to_owned(),
                ),
                product,
                target,
            )
        }
        PackageInterfaceExportError::IncompleteSemanticFragment {
            declaration,
            table,
            reference,
        } => {
            // The emitted diagnostic remains valid after the product error is released.
            package_compiler_defect_diagnostic(
                DiagnosticPackageInterfaceFailure::LostDeclarationReference {
                    declaration: declaration.clone(),
                    table: table.as_str().to_owned(),
                    reference: *reference,
                },
                product,
                target,
            )
        }
        PackageInterfaceExportError::ConflictingSemanticFragment {
            first,
            second,
            first_span,
            second_span,
            identity,
        } => {
            // The emitted diagnostic remains valid after the product error is released.
            let diagnostic = package_compiler_defect_diagnostic(
                DiagnosticPackageInterfaceFailure::DuplicateDeclarationIdentity {
                    first: first.clone(),
                    second: second.clone(),
                    identity: diagnostic_external_symbol_identity(identity),
                },
                product,
                target,
            );

            with_duplicate_declaration_locations(diagnostic, *first_span, *second_span)
        }
        PackageInterfaceExportError::CyclicSemanticFragment {
            declaration,
            table,
            reference,
        } => {
            // The emitted diagnostic remains valid after the product error is released.
            package_compiler_defect_diagnostic(
                DiagnosticPackageInterfaceFailure::RecursiveDeclarationData {
                    declaration: declaration.clone(),
                    table: table.as_str().to_owned(),
                    reference: *reference,
                },
                product,
                target,
            )
        }
        PackageInterfaceExportError::FragmentCoordination(error) => {
            let cycle: Box<[String]> = match error {
                FactQueryError::Cycle(cycle) => cycle
                    .facts()
                    .iter()
                    .map(|fact| {
                        crate::profile::ProfileQueryKind::from_key(fact)
                            .name()
                            .to_owned()
                    })
                    .collect(),
                _ => Box::new([]),
            };

            package_compiler_defect_diagnostic(
                DiagnosticPackageInterfaceFailure::DeclarationDiscoveryFailure {
                    cause: diagnostic_evaluation_failure(error),
                    cycle,
                },
                product,
                target,
            )
        }
        PackageInterfaceExportError::FragmentCommit(error) => {
            package_interface_fragment_failure_diagnostic(error, product, target)
        }
        PackageInterfaceExportError::Surface(error) => {
            package_interface_surface_failure_diagnostic(error, product, target)
        }
        PackageInterfaceExportError::Bundle(error) => {
            package_interface_bundle_failure_diagnostic(error, product, target)?
        }
    };

    Some(diagnostic)
}

fn package_compiler_defect_diagnostic(
    failure: DiagnosticPackageInterfaceFailure,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    let source = match &failure {
        DiagnosticPackageInterfaceFailure::DeclarationDiscoveryFailure { cause, .. } => {
            crate::compilation::diagnostics::code_production_failure_source(*cause)
        }
        _ => None,
    };

    let diagnostic = package_failure_diagnostic(failure, product, target);

    match source {
        Some(source) => {
            crate::compilation::diagnostics::with_compiler_defect_source(diagnostic, source)
        }
        None => diagnostic.with_note(DiagnosticNote::new(
            DiagnosticNoteKind::ReportCompilerDefect,
        )),
    }
}

fn package_evaluation_failure_diagnostic(
    failure: DiagnosticPackageInterfaceFailure,
    cause: DiagnosticEmissionEvaluationFailure,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    let diagnostic = package_failure_diagnostic(failure, product, target);

    match crate::compilation::diagnostics::code_production_failure_source(cause) {
        Some(source) => {
            crate::compilation::diagnostics::with_compiler_defect_source(diagnostic, source)
        }
        None => diagnostic,
    }
}

fn with_duplicate_declaration_locations(
    mut diagnostic: Diagnostic,
    first: Option<bray_source::SourceSpan>,
    second: Option<bray_source::SourceSpan>,
) -> Diagnostic {
    let primary = second.or(first);

    if let Some(primary) = primary {
        diagnostic = diagnostic
            .with_primary_span(primary)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::DuplicateDeclaration,
                primary,
            ));
    }

    if let Some(first) = first
        && Some(first) != primary
    {
        diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::FirstDeclaration,
            first,
        ));
    }

    diagnostic
}

fn package_interface_fragment_failure_diagnostic(
    error: &InterfaceSemanticCommitError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    let failure = match error {
        InterfaceSemanticCommitError::MissingReference { table, reference } => {
            DiagnosticPackageInterfaceFailure::MissingPackageReference {
                table: table.as_str().to_owned(),
                reference: *reference,
            }
        }
        InterfaceSemanticCommitError::UnexpectedPackageRecord(table) => {
            DiagnosticPackageInterfaceFailure::MisassignedPackageRecord(table.as_str().to_owned())
        }
        InterfaceSemanticCommitError::ConflictingRecord { owner, kind } => {
            DiagnosticPackageInterfaceFailure::ConflictingDeclarationRecord {
                kind: kind.as_str().to_owned(),
                owner: diagnostic_interface_symbol_reference(owner),
            }
        }
        InterfaceSemanticCommitError::CyclicReference(table) => {
            DiagnosticPackageInterfaceFailure::RecursiveDeclarationReference(
                table.as_str().to_owned(),
            )
        }
        InterfaceSemanticCommitError::IdentityOverflow(table) => {
            DiagnosticPackageInterfaceFailure::SemanticTableOverflow {
                table: table.as_str().to_owned(),
                maximum: u32::MAX,
            }
        }
    };

    package_compiler_defect_diagnostic(failure, product, target)
}

fn package_interface_surface_failure_diagnostic(
    error: &PackageInterfaceExportSurfaceError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    match error {
        PackageInterfaceExportSurfaceError::DuplicateSymbol(key) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::DuplicateSymbol(key.kind().as_str().to_owned()),
            product,
            target,
        ),
        PackageInterfaceExportSurfaceError::MissingSymbol(key) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::MissingSymbol(key.kind().as_str().to_owned()),
            product,
            target,
        ),
        PackageInterfaceExportSurfaceError::SymbolCountOverflow => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::SymbolCountOverflow,
            product,
            target,
        ),
        PackageInterfaceExportSurfaceError::Surface(error) => {
            package_interface_structural_failure_diagnostic(error, product, target)
        }
    }
}

fn package_interface_bundle_failure_diagnostic(
    error: &PackageInterfaceExportBuildError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Option<Diagnostic> {
    let failure = match error {
        PackageInterfaceExportBuildError::MissingSemantics(key) => {
            return Some(package_compiler_defect_diagnostic(
                DiagnosticPackageInterfaceFailure::MissingDeclarationData(
                    diagnostic_external_symbol_identity(key),
                ),
                product,
                target,
            ));
        }
        PackageInterfaceExportBuildError::Validation(_) => return None,
        PackageInterfaceExportBuildError::DuplicateConstantCallableBody(owner) => {
            DiagnosticPackageInterfaceFailure::DuplicateConstantCallableBody(owner.raw())
        }
        PackageInterfaceExportBuildError::DuplicateExecutableTemplate(owner) => {
            DiagnosticPackageInterfaceFailure::DuplicateExecutableTemplate(owner.raw())
        }
        PackageInterfaceExportBuildError::InvalidExecutableTemplateFamily(owner) => {
            DiagnosticPackageInterfaceFailure::InvalidExecutableTemplateFamily(owner.raw())
        }
        PackageInterfaceExportBuildError::DuplicateNativeBoundary(owner) => {
            DiagnosticPackageInterfaceFailure::DuplicateNativeBoundary(owner.raw())
        }
    };

    Some(package_failure_diagnostic(failure, product, target))
}

fn package_implementation_failure_diagnostic(
    error: &PackageImplementationArtifactBuildError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Option<Diagnostic> {
    let failure = match error {
        PackageImplementationArtifactBuildError::DuplicateCallableBody(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationDuplicateCallableBody(owner.raw())
        }
        PackageImplementationArtifactBuildError::DuplicateExecutableTemplate(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationDuplicateExecutableTemplate(
                owner.raw(),
            )
        }
        PackageImplementationArtifactBuildError::InvalidExecutableTemplateFamily(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationInvalidExecutableTemplateFamily(
                owner.raw(),
            )
        }
        PackageImplementationArtifactBuildError::DuplicateNativeBoundary(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationDuplicateNativeBoundary(owner.raw())
        }
        PackageImplementationArtifactBuildError::DuplicateSpecialization => {
            DiagnosticPackageInterfaceFailure::ImplementationDuplicateSpecialization
        }
        PackageImplementationArtifactBuildError::SpecializationIdentityMismatch => {
            DiagnosticPackageInterfaceFailure::ImplementationSpecializationIdentityMismatch
        }
        PackageImplementationArtifactBuildError::InvalidExecutableOwner(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationInvalidExecutableOwner(owner.raw())
        }
        PackageImplementationArtifactBuildError::InvalidNativeBoundaryOwner(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationInvalidNativeBoundaryOwner(owner.raw())
        }
        PackageImplementationArtifactBuildError::InvalidCallableOwner(owner) => {
            DiagnosticPackageInterfaceFailure::ImplementationInvalidCallableOwner(owner.raw())
        }
        PackageImplementationArtifactBuildError::InvalidBody(_)
        | PackageImplementationArtifactBuildError::InvalidArtifact(_) => return None,
    };

    Some(package_failure_diagnostic(failure, product, target))
}

fn package_interface_structural_failure_diagnostic(
    error: &PackageInterfaceSurfaceBuildError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    match error {
        PackageInterfaceSurfaceBuildError::NonLibraryProduct => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::NonLibraryProduct,
            product,
            target,
        ),
        PackageInterfaceSurfaceBuildError::DependencyCountOverflow => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::DependencyCountOverflow,
            product,
            target,
        ),
        PackageInterfaceSurfaceBuildError::DuplicateDependencyPackage(package) => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::DuplicateDependencyPackage(
                    package.as_str().to_owned(),
                ),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::NonCanonicalSymbolOrder { previous, current } => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::NonCanonicalSymbolOrder {
                    previous: previous.raw(),
                    current: current.raw(),
                },
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::Identity(error) => {
            package_interface_identity_failure_diagnostic(error, product, target)
        }
        PackageInterfaceSurfaceBuildError::RelationshipSymbolOutOfBounds(relationship) => {
            package_failure_diagnostic(
                relationship_failure(*relationship, RelationshipFailureKind::SymbolOutOfBounds),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::InvalidRelationship(relationship) => {
            package_failure_diagnostic(
                relationship_failure(*relationship, RelationshipFailureKind::Invalid),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::DuplicateRelationshipPosition(relationship) => {
            package_failure_diagnostic(
                relationship_failure(*relationship, RelationshipFailureKind::DuplicatePosition),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::ExportOwnerOutOfBounds(owner) => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::ExportOwnerOutOfBounds(owner.raw()),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::InvalidExportOwner(owner) => package_failure_diagnostic(
            DiagnosticPackageInterfaceFailure::InvalidExportOwner(owner.raw()),
            product,
            target,
        ),
        PackageInterfaceSurfaceBuildError::ExportTargetOutOfBounds(target_symbol) => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::ExportTargetOutOfBounds(target_symbol.raw()),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::DependencyOutOfBounds(dependency) => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::DependencyOutOfBounds(dependency.raw()),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::DependencyKeyPackageMismatch(dependency) => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::DependencyKeyPackageMismatch(dependency.raw()),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::InvalidDirectExportTarget(target_symbol) => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::InvalidDirectExportTarget(target_symbol.raw()),
                product,
                target,
            )
        }
        PackageInterfaceSurfaceBuildError::DuplicateExportName { owner, name } => {
            package_failure_diagnostic(
                DiagnosticPackageInterfaceFailure::DuplicateExportName {
                    owner: owner.raw(),
                    name: name.as_str().to_owned(),
                },
                product,
                target,
            )
        }
    }
}

fn package_interface_identity_failure_diagnostic(
    error: &ImportedIdentitySurfaceError,
    product: &ProductIdentity,
    target: &TargetIdentity,
) -> Diagnostic {
    let failure = match error {
        ImportedIdentitySurfaceError::Empty => DiagnosticPackageInterfaceFailure::IdentityEmpty,
        ImportedIdentitySurfaceError::SymbolCountOverflow => {
            DiagnosticPackageInterfaceFailure::IdentitySymbolCountOverflow
        }
        ImportedIdentitySurfaceError::NonCanonicalSymbolId { expected, actual } => {
            DiagnosticPackageInterfaceFailure::IdentityNonCanonicalSymbolId {
                expected: expected.raw(),
                actual: actual.raw(),
            }
        }
        ImportedIdentitySurfaceError::MissingPackageRoot { actual } => {
            DiagnosticPackageInterfaceFailure::IdentityMissingPackageRoot {
                actual: actual.as_str().to_owned(),
            }
        }
        ImportedIdentitySurfaceError::PackageRootHasContainer { container } => {
            DiagnosticPackageInterfaceFailure::IdentityPackageRootHasContainer(container.raw())
        }
        ImportedIdentitySurfaceError::PackageIdentityMismatch { symbol } => {
            DiagnosticPackageInterfaceFailure::IdentityPackageMismatch(symbol.raw())
        }
        ImportedIdentitySurfaceError::SymbolKindMismatch {
            symbol,
            declared,
            keyed,
        } => DiagnosticPackageInterfaceFailure::IdentitySymbolKindMismatch {
            record: symbol.raw(),
            declared: declared.as_str().to_owned(),
            keyed: keyed.as_str().to_owned(),
        },
        ImportedIdentitySurfaceError::DuplicateExternalKey { first, duplicate } => {
            DiagnosticPackageInterfaceFailure::IdentityDuplicateExternalKey {
                first: first.raw(),
                duplicate: duplicate.raw(),
            }
        }
        ImportedIdentitySurfaceError::MissingContainer { symbol } => {
            DiagnosticPackageInterfaceFailure::IdentityMissingContainer(symbol.raw())
        }
        ImportedIdentitySurfaceError::InvalidContainer { symbol, container } => {
            DiagnosticPackageInterfaceFailure::IdentityInvalidContainer {
                record: symbol.raw(),
                container: container.raw(),
            }
        }
        ImportedIdentitySurfaceError::ContainerKeyMismatch { symbol, container } => {
            DiagnosticPackageInterfaceFailure::IdentityContainerKeyMismatch {
                record: symbol.raw(),
                container: container.raw(),
            }
        }
        ImportedIdentitySurfaceError::UnexpectedRoot { symbol, kind } => {
            DiagnosticPackageInterfaceFailure::IdentityUnexpectedRoot {
                record: symbol.raw(),
                kind: kind.as_str().to_owned(),
            }
        }
    };

    package_failure_diagnostic(failure, product, target)
}

#[derive(Clone, Copy)]
enum RelationshipFailureKind {
    SymbolOutOfBounds,
    Invalid,
    DuplicatePosition,
}

fn relationship_failure(
    relationship: bray_package_interface::SymbolRelationship,
    kind: RelationshipFailureKind,
) -> DiagnosticPackageInterfaceFailure {
    let owner = relationship.owner().raw();
    let member = relationship.member().raw();
    let ordinal = relationship.ordinal();

    match kind {
        RelationshipFailureKind::SymbolOutOfBounds => {
            DiagnosticPackageInterfaceFailure::RelationshipSymbolOutOfBounds {
                owner,
                member,
                ordinal,
            }
        }
        RelationshipFailureKind::Invalid => {
            DiagnosticPackageInterfaceFailure::InvalidRelationship {
                owner,
                member,
                ordinal,
            }
        }
        RelationshipFailureKind::DuplicatePosition => {
            DiagnosticPackageInterfaceFailure::DuplicateRelationshipPosition {
                owner,
                member,
                ordinal,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticEmissionEvaluationFailure, DiagnosticInterfaceSymbolIdentity,
        DiagnosticInterfaceSymbolKind, DiagnosticLabelKind, DiagnosticLoweringFailure,
        DiagnosticLoweringFailureKind, DiagnosticNoteKind, DiagnosticRelatedLocationKind,
    };
    use bray_lowering::LoweringError;
    use bray_messages::DiagnosticRenderer;
    use bray_package_interface::{InterfaceSemanticCommitError, InterfaceSemanticTableKind};
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
    use bray_symbols::{ExternalSymbolKey, PackageIdentity, ProductIdentity};
    use bray_target::TargetIdentity;

    use super::super::evaluation::diagnostic_evaluation_failure;
    use super::{
        package_interface_export_failure_diagnostic, package_interface_fragment_failure_diagnostic,
    };
    use crate::LocatedLoweringFailure;
    use crate::compilation::PackageInterfaceExportError;
    use crate::fact::FactQueryError;

    fn identities() -> (ProductIdentity, TargetIdentity) {
        let package = PackageIdentity::try_new("example.package")
            .unwrap_or_else(|| panic!("test package identity must be valid"));

        let product = ProductIdentity::try_new(package, "library")
            .unwrap_or_else(|| panic!("test product identity must be valid"));

        let target = TargetIdentity::try_new("test-target")
            .unwrap_or_else(|| panic!("test target identity must be valid"));

        (product, target)
    }

    #[test]
    fn cancellation_does_not_produce_a_user_diagnostic() {
        let (product, target) = identities();

        assert!(
            package_interface_export_failure_diagnostic(
                &PackageInterfaceExportError::Cancelled,
                &product,
                &target,
            )
            .is_none()
        );
    }

    #[test]
    fn terminal_evaluation_failures_retain_the_lowering_cause() {
        let source = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(10), TextSize::new(20)),
        );

        assert_eq!(
            diagnostic_evaluation_failure(&FactQueryError::Lowering(LocatedLoweringFailure::new(
                LoweringError::InvalidFrameDescriptor,
                source
            ),)),
            DiagnosticEmissionEvaluationFailure::Lowering(DiagnosticLoweringFailure::new(
                DiagnosticLoweringFailureKind::InvalidFrameDescriptor,
                source,
            ),)
        );
    }

    #[test]
    fn package_evaluation_code_production_failures_retain_source_context() {
        let (product, target) = identities();

        let source = SourceSpan::new(
            SourceId::new(0),
            TextRange::new(TextSize::new(10), TextSize::new(20)),
        );

        let lowering_failure = || {
            FactQueryError::Lowering(LocatedLoweringFailure::new(
                LoweringError::InvalidFrameDescriptor,
                source,
            ))
        };

        let errors = [
            PackageInterfaceExportError::ConstantCallableEvaluation {
                declaration: DiagnosticInterfaceSymbolIdentity::Package(
                    "example.package".to_owned(),
                ),
                cause: lowering_failure(),
            },
            PackageInterfaceExportError::ExecutableTemplateEvaluation {
                declaration: DiagnosticInterfaceSymbolIdentity::Package(
                    "example.package".to_owned(),
                ),
                cause: lowering_failure(),
            },
        ];

        for error in errors {
            let diagnostic = package_interface_export_failure_diagnostic(&error, &product, &target)
                .unwrap_or_else(|| panic!("package evaluation failure must diagnose"));

            assert_eq!(diagnostic.primary_span(), Some(source));

            assert!(diagnostic.labels().iter().any(|label| {
                label.kind() == DiagnosticLabelKind::CompilerDefectSource && label.span() == source
            }));

            assert!(
                diagnostic
                    .notes()
                    .iter()
                    .any(|note| note.kind() == DiagnosticNoteKind::ReportCompilerDefect)
            );

            let rendered = DiagnosticRenderer::english().render(&diagnostic);

            assert!(rendered.message().contains(
                "an internal compiler error prevented Bray from generating resumable code for the highlighted callable"
            ));
        }
    }

    #[test]
    fn compiler_owned_interface_failures_include_reporting_guidance() {
        let (product, target) = identities();

        let diagnostic = package_interface_fragment_failure_diagnostic(
            &InterfaceSemanticCommitError::MissingReference {
                table: InterfaceSemanticTableKind::Type,
                reference: 7,
            },
            &product,
            &target,
        );

        assert!(
            diagnostic
                .notes()
                .iter()
                .any(|note| { note.kind() == DiagnosticNoteKind::ReportCompilerDefect })
        );
    }

    #[test]
    fn duplicate_interface_identities_point_to_both_source_declarations() {
        let (product, target) = identities();

        let first_span = SourceSpan::new(
            SourceId::new(1),
            TextRange::new(TextSize::new(10), TextSize::new(20)),
        );

        let second_span = SourceSpan::new(
            SourceId::new(1),
            TextRange::new(TextSize::new(30), TextSize::new(40)),
        );

        let owner = DiagnosticInterfaceSymbolIdentity::Package("example.package".to_owned());

        let declaration = |ordinal| DiagnosticInterfaceSymbolIdentity::SourceDeclaration {
            owner: Box::new(owner.clone()),
            kind: DiagnosticInterfaceSymbolKind::Function,
            declaration: ordinal,
        };

        let error = PackageInterfaceExportError::ConflictingSemanticFragment {
            first: declaration(1),
            second: declaration(2),
            first_span: Some(first_span),
            second_span: Some(second_span),
            identity: ExternalSymbolKey::package(
                PackageIdentity::try_new("example.shared")
                    .unwrap_or_else(|| panic!("test package identity must be valid")),
            ),
        };

        let diagnostic = package_interface_export_failure_diagnostic(&error, &product, &target)
            .unwrap_or_else(|| panic!("duplicate identities must produce a diagnostic"));

        assert_eq!(diagnostic.primary_span(), Some(second_span));

        assert!(diagnostic.labels().iter().any(|label| {
            label.kind() == DiagnosticLabelKind::DuplicateDeclaration && label.span() == second_span
        }));

        assert!(diagnostic.related_locations().iter().any(|location| {
            location.kind() == DiagnosticRelatedLocationKind::FirstDeclaration
                && location.span() == first_span
        }));

        assert_eq!(
            DiagnosticRenderer::english().render(&diagnostic).message(),
            "cannot emit product 'example.package/library' for target 'test-target': exported declarations 'example.package::function declaration 1' and 'example.package::function declaration 2' both resolve to stable identity 'example.shared'"
        );
    }
}
