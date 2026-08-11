use serde::Serialize;

use super::super::{DiagnosticInterfaceSymbolIdentityJson, DiagnosticTypeJson};

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticSelectionCandidateJson {
    identity: DiagnosticSelectionCandidateIdentityJson,
    signature: DiagnosticSelectionCandidateSignatureJson,
}

impl DiagnosticSelectionCandidateJson {
    pub(in crate::output::diagnostic::json) fn from_candidate(
        candidate: &bray_diagnostics::DiagnosticSelectionCandidate,
    ) -> Self {
        Self {
            identity: DiagnosticSelectionCandidateIdentityJson::from_identity(
                candidate.identity(),
            ),
            signature: DiagnosticSelectionCandidateSignatureJson::from_signature(
                candidate.signature(),
            ),
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DiagnosticSelectionCandidateIdentityJson {
    BuiltIn,
    Declaration {
        identity: DiagnosticInterfaceSymbolIdentityJson,
    },
    NamedDeclaration {
        identity: DiagnosticInterfaceSymbolIdentityJson,
        name: String,
    },
    Iteration {
        iterable: DiagnosticInterfaceSymbolIdentityJson,
        iterator: DiagnosticInterfaceSymbolIdentityJson,
    },
    ExpressionValue,
    PatternValue,
    LocalValue,
    SurfaceValue {
        identity: DiagnosticInterfaceSymbolIdentityJson,
    },
    NamedSurfaceValue {
        identity: DiagnosticInterfaceSymbolIdentityJson,
        name: String,
    },
}

impl DiagnosticSelectionCandidateIdentityJson {
    fn from_identity(identity: &bray_diagnostics::DiagnosticSelectionCandidateIdentity) -> Self {
        use bray_diagnostics::DiagnosticSelectionCandidateIdentity as Identity;

        match identity {
            Identity::BuiltIn => Self::BuiltIn,
            Identity::Declaration(identity) => Self::Declaration {
                identity: DiagnosticInterfaceSymbolIdentityJson::from_identity(identity),
            },
            Identity::NamedDeclaration { identity, name } => Self::NamedDeclaration {
                identity: DiagnosticInterfaceSymbolIdentityJson::from_identity(identity),
                name: name.clone(),
            },
            Identity::Iteration { iterable, iterator } => Self::Iteration {
                iterable: DiagnosticInterfaceSymbolIdentityJson::from_identity(iterable),
                iterator: DiagnosticInterfaceSymbolIdentityJson::from_identity(iterator),
            },
            Identity::ExpressionValue => Self::ExpressionValue,
            Identity::PatternValue => Self::PatternValue,
            Identity::LocalValue => Self::LocalValue,
            Identity::SurfaceValue(identity) => Self::SurfaceValue {
                identity: DiagnosticInterfaceSymbolIdentityJson::from_identity(identity),
            },
            Identity::NamedSurfaceValue { identity, name } => Self::NamedSurfaceValue {
                identity: DiagnosticInterfaceSymbolIdentityJson::from_identity(identity),
                name: name.clone(),
            },
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum DiagnosticSelectionCandidateSignatureJson {
    Callable {
        parameter_types: Vec<DiagnosticTypeJson>,
        result_type: DiagnosticTypeJson,
    },
    Operation {
        operand_types: Vec<DiagnosticTypeJson>,
        result_type: Option<DiagnosticTypeJson>,
    },
    Iteration {
        source_type: DiagnosticTypeJson,
        cursor_type: DiagnosticTypeJson,
        element_type: DiagnosticTypeJson,
    },
}

impl DiagnosticSelectionCandidateSignatureJson {
    fn from_signature(
        signature: &bray_diagnostics::DiagnosticSelectionCandidateSignature,
    ) -> Self {
        use bray_diagnostics::DiagnosticSelectionCandidateSignature as Signature;

        match signature {
            Signature::Callable {
                parameter_types,
                result_type,
            } => Self::Callable {
                parameter_types: parameter_types
                    .iter()
                    .map(DiagnosticTypeJson::from_type)
                    .collect(),
                result_type: DiagnosticTypeJson::from_type(result_type),
            },
            Signature::Operation {
                operand_types,
                result_type,
            } => Self::Operation {
                operand_types: operand_types
                    .iter()
                    .map(DiagnosticTypeJson::from_type)
                    .collect(),
                result_type: result_type.as_ref().map(DiagnosticTypeJson::from_type),
            },
            Signature::Iteration {
                source_type,
                cursor_type,
                element_type,
            } => Self::Iteration {
                source_type: DiagnosticTypeJson::from_type(source_type),
                cursor_type: DiagnosticTypeJson::from_type(cursor_type),
                element_type: DiagnosticTypeJson::from_type(element_type),
            },
        }
    }
}
