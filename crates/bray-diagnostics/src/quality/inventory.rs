// rust-style: allow(module-too-large, reason = "the exhaustive diagnostic-kind-to-contract table stays together so additions remain compiler-enforced")

use super::contract::{DiagnosticComponentContract, DiagnosticQualityContract};
use crate::{
    DiagnosticArgName, DiagnosticKind, DiagnosticNoteKind, DiagnosticRelatedLocationKind,
    DiagnosticSuggestionApplicability, DiagnosticSuggestionKind,
};

macro_rules! primary_components {
    ($args:expr) => {
        &[DiagnosticComponentContract::PrimaryMessage { args: $args }]
    };
}

macro_rules! note_components {
    ($primary_args:expr, $kind:expr) => {
        &[
            DiagnosticComponentContract::PrimaryMessage {
                args: $primary_args,
            },
            DiagnosticComponentContract::Note {
                kind: $kind,
                args: &[],
            },
        ]
    };
}

macro_rules! related_components {
    ($primary_args:expr, $kind:expr) => {
        &[
            DiagnosticComponentContract::PrimaryMessage {
                args: $primary_args,
            },
            DiagnosticComponentContract::RelatedLocation {
                kind: $kind,
                args: &[],
            },
        ]
    };
}

macro_rules! related_note_components {
    ($primary_args:expr, $related_kind:expr, $note_kind:expr) => {
        &[
            DiagnosticComponentContract::PrimaryMessage {
                args: $primary_args,
            },
            DiagnosticComponentContract::RelatedLocation {
                kind: $related_kind,
                args: &[],
            },
            DiagnosticComponentContract::Note {
                kind: $note_kind,
                args: &[],
            },
        ]
    };
}

macro_rules! note_suggestion_components {
    ($primary_args:expr, $note_kind:expr, $suggestion_kind:expr, $applicability:ident) => {
        &[
            DiagnosticComponentContract::PrimaryMessage {
                args: $primary_args,
            },
            DiagnosticComponentContract::Note {
                kind: $note_kind,
                args: &[],
            },
            DiagnosticComponentContract::Suggestion {
                kind: $suggestion_kind,
                applicability: DiagnosticSuggestionApplicability::$applicability,
                args: &[],
            },
        ]
    };
}

macro_rules! interface_components {
    ($primary_args:expr) => {
        &[
            DiagnosticComponentContract::PrimaryMessage {
                args: $primary_args,
            },
            DiagnosticComponentContract::Note {
                kind: DiagnosticNoteKind::InterfaceDependencyContext,
                args: &[
                    DiagnosticArgName::ExpectedPackageIdentity,
                    DiagnosticArgName::ExpectedProductIdentity,
                    DiagnosticArgName::ArtifactPath,
                ],
            },
        ]
    };
}

macro_rules! interface_defect_components {
    ($primary_args:expr) => {
        &[
            DiagnosticComponentContract::PrimaryMessage {
                args: $primary_args,
            },
            DiagnosticComponentContract::Note {
                kind: DiagnosticNoteKind::InterfaceDependencyContext,
                args: &[
                    DiagnosticArgName::ExpectedPackageIdentity,
                    DiagnosticArgName::ExpectedProductIdentity,
                    DiagnosticArgName::ArtifactPath,
                ],
            },
            DiagnosticComponentContract::Note {
                kind: DiagnosticNoteKind::ReportCompilerDefect,
                args: &[],
            },
        ]
    };
}

macro_rules! link_plan_components {
    ($primary_args:expr) => {
        &[
            DiagnosticComponentContract::PrimaryMessage {
                args: $primary_args,
            },
            DiagnosticComponentContract::Note {
                kind: DiagnosticNoteKind::LinkPlanContext,
                args: &[
                    DiagnosticArgName::ActualProductIdentity,
                    DiagnosticArgName::TargetTriple,
                    DiagnosticArgName::LinkerDriverIdentity,
                ],
            },
        ]
    };
}

macro_rules! compiler_defect_components {
    ($primary_args:expr) => {
        &[
            DiagnosticComponentContract::PrimaryMessage {
                args: $primary_args,
            },
            DiagnosticComponentContract::Note {
                kind: DiagnosticNoteKind::ReportCompilerDefect,
                args: &[],
            },
        ]
    };
}

macro_rules! link_plan_defect_components {
    ($primary_args:expr) => {
        &[
            DiagnosticComponentContract::PrimaryMessage {
                args: $primary_args,
            },
            DiagnosticComponentContract::Note {
                kind: DiagnosticNoteKind::LinkPlanContext,
                args: &[
                    DiagnosticArgName::ActualProductIdentity,
                    DiagnosticArgName::TargetTriple,
                    DiagnosticArgName::LinkerDriverIdentity,
                ],
            },
            DiagnosticComponentContract::Note {
                kind: DiagnosticNoteKind::ReportCompilerDefect,
                args: &[],
            },
        ]
    };
}

impl DiagnosticKind {
    /// Returns this category's exhaustive goal-state quality contract.
    // rust-style: allow(function-too-large, reason = "all diagnostic quality contracts form one exhaustive protocol inventory")
    pub const fn quality_contract(self) -> DiagnosticQualityContract {
        use DiagnosticArgName::{
            ActualArtifactDigest, ActualByteCount, ActualCount, ActualPackageIdentity,
            ActualProductIdentity, ActualProductKind, ActualRevision, ActualRuntimeAbi,
            ActualSyntaxKind, ActualTargetIdentity, ActualTargetPredicateValueKind,
            ActualTargetTriple, ActualType, AlignmentKind, ArrayGeneratorCardinalityProblem,
            ArtifactKind, ArtifactOrdinal, ArtifactPath, ByteCount, CallableAbi,
            CallableOverloadProblem, CallbackStateProblem, CodegenBackendIdentity,
            CodegenBackendReport, CodegenVerificationStage, ConstantOperation, CopyContractProblem,
            DeclarationName, DependencyRequirementKind, DependencySubjectKind, DocumentColumn,
            DocumentLine, DocumentParseKind, EmissionArtifactOperation, EmissionFailure,
            ExpectedArtifactDigest, ExpectedByteCount, ExpectedNameKind, ExpectedPackageIdentity,
            ExpectedProductIdentity, ExpectedRevision, ExpectedRuntimeAbi, ExpectedSyntaxKind,
            ExpectedTargetIdentity, ExpectedTargetPredicateValueKind, ExpectedTargetTriple,
            ExpectedType, ExpressionCategory, ExternalToolExit, ExternalToolFailureKind,
            ExternalToolOperation, FilePath, ImplementationOverloadProblem, InputIndex,
            InterfaceLimit, InterfaceRecordIndex, InterfaceSemanticProblem,
            InterfaceSymbolGraphProblem, InterfaceSymbolIdentity, InterfaceValidationFailure,
            IoErrorKind, LayoutProblem, LinkOptimizationReportProblem, LinkRequirement,
            LinkerDriverIdentity, MaximumAlignment, MaximumCount, MemoryOperation,
            NativeLinkDirectiveProblem, NativeProductFailureKind, NativeSymbolDirectiveProblem,
            PatternCoverage, PatternUnreachability, PlatformServiceSignatureProblem,
            ProjectCommandFailure, ProjectDependencyCycleMember, ProjectManifestField, ProjectPath,
            ProjectSelectionProblem, PropagationProblem, ReferencedName, RefinementCapacity,
            RequiredAlignment, RuntimeArtifactProblem, SelectionCandidates, SelectionKind,
            SelectionRejections, SourceCount, SourceInput, StandardLibraryManifestProblem,
            StorageAccess, StoredTypeProblem, TargetRepresentation, TargetTriple, TextOffset,
            TokenText, TraitFulfillmentMismatch, TraitMemberName, UnionTagProblem,
            UnsupportedEmissionReason, WorkerCount,
        };

        use DiagnosticNoteKind::{
            BlockCommentNeedsTerminator, BomOnlyAllowedAtStart,
            CharacterLiteralMustContainOneScalar, CharacterLiteralNeedsTerminator,
            CharacterNotAccepted, EscapeMustBeKnown, IdentifierSpellingMustBeValid,
            IdentifiersMustBeAscii, LineBreaksMustBeLfOrCrlf, OnlyImaginaryNumericSuffix,
            PackageIdentityMustBeValid, PackageVersionMustBeValid, ReportCompilerDefect,
            RuntimeArtifactMustBeUsable, SourceFileMustBeReadable, SourceIdsAreCompact,
            SourceInputNeedsStableIdentity, SourceInputRequired, SourceMustBeUtf8,
            SourceMustMatchFormatterOutput, SourceTextOffsetsAreCompact,
            StringLiteralNeedsTerminator, UnicodeEscapeMustBeScalar, WorkerBudgetMustBePositive,
        };

        match self {
            Self::SourceFileReadFailed => Self::quality_external(
                &[SourceInput, IoErrorKind],
                note_components!(&[SourceInput, IoErrorKind], SourceFileMustBeReadable),
            ),
            Self::SourceInvalidUtf8 => Self::quality_external(
                &[SourceInput, TextOffset],
                note_components!(&[SourceInput, TextOffset], SourceMustBeUtf8),
            ),
            Self::SourceTooManyInputs => Self::quality_external(
                &[SourceInput, SourceCount],
                note_components!(&[SourceInput, SourceCount], SourceIdsAreCompact),
            ),
            Self::SourceTextTooLarge => Self::quality_external(
                &[SourceInput, ByteCount],
                note_components!(&[SourceInput, ByteCount], SourceTextOffsetsAreCompact),
            ),
            Self::RequestMissingSourceInput => Self::quality_invocation(
                &[SourceCount],
                note_components!(&[SourceCount], SourceInputRequired),
            ),
            Self::RequestInvalidSourceInput => Self::quality_external(
                &[SourceInput],
                note_components!(&[SourceInput], SourceInputNeedsStableIdentity),
            ),
            Self::RequestReservedPackageIdentity
            | Self::RequestStandardLibraryPackageIdentityRequired => {
                Self::quality_external(&[ReferencedName], primary_components!(&[ReferencedName]))
            }
            Self::RequestDuplicateSourceInput => {
                Self::quality_external(&[SourceInput], primary_components!(&[SourceInput]))
            }
            Self::RequestInvalidWorkerBudget => Self::quality_external(
                &[WorkerCount],
                note_components!(&[WorkerCount], WorkerBudgetMustBePositive),
            ),
            Self::RequestUnsupportedProductEmission => Self::quality_invocation(
                &[TargetTriple, UnsupportedEmissionReason],
                primary_components!(&[TargetTriple, UnsupportedEmissionReason]),
            ),
            Self::RuntimeArtifactMetadataReadFailed | Self::RuntimeArtifactArchiveReadFailed => {
                Self::quality_artifact(
                    &[ArtifactPath, IoErrorKind],
                    note_components!(&[ArtifactPath, IoErrorKind], RuntimeArtifactMustBeUsable),
                )
            }
            Self::RuntimeArtifactMetadataInvalid => Self::quality_artifact(
                &[ArtifactPath, RuntimeArtifactProblem],
                note_components!(
                    &[ArtifactPath, RuntimeArtifactProblem],
                    RuntimeArtifactMustBeUsable
                ),
            ),
            Self::RuntimeArtifactTargetMismatch => Self::quality_artifact(
                &[ArtifactPath, ExpectedTargetIdentity, ActualTargetIdentity],
                note_components!(
                    &[ArtifactPath, ExpectedTargetIdentity, ActualTargetIdentity,],
                    RuntimeArtifactMustBeUsable
                ),
            ),
            Self::RuntimeArtifactAbiMismatch => Self::quality_artifact(
                &[ArtifactPath, ExpectedRuntimeAbi, ActualRuntimeAbi],
                note_components!(
                    &[ArtifactPath, ExpectedRuntimeAbi, ActualRuntimeAbi],
                    RuntimeArtifactMustBeUsable
                ),
            ),
            Self::RuntimeArtifactArchiveInvalid => Self::quality_artifact(
                &[ArtifactPath],
                note_components!(&[ArtifactPath], RuntimeArtifactMustBeUsable),
            ),
            Self::RuntimeArtifactArchiveDigestMismatch => Self::quality_artifact(
                &[ArtifactPath, ExpectedArtifactDigest, ActualArtifactDigest],
                note_components!(
                    &[ArtifactPath, ExpectedArtifactDigest, ActualArtifactDigest],
                    RuntimeArtifactMustBeUsable
                ),
            ),
            Self::StandardLibraryArtifactReadFailed => Self::quality_artifact(
                &[FilePath, IoErrorKind],
                interface_components!(&[FilePath, IoErrorKind]),
            ),
            Self::StandardLibraryManifestInvalid => Self::quality_artifact(
                &[FilePath, StandardLibraryManifestProblem],
                interface_components!(&[FilePath, StandardLibraryManifestProblem]),
            ),
            Self::StandardLibraryInfrastructureFailure => Self::quality_artifact(
                &[ArtifactPath],
                interface_defect_components!(&[ArtifactPath]),
            ),
            Self::StandardLibraryArtifactLengthMismatch => Self::quality_artifact(
                &[FilePath, ExpectedByteCount, ActualByteCount],
                interface_components!(&[FilePath, ExpectedByteCount, ActualByteCount]),
            ),
            Self::StandardLibraryArtifactDigestMismatch => Self::quality_artifact(
                &[FilePath, ExpectedArtifactDigest, ActualArtifactDigest],
                interface_components!(&[FilePath, ExpectedArtifactDigest, ActualArtifactDigest]),
            ),
            Self::StandardLibraryTargetUnavailable
            | Self::StandardLibraryOptimizationUnavailable => {
                Self::quality_artifact(&[TargetTriple], interface_components!(&[TargetTriple]))
            }
            Self::StandardLibraryRuntimeAbiMismatch => Self::quality_artifact(
                &[TargetTriple, ExpectedRuntimeAbi, ActualRuntimeAbi],
                interface_components!(&[TargetTriple, ExpectedRuntimeAbi, ActualRuntimeAbi]),
            ),
            Self::InspectionReportWriteFailed | Self::CompilerProfileWriteFailed => {
                Self::quality_external(
                    &[ProjectCommandFailure],
                    primary_components!(&[ProjectCommandFailure]),
                )
            }
            Self::ProjectManifestReadFailed => Self::quality_external(
                &[FilePath, IoErrorKind],
                primary_components!(&[FilePath, IoErrorKind]),
            ),
            Self::ProjectManifestParseFailed => Self::quality_external(
                &[FilePath, DocumentParseKind, DocumentLine, DocumentColumn],
                primary_components!(&[FilePath, DocumentParseKind, DocumentLine, DocumentColumn]),
            ),
            Self::ProjectPackageVersionInvalid => Self::quality_external(
                &[FilePath, ProjectManifestField, ReferencedName],
                note_components!(
                    &[FilePath, ProjectManifestField, ReferencedName],
                    PackageVersionMustBeValid
                ),
            ),
            Self::ProjectPackageVersionMissingWorkspace => Self::quality_external(
                &[FilePath, ProjectManifestField],
                note_components!(&[FilePath, ProjectManifestField], PackageVersionMustBeValid),
            ),
            Self::ProjectManifestUnsupportedFormat => Self::quality_external(
                &[
                    FilePath,
                    ProjectManifestField,
                    ActualRevision,
                    ExpectedRevision,
                ],
                primary_components!(&[
                    FilePath,
                    ProjectManifestField,
                    ActualRevision,
                    ExpectedRevision,
                ]),
            ),
            Self::ProjectManifestInvalidPath => Self::quality_external(
                &[FilePath, ProjectManifestField, ProjectPath],
                primary_components!(&[FilePath, ProjectManifestField, ProjectPath]),
            ),
            Self::ProjectManifestInvalidName => Self::quality_external(
                &[FilePath, ProjectManifestField, ReferencedName],
                primary_components!(&[FilePath, ProjectManifestField, ReferencedName]),
            ),
            Self::ProjectManifestMissingSelection | Self::ProjectManifestMissingRootPackage => {
                Self::quality_external(
                    &[FilePath, ProjectManifestField],
                    primary_components!(&[FilePath, ProjectManifestField]),
                )
            }
            Self::ProjectManifestUndeclaredFeature | Self::ProjectManifestUnknownSourceRoot => {
                Self::quality_external(
                    &[FilePath, ProjectManifestField, ReferencedName],
                    primary_components!(&[FilePath, ProjectManifestField, ReferencedName]),
                )
            }
            Self::ProjectManifestUnknownTarget => Self::quality_external(
                &[FilePath, ProjectManifestField, TargetTriple],
                primary_components!(&[FilePath, ProjectManifestField, TargetTriple]),
            ),
            Self::ProjectManifestUnexpectedTestedLibrary => Self::quality_external(
                &[FilePath, ProjectManifestField, ActualProductIdentity],
                primary_components!(&[FilePath, ProjectManifestField, ActualProductIdentity]),
            ),
            Self::ProjectSourceRootInvalid
            | Self::ProjectSourceRootContainsSymlink
            | Self::ProjectSourceRootContainsNonUtf8Path => Self::quality_external(
                &[FilePath, ProjectManifestField, ProjectPath],
                primary_components!(&[FilePath, ProjectManifestField, ProjectPath]),
            ),
            Self::ProjectDependencyProductUnknown | Self::ProjectDependencyProductNotLibrary => {
                Self::quality_external(
                    &[
                        FilePath,
                        ProjectManifestField,
                        ActualPackageIdentity,
                        ActualProductIdentity,
                    ],
                    primary_components!(&[
                        FilePath,
                        ProjectManifestField,
                        ActualPackageIdentity,
                        ActualProductIdentity,
                    ]),
                )
            }
            Self::ProjectDependencyProductTargetUnavailable => Self::quality_external(
                &[
                    FilePath,
                    ProjectManifestField,
                    ActualPackageIdentity,
                    ActualProductIdentity,
                    TargetTriple,
                ],
                primary_components!(&[
                    FilePath,
                    ProjectManifestField,
                    ActualPackageIdentity,
                    ActualProductIdentity,
                    TargetTriple,
                ]),
            ),
            Self::ProjectManifestUnknownTargetPredicateProperty => Self::quality_external(
                &[FilePath, ProjectManifestField, ReferencedName],
                primary_components!(&[FilePath, ProjectManifestField, ReferencedName]),
            ),
            Self::ProjectManifestTargetPredicateValueKindMismatch => Self::quality_external(
                &[
                    FilePath,
                    ProjectManifestField,
                    ReferencedName,
                    ExpectedTargetPredicateValueKind,
                    ActualTargetPredicateValueKind,
                ],
                primary_components!(&[
                    FilePath,
                    ProjectManifestField,
                    ReferencedName,
                    ExpectedTargetPredicateValueKind,
                    ActualTargetPredicateValueKind,
                ]),
            ),
            Self::ProjectPackageIdentityReserved
            | Self::ProjectStandardLibraryPackageIdentityRequired
            | Self::ProjectStandardLibraryRootPackageRequired => Self::quality_external(
                &[FilePath, ProjectManifestField, ActualPackageIdentity],
                primary_components!(&[FilePath, ProjectManifestField, ActualPackageIdentity]),
            ),
            Self::ProjectManifestDuplicateSelection => Self::quality_external(
                &[FilePath, ProjectManifestField, ReferencedName],
                primary_components!(&[FilePath, ProjectManifestField, ReferencedName]),
            ),
            Self::ProjectDependencyPackageUnknown => Self::quality_external(
                &[FilePath, ProjectManifestField, ActualPackageIdentity],
                primary_components!(&[FilePath, ProjectManifestField, ActualPackageIdentity]),
            ),
            Self::ProjectDependencyCycle => Self::quality_external(
                &[FilePath, ProjectManifestField, ProjectDependencyCycleMember],
                primary_components!(&[
                    FilePath,
                    ProjectManifestField,
                    ProjectDependencyCycleMember,
                ]),
            ),
            Self::ProjectCommandSelectionInvalid => Self::quality_external(
                &[ProjectSelectionProblem],
                primary_components!(&[ProjectSelectionProblem]),
            ),
            Self::ProjectCommandFailed => Self::quality_external(
                &[ProjectCommandFailure],
                primary_components!(&[ProjectCommandFailure]),
            ),
            Self::ProjectCompilerDefect => Self::quality_external(
                &[ProjectCommandFailure],
                note_components!(&[ProjectCommandFailure], ReportCompilerDefect),
            ),
            Self::ProjectInitializationIdentityInvalid => Self::quality_external(
                &[ReferencedName],
                note_components!(&[ReferencedName], PackageIdentityMustBeValid),
            ),
            Self::ProjectInitializationPathConflict => {
                Self::quality_external(&[FilePath], primary_components!(&[FilePath]))
            }
            Self::ProjectInitializationWriteFailed => Self::quality_external(
                &[FilePath, IoErrorKind],
                primary_components!(&[FilePath, IoErrorKind]),
            ),
            Self::ProjectInitializationTargetUnsupported => {
                Self::quality_invocation(&[], primary_components!(&[]))
            }
            Self::FormatterSourceNotFormatted => Self::quality_external(
                &[FilePath],
                note_suggestion_components!(
                    &[FilePath],
                    SourceMustMatchFormatterOutput,
                    DiagnosticSuggestionKind::FormatSource,
                    Manual
                ),
            ),
            Self::FormatterSourceInvalidUtf8 => {
                Self::quality_external(&[FilePath], primary_components!(&[FilePath]))
            }
            Self::FormatterConfigurationMalformed => Self::quality_external(
                &[FilePath, DocumentParseKind, DocumentLine, DocumentColumn],
                primary_components!(&[FilePath, DocumentParseKind, DocumentLine, DocumentColumn]),
            ),
            Self::FormatterSourceTooLarge => Self::quality_external(
                &[FilePath, ByteCount],
                primary_components!(&[FilePath, ByteCount]),
            ),
            Self::FormatterSourceWriteFailed | Self::FormatterConfigurationReadFailed => {
                Self::quality_external(
                    &[FilePath, IoErrorKind],
                    primary_components!(&[FilePath, IoErrorKind]),
                )
            }
            Self::FormatterConfigurationUnknownRule => Self::quality_external(
                &[FilePath, ReferencedName],
                primary_components!(&[FilePath, ReferencedName]),
            ),
            Self::FormatterConfigurationInvalidMaximumWidth => Self::quality_external(
                &[FilePath, ActualCount],
                primary_components!(&[FilePath, ActualCount]),
            ),
            Self::LexicalInvalidCharacter => {
                Self::quality_source(&[], note_components!(&[], CharacterNotAccepted))
            }
            Self::LexicalMisplacedBom => {
                Self::quality_source(&[], note_components!(&[], BomOnlyAllowedAtStart))
            }
            Self::LexicalLoneCarriageReturn => Self::quality_source(
                &[],
                note_suggestion_components!(
                    &[],
                    LineBreaksMustBeLfOrCrlf,
                    DiagnosticSuggestionKind::ReplaceWithLineFeed,
                    MachineApplicable
                ),
            ),
            Self::LexicalNonAsciiIdentifier => {
                Self::quality_source(&[], note_components!(&[], IdentifiersMustBeAscii))
            }
            Self::LexicalInvalidIdentifier => {
                Self::quality_source(&[], note_components!(&[], IdentifierSpellingMustBeValid))
            }
            Self::LexicalInvalidOperatorOrPunctuation | Self::LexicalMalformedNumericLiteral => {
                Self::quality_source(&[], primary_components!(&[]))
            }
            Self::LexicalInvalidNumericSuffix => {
                Self::quality_source(&[], note_components!(&[], OnlyImaginaryNumericSuffix))
            }
            Self::LexicalMalformedCharacterLiteral => Self::quality_source(
                &[],
                note_components!(&[], CharacterLiteralMustContainOneScalar),
            ),
            Self::LexicalUnterminatedCharacterLiteral => Self::quality_source(
                &[],
                note_suggestion_components!(
                    &[],
                    CharacterLiteralNeedsTerminator,
                    DiagnosticSuggestionKind::AddTerminator,
                    MachineApplicable
                ),
            ),
            Self::LexicalUnterminatedStringLiteral => Self::quality_source(
                &[],
                note_suggestion_components!(
                    &[],
                    StringLiteralNeedsTerminator,
                    DiagnosticSuggestionKind::AddTerminator,
                    MachineApplicable
                ),
            ),
            Self::LexicalUnknownEscape => {
                Self::quality_source(&[], note_components!(&[], EscapeMustBeKnown))
            }
            Self::LexicalInvalidUnicodeEscape => {
                Self::quality_source(&[], note_components!(&[], UnicodeEscapeMustBeScalar))
            }
            Self::LexicalUnterminatedBlockComment => Self::quality_source(
                &[],
                note_suggestion_components!(
                    &[],
                    BlockCommentNeedsTerminator,
                    DiagnosticSuggestionKind::AddTerminator,
                    MachineApplicable
                ),
            ),
            Self::SyntaxExpectedToken => Self::quality_source(
                &[ExpectedSyntaxKind, ActualSyntaxKind],
                primary_components!(&[ExpectedSyntaxKind, ActualSyntaxKind]),
            ),
            Self::SyntaxUnexpectedEof => Self::quality_source(
                &[ExpectedSyntaxKind],
                primary_components!(&[ExpectedSyntaxKind]),
            ),
            Self::SyntaxExpectedExpression | Self::SyntaxNestingLimitExceeded => {
                Self::quality_source(&[], primary_components!(&[]))
            }
            Self::SyntaxInvalidDirectiveTarget => Self::quality_source(
                &[DiagnosticArgName::DirectiveKind],
                primary_components!(&[]),
            ),
            Self::DeclarationDuplicateName => Self::quality_source(
                &[DeclarationName],
                related_components!(
                    &[DeclarationName],
                    DiagnosticRelatedLocationKind::FirstDeclaration
                ),
            ),
            Self::DeclarationConflictingModuleVisibility
            | Self::DeclarationConflictingModuleTrust => Self::quality_source(
                &[],
                related_components!(&[], DiagnosticRelatedLocationKind::FirstDeclaration),
            ),
            Self::DeclarationDuplicateLifecycleSlot => Self::quality_source(
                &[],
                related_components!(&[], DiagnosticRelatedLocationKind::FirstDeclaration),
            ),
            Self::DeclarationDuplicateModifier
            | Self::DeclarationIncompatibleModifiers
            | Self::DeclarationInvalidModifier
            | Self::DeclarationBodyRequired
            | Self::DeclarationBodyNotAllowed
            | Self::DeclarationDuplicateDirective
            | Self::DeclarationIncompatibleDirectives
            | Self::DeclarationInvalidDirectiveTarget
            | Self::DeclarationInvalidParameterOrder
            | Self::DeclarationInvalidMemberPlacement => {
                Self::quality_source(&[], primary_components!(&[]))
            }
            Self::InterfaceInvalidMagic
            | Self::InterfaceUnsupportedFormatRevision
            | Self::InterfaceUnsupportedLanguageRevision
            | Self::InterfaceUnsupportedEncoding
            | Self::InterfaceValidationFailed
            | Self::InterfaceHashMismatch
            | Self::InterfaceSectionChecksumMismatch
            | Self::InterfaceResourceLimitExceeded => Self::quality_artifact(
                &[InterfaceValidationFailure],
                interface_components!(&[InterfaceValidationFailure]),
            ),
            Self::InterfacePackageIdentityMismatch => Self::quality_artifact(
                &[ExpectedPackageIdentity, ActualPackageIdentity],
                interface_components!(&[ExpectedPackageIdentity, ActualPackageIdentity]),
            ),
            Self::InterfaceProductIdentityMismatch => Self::quality_artifact(
                &[ExpectedProductIdentity, ActualProductIdentity],
                interface_components!(&[ExpectedProductIdentity, ActualProductIdentity]),
            ),
            Self::InterfaceConstantCallableBodyUnavailable
            | Self::InterfaceExecutableTemplateUnavailable => {
                Self::quality_artifact(&[], interface_components!(&[]))
            }
            Self::InterfaceSymbolCapacityExceeded => Self::quality_external(
                &[
                    ActualPackageIdentity,
                    InterfaceLimit,
                    ActualCount,
                    MaximumCount,
                ],
                primary_components!(&[
                    ActualPackageIdentity,
                    InterfaceLimit,
                    ActualCount,
                    MaximumCount,
                ]),
            ),
            Self::InterfaceSymbolGraphInvalid => Self::quality_artifact(
                &[InterfaceSymbolGraphProblem],
                interface_components!(&[InterfaceSymbolGraphProblem]),
            ),
            Self::InterfaceSemanticSymbolUnresolved
            | Self::InterfaceSemanticSymbolKindInvalid
            | Self::InterfaceSemanticValueGraphInvalid
            | Self::InterfaceSemanticValueInvalid
            | Self::InterfaceExecutableTemplateInvalid
            | Self::InterfaceSupportEntityInvalid => Self::quality_artifact(
                &[InterfaceSemanticProblem],
                interface_components!(&[InterfaceSemanticProblem]),
            ),
            Self::InterfaceDuplicatePackage => Self::quality_artifact(
                &[ActualPackageIdentity],
                interface_components!(&[ActualPackageIdentity]),
            ),
            Self::InterfaceSymbolReferenceInvalid | Self::InterfaceDependencyReferenceInvalid => {
                Self::quality_artifact(
                    &[InterfaceRecordIndex],
                    interface_components!(&[InterfaceRecordIndex]),
                )
            }
            Self::InterfaceMissingDependency => Self::quality_artifact(
                &[ExpectedPackageIdentity],
                interface_components!(&[ExpectedPackageIdentity]),
            ),
            Self::InterfaceDependencyProductMismatch => Self::quality_artifact(
                &[ExpectedProductIdentity, ActualProductIdentity],
                interface_components!(&[ExpectedProductIdentity, ActualProductIdentity]),
            ),
            Self::InterfaceDependencyContentMismatch => Self::quality_artifact(
                &[ExpectedArtifactDigest, ActualArtifactDigest],
                interface_components!(&[ExpectedArtifactDigest, ActualArtifactDigest]),
            ),
            Self::InterfaceDependencySymbolMissing => Self::quality_artifact(
                &[ExpectedPackageIdentity, InterfaceSymbolIdentity],
                interface_components!(&[ExpectedPackageIdentity, InterfaceSymbolIdentity]),
            ),
            Self::InterfaceCompilerDeclarationExported => Self::quality_artifact(
                &[InterfaceSymbolIdentity],
                interface_components!(&[InterfaceSymbolIdentity]),
            ),
            Self::BindingUnresolvedName
            | Self::BindingAmbiguousName
            | Self::BindingInaccessibleName
            | Self::BindingMalformedName
            | Self::BindingCyclicModuleExport
            | Self::BindingInvalidModuleExportTarget => {
                Self::quality_source(&[ReferencedName], primary_components!(&[ReferencedName]))
            }
            Self::BindingNameAlreadyDefined => Self::quality_source(
                &[ReferencedName],
                related_components!(
                    &[ReferencedName],
                    DiagnosticRelatedLocationKind::FirstDeclaration
                ),
            ),
            Self::BindingConflictingModuleExport => Self::quality_source(
                &[ReferencedName],
                related_components!(
                    &[ReferencedName],
                    DiagnosticRelatedLocationKind::ConflictingDeclaration
                ),
            ),
            Self::BindingWrongNameKind => Self::quality_source(
                &[ReferencedName, ExpectedNameKind],
                primary_components!(&[ReferencedName, ExpectedNameKind]),
            ),
            Self::BindingInvalidCallableAbi => Self::quality_source(
                &[TokenText],
                note_components!(
                    &[TokenText],
                    DiagnosticNoteKind::CallableAbiDirectiveMustNameSupportedAbi
                ),
            ),
            Self::BindingDuplicateCallableAbi => Self::quality_source(
                &[TokenText],
                related_components!(&[TokenText], DiagnosticRelatedLocationKind::FirstDirective),
            ),
            Self::BindingIncoherentAlternativePattern => {
                Self::quality_source(&[], primary_components!(&[]))
            }
            Self::BindingMalformedDirectiveArgument => Self::quality_source(
                &[],
                note_components!(
                    &[],
                    DiagnosticNoteKind::DirectiveArgumentMustHaveCompleteForm
                ),
            ),
            Self::CheckingIncompatibleExpressionType => Self::quality_source(
                &[ExpectedType, ActualType],
                primary_components!(&[ExpectedType, ActualType]),
            ),
            Self::CheckingCompilerDefect => Self::quality_source(
                &[EmissionFailure],
                note_components!(&[EmissionFailure], DiagnosticNoteKind::ReportCompilerDefect),
            ),
            Self::CheckingIncompatiblePattern => {
                Self::quality_source(&[ActualType], primary_components!(&[ActualType]))
            }
            Self::CheckingTypeRepresentationRecursionLimitExceeded => Self::quality_source(
                &[ActualCount, MaximumCount],
                primary_components!(&[ActualCount, MaximumCount]),
            ),
            Self::CheckingImplementationCoherenceLimitExceeded => Self::quality_source(
                &[ActualCount, MaximumCount],
                primary_components!(&[ActualCount, MaximumCount]),
            ),
            Self::CheckingCallableOverloadLimitExceeded => Self::quality_source(
                &[ActualCount, MaximumCount],
                primary_components!(&[ActualCount, MaximumCount]),
            ),
            Self::CheckingMissingTraitFulfillment | Self::CheckingExtraTraitFulfillment => {
                Self::quality_source(&[TraitMemberName], primary_components!(&[TraitMemberName]))
            }
            Self::CheckingIncompatibleTraitFulfillment => Self::quality_source(
                &[TraitMemberName, TraitFulfillmentMismatch],
                primary_components!(&[TraitMemberName, TraitFulfillmentMismatch]),
            ),
            Self::CheckingDuplicateTraitFulfillment => Self::quality_source(
                &[TraitMemberName],
                related_components!(
                    &[TraitMemberName],
                    DiagnosticRelatedLocationKind::FirstDeclaration
                ),
            ),
            Self::CheckingMissingForeignCallableDirective => Self::quality_source(
                &[ExpectedSyntaxKind],
                primary_components!(&[ExpectedSyntaxKind]),
            ),
            Self::CheckingForeignCallableRequiresCapability
            | Self::CheckingUnavailableNativeLinkInput
            | Self::CheckingUndeclaredTrustedCapability
            | Self::CheckingUnusedTrustedCapability
            | Self::CheckingTrustedCapabilityRequiresTrustedCallable
            | Self::CheckingMutableIndexContractRequired
            | Self::CheckingUnknownUnionVariant => {
                Self::quality_source(&[ReferencedName], primary_components!(&[ReferencedName]))
            }
            Self::CheckingForeignAbiTypeUnsupported => Self::quality_source(
                &[ActualType, CallableAbi],
                primary_components!(&[ActualType, CallableAbi]),
            ),
            Self::CheckingDuplicateNativeSymbol => Self::quality_source(
                &[DeclarationName],
                related_components!(
                    &[DeclarationName],
                    DiagnosticRelatedLocationKind::FirstDeclaration
                ),
            ),
            Self::CheckingNoApplicableCandidate => {
                Self::quality_source(&[SelectionKind], primary_components!(&[SelectionKind]))
            }
            Self::CheckingIncompatibleCandidate => Self::quality_source(
                &[SelectionKind, SelectionRejections],
                primary_components!(&[SelectionKind, SelectionRejections]),
            ),
            Self::CheckingAmbiguousCandidate => Self::quality_source(
                &[SelectionKind, SelectionCandidates],
                note_components!(
                    &[SelectionKind, SelectionCandidates],
                    DiagnosticNoteKind::SelectionMustBeDisambiguated
                ),
            ),
            Self::CheckingTargetRepresentationUnavailable => Self::quality_source(
                &[TargetTriple, TargetRepresentation],
                primary_components!(&[TargetTriple, TargetRepresentation]),
            ),
            Self::CheckingTargetCallableAbiUnavailable => Self::quality_source(
                &[TargetTriple, CallableAbi],
                primary_components!(&[TargetTriple, CallableAbi]),
            ),
            Self::CheckingThreadLocalStaticUnavailable => {
                Self::quality_source(&[TargetTriple], primary_components!(&[TargetTriple]))
            }
            Self::CheckingStaticDependencyOutlivesOwner => Self::quality_source(
                &[DependencySubjectKind],
                primary_components!(&[DependencySubjectKind]),
            ),
            Self::CheckingStaticLifecycleCycle
            | Self::CheckingStaticSpecializationDivergence
            | Self::CheckingStaticConstraintUnsatisfied
            | Self::CheckingExternStaticSurfaceUnsupported
            | Self::CheckingExportedStaticSurfaceUnsupported
            | Self::CheckingNativeStaticTypeUnsupported => {
                Self::quality_source(&[], primary_components!(&[]))
            }
            Self::CheckingTargetAbiRepresentationUnsupported => Self::quality_source(
                &[TargetTriple, CallableAbi, TargetRepresentation],
                primary_components!(&[TargetTriple, CallableAbi, TargetRepresentation]),
            ),
            Self::CheckingTargetAlignmentUnsupported => Self::quality_source(
                &[
                    TargetTriple,
                    AlignmentKind,
                    RequiredAlignment,
                    MaximumAlignment,
                ],
                primary_components!(&[
                    TargetTriple,
                    AlignmentKind,
                    RequiredAlignment,
                    MaximumAlignment,
                ]),
            ),
            Self::CheckingDuplicateModuleContributionDirective => Self::quality_source(
                &[ActualSyntaxKind],
                related_components!(
                    &[ActualSyntaxKind],
                    DiagnosticRelatedLocationKind::FirstDeclaration
                ),
            ),
            Self::CheckingCannotInferExpressionType => Self::quality_source(
                &[ExpressionCategory],
                note_components!(
                    &[ExpressionCategory],
                    DiagnosticNoteKind::TypeInferenceNeedsConstraint
                ),
            ),
            Self::CheckingRangeElementTypeMustBeInteger => {
                Self::quality_source(&[ActualType], primary_components!(&[ActualType]))
            }
            Self::CheckingInvalidConstantExpression => Self::quality_source(
                &[ExpressionCategory],
                note_components!(
                    &[ExpressionCategory],
                    DiagnosticNoteKind::ConstantExpressionMustBeEvaluable
                ),
            ),
            Self::CheckingInvalidConstantOperation => Self::quality_source(
                &[ConstantOperation],
                note_components!(
                    &[ConstantOperation],
                    DiagnosticNoteKind::ConstantExpressionMustBeEvaluable
                ),
            ),
            Self::CheckingConstantLiteralNotRepresentable => {
                Self::quality_source(&[ActualType], primary_components!(&[ActualType]))
            }
            Self::CheckingConstantEvaluationStepLimitExceeded
            | Self::CheckingConstantAggregateLimitExceeded
            | Self::CheckingConstantExpansionLimitExceeded
            | Self::CheckingConstantLiteralSizeLimitExceeded => Self::quality_source(
                &[ActualCount, MaximumCount],
                note_components!(
                    &[ActualCount, MaximumCount],
                    DiagnosticNoteKind::ConstantEvaluationMustFitLimits
                ),
            ),
            Self::CheckingConstantIntegerSizeLimitExceeded => Self::quality_source(
                &[ConstantOperation, ActualCount, MaximumCount],
                note_components!(
                    &[ConstantOperation, ActualCount, MaximumCount],
                    DiagnosticNoteKind::ConstantEvaluationMustFitLimits
                ),
            ),
            Self::CheckingConstantDivisionByZero => Self::quality_source(
                &[ConstantOperation],
                primary_components!(&[ConstantOperation]),
            ),
            Self::CheckingConstantValueNotRepresentable => Self::quality_source(
                &[ConstantOperation, ActualType],
                primary_components!(&[ConstantOperation, ActualType]),
            ),
            Self::CheckingCyclicConstantDefinition => {
                Self::quality_source(&[], primary_components!(&[]))
            }
            Self::CheckingRefinementCapacityExceeded => Self::quality_source(
                &[RefinementCapacity],
                primary_components!(&[RefinementCapacity]),
            ),
            Self::CheckingNoCompatiblePropagationBoundary => Self::quality_source(
                &[PropagationProblem],
                note_components!(
                    &[PropagationProblem],
                    DiagnosticNoteKind::PropagationBoundaryMustMatch
                ),
            ),
            Self::CheckingArrayGeneratorCardinalityNotProvable => Self::quality_source(
                &[ArrayGeneratorCardinalityProblem],
                note_components!(
                    &[ArrayGeneratorCardinalityProblem],
                    DiagnosticNoteKind::ArrayGeneratorMustYieldOncePerElement
                ),
            ),
            Self::CheckingInvalidStoredType => Self::quality_source(
                &[StoredTypeProblem],
                note_components!(
                    &[StoredTypeProblem],
                    DiagnosticNoteKind::StoredTypeRequiresIndirection
                ),
            ),
            Self::CheckingRecursiveTypeRepresentation => Self::quality_source(
                &[ActualCount],
                related_components!(
                    &[ActualCount],
                    DiagnosticRelatedLocationKind::RepresentationCycleLocation
                ),
            ),
            Self::CheckingInvalidLayoutDirective => Self::quality_source(
                &[LayoutProblem],
                note_components!(
                    &[LayoutProblem],
                    DiagnosticNoteKind::TypeLayoutDirectiveForms
                ),
            ),
            Self::CheckingInvalidCopyContract => Self::quality_source(
                &[CopyContractProblem],
                note_components!(
                    &[CopyContractProblem],
                    DiagnosticNoteKind::CopyContractRequirements
                ),
            ),
            Self::CheckingInvalidUnionTag => Self::quality_source(
                &[UnionTagProblem],
                note_components!(
                    &[UnionTagProblem],
                    DiagnosticNoteKind::UnionTagDirectiveForms
                ),
            ),
            Self::CheckingTargetMemoryOperationUnavailable => Self::quality_source(
                &[TargetTriple, MemoryOperation],
                primary_components!(&[TargetTriple, MemoryOperation]),
            ),
            Self::CheckingInvalidTargetControlContract => Self::quality_source(
                &[TargetTriple, MemoryOperation],
                primary_components!(&[TargetTriple, MemoryOperation]),
            ),
            Self::CheckingInvalidAtomicMemoryOrder => {
                Self::quality_source(&[MemoryOperation], primary_components!(&[MemoryOperation]))
            }
            Self::CheckingInvalidCallbackStateContext => Self::quality_source(
                &[CallbackStateProblem],
                note_components!(
                    &[CallbackStateProblem],
                    DiagnosticNoteKind::CallbackStateRequirements
                ),
            ),
            Self::CheckingMissingTrustedMemoryGuarantees => {
                Self::quality_source(&[MemoryOperation], primary_components!(&[MemoryOperation]))
            }
            Self::CheckingMemoryOperationAfterDeallocation => Self::quality_source(
                &[MemoryOperation],
                related_components!(
                    &[MemoryOperation],
                    DiagnosticRelatedLocationKind::DeallocationOrigin
                ),
            ),
            Self::CheckingUninitializedRawStorage => {
                Self::quality_source(&[MemoryOperation], primary_components!(&[MemoryOperation]))
            }
            Self::CheckingDeallocationWithOutstandingObligations => Self::quality_source(
                &[MemoryOperation],
                related_components!(
                    &[MemoryOperation],
                    DiagnosticRelatedLocationKind::InitializationOrigin
                ),
            ),
            Self::CheckingMissingMutationAuthority | Self::CheckingMissingStorageOwnership => {
                Self::quality_source(&[StorageAccess], primary_components!(&[StorageAccess]))
            }
            Self::CheckingConflictingBorrow => Self::quality_source(
                &[StorageAccess],
                related_components!(
                    &[StorageAccess],
                    DiagnosticRelatedLocationKind::BorrowOrigin
                ),
            ),
            Self::CheckingEscapingStorageDependency => Self::quality_source(
                &[],
                related_note_components!(
                    &[],
                    DiagnosticRelatedLocationKind::DependencyStorageOrigin,
                    DiagnosticNoteKind::EscapingStorageDependencyResolution
                ),
            ),
            Self::CheckingUseOfMovedStorage => Self::quality_source(
                &[StorageAccess],
                related_components!(&[StorageAccess], DiagnosticRelatedLocationKind::MoveOrigin),
            ),
            Self::CheckingIncompleteLifecycleStorage => {
                Self::quality_source(&[ActualType], primary_components!(&[ActualType]))
            }
            Self::CheckingRefutablePattern => Self::quality_source(
                &[ActualType],
                note_components!(
                    &[ActualType],
                    DiagnosticNoteKind::RefutablePatternRequiresConditionalContext
                ),
            ),
            Self::CheckingBindingInPatternTest => Self::quality_source(
                &[],
                note_components!(&[], DiagnosticNoteKind::PatternTestMustNotBind),
            ),
            Self::CheckingNonExhaustiveMatch => {
                Self::quality_source(&[PatternCoverage], primary_components!(&[PatternCoverage]))
            }
            Self::CheckingUnreachableMatchArm => Self::quality_source(
                &[PatternUnreachability],
                primary_components!(&[PatternUnreachability]),
            ),
            Self::CheckingUnreachablePatternAlternative => Self::quality_source(
                &[PatternUnreachability],
                related_components!(
                    &[PatternUnreachability],
                    DiagnosticRelatedLocationKind::CoveredByPattern
                ),
            ),
            Self::CheckingOverlappingImplementation
            | Self::CheckingUngroupedImplementationOverloads => Self::quality_source(
                &[ActualType, InterfaceSymbolIdentity],
                related_components!(
                    &[ActualType, InterfaceSymbolIdentity],
                    DiagnosticRelatedLocationKind::ConflictingDeclaration
                ),
            ),
            Self::CheckingInvalidImplementationOverloadHeader
            | Self::CheckingInvalidImplementationOverloadArm => Self::quality_source(
                &[ImplementationOverloadProblem],
                primary_components!(&[ImplementationOverloadProblem]),
            ),
            Self::CheckingDuplicateImplementationOverloadArm => Self::quality_source(
                &[ImplementationOverloadProblem],
                related_components!(
                    &[ImplementationOverloadProblem],
                    DiagnosticRelatedLocationKind::FirstDeclaration
                ),
            ),
            Self::CheckingInvalidCallableOverloadArm => Self::quality_source(
                &[CallableOverloadProblem],
                primary_components!(&[CallableOverloadProblem]),
            ),
            Self::CheckingDuplicateCallableOverloadArm => Self::quality_source(
                &[CallableOverloadProblem],
                related_components!(
                    &[CallableOverloadProblem],
                    DiagnosticRelatedLocationKind::FirstDeclaration
                ),
            ),
            Self::CheckingConflictingCallableOverloadFamily
            | Self::CheckingConflictingCallableOverloadSignature => Self::quality_source(
                &[CallableOverloadProblem],
                related_components!(
                    &[CallableOverloadProblem],
                    DiagnosticRelatedLocationKind::ConflictingDeclaration
                ),
            ),
            Self::CheckingInvalidNativeLinkDirective => Self::quality_source(
                &[NativeLinkDirectiveProblem],
                primary_components!(&[NativeLinkDirectiveProblem]),
            ),
            Self::CheckingInvalidNativeSymbolDirective => Self::quality_source(
                &[NativeSymbolDirectiveProblem],
                primary_components!(&[NativeSymbolDirectiveProblem]),
            ),
            Self::CheckingForeignCallableRequiresTrusted
            | Self::CheckingForeignCallableExecutionUnsupported => {
                Self::quality_source(&[CallableAbi], primary_components!(&[CallableAbi]))
            }
            Self::CheckingVariadicCallableContractUnsupported => {
                Self::quality_source(&[], primary_components!(&[]))
            }
            Self::CheckingCallableAddressTypeUnsupported => {
                Self::quality_source(&[], primary_components!(&[]))
            }
            Self::CheckingFixedLayoutQueryTypeUnsupported => {
                Self::quality_source(&[MemoryOperation], primary_components!(&[MemoryOperation]))
            }
            Self::CheckingTrailingLayoutQueryTypeUnsupported => {
                Self::quality_source(&[], primary_components!(&[]))
            }
            Self::CheckingMemoryPointeeTypeUnsupported => {
                Self::quality_source(&[MemoryOperation], primary_components!(&[MemoryOperation]))
            }
            Self::CheckingTaglessUnionPatternRequiresVariant => {
                Self::quality_source(&[], primary_components!(&[]))
            }
            Self::CheckingPlatformServiceSignatureMismatch => Self::quality_source(
                &[PlatformServiceSignatureProblem],
                primary_components!(&[PlatformServiceSignatureProblem]),
            ),
            Self::CheckingAwaitOutsideAsyncCallable
            | Self::CheckingTaskStartOutsideAsyncCallable => Self::quality_source(
                &[],
                note_components!(&[], DiagnosticNoteKind::AsynchronousCallableRequired),
            ),
            Self::CheckingUnavailableAwaitDependency => Self::quality_source(
                &[DependencySubjectKind, DependencyRequirementKind],
                primary_components!(&[DependencySubjectKind, DependencyRequirementKind]),
            ),
            Self::CheckingEntrypointNotAllowed => Self::quality_source(
                &[ActualProductKind],
                note_components!(
                    &[ActualProductKind],
                    DiagnosticNoteKind::EntrypointDirectiveRequiresExecutableProduct
                ),
            ),
            Self::CheckingMissingEntrypoint => Self::quality_source(
                &[],
                note_components!(&[], DiagnosticNoteKind::ExecutableEntrypointRequired),
            ),
            Self::CheckingDuplicateEntrypoint => Self::quality_source(
                &[ActualCount],
                related_note_components!(
                    &[ActualCount],
                    DiagnosticRelatedLocationKind::FirstDeclaration,
                    DiagnosticNoteKind::ExecutableEntrypointRequired
                ),
            ),
            Self::CheckingEntryCannotBeGeneric | Self::CheckingEntryCannotTakeParameters => {
                Self::quality_source(
                    &[ActualCount],
                    note_components!(&[ActualCount], DiagnosticNoteKind::ProductEntryRequirements),
                )
            }
            Self::CheckingInvalidEntrypointResult | Self::CheckingInvalidTestResult => {
                Self::quality_source(
                    &[ActualType],
                    note_components!(&[ActualType], DiagnosticNoteKind::ProductEntryRequirements),
                )
            }
            Self::CheckingInvalidTestEntryDirective | Self::CheckingInvalidTestModuleDirective => {
                Self::quality_source(
                    &[ActualCount, TokenText],
                    note_components!(
                        &[ActualCount, TokenText],
                        DiagnosticNoteKind::TestDirectiveRequirements
                    ),
                )
            }
            Self::CheckingEntryCannotBeConstant | Self::CheckingEntryCannotRequireTrust => {
                Self::quality_source(
                    &[],
                    note_components!(&[], DiagnosticNoteKind::ProductEntryRequirements),
                )
            }
            Self::CheckingExportDependsOnInternalDeclaration => Self::quality_source(
                &[InterfaceSymbolIdentity],
                related_note_components!(
                    &[InterfaceSymbolIdentity],
                    DiagnosticRelatedLocationKind::RequirementOrigin,
                    DiagnosticNoteKind::PublicDependencyRequired
                ),
            ),
            Self::CheckingDuplicateTestIdentity => Self::quality_source(
                &[DeclarationName],
                related_note_components!(
                    &[DeclarationName],
                    DiagnosticRelatedLocationKind::FirstDeclaration,
                    DiagnosticNoteKind::UniqueTestIdentityRequired
                ),
            ),
            Self::EmissionMissingContribution | Self::EmissionInvalidContribution => {
                Self::quality_artifact(
                    &[ArtifactKind, ArtifactOrdinal],
                    primary_components!(&[ArtifactKind, ArtifactOrdinal]),
                )
            }
            Self::EmissionArtifactReadFailed
            | Self::EmissionArtifactOpenFailed
            | Self::EmissionArtifactWriteFailed
            | Self::EmissionArtifactFlushFailed
            | Self::EmissionArtifactCommitFailed => Self::quality_artifact(
                &[ArtifactKind, ArtifactOrdinal, IoErrorKind],
                primary_components!(&[ArtifactKind, ArtifactOrdinal, IoErrorKind]),
            ),
            Self::EmissionArtifactDigestMismatch => Self::quality_artifact(
                &[
                    ArtifactKind,
                    ArtifactOrdinal,
                    ExpectedArtifactDigest,
                    ActualArtifactDigest,
                ],
                primary_components!(&[
                    ArtifactKind,
                    ArtifactOrdinal,
                    ExpectedArtifactDigest,
                    ActualArtifactDigest,
                ]),
            ),
            Self::EmissionArtifactLengthMismatch => Self::quality_artifact(
                &[
                    ArtifactKind,
                    ArtifactOrdinal,
                    ExpectedByteCount,
                    ActualByteCount,
                ],
                primary_components!(&[
                    ArtifactKind,
                    ArtifactOrdinal,
                    ExpectedByteCount,
                    ActualByteCount,
                ]),
            ),
            Self::EmissionManagedPublicationUnsupported
            | Self::EmissionGenerationCollision
            | Self::EmissionGenerationManifestInvalid => Self::quality_artifact(
                &[ArtifactKind, ArtifactOrdinal],
                primary_components!(&[ArtifactKind, ArtifactOrdinal]),
            ),
            Self::RetainedGenerationInvalid => Self::quality_artifact(
                &[
                    FilePath,
                    crate::DiagnosticArgName::RetainedGenerationProblem,
                ],
                primary_components!(&[
                    FilePath,
                    crate::DiagnosticArgName::RetainedGenerationProblem
                ]),
            ),
            Self::RetainedArtifactLengthMismatch => Self::quality_artifact(
                &[FilePath, ExpectedByteCount, ActualByteCount],
                primary_components!(&[FilePath, ExpectedByteCount, ActualByteCount]),
            ),
            Self::RetainedArtifactDigestMismatch => Self::quality_artifact(
                &[FilePath, ExpectedArtifactDigest, ActualArtifactDigest],
                primary_components!(&[FilePath, ExpectedArtifactDigest, ActualArtifactDigest]),
            ),
            Self::BuildStorageMetadataInvalid => Self::quality_artifact(
                &[FilePath, DocumentParseKind],
                primary_components!(&[FilePath, DocumentParseKind]),
            ),
            Self::BuildStorageRevisionMismatch => Self::quality_artifact(
                &[FilePath, ExpectedRevision, ActualRevision],
                primary_components!(&[FilePath, ExpectedRevision, ActualRevision]),
            ),
            Self::BuildStorageIoFailed => Self::quality_artifact(
                &[
                    FilePath,
                    crate::DiagnosticArgName::StorageOperation,
                    IoErrorKind,
                ],
                primary_components!(&[
                    FilePath,
                    crate::DiagnosticArgName::StorageOperation,
                    IoErrorKind
                ]),
            ),
            Self::BuildStorageUnsafePath
            | Self::RetainedBuildStateUnavailable
            | Self::BuildStorageCancelled => {
                Self::quality_artifact(&[FilePath], primary_components!(&[FilePath]))
            }
            Self::EmissionLinkedPlanMissing => Self::quality_artifact(
                &[ActualProductIdentity, TargetTriple],
                compiler_defect_components!(&[ActualProductIdentity, TargetTriple]),
            ),
            Self::EmissionFailed => Self::quality_artifact(
                &[ActualProductIdentity, TargetTriple, EmissionFailure],
                primary_components!(&[ActualProductIdentity, TargetTriple, EmissionFailure]),
            ),
            Self::EmissionTargetMismatch => Self::quality_artifact(
                &[
                    ActualProductIdentity,
                    ExpectedTargetTriple,
                    ActualTargetTriple,
                ],
                primary_components!(&[
                    ActualProductIdentity,
                    ExpectedTargetTriple,
                    ActualTargetTriple,
                ]),
            ),
            Self::EmissionProductMismatch => Self::quality_artifact(
                &[ActualProductIdentity, ExpectedPackageIdentity],
                primary_components!(&[ActualProductIdentity, ExpectedPackageIdentity]),
            ),
            Self::EmissionArtifactIoFailed => Self::quality_artifact(
                &[
                    ActualProductIdentity,
                    TargetTriple,
                    ArtifactKind,
                    ArtifactOrdinal,
                    EmissionArtifactOperation,
                    IoErrorKind,
                ],
                primary_components!(&[
                    ActualProductIdentity,
                    TargetTriple,
                    ArtifactKind,
                    ArtifactOrdinal,
                    EmissionArtifactOperation,
                    IoErrorKind,
                ]),
            ),
            Self::CodegenUnsupportedTarget => Self::quality_artifact(
                &[CodegenBackendIdentity, TargetTriple],
                primary_components!(&[CodegenBackendIdentity, TargetTriple]),
            ),
            Self::CodegenInvalidConfiguration
            | Self::CodegenResourceExhausted
            | Self::CodegenGeneratedModuleInvalid => Self::quality_artifact(
                &[CodegenBackendIdentity, TargetTriple],
                compiler_defect_components!(&[CodegenBackendIdentity, TargetTriple]),
            ),
            Self::CodegenBackendLibraryFailed => Self::quality_artifact(
                &[CodegenBackendIdentity, TargetTriple, CodegenBackendReport],
                compiler_defect_components!(&[
                    CodegenBackendIdentity,
                    TargetTriple,
                    CodegenBackendReport,
                ]),
            ),
            Self::CodegenBackendToolExited => Self::quality_artifact(
                &[
                    CodegenBackendIdentity,
                    TargetTriple,
                    FilePath,
                    ExternalToolExit,
                ],
                compiler_defect_components!(&[
                    CodegenBackendIdentity,
                    TargetTriple,
                    FilePath,
                    ExternalToolExit,
                ]),
            ),
            Self::CodegenBackendRejectedModule => Self::quality_artifact(
                &[
                    CodegenBackendIdentity,
                    TargetTriple,
                    CodegenVerificationStage,
                    CodegenBackendReport,
                ],
                compiler_defect_components!(&[
                    CodegenBackendIdentity,
                    TargetTriple,
                    CodegenVerificationStage,
                    CodegenBackendReport,
                ]),
            ),
            Self::CodegenUnsupportedArtifact => Self::quality_artifact(
                &[CodegenBackendIdentity, TargetTriple, ArtifactKind],
                primary_components!(&[CodegenBackendIdentity, TargetTriple, ArtifactKind]),
            ),
            Self::CodegenArtifactConstructionFailed => Self::quality_artifact(
                &[CodegenBackendIdentity, TargetTriple, ArtifactKind],
                compiler_defect_components!(&[CodegenBackendIdentity, TargetTriple, ArtifactKind,]),
            ),
            Self::NativeProductPreparationFailed => Self::quality_artifact(
                &[
                    ActualProductIdentity,
                    TargetTriple,
                    NativeProductFailureKind,
                ],
                primary_components!(&[
                    ActualProductIdentity,
                    TargetTriple,
                    NativeProductFailureKind
                ]),
            ),
            Self::LinkerUnsupportedTarget
            | Self::LinkerUnsupportedProduct
            | Self::LinkerUnsupportedInput
            | Self::LinkerUnsupportedInputMode
            | Self::LinkerUnsupportedOutput
            | Self::LinkerUnsupportedSearchPath
            | Self::LinkerUnsupportedLinkModel
            | Self::LinkerUnsupportedDeadStrip
            | Self::LinkerUnsupportedSectionGarbageCollection
            | Self::LinkerUnsupportedDebug
            | Self::LinkerUnsupportedSubsystem
            | Self::LinkerUnsupportedSymbol
            | Self::LinkerUnsupportedStartup
            | Self::LinkerUnsupportedRuntime
            | Self::LinkerUnsupportedOptimization => {
                Self::quality_artifact(&[LinkRequirement], primary_components!(&[LinkRequirement]))
            }
            Self::LinkerInputMissing => Self::quality_artifact(
                &[
                    InputIndex,
                    LinkRequirement,
                    TargetTriple,
                    ActualProductIdentity,
                    LinkerDriverIdentity,
                ],
                link_plan_components!(&[InputIndex, LinkRequirement,]),
            ),
            Self::LinkerOutputMissing | Self::LinkerOutputInvalid => Self::quality_artifact(
                &[
                    ArtifactOrdinal,
                    LinkRequirement,
                    FilePath,
                    TargetTriple,
                    ActualProductIdentity,
                    LinkerDriverIdentity,
                ],
                link_plan_components!(&[ArtifactOrdinal, LinkRequirement, FilePath,]),
            ),
            Self::LinkerDriverUnavailable | Self::LinkerResourceExhausted => {
                Self::quality_artifact(
                    &[TargetTriple, ActualProductIdentity, LinkerDriverIdentity],
                    link_plan_components!(&[]),
                )
            }
            Self::LinkerDriverIncompatible | Self::LinkerInvocationFailed => {
                Self::quality_artifact(
                    &[TargetTriple, ActualProductIdentity, LinkerDriverIdentity],
                    link_plan_defect_components!(&[]),
                )
            }
            Self::LinkerOptimizationReportInvalid => Self::quality_artifact(
                &[
                    LinkOptimizationReportProblem,
                    TargetTriple,
                    ActualProductIdentity,
                    LinkerDriverIdentity,
                ],
                link_plan_components!(&[LinkOptimizationReportProblem]),
            ),
            Self::LinkerResponseFileFailed => Self::quality_artifact(
                &[
                    ExternalToolOperation,
                    FilePath,
                    IoErrorKind,
                    TargetTriple,
                    ActualProductIdentity,
                    LinkerDriverIdentity,
                ],
                link_plan_components!(&[ExternalToolOperation, FilePath, IoErrorKind,]),
            ),
            Self::LinkerExternalToolIoFailed => Self::quality_artifact(
                &[
                    ExternalToolOperation,
                    IoErrorKind,
                    TargetTriple,
                    ActualProductIdentity,
                    LinkerDriverIdentity,
                ],
                link_plan_components!(&[ExternalToolOperation, IoErrorKind,]),
            ),
            Self::LinkerExternalToolContractFailed => Self::quality_artifact(
                &[
                    ExternalToolOperation,
                    ExternalToolFailureKind,
                    TargetTriple,
                    ActualProductIdentity,
                    LinkerDriverIdentity,
                ],
                link_plan_components!(&[ExternalToolOperation, ExternalToolFailureKind,]),
            ),
            Self::LinkerExternalToolExitedUnsuccessfully => Self::quality_artifact(
                &[
                    ExternalToolExit,
                    TargetTriple,
                    ActualProductIdentity,
                    LinkerDriverIdentity,
                ],
                &[
                    DiagnosticComponentContract::PrimaryMessage {
                        args: &[ExternalToolExit],
                    },
                    DiagnosticComponentContract::Note {
                        kind: DiagnosticNoteKind::LinkPlanContext,
                        args: &[ActualProductIdentity, TargetTriple, LinkerDriverIdentity],
                    },
                    DiagnosticComponentContract::Note {
                        kind: DiagnosticNoteKind::ExternalToolExitRequiresCorrection,
                        args: &[],
                    },
                ],
            ),
        }
    }

    const fn quality_invocation(
        args: &'static [DiagnosticArgName],
        components: &'static [DiagnosticComponentContract],
    ) -> DiagnosticQualityContract {
        DiagnosticQualityContract::invocation(args, components)
    }

    const fn quality_external(
        args: &'static [DiagnosticArgName],
        components: &'static [DiagnosticComponentContract],
    ) -> DiagnosticQualityContract {
        DiagnosticQualityContract::external(args, components)
    }

    const fn quality_source(
        args: &'static [DiagnosticArgName],
        components: &'static [DiagnosticComponentContract],
    ) -> DiagnosticQualityContract {
        DiagnosticQualityContract::source(args, components)
    }

    const fn quality_artifact(
        required_args: &'static [DiagnosticArgName],
        components: &'static [DiagnosticComponentContract],
    ) -> DiagnosticQualityContract {
        DiagnosticQualityContract::artifact(required_args, components)
    }
}

#[cfg(test)]
mod tests {
    use bray_source::{SourceId, SourceSpan, TextSize};
    use bray_syntax::SyntaxKind;

    use crate::{
        Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticLabel,
        DiagnosticLabelKind, DiagnosticNote, DiagnosticNoteKind, DiagnosticQualityIssue,
        DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, SeverityKind,
    };

    #[test]
    fn every_actionable_suggestion_has_an_explanatory_note() {
        for &kind in DiagnosticKind::ALL {
            let contract = kind.quality_contract();

            assert!(
                !contract.requires_suggestion() || contract.requires_note(),
                "{kind:?} requires a suggestion without an explanatory note"
            );
        }
    }

    #[test]
    fn quality_contracts_reject_missing_structure_and_accept_complete_producers() {
        let span = SourceSpan::empty(SourceId::new(0), TextSize::new(4));

        let incomplete = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::SyntaxExpectedToken,
            SeverityKind::Error,
        );

        assert_eq!(
            DiagnosticKind::SyntaxExpectedToken
                .quality_contract()
                .unmet_requirements(&incomplete),
            [
                DiagnosticQualityIssue::MissingPrimarySpan,
                DiagnosticQualityIssue::MissingTypedContext,
            ],
        );

        let complete = incomplete
            .with_primary_span(span)
            .with_arg(DiagnosticArg::expected_syntax_kind(
                SyntaxKind::CloseParenToken,
            ))
            .with_arg(DiagnosticArg::actual_syntax_kind(
                SyntaxKind::EndOfFileToken,
            ))
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::ExpectedTokenInsertionPoint,
                span,
            ));

        assert!(
            DiagnosticKind::SyntaxExpectedToken
                .quality_contract()
                .unmet_requirements(&complete)
                .is_empty()
        );

        let external = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::SourceFileReadFailed,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::file_path("main.bray"));

        assert_eq!(
            DiagnosticKind::SourceFileReadFailed
                .quality_contract()
                .unmet_requirements(&external),
            [
                DiagnosticQualityIssue::MissingTypedContext,
                DiagnosticQualityIssue::MissingComponent,
            ],
        );
    }

    #[test]
    fn related_location_contracts_reject_the_primary_location() {
        let span = SourceSpan::empty(SourceId::new(0), TextSize::new(4));

        let diagnostic = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::CheckingDuplicateNativeSymbol,
            SeverityKind::Error,
        )
        .with_primary_span(span)
        .with_arg(DiagnosticArg::declaration_name("native_write"))
        .with_label(DiagnosticLabel::primary(
            DiagnosticLabelKind::InvalidForeignBoundary,
            span,
        ))
        .with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::FirstDeclaration,
            span,
        ));

        assert_eq!(
            DiagnosticKind::CheckingDuplicateNativeSymbol
                .quality_contract()
                .unmet_requirements(&diagnostic),
            [DiagnosticQualityIssue::InvalidRelatedLocation]
        );
    }

    #[test]
    fn quality_contracts_reject_ambiguous_component_content() {
        let duplicate_argument = Diagnostic::new(
            DiagnosticId::new(0),
            DiagnosticKind::RequestReservedPackageIdentity,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::referenced_name("first"))
        .with_arg(DiagnosticArg::referenced_name("second"));

        assert_eq!(
            duplicate_argument
                .kind()
                .quality_contract()
                .unmet_requirements(&duplicate_argument),
            [DiagnosticQualityIssue::DuplicateArgument]
        );

        let span = SourceSpan::empty(SourceId::new(1), TextSize::new(0));
        let label = DiagnosticLabel::secondary(DiagnosticLabelKind::InvalidForeignBoundary, span);

        let duplicate_label = Diagnostic::new(
            DiagnosticId::new(1),
            DiagnosticKind::RequestInvalidWorkerBudget,
            SeverityKind::Error,
        )
        .with_arg(DiagnosticArg::worker_count(0))
        .with_label(label.clone())
        .with_label(label)
        .with_note(DiagnosticNote::new(
            DiagnosticNoteKind::WorkerBudgetMustBePositive,
        ));

        assert_eq!(
            duplicate_label
                .kind()
                .quality_contract()
                .unmet_requirements(&duplicate_label),
            [DiagnosticQualityIssue::DuplicateAction]
        );
    }
}
