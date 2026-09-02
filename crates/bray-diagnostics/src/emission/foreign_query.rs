/// Exact foreign-boundary operation or identity associated with a failed query.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticForeignQueryContext {
    kind: DiagnosticForeignQueryContextKind,
    identity: String,
}

impl DiagnosticForeignQueryContext {
    /// Creates a foreign-query context from its typed category and exact structural identity.
    pub fn new(kind: DiagnosticForeignQueryContextKind, identity: impl Into<String>) -> Self {
        Self {
            kind,
            identity: identity.into(),
        }
    }

    /// Returns the typed foreign-boundary context category.
    pub const fn kind(&self) -> DiagnosticForeignQueryContextKind {
        self.kind
    }

    /// Returns the exact locale-neutral structural identity retained by the compiler.
    pub fn identity(&self) -> &str {
        &self.identity
    }
}

/// Locale-neutral category of a foreign-boundary operation or identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticForeignQueryContextKind {
    /// A semantic symbol.
    Symbol,
    /// A source function.
    Function,
    /// A source static.
    Static,
    /// A source directive and its exact syntax range.
    Directive,
    /// A loaded source.
    Source,
    /// A generic substitution.
    Substitution,
    /// A compiler-known representation lookup.
    CompilerKnownRepresentation,
    /// A platform service.
    PlatformService,
}

impl DiagnosticForeignQueryContextKind {
    /// Returns this context category's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Symbol => "symbol",
            Self::Function => "function",
            Self::Static => "static",
            Self::Directive => "directive",
            Self::Source => "source",
            Self::Substitution => "substitution",
            Self::CompilerKnownRepresentation => "compiler_known_representation",
            Self::PlatformService => "platform_service",
        }
    }
}

/// Locale-neutral category of data required by a foreign-boundary query.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticForeignDataKind {
    /// A declaration's containing module.
    ContainingModule,
    /// A declaration source anchor.
    SourceAnchor,
    /// A bound function record.
    FunctionBindingRecord,
    /// A bound static record.
    StaticBindingRecord,
    /// A function declaration syntax node.
    FunctionDeclarationSyntax,
    /// A static declaration syntax node.
    StaticDeclarationSyntax,
    /// A loaded source snapshot.
    SourceSnapshot,
    /// Exact text for a retained source range.
    SourceText,
    /// A semantic structure record.
    StructureRecord,
    /// A semantic union record.
    UnionRecord,
    /// A semantic union-variant record.
    UnionVariantRecord,
    /// The type argument of a unary representation.
    UnaryRepresentationArgument,
    /// A compiler-known representation symbol.
    RepresentationSymbol,
}

impl DiagnosticForeignDataKind {
    /// Returns this data category's stable machine-readable name.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContainingModule => "containing_module",
            Self::SourceAnchor => "source_anchor",
            Self::FunctionBindingRecord => "function_binding_record",
            Self::StaticBindingRecord => "static_binding_record",
            Self::FunctionDeclarationSyntax => "function_declaration_syntax",
            Self::StaticDeclarationSyntax => "static_declaration_syntax",
            Self::SourceSnapshot => "source_snapshot",
            Self::SourceText => "source_text",
            Self::StructureRecord => "structure_record",
            Self::UnionRecord => "union_record",
            Self::UnionVariantRecord => "union_variant_record",
            Self::UnaryRepresentationArgument => "unary_representation_argument",
            Self::RepresentationSymbol => "representation_symbol",
        }
    }
}

/// Exact locale-neutral foreign-boundary query failure retained for diagnostics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticForeignQueryFailure {
    /// Required data is unavailable for an exact foreign-boundary context.
    Missing {
        /// Exact operation or identity requiring the data.
        context: DiagnosticForeignQueryContext,
        /// Missing data category.
        data: DiagnosticForeignDataKind,
    },
    /// A foreign-boundary collection has an incompatible count.
    CountMismatch {
        /// Exact operation or identity owning the collection.
        context: DiagnosticForeignQueryContext,
        /// Counted data category.
        data: DiagnosticForeignDataKind,
        /// Required count.
        expected: usize,
        /// Actual count.
        actual: usize,
    },
    /// A semantic type has an incompatible category.
    UnexpectedSemanticType {
        /// Exact semantic type identity.
        ty: String,
        /// Required type category.
        expected: String,
        /// Exact retained semantic type data.
        actual: String,
    },
    /// A retained type template has an incompatible category.
    UnexpectedTypeTemplate {
        /// Exact owning foreign-boundary context.
        context: DiagnosticForeignQueryContext,
        /// Required type-template category.
        expected: String,
        /// Exact retained type template.
        actual: String,
    },
    /// A generic substitution argument has an incompatible category.
    UnexpectedGenericArgument {
        /// Exact substitution identity.
        substitution: String,
        /// Required generic-argument category.
        expected: String,
        /// Actual generic-argument category.
        actual: String,
    },
    /// A count cannot fit its required stable integer representation.
    NumericOverflow {
        /// Exact owning foreign-boundary context.
        context: DiagnosticForeignQueryContext,
        /// Rejected count.
        value: usize,
        /// Required stable integer representation.
        target: String,
    },
    /// A platform-service role cannot participate in the requested contract.
    InvalidPlatformServiceRole {
        /// Exact platform-service role.
        role: String,
    },
    /// A callable signature violated its exact structural contract.
    CallableSignature {
        /// Exact source function identity.
        function: String,
        /// Exact locale-neutral signature failure.
        cause: String,
    },
    /// Runtime and platform metadata assign conflicting roles to one function.
    ConflictingSourceRoles {
        /// Exact source function identity.
        function: String,
        /// Exact runtime ABI role.
        runtime: String,
        /// Exact platform-service role.
        platform: String,
    },
    /// One source role is assigned more than once to a function.
    DuplicateSourceRole {
        /// Exact source function identity.
        function: String,
        /// First retained role.
        first: String,
        /// Duplicate retained role.
        duplicate: String,
    },
}

impl DiagnosticForeignQueryFailure {
    /// Returns this failure category's stable machine-readable name.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Missing { .. } => "foreign_query_missing",
            Self::CountMismatch { .. } => "foreign_query_count_mismatch",
            Self::UnexpectedSemanticType { .. } => "foreign_query_unexpected_semantic_type",
            Self::UnexpectedTypeTemplate { .. } => "foreign_query_unexpected_type_template",
            Self::UnexpectedGenericArgument { .. } => "foreign_query_unexpected_generic_argument",
            Self::NumericOverflow { .. } => "foreign_query_numeric_overflow",
            Self::InvalidPlatformServiceRole { .. } => {
                "foreign_query_invalid_platform_service_role"
            }
            Self::CallableSignature { .. } => "foreign_query_callable_signature",
            Self::ConflictingSourceRoles { .. } => "foreign_query_conflicting_source_roles",
            Self::DuplicateSourceRole { .. } => "foreign_query_duplicate_source_role",
        }
    }
}
