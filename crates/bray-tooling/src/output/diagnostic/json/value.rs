use bray_diagnostics::DiagnosticLinkRequirement;
use bray_source::SourceSpan;
use serde::Serialize;

use crate::output::diagnostic::source_map::DiagnosticSourceMap;
use crate::output::{SourceLocationOutput, SourceOriginOutput, path_to_output_string};

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticLinkRequirementJson {
    kind: &'static str,
    value: String,
}

impl DiagnosticLinkRequirementJson {
    pub(in crate::output::diagnostic::json) fn from_requirement(requirement: &DiagnosticLinkRequirement) -> Self {
        Self {
            kind: requirement.kind().as_str(),
            value: requirement.value().to_owned(),
        }
    }
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticRuntimeAbiVersionJson {
    pub(in crate::output::diagnostic::json) major: u16,
    pub(in crate::output::diagnostic::json) minor: u16,
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticTypeJson {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    element_count: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    path: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    arguments: Option<Vec<DiagnosticTypeArgumentJson>>,
}

impl DiagnosticTypeJson {
    pub(in crate::output::diagnostic::json) fn from_type(ty: &bray_diagnostics::DiagnosticType) -> Self {
        use bray_diagnostics::DiagnosticType;

        let (kind, element_count, path, arguments) = match ty {
            DiagnosticType::Error => ("error", None, None, None),
            DiagnosticType::Boolean => ("boolean", None, None, None),
            DiagnosticType::Character => ("character", None, None, None),
            DiagnosticType::I8 => ("i8", None, None, None),
            DiagnosticType::I16 => ("i16", None, None, None),
            DiagnosticType::I32 => ("i32", None, None, None),
            DiagnosticType::I64 => ("i64", None, None, None),
            DiagnosticType::I128 => ("i128", None, None, None),
            DiagnosticType::U8 => ("u8", None, None, None),
            DiagnosticType::U16 => ("u16", None, None, None),
            DiagnosticType::U32 => ("u32", None, None, None),
            DiagnosticType::U64 => ("u64", None, None, None),
            DiagnosticType::U128 => ("u128", None, None, None),
            DiagnosticType::Isize => ("isize", None, None, None),
            DiagnosticType::Usize => ("usize", None, None, None),
            DiagnosticType::Unit => ("unit", None, None, None),
            DiagnosticType::Never => ("never", None, None, None),
            DiagnosticType::String => ("string", None, None, None),
            DiagnosticType::R16 => ("r16", None, None, None),
            DiagnosticType::R32 => ("r32", None, None, None),
            DiagnosticType::R64 => ("r64", None, None, None),
            DiagnosticType::R128 => ("r128", None, None, None),
            DiagnosticType::C32 => ("c32", None, None, None),
            DiagnosticType::C64 => ("c64", None, None, None),
            DiagnosticType::C128 => ("c128", None, None, None),
            DiagnosticType::C256 => ("c256", None, None, None),
            DiagnosticType::Named(named) => (
                "named",
                None,
                Some(named.path().to_vec()),
                Some(
                    named
                        .arguments()
                        .iter()
                        .map(DiagnosticTypeArgumentJson::from_argument)
                        .collect(),
                ),
            ),
            DiagnosticType::Unknown => ("unknown", None, None, None),
            DiagnosticType::TypeParameter => ("type_parameter", None, None, None),
            DiagnosticType::ContextualSelf => ("contextual_self", None, None, None),
            DiagnosticType::TypeValuedMember => ("type_valued_member", None, None, None),
            DiagnosticType::Tuple(count) => ("tuple", Some(*count), None, None),
            DiagnosticType::Array => ("array", None, None, None),
            DiagnosticType::Slice => ("slice", None, None, None),
            DiagnosticType::Generator => ("generator", None, None, None),
            DiagnosticType::Nullable => ("nullable", None, None, None),
            DiagnosticType::Borrow => ("borrow", None, None, None),
            DiagnosticType::TraitView => ("trait_view", None, None, None),
            DiagnosticType::OwnedIndirection => ("owned_indirection", None, None, None),
            DiagnosticType::Callable => ("callable", None, None, None),
        };

        Self {
            kind,
            element_count,
            path,
            arguments,
        }
    }
}

#[derive(Serialize)]
struct DiagnosticTypeArgumentJson {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#type: Option<Box<DiagnosticTypeJson>>,
}

impl DiagnosticTypeArgumentJson {
    fn from_argument(argument: &bray_diagnostics::DiagnosticTypeArgument) -> Self {
        match argument {
            bray_diagnostics::DiagnosticTypeArgument::Type(ty) => Self {
                kind: "type",
                r#type: Some(Box::new(DiagnosticTypeJson::from_type(ty))),
            },
            bray_diagnostics::DiagnosticTypeArgument::Constant => Self {
                kind: "constant",
                r#type: None,
            },
        }
    }
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticArtifactDigestJson {
    algorithm: &'static str,
    bytes: Vec<u8>,
}

impl DiagnosticArtifactDigestJson {
    pub(in crate::output::diagnostic::json) fn from_digest(digest: &bray_diagnostics::DiagnosticArtifactDigest) -> Self {
        Self {
            algorithm: digest.algorithm().as_str(),
            // JSON output owns its DTO independently of the diagnostic bag.
            bytes: digest.bytes().to_vec(),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticOutputSinkJson {
    Filesystem(String),
    Memory(String),
    Stream(String),
}

impl DiagnosticOutputSinkJson {
    pub(in crate::output::diagnostic::json) fn from_sink(sink: &bray_diagnostics::DiagnosticOutputSink) -> Self {
        match sink {
            bray_diagnostics::DiagnosticOutputSink::Filesystem(path) => {
                Self::Filesystem(path_to_output_string(path))
            }
            bray_diagnostics::DiagnosticOutputSink::Memory(identity) => {
                Self::Memory(identity.to_owned())
            }
            bray_diagnostics::DiagnosticOutputSink::Stream(identity) => {
                Self::Stream(identity.to_owned())
            }
        }
    }
}

#[derive(Serialize)]
pub(crate) struct SourceSpanJson {
    source_id: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    source_origin: Option<SourceOriginOutput>,
    start: u32,
    end: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    location: Option<SourceLocationOutput>,
}

impl SourceSpanJson {
    pub(in crate::output::diagnostic::json) fn from_span(span: SourceSpan, source_map: &DiagnosticSourceMap<'_>) -> Self {
        Self {
            source_id: span.source_id().raw(),
            source_origin: source_map.source_origin(span),
            start: span.start().bytes(),
            end: span.end().bytes(),
            location: source_map
                .resolve(span)
                .map(SourceLocationOutput::from_location),
        }
    }

    #[cfg(feature = "analysis")]
    pub(crate) const fn start(&self) -> u32 {
        self.start
    }

    #[cfg(feature = "analysis")]
    pub(crate) const fn end(&self) -> u32 {
        self.end
    }

    #[cfg(feature = "analysis")]
    pub(crate) const fn location(&self) -> Option<SourceLocationOutput> {
        self.location
    }
}
