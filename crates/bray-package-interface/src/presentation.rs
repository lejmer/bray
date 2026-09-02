use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticArtifactDigest, DiagnosticArtifactDigestAlgorithm,
    DiagnosticId, DiagnosticInterfaceCompressionFailure, DiagnosticInterfaceDependency,
    DiagnosticInterfaceIntegerTarget, DiagnosticInterfaceLimit, DiagnosticInterfaceMalformedCause,
    DiagnosticInterfaceProductKind, DiagnosticInterfaceSection,
    DiagnosticInterfaceSemanticRecordKind, DiagnosticInterfaceUtf8Failure,
    DiagnosticInterfaceValidationContext, DiagnosticInterfaceValidationFailure,
    DiagnosticInterfaceValidationField, DiagnosticKind, DiagnosticPackageInterfaceIdentity,
    SeverityKind,
};

use crate::{
    CURRENT_FORMAT_REVISION, InterfaceCompressionFailure, InterfaceDependency,
    InterfaceIntegerTarget, InterfaceLimit, InterfaceMalformedCause, InterfaceProductKind,
    InterfaceSectionTag, InterfaceSemanticRecordKind, InterfaceUtf8Failure,
    InterfaceValidationContext, InterfaceValidationError, InterfaceValidationField,
    PackageInterfaceIdentity,
};

pub(crate) fn validation_diagnostic(
    error: InterfaceValidationError,
    id: DiagnosticId,
) -> Diagnostic {
    let kind = diagnostic_kind(&error);
    let failure = diagnostic_failure(&error);

    Diagnostic::new(id, kind, SeverityKind::Error)
        .with_arg(DiagnosticArg::interface_validation_failure(failure))
}

const fn diagnostic_kind(error: &InterfaceValidationError) -> DiagnosticKind {
    match error {
        InterfaceValidationError::InvalidMagic { .. } => DiagnosticKind::InterfaceInvalidMagic,
        InterfaceValidationError::UnsupportedFormatRevision { .. } => {
            DiagnosticKind::InterfaceUnsupportedFormatRevision
        }
        InterfaceValidationError::UnsupportedLanguageRevision { .. } => {
            DiagnosticKind::InterfaceUnsupportedLanguageRevision
        }
        InterfaceValidationError::UnsupportedByteOrder { .. }
        | InterfaceValidationError::UnsupportedRequiredFlags { .. } => {
            DiagnosticKind::InterfaceUnsupportedEncoding
        }
        InterfaceValidationError::Truncated { .. }
        | InterfaceValidationError::TrailingBytes { .. }
        | InterfaceValidationError::Malformed { .. }
        | InterfaceValidationError::InvalidUtf8 { .. }
        | InterfaceValidationError::Compression { .. }
        | InterfaceValidationError::DigestUnavailable { .. }
        | InterfaceValidationError::AllocationUnavailable { .. }
        | InterfaceValidationError::SurfaceBuild { .. }
        | InterfaceValidationError::SpecializationKeyMismatch { .. }
        | InterfaceValidationError::ImplementationConfigurationMismatch { .. }
        | InterfaceValidationError::ImplementationInterfaceIdentityMismatch { .. }
        | InterfaceValidationError::ImplementationDependencyMismatch { .. } => {
            DiagnosticKind::InterfaceValidationFailed
        }
        InterfaceValidationError::ArtifactHashMismatch { .. }
        | InterfaceValidationError::ContentHashMismatch { .. }
        | InterfaceValidationError::PayloadContentHashMismatch { .. }
        | InterfaceValidationError::UnknownSectionChecksumMismatch { .. } => {
            DiagnosticKind::InterfaceHashMismatch
        }
        InterfaceValidationError::SectionChecksumMismatch { .. }
        | InterfaceValidationError::SectionContentHashMismatch { .. }
        | InterfaceValidationError::PayloadChecksumMismatch { .. } => {
            DiagnosticKind::InterfaceSectionChecksumMismatch
        }
        InterfaceValidationError::ResourceLimitExceeded { .. } => {
            DiagnosticKind::InterfaceResourceLimitExceeded
        }
    }
}

pub(crate) fn diagnostic_failure(
    error: &InterfaceValidationError,
) -> DiagnosticInterfaceValidationFailure {
    match error {
        InterfaceValidationError::InvalidMagic { actual } => {
            DiagnosticInterfaceValidationFailure::InvalidMagic { actual: *actual }
        }
        InterfaceValidationError::UnsupportedFormatRevision { actual } => {
            DiagnosticInterfaceValidationFailure::UnsupportedFormatRevision {
                expected: u64::from(CURRENT_FORMAT_REVISION.raw()),
                actual: u64::from(actual.raw()),
            }
        }
        InterfaceValidationError::UnsupportedLanguageRevision { expected, actual } => {
            DiagnosticInterfaceValidationFailure::UnsupportedLanguageRevision {
                expected: u64::from(expected.raw()),
                actual: u64::from(actual.raw()),
            }
        }
        InterfaceValidationError::UnsupportedByteOrder { expected, actual } => {
            DiagnosticInterfaceValidationFailure::UnsupportedByteOrder {
                expected: *expected,
                actual: *actual,
            }
        }
        InterfaceValidationError::UnsupportedRequiredFlags { actual } => {
            DiagnosticInterfaceValidationFailure::UnsupportedRequiredFlags {
                actual: actual.bits(),
            }
        }
        InterfaceValidationError::Truncated {
            context,
            field,
            offset,
            expected_length,
            actual_length,
        } => DiagnosticInterfaceValidationFailure::Truncated {
            context: diagnostic_context(*context),
            field: diagnostic_field(*field),
            offset: *offset,
            expected_length: *expected_length,
            actual_length: *actual_length,
        },
        InterfaceValidationError::TrailingBytes {
            context,
            offset,
            count,
        } => DiagnosticInterfaceValidationFailure::TrailingBytes {
            context: diagnostic_context(*context),
            offset: *offset,
            count: *count,
        },
        InterfaceValidationError::Malformed { context, cause } => {
            DiagnosticInterfaceValidationFailure::Malformed {
                context: diagnostic_context(*context),
                cause: diagnostic_malformed_cause(*cause),
            }
        }
        InterfaceValidationError::InvalidUtf8 {
            context,
            field,
            offset,
            length,
            cause,
        } => DiagnosticInterfaceValidationFailure::InvalidUtf8 {
            context: diagnostic_context(*context),
            field: diagnostic_field(*field),
            offset: *offset,
            length: *length,
            cause: diagnostic_utf8_failure(*cause),
        },
        InterfaceValidationError::Compression { context, cause } => {
            DiagnosticInterfaceValidationFailure::Compression {
                context: diagnostic_context(*context),
                cause: diagnostic_compression_failure(*cause),
            }
        }
        InterfaceValidationError::DigestUnavailable { context, field } => {
            DiagnosticInterfaceValidationFailure::DigestUnavailable {
                context: diagnostic_context(*context),
                field: diagnostic_field(*field),
            }
        }
        InterfaceValidationError::AllocationUnavailable {
            context,
            field,
            requested,
        } => DiagnosticInterfaceValidationFailure::AllocationUnavailable {
            context: diagnostic_context(*context),
            field: diagnostic_field(*field),
            requested: *requested,
        },
        InterfaceValidationError::SurfaceBuild { cause } => {
            DiagnosticInterfaceValidationFailure::SurfaceBuild {
                cause: Box::new(crate::surface::diagnostic_surface_problem(cause)),
            }
        }
        InterfaceValidationError::ArtifactHashMismatch { expected, actual } => {
            DiagnosticInterfaceValidationFailure::ArtifactHashMismatch {
                expected: diagnostic_digest(*expected.as_bytes()),
                actual: diagnostic_digest(*actual.as_bytes()),
            }
        }
        InterfaceValidationError::ContentHashMismatch { expected, actual } => {
            DiagnosticInterfaceValidationFailure::ContentHashMismatch {
                expected: diagnostic_digest(*expected.as_bytes()),
                actual: diagnostic_digest(*actual.as_bytes()),
            }
        }
        InterfaceValidationError::SectionChecksumMismatch {
            section,
            expected,
            actual,
        } => DiagnosticInterfaceValidationFailure::SectionChecksumMismatch {
            section: diagnostic_section(*section),
            expected: diagnostic_digest(*expected.as_bytes()),
            actual: diagnostic_digest(*actual.as_bytes()),
        },
        InterfaceValidationError::UnknownSectionChecksumMismatch {
            raw_tag,
            expected,
            actual,
        } => DiagnosticInterfaceValidationFailure::UnknownSectionChecksumMismatch {
            raw_tag: *raw_tag,
            expected: diagnostic_digest(*expected.as_bytes()),
            actual: diagnostic_digest(*actual.as_bytes()),
        },
        InterfaceValidationError::SectionContentHashMismatch {
            section,
            expected,
            actual,
        } => DiagnosticInterfaceValidationFailure::SectionContentHashMismatch {
            section: diagnostic_section(*section),
            expected: diagnostic_digest(*expected),
            actual: diagnostic_digest(*actual),
        },
        InterfaceValidationError::PayloadChecksumMismatch {
            context,
            expected,
            actual,
        } => DiagnosticInterfaceValidationFailure::PayloadChecksumMismatch {
            context: diagnostic_context(*context),
            expected: diagnostic_digest(*expected),
            actual: diagnostic_digest(*actual),
        },
        InterfaceValidationError::PayloadContentHashMismatch {
            context,
            expected,
            actual,
        } => DiagnosticInterfaceValidationFailure::PayloadContentHashMismatch {
            context: diagnostic_context(*context),
            expected: diagnostic_digest(*expected),
            actual: diagnostic_digest(*actual),
        },
        InterfaceValidationError::SpecializationKeyMismatch { expected, actual } => {
            DiagnosticInterfaceValidationFailure::SpecializationKeyMismatch {
                expected: *expected,
                actual: *actual,
            }
        }
        InterfaceValidationError::ImplementationConfigurationMismatch { expected, actual } => {
            DiagnosticInterfaceValidationFailure::ImplementationConfigurationMismatch {
                expected: *expected,
                actual: *actual,
            }
        }
        InterfaceValidationError::ImplementationInterfaceIdentityMismatch { expected, actual } => {
            DiagnosticInterfaceValidationFailure::ImplementationInterfaceIdentityMismatch {
                expected: Box::new(diagnostic_interface_identity(&expected)),
                actual: Box::new(diagnostic_interface_identity(&actual)),
            }
        }
        InterfaceValidationError::ImplementationDependencyMismatch {
            index,
            expected,
            actual,
        } => DiagnosticInterfaceValidationFailure::ImplementationDependencyMismatch {
            index: *index,
            expected: expected.as_deref().map(diagnostic_dependency).map(Box::new),
            actual: actual.as_deref().map(diagnostic_dependency).map(Box::new),
        },
        InterfaceValidationError::ResourceLimitExceeded {
            limit,
            actual,
            maximum,
        } => DiagnosticInterfaceValidationFailure::ResourceLimitExceeded {
            limit: diagnostic_limit(*limit),
            actual: *actual,
            maximum: *maximum,
        },
    }
}

const fn diagnostic_context(
    context: InterfaceValidationContext,
) -> DiagnosticInterfaceValidationContext {
    match context {
        InterfaceValidationContext::Artifact => DiagnosticInterfaceValidationContext::Artifact,
        InterfaceValidationContext::Header => DiagnosticInterfaceValidationContext::Header,
        InterfaceValidationContext::Directory => DiagnosticInterfaceValidationContext::Directory,
        InterfaceValidationContext::DirectoryEntry { index, raw_tag } => {
            DiagnosticInterfaceValidationContext::DirectoryEntry { index, raw_tag }
        }
        InterfaceValidationContext::ImplementationEntry { index, raw_kind } => {
            DiagnosticInterfaceValidationContext::ImplementationEntry { index, raw_kind }
        }
        InterfaceValidationContext::Section(section) => {
            DiagnosticInterfaceValidationContext::Section(diagnostic_section(section))
        }
        InterfaceValidationContext::Record { section, index } => {
            DiagnosticInterfaceValidationContext::Record {
                section: diagnostic_section(section),
                index,
            }
        }
        InterfaceValidationContext::SemanticRecord { kind, index } => {
            DiagnosticInterfaceValidationContext::SemanticRecord {
                kind: diagnostic_semantic_record_kind(kind),
                index,
            }
        }
        InterfaceValidationContext::ExternalSymbolKey { component } => {
            DiagnosticInterfaceValidationContext::ExternalSymbolKey { component }
        }
    }
}

const fn diagnostic_field(field: InterfaceValidationField) -> DiagnosticInterfaceValidationField {
    use DiagnosticInterfaceValidationField as Diagnostic;
    use InterfaceValidationField as Field;

    match field {
        Field::Magic => Diagnostic::Magic,
        Field::FormatRevision => Diagnostic::FormatRevision,
        Field::LanguageRevision => Diagnostic::LanguageRevision,
        Field::ByteOrderMarker => Diagnostic::ByteOrderMarker,
        Field::RequiredFlags => Diagnostic::RequiredFlags,
        Field::DeclaredFileLength => Diagnostic::DeclaredFileLength,
        Field::DirectoryOffset => Diagnostic::DirectoryOffset,
        Field::DirectoryLength => Diagnostic::DirectoryLength,
        Field::ContentHash => Diagnostic::ContentHash,
        Field::ArtifactHash => Diagnostic::ArtifactHash,
        Field::SectionTag => Diagnostic::SectionTag,
        Field::SectionRevision => Diagnostic::SectionRevision,
        Field::SectionCompatibility => Diagnostic::SectionCompatibility,
        Field::SectionEncoding => Diagnostic::SectionEncoding,
        Field::SectionOffset => Diagnostic::SectionOffset,
        Field::EncodedLength => Diagnostic::EncodedLength,
        Field::DecodedLength => Diagnostic::DecodedLength,
        Field::RecordCount => Diagnostic::RecordCount,
        Field::RecordOffset => Diagnostic::RecordOffset,
        Field::RecordLength => Diagnostic::RecordLength,
        Field::RecordPayload => Diagnostic::RecordPayload,
        Field::Discriminant => Diagnostic::Discriminant,
        Field::String => Diagnostic::String,
        Field::StringIndex => Diagnostic::StringIndex,
        Field::PackageName => Diagnostic::PackageName,
        Field::PackageVersion => Diagnostic::PackageVersion,
        Field::ProductName => Diagnostic::ProductName,
        Field::SymbolName => Diagnostic::SymbolName,
        Field::SymbolKind => Diagnostic::SymbolKind,
        Field::Container => Diagnostic::Container,
        Field::Owner => Diagnostic::Owner,
        Field::Subject => Diagnostic::Subject,
        Field::Role => Diagnostic::Role,
        Field::Ordinal => Diagnostic::Ordinal,
        Field::Dependency => Diagnostic::Dependency,
        Field::Reference => Diagnostic::Reference,
        Field::Index => Diagnostic::Index,
        Field::Declaration => Diagnostic::Declaration,
        Field::Parameter => Diagnostic::Parameter,
        Field::Argument => Diagnostic::Argument,
        Field::Type => Diagnostic::Type,
        Field::Constant => Diagnostic::Constant,
        Field::Template => Diagnostic::Template,
        Field::Support => Diagnostic::Support,
        Field::SchemaRevision => Diagnostic::SchemaRevision,
        Field::Identity => Diagnostic::Identity,
        Field::Hash => Diagnostic::Hash,
        Field::RuntimeRequirements => Diagnostic::RuntimeRequirements,
        Field::Configuration => Diagnostic::Configuration,
        Field::TargetProperty => Diagnostic::TargetProperty,
        Field::EntryOffset => Diagnostic::EntryOffset,
        Field::EntryLength => Diagnostic::EntryLength,
        Field::EntryEncoding => Diagnostic::EntryEncoding,
        Field::EntryKind => Diagnostic::EntryKind,
        Field::Ordering => Diagnostic::Ordering,
        Field::Value => Diagnostic::Value,
    }
}

const fn diagnostic_malformed_cause(
    cause: InterfaceMalformedCause,
) -> DiagnosticInterfaceMalformedCause {
    match cause {
        InterfaceMalformedCause::Missing { field } => DiagnosticInterfaceMalformedCause::Missing {
            field: diagnostic_field(field),
        },
        InterfaceMalformedCause::InvalidValue { field } => {
            DiagnosticInterfaceMalformedCause::InvalidValue {
                field: diagnostic_field(field),
            }
        }
        InterfaceMalformedCause::InvalidDiscriminant { field, actual } => {
            DiagnosticInterfaceMalformedCause::InvalidDiscriminant {
                field: diagnostic_field(field),
                actual,
            }
        }
        InterfaceMalformedCause::ValueMismatch {
            field,
            expected,
            actual,
        } => DiagnosticInterfaceMalformedCause::ValueMismatch {
            field: diagnostic_field(field),
            expected,
            actual,
        },
        InterfaceMalformedCause::CountMismatch {
            field,
            expected,
            actual,
        } => DiagnosticInterfaceMalformedCause::CountMismatch {
            field: diagnostic_field(field),
            expected,
            actual,
        },
        InterfaceMalformedCause::InvalidReference {
            field,
            index,
            available,
        } => DiagnosticInterfaceMalformedCause::InvalidReference {
            field: diagnostic_field(field),
            index,
            available,
        },
        InterfaceMalformedCause::OrderingViolation {
            field,
            previous,
            actual,
        } => DiagnosticInterfaceMalformedCause::OrderingViolation {
            field: diagnostic_field(field),
            previous,
            actual,
        },
        InterfaceMalformedCause::Duplicate { field, index } => {
            DiagnosticInterfaceMalformedCause::Duplicate {
                field: diagnostic_field(field),
                index,
            }
        }
        InterfaceMalformedCause::Cycle { field, index } => {
            DiagnosticInterfaceMalformedCause::Cycle {
                field: diagnostic_field(field),
                index,
            }
        }
        InterfaceMalformedCause::NumericOverflow {
            field,
            value,
            target,
        } => DiagnosticInterfaceMalformedCause::NumericOverflow {
            field: diagnostic_field(field),
            value,
            target: diagnostic_integer_target(target),
        },
        InterfaceMalformedCause::InvalidAlignment {
            field,
            value,
            alignment,
        } => DiagnosticInterfaceMalformedCause::InvalidAlignment {
            field: diagnostic_field(field),
            value,
            alignment,
        },
        InterfaceMalformedCause::RangeOverflow { offset, length } => {
            DiagnosticInterfaceMalformedCause::RangeOverflow { offset, length }
        }
        InterfaceMalformedCause::RangeOverlap {
            offset,
            length,
            conflicting_offset,
            conflicting_length,
        } => DiagnosticInterfaceMalformedCause::RangeOverlap {
            offset,
            length,
            conflicting_offset,
            conflicting_length,
        },
        InterfaceMalformedCause::LengthMismatch {
            field,
            expected,
            actual,
        } => DiagnosticInterfaceMalformedCause::LengthMismatch {
            field: diagnostic_field(field),
            expected,
            actual,
        },
    }
}

const fn diagnostic_integer_target(
    target: InterfaceIntegerTarget,
) -> DiagnosticInterfaceIntegerTarget {
    match target {
        InterfaceIntegerTarget::U16 => DiagnosticInterfaceIntegerTarget::U16,
        InterfaceIntegerTarget::U32 => DiagnosticInterfaceIntegerTarget::U32,
        InterfaceIntegerTarget::U64 => DiagnosticInterfaceIntegerTarget::U64,
        InterfaceIntegerTarget::Usize => DiagnosticInterfaceIntegerTarget::Usize,
    }
}

const fn diagnostic_utf8_failure(cause: InterfaceUtf8Failure) -> DiagnosticInterfaceUtf8Failure {
    match cause {
        InterfaceUtf8Failure::InvalidSequence { error_length } => {
            DiagnosticInterfaceUtf8Failure::InvalidSequence { error_length }
        }
        InterfaceUtf8Failure::IncompleteSequence => {
            DiagnosticInterfaceUtf8Failure::IncompleteSequence
        }
    }
}

const fn diagnostic_compression_failure(
    cause: InterfaceCompressionFailure,
) -> DiagnosticInterfaceCompressionFailure {
    match cause {
        InterfaceCompressionFailure::EncoderInitialization => {
            DiagnosticInterfaceCompressionFailure::EncoderInitialization
        }
        InterfaceCompressionFailure::EncoderConfiguration => {
            DiagnosticInterfaceCompressionFailure::EncoderConfiguration
        }
        InterfaceCompressionFailure::Encoding => DiagnosticInterfaceCompressionFailure::Encoding,
        InterfaceCompressionFailure::FrameLength => {
            DiagnosticInterfaceCompressionFailure::FrameLength
        }
        InterfaceCompressionFailure::FrameLengthMismatch { expected, actual } => {
            DiagnosticInterfaceCompressionFailure::FrameLengthMismatch { expected, actual }
        }
        InterfaceCompressionFailure::ContentSize => {
            DiagnosticInterfaceCompressionFailure::ContentSize
        }
        InterfaceCompressionFailure::ContentSizeMismatch { expected, actual } => {
            DiagnosticInterfaceCompressionFailure::ContentSizeMismatch { expected, actual }
        }
        InterfaceCompressionFailure::MissingHeaderByte { offset } => {
            DiagnosticInterfaceCompressionFailure::MissingHeaderByte { offset }
        }
        InterfaceCompressionFailure::InvalidHeaderFlags { descriptor } => {
            DiagnosticInterfaceCompressionFailure::InvalidHeaderFlags { descriptor }
        }
        InterfaceCompressionFailure::WindowSizeOverflow { descriptor } => {
            DiagnosticInterfaceCompressionFailure::WindowSizeOverflow { descriptor }
        }
        InterfaceCompressionFailure::WindowSizeExceeded { actual, maximum } => {
            DiagnosticInterfaceCompressionFailure::WindowSizeExceeded { actual, maximum }
        }
        InterfaceCompressionFailure::DecoderInitialization => {
            DiagnosticInterfaceCompressionFailure::DecoderInitialization
        }
        InterfaceCompressionFailure::DecoderConfiguration => {
            DiagnosticInterfaceCompressionFailure::DecoderConfiguration
        }
        InterfaceCompressionFailure::Decoding => DiagnosticInterfaceCompressionFailure::Decoding,
        InterfaceCompressionFailure::DecodedLengthMismatch { expected, actual } => {
            DiagnosticInterfaceCompressionFailure::DecodedLengthMismatch { expected, actual }
        }
    }
}

const fn diagnostic_limit(limit: InterfaceLimit) -> DiagnosticInterfaceLimit {
    match limit {
        InterfaceLimit::FileSize => DiagnosticInterfaceLimit::FileSize,
        InterfaceLimit::SectionCount => DiagnosticInterfaceLimit::SectionCount,
        InterfaceLimit::ImplementationEntryCount => {
            DiagnosticInterfaceLimit::ImplementationEntryCount
        }
        InterfaceLimit::RecordCount => DiagnosticInterfaceLimit::RecordCount,
        InterfaceLimit::StringLength => DiagnosticInterfaceLimit::StringLength,
        InterfaceLimit::BlobLength => DiagnosticInterfaceLimit::BlobLength,
        InterfaceLimit::DecodedAllocation => DiagnosticInterfaceLimit::DecodedAllocation,
        InterfaceLimit::SemanticTypeDepth => DiagnosticInterfaceLimit::SemanticTypeDepth,
        InterfaceLimit::TemplateGraphSize => DiagnosticInterfaceLimit::TemplateGraphSize,
        InterfaceLimit::ExternalReferenceCount => DiagnosticInterfaceLimit::ExternalReferenceCount,
    }
}

const fn diagnostic_section(section: InterfaceSectionTag) -> DiagnosticInterfaceSection {
    match section {
        InterfaceSectionTag::Strings => DiagnosticInterfaceSection::Strings,
        InterfaceSectionTag::PackageMetadata => DiagnosticInterfaceSection::PackageMetadata,
        InterfaceSectionTag::Dependencies => DiagnosticInterfaceSection::Dependencies,
        InterfaceSectionTag::SymbolIdentities => DiagnosticInterfaceSection::SymbolIdentities,
        InterfaceSectionTag::Relationships => DiagnosticInterfaceSection::Relationships,
        InterfaceSectionTag::ExportedLookup => DiagnosticInterfaceSection::ExportedLookup,
        InterfaceSectionTag::SemanticRecordDirectory => DiagnosticInterfaceSection::SymbolDirectory,
        InterfaceSectionTag::SemanticTypes => DiagnosticInterfaceSection::SemanticTypes,
        InterfaceSectionTag::Constants => DiagnosticInterfaceSection::Constants,
        InterfaceSectionTag::Contracts => DiagnosticInterfaceSection::Contracts,
        InterfaceSectionTag::DeclarationSemantics => DiagnosticInterfaceSection::Declarations,
        InterfaceSectionTag::DeclarationTemplates => {
            DiagnosticInterfaceSection::DeclarationTemplates
        }
        InterfaceSectionTag::Implementations => DiagnosticInterfaceSection::Implementations,
        InterfaceSectionTag::TargetDependencies => DiagnosticInterfaceSection::TargetDependencies,
        InterfaceSectionTag::SourceProvenance => DiagnosticInterfaceSection::SourceProvenance,
        InterfaceSectionTag::SupportGraph => DiagnosticInterfaceSection::SupportGraph,
    }
}

const fn diagnostic_semantic_record_kind(
    kind: InterfaceSemanticRecordKind,
) -> DiagnosticInterfaceSemanticRecordKind {
    match kind {
        InterfaceSemanticRecordKind::CallableSignature => {
            DiagnosticInterfaceSemanticRecordKind::CallableSignature
        }
        InterfaceSemanticRecordKind::GenericDeclaration => {
            DiagnosticInterfaceSemanticRecordKind::GenericDeclaration
        }
        InterfaceSemanticRecordKind::CallableParameterDefault => {
            DiagnosticInterfaceSemanticRecordKind::CallableParameterDefault
        }
        InterfaceSemanticRecordKind::PredicateDefinition => {
            DiagnosticInterfaceSemanticRecordKind::PredicateDefinition
        }
        InterfaceSemanticRecordKind::DeclaredType => {
            DiagnosticInterfaceSemanticRecordKind::DeclaredType
        }
        InterfaceSemanticRecordKind::TypeRepresentation => {
            DiagnosticInterfaceSemanticRecordKind::TypeRepresentation
        }
        InterfaceSemanticRecordKind::GenericConstraint => {
            DiagnosticInterfaceSemanticRecordKind::GenericConstraint
        }
        InterfaceSemanticRecordKind::CallableContracts => {
            DiagnosticInterfaceSemanticRecordKind::CallableContracts
        }
        InterfaceSemanticRecordKind::DeclarationTemplate => {
            DiagnosticInterfaceSemanticRecordKind::DeclarationTemplate
        }
        InterfaceSemanticRecordKind::Implementation => {
            DiagnosticInterfaceSemanticRecordKind::Implementation
        }
        InterfaceSemanticRecordKind::TargetProperty => {
            DiagnosticInterfaceSemanticRecordKind::TargetProperty
        }
        InterfaceSemanticRecordKind::Abi => DiagnosticInterfaceSemanticRecordKind::Abi,
        InterfaceSemanticRecordKind::Runtime => DiagnosticInterfaceSemanticRecordKind::Runtime,
    }
}

const fn diagnostic_digest(bytes: [u8; 32]) -> DiagnosticArtifactDigest {
    DiagnosticArtifactDigest::new(DiagnosticArtifactDigestAlgorithm::Blake3, bytes)
}

fn diagnostic_interface_identity(
    identity: &PackageInterfaceIdentity,
) -> DiagnosticPackageInterfaceIdentity {
    DiagnosticPackageInterfaceIdentity::new(
        identity.package().as_str(),
        identity.version().to_string(),
        identity.product().as_str(),
        diagnostic_product_kind(identity.kind()),
        identity.public_surface(),
    )
}

fn diagnostic_dependency(dependency: &InterfaceDependency) -> DiagnosticInterfaceDependency {
    DiagnosticInterfaceDependency::new(
        dependency.package().as_str(),
        dependency.product().as_str(),
        *dependency.content_hash().as_bytes(),
    )
}

const fn diagnostic_product_kind(kind: InterfaceProductKind) -> DiagnosticInterfaceProductKind {
    match kind {
        InterfaceProductKind::Library => DiagnosticInterfaceProductKind::Library,
        InterfaceProductKind::Executable => DiagnosticInterfaceProductKind::Executable,
        InterfaceProductKind::Test => DiagnosticInterfaceProductKind::Test,
    }
}
