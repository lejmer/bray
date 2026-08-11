/// Exact source-report construction failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSourceInspectionFailure {
    /// Source line indexing failed.
    SourceIndex,
    /// JSON report serialization failed.
    Json,
}

/// Exact token-report construction failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticTokenInspectionFailure {
    /// Source line indexing failed.
    SourceIndex,
    /// Token text could not be correlated with source text.
    TokenText,
    /// Trivia text could not be correlated with source text.
    TriviaText,
    /// JSON report serialization failed.
    Json,
}

/// Exact syntax-report construction failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSyntaxInspectionFailure {
    /// Source line indexing failed.
    SourceIndex,
    /// A syntax result does not belong to its selected source.
    SourceMismatch,
    /// Token text could not be correlated with source text.
    TokenText,
    /// Trivia text could not be correlated with source text.
    TriviaText,
    /// The syntax tree violates its traversal contract.
    TreeStructure,
    /// JSON report serialization failed.
    Json,
}

/// Exact declaration-report construction failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticDeclarationInspectionFailure {
    /// A declaration container reference is invalid.
    Container,
    /// A declaration reference is invalid.
    Declaration,
    /// A module-part reference is invalid.
    ModulePart,
    /// A declaration has no source correlation.
    Source,
    /// Source line indexing failed.
    SourceIndex,
    /// JSON report serialization failed.
    Json,
}

/// Exact symbol-report construction failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSymbolInspectionFailure {
    /// A declaration reference is invalid.
    Declaration,
    /// The symbol graph could not be evaluated.
    Graph,
    /// JSON report serialization failed.
    Json,
    /// A symbol has no source correlation.
    Source,
    /// Source line indexing failed.
    SourceIndex,
    /// A symbol reference is invalid.
    Symbol,
    /// Symbol identity traversal contains a cycle.
    SymbolCycle,
    /// Required symbol semantic content is unavailable.
    SymbolState,
    /// A semantic type cannot be represented in the report.
    Type,
    /// A relationship kind has no inspection representation.
    UnsupportedRelationship,
}

/// Exact bound-tree report construction failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticBoundInspectionFailure {
    /// Required bound-tree semantic content is unavailable.
    BoundState,
    /// JSON report serialization failed.
    Json,
    /// A selected bound node is absent.
    MissingNode,
    /// A bound node has no source correlation.
    Source,
    /// Source line indexing failed.
    SourceIndex,
    /// Required storage-plan state is unavailable.
    StorageState,
    /// A symbol reference is invalid.
    Symbol,
    /// Required symbol semantic content is unavailable.
    SymbolState,
    /// A semantic type cannot be represented in the report.
    Type,
    /// Required expression-type state is unavailable.
    TypeState,
    /// Required semantic-selection state is unavailable.
    SelectionState,
    /// A semantic selection cannot be represented in the report.
    Selection,
}

/// Exact lowered or MIR report construction failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLoweredInspectionFailure {
    /// JSON report serialization failed.
    Json,
    /// Required lowering state is unavailable.
    LoweringState,
    /// MIR model construction rejected the lowered unit.
    Model,
    /// A lowered node has no source correlation.
    Source,
    /// Required symbol semantic content is unavailable.
    SymbolState,
    /// Required lowered-unit state is unavailable.
    UnitState,
}

/// Selected rendering format for a compiler inspection report.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInspectionOutputFormat {
    /// Human-readable text report.
    Text,
    /// Structured JSON report.
    Json,
}

/// Source and optional byte position selected for a semantic-unit inspection.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticInspectionTarget {
    /// Raw loaded-source identity.
    pub source_id: u32,
    /// Selected UTF-8 byte offset when inspection is position-filtered.
    pub position: Option<u32>,
}

/// Payload-owning inspection report failure at the public tooling boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInspectionFailure {
    /// Source snapshot report failure.
    Source {
        /// Requested report format.
        format: DiagnosticInspectionOutputFormat,
        /// Exact report-construction failure.
        cause: DiagnosticSourceInspectionFailure,
    },
    /// Lexical token report failure.
    Token {
        /// Requested report format.
        format: DiagnosticInspectionOutputFormat,
        /// Exact report-construction failure.
        cause: DiagnosticTokenInspectionFailure,
    },
    /// Parsed syntax report failure.
    Syntax {
        /// Requested report format.
        format: DiagnosticInspectionOutputFormat,
        /// Exact report-construction failure.
        cause: DiagnosticSyntaxInspectionFailure,
    },
    /// Declaration report failure.
    Declaration {
        /// Requested report format.
        format: DiagnosticInspectionOutputFormat,
        /// Exact report-construction failure.
        cause: DiagnosticDeclarationInspectionFailure,
    },
    /// Symbol graph report failure.
    Symbol {
        /// Requested report format.
        format: DiagnosticInspectionOutputFormat,
        /// Exact report-construction failure.
        cause: DiagnosticSymbolInspectionFailure,
    },
    /// Bound-tree report failure.
    Bound {
        /// Selected source and optional byte position.
        target: DiagnosticInspectionTarget,
        /// Requested report format.
        format: DiagnosticInspectionOutputFormat,
        /// Exact report-construction failure.
        cause: DiagnosticBoundInspectionFailure,
    },
    /// Lowered-tree report failure.
    Lowered {
        /// Selected source and optional byte position.
        target: DiagnosticInspectionTarget,
        /// Requested report format.
        format: DiagnosticInspectionOutputFormat,
        /// Exact report-construction failure.
        cause: DiagnosticLoweredInspectionFailure,
    },
    /// MIR report failure.
    Mir {
        /// Selected source and optional byte position.
        target: DiagnosticInspectionTarget,
        /// Requested report format.
        format: DiagnosticInspectionOutputFormat,
        /// Exact report-construction failure.
        cause: DiagnosticLoweredInspectionFailure,
    },
}
