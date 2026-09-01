use super::problem::format_english_interface_symbol_graph_problem;
use super::{format_english_interface_limit, format_english_interface_section};
use crate::catalog::english::argument::source::format_english_artifact_digest;

pub(crate) fn format_english_interface_validation_failure(
    failure: &bray_diagnostics::DiagnosticInterfaceValidationFailure,
) -> String {
    use bray_diagnostics::DiagnosticInterfaceValidationFailure as Failure;

    match failure {
        Failure::InvalidMagic { actual } => format!(
            "package interface has file identity 0x{}, which is not a Bray package interface",
            hex_bytes(actual)
        ),
        Failure::UnsupportedFormatRevision { expected, actual } => format!(
            "package interface uses format revision {actual}, but this compiler requires {expected}"
        ),
        Failure::UnsupportedLanguageRevision { expected, actual } => format!(
            "package interface uses language revision {actual}, but this compiler requires {expected}"
        ),
        Failure::UnsupportedByteOrder { expected, actual } => format!(
            "package interface has byte-order marker 0x{actual:08X}, but 0x{expected:08X} is required"
        ),
        Failure::UnsupportedRequiredFlags { actual } => {
            format!("package interface requires unsupported compatibility flags 0x{actual:016X}")
        }
        Failure::Truncated {
            context,
            field,
            offset,
            expected_length,
            actual_length,
        } => format!(
            "{} ends while reading {} at byte {offset}: {expected_length} bytes are required, but {actual_length} remain",
            format_context(context),
            format_field(*field),
        ),
        Failure::TrailingBytes {
            context,
            offset,
            count,
        } => format!(
            "{} has {count} trailing bytes starting at byte {offset}",
            format_context(context)
        ),
        Failure::Malformed { context, cause } => format_malformed(context, *cause),
        Failure::InvalidUtf8 {
            context,
            field,
            offset,
            length,
            cause,
        } => format!(
            "{} has invalid UTF-8 in {} at byte {offset} within a {length}-byte value: {}",
            format_context(context),
            format_field(*field),
            format_utf8_failure(*cause),
        ),
        Failure::Compression { context, cause } => format!(
            "{} has invalid compressed data: {}",
            format_context(context),
            format_compression_failure(*cause),
        ),
        Failure::DigestUnavailable { context, field } => format!(
            "cannot compute {} for {} because its retained bytes are structurally invalid",
            format_field(*field),
            format_context(context),
        ),
        Failure::AllocationUnavailable {
            context,
            field,
            requested,
        } => format!(
            "cannot reserve {requested} bytes for {} in {}",
            format_field(*field),
            format_context(context),
        ),
        Failure::SurfaceBuild { cause } => format!(
            "package-interface declaration surface is invalid: {}",
            format_english_interface_symbol_graph_problem(cause)
        ),
        Failure::ArtifactHashMismatch { expected, actual } => {
            format_digest_mismatch("artifact hash", expected, actual)
        }
        Failure::ContentHashMismatch { expected, actual } => {
            format_digest_mismatch("semantic content hash", expected, actual)
        }
        Failure::PayloadChecksumMismatch {
            context,
            expected,
            actual,
        } => format!(
            "{} declares checksum {}, but its encoded bytes have checksum {}",
            format_context(context),
            format_english_artifact_digest(expected),
            format_english_artifact_digest(actual),
        ),
        Failure::PayloadContentHashMismatch {
            context,
            expected,
            actual,
        } => format!(
            "{} declares content hash {}, but its decoded content has hash {}",
            format_context(context),
            format_english_artifact_digest(expected),
            format_english_artifact_digest(actual),
        ),
        Failure::SpecializationKeyMismatch { expected, actual } => format!(
            "implementation payload declares specialization key 0x{}, but its content has key 0x{}",
            hex_bytes(expected),
            hex_bytes(actual),
        ),
        Failure::ImplementationConfigurationMismatch { expected, actual } => format!(
            "implementation artifact declares configuration 0x{}, but configuration 0x{} is required",
            hex_bytes(actual),
            hex_bytes(expected),
        ),
        Failure::ImplementationInterfaceIdentityMismatch { expected, actual } => format!(
            "implementation artifact targets {}, but {} is required",
            format_package_interface_identity(actual),
            format_package_interface_identity(expected),
        ),
        Failure::ImplementationDependencyMismatch {
            index,
            expected,
            actual,
        } => format!(
            "implementation dependency {index} is {}, but {} is required",
            format_interface_dependency(actual.as_deref()),
            format_interface_dependency(expected.as_deref()),
        ),
        Failure::SectionChecksumMismatch {
            section,
            expected,
            actual,
        } => format!(
            "package-interface {} section declares checksum {}, but its bytes have checksum {}",
            format_english_interface_section(*section),
            format_english_artifact_digest(expected),
            format_english_artifact_digest(actual),
        ),
        Failure::UnknownSectionChecksumMismatch {
            raw_tag,
            expected,
            actual,
        } => format!(
            "package-interface section tag {raw_tag} declares checksum {}, but its bytes have checksum {}",
            format_english_artifact_digest(expected),
            format_english_artifact_digest(actual),
        ),
        Failure::SectionContentHashMismatch {
            section,
            expected,
            actual,
        } => format!(
            "decoded package-interface {} section declares content hash {}, but its content has hash {}",
            format_english_interface_section(*section),
            format_english_artifact_digest(expected),
            format_english_artifact_digest(actual),
        ),
        Failure::ResourceLimitExceeded {
            limit,
            actual,
            maximum,
        } => format!(
            "package interface requires {} of {actual}, but the configured maximum is {maximum}",
            format_english_interface_limit(*limit)
        ),
    }
}

fn format_malformed(
    context: &bray_diagnostics::DiagnosticInterfaceValidationContext,
    cause: bray_diagnostics::DiagnosticInterfaceMalformedCause,
) -> String {
    use bray_diagnostics::DiagnosticInterfaceMalformedCause as Cause;

    let context = format_context(context);

    match cause {
        Cause::Missing { field } => {
            format!("{context} is missing required {}", format_field(field))
        }
        Cause::InvalidValue { field } => {
            format!("{context} has invalid {}", format_field(field))
        }
        Cause::InvalidDiscriminant { field, actual } => format!(
            "{context} has unknown {} value {actual}",
            format_field(field)
        ),
        Cause::CountMismatch {
            field,
            expected,
            actual,
        } => format!(
            "{context} declares {} of {expected}, but contains {actual}",
            format_field(field)
        ),
        Cause::InvalidReference {
            field,
            index,
            available,
        } => format!(
            "{context} has {} reference {index}, but only {available} entries are available",
            format_field(field)
        ),
        Cause::OrderingViolation {
            field,
            previous,
            actual,
        } => format!(
            "{context} has out-of-order {} values {previous} then {actual}",
            format_field(field)
        ),
        Cause::Duplicate { field, index } => {
            format!("{context} repeats {} value {index}", format_field(field))
        }
        Cause::Cycle { field, index } => format!(
            "{context} has a cycle in {} at record {index}",
            format_field(field)
        ),
        Cause::NumericOverflow {
            field,
            value,
            target,
        } => format!(
            "{context} has {} value {value}, which does not fit {}",
            format_field(field),
            target.as_str()
        ),
        Cause::RangeOverflow { offset, length } => {
            format!("{context} has byte range {offset} + {length}, which overflows")
        }
        Cause::RangeOverlap {
            offset,
            length,
            conflicting_offset,
            conflicting_length,
        } => format!(
            "{context} has overlapping byte ranges {offset} + {length} and {conflicting_offset} + {conflicting_length}"
        ),
        Cause::ValueMismatch {
            field,
            expected,
            actual,
        } => format!(
            "{context} has {} value {actual}, but {expected} is required",
            format_field(field)
        ),
        Cause::InvalidAlignment {
            field,
            value,
            alignment,
        } => format!(
            "{context} has {} value {value}, which is not aligned to {alignment} bytes",
            format_field(field)
        ),
        Cause::LengthMismatch {
            field,
            expected,
            actual,
        } => format!(
            "{context} declares {} length {expected}, but contains {actual} bytes",
            format_field(field)
        ),
    }
}

fn format_compression_failure(
    failure: bray_diagnostics::DiagnosticInterfaceCompressionFailure,
) -> String {
    use bray_diagnostics::DiagnosticInterfaceCompressionFailure as Failure;

    match failure {
        Failure::EncoderInitialization => "compressor initialization failed".to_owned(),
        Failure::EncoderConfiguration => "compressor configuration failed".to_owned(),
        Failure::Encoding => "compression failed".to_owned(),
        Failure::FrameLength => "compressed frame length cannot be read".to_owned(),
        Failure::FrameLengthMismatch { expected, actual } => {
            format!("compressed frame length is {actual} bytes, but {expected} bytes were declared")
        }
        Failure::ContentSize => "decoded content size cannot be read".to_owned(),
        Failure::ContentSizeMismatch { expected, actual } => match actual {
            Some(actual) => format!(
                "decoded content size is {actual} bytes, but {expected} bytes were declared"
            ),
            None => format!("decoded content size is absent, but {expected} bytes were declared"),
        },
        Failure::MissingHeaderByte { offset } => {
            format!("compressed frame header is missing byte {offset}")
        }
        Failure::InvalidHeaderFlags { descriptor } => {
            format!("compressed frame header has invalid flags 0x{descriptor:02X}")
        }
        Failure::WindowSizeOverflow { descriptor } => {
            format!("compressed frame window overflows for descriptor 0x{descriptor:02X}")
        }
        Failure::WindowSizeExceeded { actual, maximum } => format!(
            "compressed frame window is {actual} bytes, but the configured maximum is {maximum}"
        ),
        Failure::DecoderInitialization => "decompressor initialization failed".to_owned(),
        Failure::DecoderConfiguration => "decompressor configuration failed".to_owned(),
        Failure::Decoding => "decompression failed".to_owned(),
        Failure::DecodedLengthMismatch { expected, actual } => {
            format!("decoded length is {actual} bytes, but {expected} bytes were declared")
        }
    }
}

fn format_utf8_failure(failure: bray_diagnostics::DiagnosticInterfaceUtf8Failure) -> String {
    use bray_diagnostics::DiagnosticInterfaceUtf8Failure as Failure;

    match failure {
        Failure::InvalidSequence {
            error_length: Some(length),
        } => format!("invalid byte sequence has length {length}"),
        Failure::InvalidSequence { error_length: None } => {
            "invalid byte sequence length is unavailable".to_owned()
        }
        Failure::IncompleteSequence => "input ends within a character".to_owned(),
    }
}

fn format_context(context: &bray_diagnostics::DiagnosticInterfaceValidationContext) -> String {
    use bray_diagnostics::DiagnosticInterfaceValidationContext as Context;

    match context {
        Context::Artifact => "package interface".to_owned(),
        Context::Header => "package-interface header".to_owned(),
        Context::Directory => "package-interface section directory".to_owned(),
        Context::DirectoryEntry { index, raw_tag } => {
            format!("package-interface directory entry {index} with section tag {raw_tag}")
        }
        Context::ImplementationEntry { index, raw_kind } => {
            format!("package-interface implementation entry {index} with category {raw_kind}")
        }
        Context::Section(section) => format!(
            "package-interface {} section",
            format_english_interface_section(*section)
        ),
        Context::Record { section, index } => format!(
            "record {index} in package-interface {} section",
            format_english_interface_section(*section)
        ),
        Context::SemanticRecord { kind, index } => format!(
            "package-interface {} semantic record {index}",
            format_key(kind.as_str())
        ),
        Context::ExternalSymbolKey { component } => {
            format!("package-interface external symbol key component {component}")
        }
    }
}

fn format_field(field: bray_diagnostics::DiagnosticInterfaceValidationField) -> String {
    format_key(field.as_str())
}

fn format_key(key: &str) -> String {
    key.replace('_', " ")
}

fn format_digest_mismatch(
    name: &str,
    expected: &bray_diagnostics::DiagnosticArtifactDigest,
    actual: &bray_diagnostics::DiagnosticArtifactDigest,
) -> String {
    format!(
        "package interface declares {name} {}, but retained content has {}",
        format_english_artifact_digest(expected),
        format_english_artifact_digest(actual),
    )
}

fn format_package_interface_identity(
    identity: &bray_diagnostics::DiagnosticPackageInterfaceIdentity,
) -> String {
    format!(
        "{} product '{}/{}' at version '{}' with public surface '{}'",
        identity.product_kind().as_str(),
        identity.package(),
        identity.product(),
        identity.version(),
        identity.public_surface(),
    )
}

fn format_interface_dependency(
    dependency: Option<&bray_diagnostics::DiagnosticInterfaceDependency>,
) -> String {
    let Some(dependency) = dependency else {
        return "absent".to_owned();
    };

    format!(
        "package product '{}/{}' with content hash 0x{}",
        dependency.package(),
        dependency.product(),
        hex_bytes(dependency.content()),
    )
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        use std::fmt::Write;

        let _ = write!(text, "{byte:02X}");
    }

    text
}
