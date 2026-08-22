#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticDirectiveArgumentProblem {
    /// A positional argument was supplied where only named arguments are accepted.
    Positional {
        /// Zero-based directive argument ordinal.
        ordinal: u64,
    },
    /// An unsupported argument name was supplied.
    Unknown {
        /// Rejected source argument name.
        name: String,
    },
    /// The same named argument was supplied more than once.
    Duplicate {
        /// Repeated source argument name.
        name: String,
    },
    /// A required named argument is absent.
    Missing {
        /// Required argument name.
        name: String,
    },
    /// A string-valued argument evaluates to the empty string.
    EmptyString {
        /// Argument whose value is empty.
        name: String,
    },
}

/// Language-defined category of one native link requirement.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticNativeLinkKind {
    /// A dynamically linked library.
    Dynamic,
    /// A statically linked library or archive.
    Static,
    /// A system library selected by the target toolchain.
    System,
    /// A target-platform framework.
    Framework,
}

/// Exact reason one native link directive is invalid.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticNativeLinkDirectiveProblem {
    /// Named-argument shape or required value is invalid.
    Argument(DiagnosticDirectiveArgumentProblem),
    /// The supplied link-kind spelling is not language-defined.
    UnsupportedKind {
        /// Rejected source spelling.
        provided: String,
    },
    /// Name and optional kind match more than one configured native input.
    AmbiguousInput {
        /// Requested native input name.
        name: String,
        /// Optional requested native link category.
        kind: Option<DiagnosticNativeLinkKind>,
        /// Number of matching configured inputs.
        matches: u64,
    },
}

/// Exact reason one native symbol directive is invalid.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticNativeSymbolDirectiveProblem {
    /// Named-argument shape or required value is invalid.
    Argument(DiagnosticDirectiveArgumentProblem),
    /// Neither target symbol identity form was supplied.
    MissingIdentity,
    /// Both target symbol identity forms were supplied.
    ConflictingIdentity,
    /// A policy option uses an unrecognized value.
    UnsupportedValue {
        /// Directive argument name.
        name: String,
        /// Supplied source spelling.
        provided: String,
    },
    /// The selected target cannot represent one requested symbol option.
    UnsupportedTargetOption {
        /// Directive argument name.
        name: String,
    },
    /// The requested policy is unavailable for this import or export category.
    IncompatiblePolicy {
        /// Directive argument name.
        name: String,
    },
}
