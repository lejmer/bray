use bray_diagnostics::DiagnosticType;

use super::super::{DiagnosticProblemJson, DiagnosticTypeJson};
use super::identity::{
    DiagnosticArrayLengthJson, DiagnosticInterfaceSymbolIdentityJson, DiagnosticProblemFieldJson,
    DiagnosticProblemFieldValueJson,
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

pub(in crate::output::diagnostic::json) fn problem(
    reason: &'static str,
    context: impl IntoIterator<Item = DiagnosticProblemFieldJson>,
) -> DiagnosticProblemJson {
    DiagnosticProblemJson {
        reason,
        context: context.into_iter().collect(),
    }
}

fn problem_count(name: &'static str, value: u32) -> DiagnosticProblemFieldJson {
    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::Count(u64::from(value)),
    }
}

pub(in crate::output::diagnostic::json) fn problem_count_u64(
    name: &'static str,
    value: u64,
) -> DiagnosticProblemFieldJson {
    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::Count(value),
    }
}

pub(in crate::output::diagnostic::json) fn problem_text(
    name: &'static str,
    value: impl Into<String>,
) -> DiagnosticProblemFieldJson {
    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::Text(value.into()),
    }
}

pub(in crate::output::diagnostic::json) fn problem_type(
    name: &'static str,
    value: &DiagnosticType,
) -> DiagnosticProblemFieldJson {
    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::Type(DiagnosticTypeJson::from_type(value)),
    }
}

pub(in crate::output::diagnostic::json) fn problem_types(
    name: &'static str,
    values: &[DiagnosticType],
) -> DiagnosticProblemFieldJson {
    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::Types(
            values.iter().map(DiagnosticTypeJson::from_type).collect(),
        ),
    }
}

pub(in crate::output::diagnostic::json) fn problem_array_length(
    name: &'static str,
    value: bray_diagnostics::DiagnosticArrayLength,
) -> DiagnosticProblemFieldJson {
    let value = match value {
        bray_diagnostics::DiagnosticArrayLength::Exact(value) => {
            DiagnosticArrayLengthJson::Exact(value)
        }
        bray_diagnostics::DiagnosticArrayLength::Symbolic => DiagnosticArrayLengthJson::Symbolic,
    };

    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::ArrayLength(value),
    }
}

fn problem_symbol_identity(
    name: &'static str,
    value: &bray_diagnostics::DiagnosticInterfaceSymbolIdentity,
) -> DiagnosticProblemFieldJson {
    DiagnosticProblemFieldJson {
        name,
        value: DiagnosticProblemFieldValueJson::SymbolIdentity(
            DiagnosticInterfaceSymbolIdentityJson::from_identity(value),
        ),
    }
}

pub(in crate::output::diagnostic::json) fn interface_symbol_graph_problem_json(
    value: &bray_diagnostics::DiagnosticInterfaceSymbolGraphProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticInterfaceSymbolGraphProblem as Problem;

    match value {
        Problem::DuplicateInterface(interface) => problem(
            "duplicate_interface",
            [problem_count("interface", *interface)],
        ),
        Problem::DuplicatePackage(package) => {
            problem("duplicate_package", [problem_text("package", package)])
        }
        Problem::SymbolCapacityExceeded { actual, maximum } => problem(
            "symbol_capacity_exceeded",
            [
                DiagnosticProblemFieldJson {
                    name: "actual",
                    value: DiagnosticProblemFieldValueJson::Count(*actual),
                },
                DiagnosticProblemFieldJson {
                    name: "maximum",
                    value: DiagnosticProblemFieldValueJson::Count(*maximum),
                },
            ],
        ),
        Problem::DuplicateExternalIdentity(identity) => problem(
            "duplicate_external_identity",
            [problem_symbol_identity("identity", identity)],
        ),
        Problem::RelationshipSymbolOutOfBounds { interface, symbol } => problem(
            "relationship_symbol_out_of_bounds",
            [
                problem_count("interface", *interface),
                problem_count("symbol", *symbol),
            ],
        ),
        Problem::InvalidRelationshipKinds {
            relationship,
            owner,
            member,
        } => problem(
            "invalid_relationship_kinds",
            [
                problem_text("relationship", relationship.as_str()),
                problem_text("owner_kind", owner.as_str()),
                problem_text("member_kind", member.as_str()),
            ],
        ),
        Problem::RelationshipContainmentMismatch { owner, member } => problem(
            "relationship_containment_mismatch",
            [
                problem_count("owner", *owner),
                problem_count("member", *member),
            ],
        ),
        Problem::NonCanonicalRelationshipOrdinal {
            relationship,
            owner,
            expected,
            actual,
        } => problem(
            "non_canonical_relationship_ordinal",
            [
                problem_text("relationship", relationship.as_str()),
                problem_count("owner", *owner),
                DiagnosticProblemFieldJson {
                    name: "expected",
                    value: DiagnosticProblemFieldValueJson::Count(*expected),
                },
                problem_count("actual", *actual),
            ],
        ),
        Problem::MissingContainment { symbol, kind } => problem(
            "missing_containment",
            [
                problem_count("symbol", *symbol),
                problem_text("symbol_kind", kind.as_str()),
            ],
        ),
        Problem::DuplicateContainment { symbol, kind } => problem(
            "duplicate_containment",
            [
                problem_count("symbol", *symbol),
                problem_text("symbol_kind", kind.as_str()),
            ],
        ),
        Problem::LookupOwnerOutOfBounds { interface, owner } => problem(
            "lookup_owner_out_of_bounds",
            [
                problem_count("interface", *interface),
                problem_count("owner", *owner),
            ],
        ),
        Problem::InvalidLookupOwner { owner, kind } => problem(
            "invalid_lookup_owner",
            [
                problem_count("owner", *owner),
                problem_text("symbol_kind", kind.as_str()),
            ],
        ),
        Problem::MissingLookupTarget(identity) => problem(
            "missing_lookup_target",
            [problem_symbol_identity("identity", identity)],
        ),
        Problem::DuplicateLookupName { owner, name } => problem(
            "duplicate_lookup_name",
            [problem_count("owner", *owner), problem_text("name", name)],
        ),
        Problem::UnsupportedSymbolKind(kind) => problem(
            "unsupported_symbol_kind",
            [problem_text("symbol_kind", kind.as_str())],
        ),
        Problem::InvalidRecordRelationships { symbol, kind } => problem(
            "invalid_record_relationships",
            [
                problem_count("symbol", *symbol),
                problem_text("symbol_kind", kind.as_str()),
            ],
        ),
        Problem::SurfaceNonLibraryProduct => problem("surface_non_library_product", []),
        Problem::SurfaceDependencyCountOverflow => problem("surface_dependency_count_overflow", []),
        Problem::SurfaceDuplicateDependencyPackage(package) => problem(
            "surface_duplicate_dependency_package",
            [problem_text("package", package)],
        ),
        Problem::SurfaceNonCanonicalSymbolOrder { previous, current } => problem(
            "surface_non_canonical_symbol_order",
            [
                problem_count("previous", *previous),
                problem_count("current", *current),
            ],
        ),
        Problem::SurfaceIdentity(cause) => {
            problem("surface_identity", identity_surface_problem_fields(*cause))
        }
        Problem::SurfaceRelationshipSymbolOutOfBounds(relationship) => problem(
            "surface_relationship_symbol_out_of_bounds",
            relationship_fields(*relationship),
        ),
        Problem::SurfaceInvalidRelationship(relationship) => problem(
            "surface_invalid_relationship",
            relationship_fields(*relationship),
        ),
        Problem::SurfaceDuplicateRelationshipPosition(relationship) => problem(
            "surface_duplicate_relationship_position",
            relationship_fields(*relationship),
        ),
        Problem::SurfaceExportOwnerOutOfBounds(owner) => problem(
            "surface_export_owner_out_of_bounds",
            [problem_count("owner", *owner)],
        ),
        Problem::SurfaceInvalidExportOwner(owner) => problem(
            "surface_invalid_export_owner",
            [problem_count("owner", *owner)],
        ),
        Problem::SurfaceExportTargetOutOfBounds(target) => problem(
            "surface_export_target_out_of_bounds",
            [problem_count("target", *target)],
        ),
        Problem::SurfaceDependencyOutOfBounds(dependency) => problem(
            "surface_dependency_out_of_bounds",
            [problem_count("dependency", *dependency)],
        ),
        Problem::SurfaceDependencyKeyPackageMismatch(dependency) => problem(
            "surface_dependency_key_package_mismatch",
            [problem_count("dependency", *dependency)],
        ),
        Problem::SurfaceInvalidDirectExportTarget(target) => problem(
            "surface_invalid_direct_export_target",
            [problem_count("target", *target)],
        ),
        Problem::SurfaceDuplicateExportName { owner, name } => problem(
            "surface_duplicate_export_name",
            [problem_count("owner", *owner), problem_text("name", name)],
        ),
    }
}

fn identity_surface_problem_fields(
    problem: bray_diagnostics::DiagnosticInterfaceIdentitySurfaceProblem,
) -> Vec<DiagnosticProblemFieldJson> {
    use bray_diagnostics::DiagnosticInterfaceIdentitySurfaceProblem as Problem;

    let mut fields = vec![problem_text("cause", problem.as_str())];

    match problem {
        Problem::Empty | Problem::SymbolCountOverflow => {}
        Problem::NonCanonicalSymbolId { expected, actual } => {
            fields.push(problem_count("expected", expected));
            fields.push(problem_count("actual", actual));
        }
        Problem::MissingPackageRoot { actual } => {
            fields.push(problem_text("actual_kind", actual.as_str()));
        }
        Problem::PackageRootHasContainer { container } => {
            fields.push(problem_count("container", container));
        }
        Problem::PackageIdentityMismatch { symbol } | Problem::MissingContainer { symbol } => {
            fields.push(problem_count("symbol", symbol));
        }
        Problem::SymbolKindMismatch {
            symbol,
            declared,
            keyed,
        } => {
            fields.push(problem_count("symbol", symbol));
            fields.push(problem_text("declared_kind", declared.as_str()));
            fields.push(problem_text("keyed_kind", keyed.as_str()));
        }
        Problem::DuplicateExternalKey { first, duplicate } => {
            fields.push(problem_count("first", first));
            fields.push(problem_count("duplicate", duplicate));
        }
        Problem::InvalidContainer { symbol, container }
        | Problem::ContainerKeyMismatch { symbol, container } => {
            fields.push(problem_count("symbol", symbol));
            fields.push(problem_count("container", container));
        }
        Problem::UnexpectedRoot { symbol, kind } => {
            fields.push(problem_count("symbol", symbol));
            fields.push(problem_text("symbol_kind", kind.as_str()));
        }
    }

    fields
}

fn relationship_fields(
    relationship: bray_diagnostics::DiagnosticInterfaceRelationship,
) -> [DiagnosticProblemFieldJson; 6] {
    let position = match relationship.position() {
        bray_diagnostics::DiagnosticCallablePosition::NamedOnly => "named_only",
        bray_diagnostics::DiagnosticCallablePosition::PositionalOrNamed => "positional_or_named",
    };

    [
        problem_text("relationship", relationship.kind().as_str()),
        problem_count("owner", relationship.owner()),
        problem_count("member", relationship.member()),
        problem_count("ordinal", relationship.ordinal()),
        problem_text("position", position),
        problem_text(
            "allows_mutation",
            if relationship.allows_mutation() {
                "true"
            } else {
                "false"
            },
        ),
    ]
}

pub(in crate::output::diagnostic::json) fn interface_semantic_problem_json(
    value: &bray_diagnostics::DiagnosticInterfaceSemanticProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticInterfaceSemanticProblem as Problem;

    match value {
        Problem::UnresolvedSymbol(reference) => {
            interface_symbol_reference_problem("unresolved_symbol", reference)
        }
        Problem::InvalidSymbolKind(reference) => {
            interface_symbol_reference_problem("invalid_symbol_kind", reference)
        }
        Problem::UnresolvedValueGraph => problem("unresolved_value_graph", []),
        Problem::SemanticContent(value) => semantic_content_problem_json(value),
        Problem::InvalidTemplate(value) => checked_template_problem_json(value),
        Problem::InvalidSupportEntity(entity) => problem(
            "invalid_support_entity",
            [problem_count("support_entity", *entity)],
        ),
    }
}

fn interface_symbol_reference_problem(
    reason: &'static str,
    reference: &bray_diagnostics::DiagnosticInterfaceSymbolReference,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticInterfaceSymbolReference as Reference;

    let context = match reference {
        Reference::Local(symbol) => vec![
            problem_text("reference_kind", "local"),
            problem_count("symbol", *symbol),
        ],
        Reference::Dependency {
            dependency,
            identity,
        } => vec![
            problem_text("reference_kind", "dependency"),
            problem_count("dependency", *dependency),
            problem_symbol_identity("identity", identity),
        ],
        Reference::CompilerKnown(identity) => vec![
            problem_text("reference_kind", "compiler_known"),
            problem_symbol_identity("identity", identity),
        ],
    };

    problem(reason, context)
}

fn semantic_content_problem_json(
    value: &bray_diagnostics::DiagnosticSemanticContentProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticSemanticContentProblem as Problem;

    match value {
        Problem::ForeignId { expected, actual } => problem(
            "semantic_content_foreign_id",
            [
                DiagnosticProblemFieldJson {
                    name: "expected_content_set",
                    value: DiagnosticProblemFieldValueJson::Count(*expected),
                },
                DiagnosticProblemFieldJson {
                    name: "actual_content_set",
                    value: DiagnosticProblemFieldValueJson::Count(*actual),
                },
            ],
        ),
        Problem::UnknownId { value_kind } => problem(
            "semantic_content_unknown_id",
            [problem_text("value_kind", value_kind.as_str())],
        ),
        Problem::CapacityExhausted { value_kind } => problem(
            "semantic_content_capacity_exhausted",
            [problem_text("value_kind", value_kind.as_str())],
        ),
        Problem::GenericOwnerMismatch {
            expected_kind,
            expected,
            actual_kind,
            actual,
        } => problem(
            "semantic_content_generic_owner_mismatch",
            [
                problem_text("expected_owner_kind", expected_kind.as_str()),
                problem_count("expected_owner", *expected),
                problem_text("actual_owner_kind", actual_kind.as_str()),
                problem_count("actual_owner", *actual),
            ],
        ),
        Problem::OpenSubstitution => problem("semantic_content_open_substitution", []),
    }
}

fn checked_template_problem_json(
    value: &bray_diagnostics::DiagnosticCheckedTemplateProblem,
) -> DiagnosticProblemJson {
    use bray_diagnostics::DiagnosticCheckedTemplateProblem as Problem;

    match value {
        Problem::CapacityExceeded => problem("template_capacity_exceeded", []),
        Problem::RecoveredTemplate => problem("template_recovered_content", []),
        Problem::MissingInput(input) => {
            problem("template_missing_input", [problem_count("input", *input)])
        }
        Problem::DuplicateInput { first, duplicate } => problem(
            "template_duplicate_input",
            [
                problem_count("first", *first),
                problem_count("duplicate", *duplicate),
            ],
        ),
        Problem::InputTypeMismatch {
            node,
            input,
            expected_type,
            actual_type,
        } => problem(
            "template_input_type_mismatch",
            [
                problem_count("node", *node),
                problem_count("input", *input),
                problem_count("expected_type", *expected_type),
                problem_count("actual_type", *actual_type),
            ],
        ),
        Problem::MissingNode(node) => {
            problem("template_missing_node", [problem_count("node", *node)])
        }
        Problem::ForwardNodeReference { node, referenced } => problem(
            "template_forward_node_reference",
            [
                problem_count("node", *node),
                problem_count("referenced", *referenced),
            ],
        ),
        Problem::MissingTemporary(temporary) => problem(
            "template_missing_temporary",
            [problem_count("temporary", *temporary)],
        ),
        Problem::UninitializedTemporary { node, temporary } => problem(
            "template_uninitialized_temporary",
            [
                problem_count("node", *node),
                problem_count("temporary", *temporary),
            ],
        ),
        Problem::TemporaryInitializerTypeMismatch {
            initializer,
            expected_type,
            actual_type,
        } => problem(
            "template_temporary_initializer_type_mismatch",
            [
                problem_count("initializer", *initializer),
                problem_count("expected_type", *expected_type),
                problem_count("actual_type", *actual_type),
            ],
        ),
        Problem::TemporaryTypeMismatch {
            node,
            temporary,
            expected_type,
            actual_type,
        } => problem(
            "template_temporary_type_mismatch",
            [
                problem_count("node", *node),
                problem_count("temporary", *temporary),
                problem_count("expected_type", *expected_type),
                problem_count("actual_type", *actual_type),
            ],
        ),
        Problem::ConversionTypeMismatch {
            node,
            expected_type,
            actual_type,
        } => type_mismatch_problem(
            "template_conversion_type_mismatch",
            "node",
            *node,
            *expected_type,
            *actual_type,
        ),
        Problem::ConditionalBranchTypeMismatch {
            node,
            when_true_type,
            when_false_type,
        } => problem(
            "template_conditional_branch_type_mismatch",
            [
                problem_count("node", *node),
                problem_count("when_true_type", *when_true_type),
                problem_count("when_false_type", *when_false_type),
            ],
        ),
        Problem::ConditionalResultTypeMismatch {
            node,
            expected_type,
            actual_type,
        } => type_mismatch_problem(
            "template_conditional_result_type_mismatch",
            "node",
            *node,
            *expected_type,
            *actual_type,
        ),
        Problem::ShortCircuitOperandTypeMismatch {
            node,
            left_type,
            right_type,
        } => problem(
            "template_short_circuit_operand_type_mismatch",
            [
                problem_count("node", *node),
                problem_count("left_type", *left_type),
                problem_count("right_type", *right_type),
            ],
        ),
        Problem::ShortCircuitResultTypeMismatch {
            node,
            expected_type,
            actual_type,
        } => type_mismatch_problem(
            "template_short_circuit_result_type_mismatch",
            "node",
            *node,
            *expected_type,
            *actual_type,
        ),
        Problem::ArrayElementTypeMismatch {
            node,
            element,
            expected_type,
            actual_type,
        } => problem(
            "template_array_element_type_mismatch",
            [
                problem_count("node", *node),
                problem_count("element", *element),
                problem_count("expected_type", *expected_type),
                problem_count("actual_type", *actual_type),
            ],
        ),
    }
}

fn type_mismatch_problem(
    reason: &'static str,
    subject_name: &'static str,
    subject: u32,
    expected: u32,
    actual: u32,
) -> DiagnosticProblemJson {
    problem(
        reason,
        [
            problem_count(subject_name, subject),
            problem_count("expected_type", expected),
            problem_count("actual_type", actual),
        ],
    )
}
