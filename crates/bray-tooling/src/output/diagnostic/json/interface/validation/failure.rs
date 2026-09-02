use super::super::super::DiagnosticProblemJson;
use super::super::identity::{DiagnosticProblemFieldJson, DiagnosticProblemFieldValueJson};
use super::problem::{
    interface_symbol_graph_problem_json, problem, problem_count_u64, problem_text,
};

pub(in crate::output::diagnostic::json) fn interface_validation_failure_json(
    failure: &bray_diagnostics::DiagnosticInterfaceValidationFailure,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticInterfaceValidationFailure as Failure;

    match failure {
        Failure::InvalidMagic { actual } => problem(
            failure.as_str(),
            [problem_text("actual", hex_bytes(actual))],
        ),
        Failure::UnsupportedFormatRevision { expected, actual }
        | Failure::UnsupportedLanguageRevision { expected, actual } => problem(
            failure.as_str(),
            [
                problem_count_u64("expected", *expected),
                problem_count_u64("actual", *actual),
            ],
        ),
        Failure::UnsupportedByteOrder { expected, actual } => problem(
            failure.as_str(),
            [
                problem_count_u64("expected", u64::from(*expected)),
                problem_count_u64("actual", u64::from(*actual)),
            ],
        ),
        Failure::UnsupportedRequiredFlags { actual } => {
            problem(failure.as_str(), [problem_count_u64("actual", *actual)])
        }
        Failure::Truncated {
            context,
            field,
            offset,
            expected_length,
            actual_length,
        } => problem(
            failure.as_str(),
            validation_context_fields(context).into_iter().chain([
                problem_text("field", field.as_str()),
                problem_count_u64("offset", *offset),
                problem_count_u64("expected_length", *expected_length),
                problem_count_u64("actual_length", *actual_length),
            ]),
        ),
        Failure::TrailingBytes {
            context,
            offset,
            count,
        } => problem(
            failure.as_str(),
            validation_context_fields(context).into_iter().chain([
                problem_count_u64("offset", *offset),
                problem_count_u64("count", *count),
            ]),
        ),
        Failure::Malformed { context, cause } => problem(
            failure.as_str(),
            validation_context_fields(context)
                .into_iter()
                .chain(malformed_cause_fields(*cause)),
        ),
        Failure::InvalidUtf8 {
            context,
            field,
            offset,
            length,
            cause,
        } => problem(
            failure.as_str(),
            validation_context_fields(context)
                .into_iter()
                .chain([
                    problem_text("field", field.as_str()),
                    problem_count_u64("offset", *offset),
                    problem_count_u64("length", *length),
                    problem_text("cause", cause.as_str()),
                ])
                .chain(utf8_failure_fields(*cause)),
        ),
        Failure::Compression { context, cause } => problem(
            failure.as_str(),
            validation_context_fields(context)
                .into_iter()
                .chain(compression_failure_fields(*cause)),
        ),
        Failure::DigestUnavailable { context, field } => problem(
            failure.as_str(),
            validation_context_fields(context)
                .into_iter()
                .chain([problem_text("field", field.as_str())]),
        ),
        Failure::AllocationUnavailable {
            context,
            field,
            requested,
        } => problem(
            failure.as_str(),
            validation_context_fields(context).into_iter().chain([
                problem_text("field", field.as_str()),
                problem_count_u64("requested", *requested),
            ]),
        ),
        Failure::SurfaceBuild { cause } => problem(
            failure.as_str(),
            [DiagnosticProblemFieldJson {
                name: "cause",
                value: DiagnosticProblemFieldValueJson::Problem(Box::new(
                    interface_symbol_graph_problem_json(cause),
                )),
            }],
        ),
        Failure::ArtifactHashMismatch { expected, actual }
        | Failure::ContentHashMismatch { expected, actual } => {
            problem(failure.as_str(), digest_fields(expected, actual))
        }
        Failure::PayloadChecksumMismatch {
            context,
            expected,
            actual,
        }
        | Failure::PayloadContentHashMismatch {
            context,
            expected,
            actual,
        } => problem(
            failure.as_str(),
            validation_context_fields(context)
                .into_iter()
                .chain(digest_fields(expected, actual)),
        ),
        Failure::SpecializationKeyMismatch { expected, actual }
        | Failure::ImplementationConfigurationMismatch { expected, actual } => problem(
            failure.as_str(),
            [
                problem_text("expected", hex_bytes(expected)),
                problem_text("actual", hex_bytes(actual)),
            ],
        ),
        Failure::ImplementationInterfaceIdentityMismatch { expected, actual } => problem(
            failure.as_str(),
            [
                nested_problem("expected", package_interface_identity_json(expected)),
                nested_problem("actual", package_interface_identity_json(actual)),
            ],
        ),
        Failure::ImplementationDependencyMismatch {
            index,
            expected,
            actual,
        } => problem(
            failure.as_str(),
            [
                problem_count_u64("index", *index),
                nested_problem("expected", interface_dependency_json(expected.as_deref())),
                nested_problem("actual", interface_dependency_json(actual.as_deref())),
            ],
        ),
        Failure::SectionChecksumMismatch {
            section,
            expected,
            actual,
        }
        | Failure::SectionContentHashMismatch {
            section,
            expected,
            actual,
        } => problem(
            failure.as_str(),
            [problem_text("section", section.as_str())]
                .into_iter()
                .chain(digest_fields(expected, actual)),
        ),
        Failure::UnknownSectionChecksumMismatch {
            raw_tag,
            expected,
            actual,
        } => problem(
            failure.as_str(),
            [problem_count_u64("raw_tag", u64::from(*raw_tag))]
                .into_iter()
                .chain(digest_fields(expected, actual)),
        ),
        Failure::ResourceLimitExceeded {
            limit,
            actual,
            maximum,
        } => problem(
            failure.as_str(),
            [
                problem_text("limit", limit.as_str()),
                problem_count_u64("actual", *actual),
                problem_count_u64("maximum", *maximum),
            ],
        ),
    }
}

fn validation_context_fields(
    context: &bray_diagnostics::DiagnosticInterfaceValidationContext,
) -> Vec<DiagnosticProblemFieldJson> {
    use bray_diagnostics::DiagnosticInterfaceValidationContext as Context;

    let mut fields = vec![problem_text("context", context.as_str())];

    match context {
        Context::Artifact | Context::Header | Context::Directory => {}
        Context::DirectoryEntry { index, raw_tag } => {
            fields.push(problem_count_u64("directory_index", *index));
            fields.push(problem_count_u64("raw_tag", u64::from(*raw_tag)));
        }
        Context::ImplementationEntry { index, raw_kind } => {
            fields.push(problem_count_u64("implementation_index", *index));
            fields.push(problem_count_u64("raw_kind", u64::from(*raw_kind)));
        }
        Context::Section(section) => fields.push(problem_text("section", section.as_str())),
        Context::Record { section, index } => {
            fields.push(problem_text("section", section.as_str()));
            fields.push(problem_count_u64("record", *index));
        }
        Context::SemanticRecord { kind, index } => {
            fields.push(problem_text("record_kind", kind.as_str()));
            fields.push(problem_count_u64("record", *index));
        }
        Context::ExternalSymbolKey { component } => {
            fields.push(problem_count_u64("component", *component));
        }
    }

    fields
}

fn package_interface_identity_json(
    identity: &bray_diagnostics::DiagnosticPackageInterfaceIdentity,
) -> DiagnosticProblemJson {
    problem(
        "package_interface_identity",
        [
            problem_text("package", identity.package()),
            problem_text("version", identity.version()),
            problem_text("product", identity.product()),
            problem_text("product_kind", identity.product_kind().as_str()),
            problem_text("public_surface", identity.public_surface()),
        ],
    )
}

fn interface_dependency_json(
    dependency: Option<&bray_diagnostics::DiagnosticInterfaceDependency>,
) -> DiagnosticProblemJson {
    let Some(dependency) = dependency else {
        return problem("absent", []);
    };

    problem(
        "interface_dependency",
        [
            problem_text("package", dependency.package()),
            problem_text("product", dependency.product()),
            problem_text("content", hex_bytes(dependency.content())),
        ],
    )
}

fn nested_problem(name: &'static str, value: DiagnosticProblemJson) -> DiagnosticProblemFieldJson {
    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::Problem(Box::new(value)),
    }
}

fn malformed_cause_fields(
    cause: bray_diagnostics::DiagnosticInterfaceMalformedCause,
) -> Vec<DiagnosticProblemFieldJson> {
    use bray_diagnostics::DiagnosticInterfaceMalformedCause as Cause;

    let mut fields = vec![problem_text("cause", cause.as_str())];

    match cause {
        Cause::Missing { field } | Cause::InvalidValue { field } => {
            fields.push(problem_text("field", field.as_str()));
        }
        Cause::InvalidDiscriminant { field, actual } => {
            fields.push(problem_text("field", field.as_str()));
            fields.push(problem_count_u64("actual", actual));
        }
        Cause::CountMismatch {
            field,
            expected,
            actual,
        }
        | Cause::LengthMismatch {
            field,
            expected,
            actual,
        } => {
            fields.push(problem_text("field", field.as_str()));
            fields.push(problem_count_u64("expected", expected));
            fields.push(problem_count_u64("actual", actual));
        }
        Cause::InvalidReference {
            field,
            index,
            available,
        } => {
            fields.push(problem_text("field", field.as_str()));
            fields.push(problem_count_u64("index", index));
            fields.push(problem_count_u64("available", available));
        }
        Cause::OrderingViolation {
            field,
            previous,
            actual,
        } => {
            fields.push(problem_text("field", field.as_str()));
            fields.push(problem_count_u64("previous", previous));
            fields.push(problem_count_u64("actual", actual));
        }
        Cause::Duplicate { field, index } | Cause::Cycle { field, index } => {
            fields.push(problem_text("field", field.as_str()));
            fields.push(problem_count_u64("index", index));
        }
        Cause::NumericOverflow {
            field,
            value,
            target,
        } => {
            fields.push(problem_text("field", field.as_str()));
            fields.push(problem_count_u64("value", value));
            fields.push(problem_text("target", target.as_str()));
        }
        Cause::RangeOverflow { offset, length } => {
            fields.push(problem_count_u64("offset", offset));
            fields.push(problem_count_u64("length", length));
        }
        Cause::RangeOverlap {
            offset,
            length,
            conflicting_offset,
            conflicting_length,
        } => {
            fields.push(problem_count_u64("offset", offset));
            fields.push(problem_count_u64("length", length));
            fields.push(problem_count_u64("conflicting_offset", conflicting_offset));
            fields.push(problem_count_u64("conflicting_length", conflicting_length));
        }
        Cause::ValueMismatch {
            field,
            expected,
            actual,
        } => {
            fields.push(problem_text("field", field.as_str()));
            fields.push(problem_count_u64("expected", expected));
            fields.push(problem_count_u64("actual", actual));
        }
        Cause::InvalidAlignment {
            field,
            value,
            alignment,
        } => {
            fields.push(problem_text("field", field.as_str()));
            fields.push(problem_count_u64("value", value));
            fields.push(problem_count_u64("alignment", alignment));
        }
    }

    fields
}

fn compression_failure_fields(
    failure: bray_diagnostics::DiagnosticInterfaceCompressionFailure,
) -> Vec<DiagnosticProblemFieldJson> {
    use bray_diagnostics::DiagnosticInterfaceCompressionFailure as Failure;

    let mut fields = vec![problem_text("cause", failure.as_str())];

    match failure {
        Failure::FrameLengthMismatch { expected, actual }
        | Failure::DecodedLengthMismatch { expected, actual } => {
            fields.push(problem_count_u64("expected", expected));
            fields.push(problem_count_u64("actual", actual));
        }
        Failure::ContentSizeMismatch { expected, actual } => {
            fields.push(problem_count_u64("expected", expected));

            if let Some(actual) = actual {
                fields.push(problem_count_u64("actual", actual));
            }
        }
        Failure::MissingHeaderByte { offset } => {
            fields.push(problem_count_u64("offset", offset));
        }
        Failure::InvalidHeaderFlags { descriptor } | Failure::WindowSizeOverflow { descriptor } => {
            fields.push(problem_count_u64("descriptor", u64::from(descriptor)));
        }
        Failure::WindowSizeExceeded { actual, maximum } => {
            fields.push(problem_count_u64("actual", actual));
            fields.push(problem_count_u64("maximum", maximum));
        }
        Failure::EncoderInitialization
        | Failure::EncoderConfiguration
        | Failure::Encoding
        | Failure::FrameLength
        | Failure::ContentSize
        | Failure::DecoderInitialization
        | Failure::DecoderConfiguration
        | Failure::Decoding => {}
    }

    fields
}

fn utf8_failure_fields(
    failure: bray_diagnostics::DiagnosticInterfaceUtf8Failure,
) -> Vec<DiagnosticProblemFieldJson> {
    match failure {
        bray_diagnostics::DiagnosticInterfaceUtf8Failure::InvalidSequence {
            error_length: Some(length),
        } => vec![problem_count_u64("error_length", length)],
        bray_diagnostics::DiagnosticInterfaceUtf8Failure::InvalidSequence {
            error_length: None,
        }
        | bray_diagnostics::DiagnosticInterfaceUtf8Failure::IncompleteSequence => Vec::new(),
    }
}

fn digest_fields(
    expected: &bray_diagnostics::DiagnosticArtifactDigest,
    actual: &bray_diagnostics::DiagnosticArtifactDigest,
) -> [DiagnosticProblemFieldJson; 2] {
    [
        problem_text("expected", digest_text(expected)),
        problem_text("actual", digest_text(actual)),
    ]
}

fn digest_text(digest: &bray_diagnostics::DiagnosticArtifactDigest) -> String {
    format!(
        "{}:{}",
        digest.algorithm().as_str(),
        hex_bytes(digest.bytes())
    )
}

fn hex_bytes(bytes: &[u8]) -> String {
    let mut text = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        use std::fmt::Write;

        let _ = write!(text, "{byte:02x}");
    }

    text
}
