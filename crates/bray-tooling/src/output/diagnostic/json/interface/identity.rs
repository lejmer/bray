use serde::Serialize;

use super::super::DiagnosticTypeJson;

#[derive(Serialize)]
pub(in crate::output::diagnostic::json) struct DiagnosticProblemFieldJson {
    pub(in crate::output::diagnostic::json) name: &'static str,
    pub(in crate::output::diagnostic::json) value: DiagnosticProblemFieldValueJson,
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticProblemFieldValueJson {
    Count(u64),
    Text(String),
    Type(DiagnosticTypeJson),
    Types(Vec<DiagnosticTypeJson>),
    ArrayLength(DiagnosticArrayLengthJson),
    SymbolIdentity(DiagnosticInterfaceSymbolIdentityJson),
}

#[derive(Serialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticArrayLengthJson {
    Exact(u64),
    Symbolic,
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticInterfaceSymbolIdentityJson {
    CompilerKnownEnvironment,
    Package {
        package: String,
    },
    Module {
        owner: Box<Self>,
        path: Box<[String]>,
    },
    CompilerKnownDeclaration {
        key: String,
        symbol_kind: &'static str,
    },
    SourceDeclaration {
        owner: Box<Self>,
        symbol_kind: &'static str,
        declaration: u32,
    },
    NamedDeclaration {
        owner: Box<Self>,
        symbol_kind: &'static str,
        name: String,
    },
    OrdinalDeclaration {
        owner: Box<Self>,
        symbol_kind: &'static str,
        ordinal: u32,
    },
    Synthesized {
        owner: Box<Self>,
        identity: DiagnosticInterfaceSynthesizedIdentityJson,
    },
}

impl DiagnosticInterfaceSymbolIdentityJson {
    pub(in crate::output::diagnostic::json) fn from_identity(
        identity: &bray_diagnostics::DiagnosticInterfaceSymbolIdentity,
    ) -> Self {
        use bray_diagnostics::DiagnosticInterfaceDeclarationIdentity as DeclarationIdentity;
        use bray_diagnostics::DiagnosticInterfaceSymbolIdentity as Identity;

        match identity {
            Identity::CompilerKnownEnvironment => Self::CompilerKnownEnvironment,
            Identity::Package(package) => Self::Package {
                package: package.clone(),
            },
            Identity::Module { owner, path } => Self::Module {
                owner: Box::new(Self::from_identity(owner)),
                path: path.clone(),
            },
            Identity::CompilerKnownDeclaration { key, kind } => {
                Self::CompilerKnownDeclaration {
                    key: key.clone(),
                    symbol_kind: kind.as_str(),
                }
            }
            Identity::SourceDeclaration {
                owner,
                kind,
                declaration,
            } => Self::SourceDeclaration {
                owner: Box::new(Self::from_identity(owner)),
                symbol_kind: kind.as_str(),
                declaration: *declaration,
            },
            Identity::Declaration {
                owner,
                kind,
                identity,
            } => match identity {
                DeclarationIdentity::Name(name) => Self::NamedDeclaration {
                    owner: Box::new(Self::from_identity(owner)),
                    symbol_kind: kind.as_str(),
                    name: name.clone(),
                },
                DeclarationIdentity::Ordinal(ordinal) => Self::OrdinalDeclaration {
                    owner: Box::new(Self::from_identity(owner)),
                    symbol_kind: kind.as_str(),
                    ordinal: *ordinal,
                },
            },
            Identity::Synthesized { owner, identity } => Self::Synthesized {
                owner: Box::new(Self::from_identity(owner)),
                identity: DiagnosticInterfaceSynthesizedIdentityJson::from_identity(*identity),
            },
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(in crate::output::diagnostic::json) enum DiagnosticInterfaceSynthesizedIdentityJson {
    ReceiverParameter,
    DeclaredGenericTypeParameter { ordinal: u32 },
    DeclaredGenericConstParameter { ordinal: u32 },
    CallableParameter { ordinal: u32 },
    PredicateParameter { ordinal: u32 },
    InferredImplementationTypeParameter { ordinal: u32 },
    InferredImplementationConstParameter { ordinal: u32 },
    CallableParameterDefaultProvider,
    StructFieldDefaultProvider,
    UnionPayloadDefaultProvider,
}

impl DiagnosticInterfaceSynthesizedIdentityJson {
    fn from_identity(
        identity: bray_diagnostics::DiagnosticInterfaceSynthesizedIdentity,
    ) -> Self {
        use bray_diagnostics::DiagnosticInterfaceSynthesizedIdentity as Identity;

        match identity {
            Identity::ReceiverParameter => Self::ReceiverParameter,
            Identity::DeclaredGenericTypeParameter(ordinal) => {
                Self::DeclaredGenericTypeParameter { ordinal }
            }
            Identity::DeclaredGenericConstParameter(ordinal) => {
                Self::DeclaredGenericConstParameter { ordinal }
            }
            Identity::CallableParameter(ordinal) => Self::CallableParameter { ordinal },
            Identity::PredicateParameter(ordinal) => Self::PredicateParameter { ordinal },
            Identity::InferredImplementationTypeParameter(ordinal) => {
                Self::InferredImplementationTypeParameter { ordinal }
            }
            Identity::InferredImplementationConstParameter(ordinal) => {
                Self::InferredImplementationConstParameter { ordinal }
            }
            Identity::CallableParameterDefaultProvider => Self::CallableParameterDefaultProvider,
            Identity::StructFieldDefaultProvider => Self::StructFieldDefaultProvider,
            Identity::UnionPayloadDefaultProvider => Self::UnionPayloadDefaultProvider,
        }
    }
}
