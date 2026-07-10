/// Package-interface resource category reported by a validation diagnostic.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceLimit {
    /// Complete artifact byte length.
    FileSize,
    /// Number of section-directory entries.
    SectionCount,
    /// Number of records in one section.
    RecordCount,
    /// Byte length of one decoded string.
    StringLength,
    /// Byte length of one decoded blob.
    BlobLength,
    /// Total decoded allocation.
    DecodedAllocation,
    /// Semantic type nesting depth.
    SemanticTypeDepth,
    /// Checked-template graph size.
    TemplateGraphSize,
    /// Number of external references.
    ExternalReferenceCount,
}

impl DiagnosticInterfaceLimit {
    /// Returns the stable machine key for this resource category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FileSize => "file_size",
            Self::SectionCount => "section_count",
            Self::RecordCount => "record_count",
            Self::StringLength => "string_length",
            Self::BlobLength => "blob_length",
            Self::DecodedAllocation => "decoded_allocation",
            Self::SemanticTypeDepth => "semantic_type_depth",
            Self::TemplateGraphSize => "template_graph_size",
            Self::ExternalReferenceCount => "external_reference_count",
        }
    }
}

/// Package-interface section category reported by a validation diagnostic.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticInterfaceSection {
    /// String table.
    Strings,
    /// Package and product metadata.
    PackageMetadata,
    /// Dependency table.
    Dependencies,
    /// Symbol identity skeleton.
    SymbolIdentities,
    /// Containment and typed relationships.
    Relationships,
    /// Exported lookup and re-export edges.
    ExportedLookup,
    /// Symbol fact directory.
    SymbolFactDirectory,
    /// Canonical semantic types.
    SemanticTypes,
    /// Constant values and templates.
    Constants,
    /// Constraints, contracts, effects, and capabilities.
    Contracts,
    /// Declaration-owned checked templates.
    DeclarationTemplates,
    /// Implementation and coherence records.
    Implementations,
    /// Target-fact and ABI dependencies.
    TargetDependencies,
    /// Optional source provenance.
    SourceProvenance,
    /// Private support graph.
    SupportGraph,
}

impl DiagnosticInterfaceSection {
    /// Returns the stable machine key for this section category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Strings => "strings",
            Self::PackageMetadata => "package_metadata",
            Self::Dependencies => "dependencies",
            Self::SymbolIdentities => "symbol_identities",
            Self::Relationships => "relationships",
            Self::ExportedLookup => "exported_lookup",
            Self::SymbolFactDirectory => "symbol_fact_directory",
            Self::SemanticTypes => "semantic_types",
            Self::Constants => "constants",
            Self::Contracts => "contracts",
            Self::DeclarationTemplates => "declaration_templates",
            Self::Implementations => "implementations",
            Self::TargetDependencies => "target_dependencies",
            Self::SourceProvenance => "source_provenance",
            Self::SupportGraph => "support_graph",
        }
    }
}
