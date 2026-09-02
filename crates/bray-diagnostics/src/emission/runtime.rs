/// One locale-neutral field retained by a compiler query-runtime failure.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticFailureField {
    name: &'static str,
    value: DiagnosticFailureValue,
}

impl DiagnosticFailureField {
    pub fn new(name: &'static str, value: DiagnosticFailureValue) -> Self {
        Self { name, value }
    }

    pub const fn name(&self) -> &'static str {
        self.name
    }

    pub const fn value(&self) -> &DiagnosticFailureValue {
        &self.value
    }
}

/// Locale-neutral value retained by a compiler query-runtime failure.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticFailureValue {
    ArtifactDigest(crate::DiagnosticArtifactDigest),
    Boolean(bool),
    Count(u64),
    Identity([u8; 32]),
    IdentityList(Box<[[u8; 32]]>),
    /// Exact nested evaluation failure retained without hashing its payload.
    Evaluation(Box<super::DiagnosticEmissionEvaluationFailure>),
    ExternalToolExit(crate::DiagnosticExternalToolExit),
    /// Exact package-interface declaration identity.
    InterfaceSymbolIdentity(crate::DiagnosticInterfaceSymbolIdentity),
    /// Exact package-interface symbol graph problem.
    InterfaceSymbolGraphProblem(crate::DiagnosticInterfaceSymbolGraphProblem),
    /// Exact package-interface validation failure.
    InterfaceValidationFailure(crate::DiagnosticInterfaceValidationFailure),
    IoErrorKind(crate::DiagnosticIoErrorKind),
    Natural(String),
    Signed(i64),
    Text(String),
    TextList(Box<[String]>),
}

/// Exact compiler query-runtime failure exposed at a diagnostic boundary.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticFactRuntimeFailure {
    reason: &'static str,
    context: Box<[DiagnosticFailureField]>,
}

impl DiagnosticFactRuntimeFailure {
    pub fn new(reason: &'static str, context: impl Into<Box<[DiagnosticFailureField]>>) -> Self {
        Self {
            reason,
            context: context.into(),
        }
    }

    pub const fn reason(&self) -> &'static str {
        self.reason
    }

    pub const fn context(&self) -> &[DiagnosticFailureField] {
        &self.context
    }
}
