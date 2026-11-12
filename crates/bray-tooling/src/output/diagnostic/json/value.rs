use bray_diagnostics::DiagnosticLinkRequirement;
use bray_source::SourceSpan;
use serde::Serialize;

use crate::output::diagnostic::source_map::DiagnosticSourceMap;
use crate::output::{SourceLocationOutput, SourceOriginOutput, path_to_output_string};

#[cfg(test)]
mod tests {
    use bray_diagnostics::{DiagnosticBorrowKind, DiagnosticType};

    #[test]
    fn callable_type_json_keeps_parameter_authority_and_signature() {
        use bray_diagnostics::{DiagnosticCallableAbi, DiagnosticCallableConstness, DiagnosticCallableExecution,
            DiagnosticCallableParameter, DiagnosticCallableParameterMode, DiagnosticCallablePosition, DiagnosticCallableTrust};

        let ty = DiagnosticType::Callable {
            parameters: vec![DiagnosticCallableParameter {
                name: "value".to_owned(), position: DiagnosticCallablePosition::NamedOnly,
                mode: DiagnosticCallableParameterMode::Mutable, ty: DiagnosticType::Borrow {
                    kind: DiagnosticBorrowKind::Mutable, target: Box::new(DiagnosticType::U32),
                },
            }],
            variadic: false, result: Box::new(DiagnosticType::Boolean),
            constness: DiagnosticCallableConstness::Constant, execution: DiagnosticCallableExecution::Synchronous,
            trust: DiagnosticCallableTrust::Trusted, abi: DiagnosticCallableAbi::Bray,
        };
        let json = serde_json::to_value(super::DiagnosticTypeJson::from_type(&ty)).expect("callable type JSON serializes");
        let signature = &json["callable"];

        assert_eq!(json["kind"], "callable");
        assert_eq!(signature["abi"], "bray");
        assert_eq!(signature["execution"], "synchronous");
        assert_eq!(signature["constness"], "constant");
        assert_eq!(signature["trust"], "trusted");
        assert_eq!(signature["variadic"], false);
        assert_eq!(signature["result"]["kind"], "boolean");
        assert_eq!(signature["parameters"][0]["name"], "value");
        assert_eq!(signature["parameters"][0]["position"], "named_only");
        assert_eq!(signature["parameters"][0]["mode"], "mutable");
        assert_eq!(signature["parameters"][0]["type"]["borrow_kind"], "mutable");
        assert_eq!(signature["parameters"][0]["type"]["target"]["kind"], "u32");
    }

    #[test]
    fn borrowed_type_json_keeps_nested_capabilities_and_targets() {
        let ty = DiagnosticType::Borrow { kind: DiagnosticBorrowKind::Mutable, target: Box::new(
            DiagnosticType::Borrow { kind: DiagnosticBorrowKind::Shared, target: Box::new(DiagnosticType::U32) },
        ) };
        let json = serde_json::to_value(super::DiagnosticTypeJson::from_type(&ty)).expect("borrow type JSON serializes");

        assert_eq!(json["kind"], "borrow");
        assert_eq!(json["borrow_kind"], "mutable");
        assert_eq!(json["target"]["borrow_kind"], "shared");
        assert_eq!(json["target"]["target"]["kind"], "u32");
    }
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticLinkRequirementJson {
    kind: &'static str,
    value: String,
}

impl DiagnosticLinkRequirementJson {
    pub(in crate::output::diagnostic::json) fn from_requirement(
        requirement: &DiagnosticLinkRequirement,
    ) -> Self {
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
    #[serde(skip_serializing_if = "Option::is_none")]
    borrow_kind: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target: Option<Box<Self>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    callable: Option<DiagnosticCallableTypeJson>,
}

#[derive(Serialize)]
struct DiagnosticCallableTypeJson {
    parameters: Vec<DiagnosticCallableParameterJson>,
    variadic: bool,
    result: Box<DiagnosticTypeJson>,
    constness: &'static str,
    execution: &'static str,
    trust: &'static str,
    abi: &'static str,
}

#[derive(Serialize)]
struct DiagnosticCallableParameterJson {
    name: String,
    position: &'static str,
    mode: &'static str,
    r#type: DiagnosticTypeJson,
}

impl DiagnosticTypeJson {
    pub(in crate::output::diagnostic::json) fn from_type(
        ty: &bray_diagnostics::DiagnosticType,
    ) -> Self {
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
            DiagnosticType::Borrow { kind, target } => return Self {
                kind: "borrow", element_count: None, path: None, arguments: None,
                borrow_kind: Some(kind.as_str()), target: Some(Box::new(Self::from_type(target))), callable: None,
            },
            DiagnosticType::TraitView => ("trait_view", None, None, None),
            DiagnosticType::OwnedIndirection => ("owned_indirection", None, None, None),
            DiagnosticType::Callable { parameters, variadic, result, constness, execution, trust, abi } => return Self {
                kind: "callable", element_count: None, path: None, arguments: None,
                borrow_kind: None, target: None,
                callable: Some(DiagnosticCallableTypeJson {
                    parameters: parameters.iter().map(|parameter| DiagnosticCallableParameterJson {
                        name: parameter.name.clone(), position: parameter.position.as_str(), mode: parameter.mode.as_str(),
                        r#type: Self::from_type(&parameter.ty),
                    }).collect(),
                    variadic: *variadic, result: Box::new(Self::from_type(result)), constness: constness.as_str(),
                    execution: execution.as_str(), trust: trust.as_str(), abi: abi.as_str(),
                }),
            },
        };

        Self {
            kind,
            element_count,
            path,
            arguments,
            borrow_kind: None,
            target: None,
            callable: None,
        }
    }
}

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticTypeArgumentJson {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#type: Option<Box<DiagnosticTypeJson>>,
}

impl DiagnosticTypeArgumentJson {
    pub(in crate::output::diagnostic::json) fn from_argument(argument: &bray_diagnostics::DiagnosticTypeArgument) -> Self {
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
    pub(in crate::output::diagnostic::json) fn from_digest(
        digest: &bray_diagnostics::DiagnosticArtifactDigest,
    ) -> Self {
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
    pub(in crate::output::diagnostic::json) fn from_sink(
        sink: &bray_diagnostics::DiagnosticOutputSink,
    ) -> Self {
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
    pub(in crate::output::diagnostic::json) fn from_span(
        span: SourceSpan,
        source_map: &DiagnosticSourceMap<'_>,
    ) -> Self {
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
