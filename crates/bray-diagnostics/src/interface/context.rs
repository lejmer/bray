use super::inventory::DiagnosticInterfaceSection;

/// Package-interface region associated with a validation failure.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceValidationContext {
    /// Complete package-interface artifact.
    Artifact,
    /// Fixed artifact header.
    Header,
    /// Section directory as a whole.
    Directory,
    /// One section-directory entry.
    DirectoryEntry {
        /// Zero-based directory index.
        index: u64,
        /// Raw section tag stored by the entry.
        raw_tag: u32,
    },
    /// One implementation payload directory entry.
    ImplementationEntry {
        /// Zero-based implementation-entry index.
        index: u64,
        /// Open implementation-entry category stored by the artifact.
        raw_kind: u8,
    },
    /// One known section payload.
    Section(DiagnosticInterfaceSection),
    /// One indexed record in a known section.
    Record {
        /// Section containing the record.
        section: DiagnosticInterfaceSection,
        /// Zero-based record index.
        index: u64,
    },
    /// One indexed semantic record of a known kind.
    SemanticRecord {
        /// Semantic record category.
        kind: DiagnosticInterfaceSemanticRecordKind,
        /// Zero-based semantic record index.
        index: u64,
    },
    /// One component of a length-delimited external symbol key.
    ExternalSymbolKey {
        /// Zero-based component index.
        component: u64,
    },
}

impl DiagnosticInterfaceValidationContext {
    /// Returns the stable machine key for this context category.
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Artifact => "artifact",
            Self::Header => "header",
            Self::Directory => "directory",
            Self::DirectoryEntry { .. } => "directory_entry",
            Self::ImplementationEntry { .. } => "implementation_entry",
            Self::Section(_) => "section",
            Self::Record { .. } => "record",
            Self::SemanticRecord { .. } => "semantic_record",
            Self::ExternalSymbolKey { .. } => "external_symbol_key",
        }
    }
}

/// Semantic record category associated with package-interface validation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceSemanticRecordKind {
    /// Complete callable signature template.
    CallableSignature,
    /// Ordered generic declaration template.
    GenericDeclaration,
    /// Callable parameter default-template presence.
    CallableParameterDefault,
    /// Validated predicate definition form.
    PredicateDefinition,
    /// Checked type owned by a declaration.
    DeclaredType,
    /// Complete type-representation contract.
    TypeRepresentation,
    /// Checked generic constraint.
    GenericConstraint,
    /// Complete callable contract set.
    CallableContracts,
    /// Declaration-owned checked template.
    DeclarationTemplate,
    /// Public implementation subject and applied trait.
    Implementation,
    /// Required target property value.
    TargetProperty,
    /// Required callable ABI.
    Abi,
    /// Required portable runtime contract.
    Runtime,
}

impl DiagnosticInterfaceSemanticRecordKind {
    /// Returns the stable machine key for this record category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CallableSignature => "callable_signature",
            Self::GenericDeclaration => "generic_declaration",
            Self::CallableParameterDefault => "callable_parameter_default",
            Self::PredicateDefinition => "predicate_definition",
            Self::DeclaredType => "declared_type",
            Self::TypeRepresentation => "type_representation",
            Self::GenericConstraint => "generic_constraint",
            Self::CallableContracts => "callable_contracts",
            Self::DeclarationTemplate => "declaration_template",
            Self::Implementation => "implementation",
            Self::TargetProperty => "target_property",
            Self::Abi => "abi",
            Self::Runtime => "runtime",
        }
    }
}
