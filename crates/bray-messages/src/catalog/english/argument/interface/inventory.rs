use bray_diagnostics::{DiagnosticInterfaceLimit, DiagnosticInterfaceSection};

pub(crate) const fn format_english_interface_limit(
    limit: DiagnosticInterfaceLimit,
) -> &'static str {
    match limit {
        DiagnosticInterfaceLimit::LoadedInterfaceCount => "selected package-interface count",
        DiagnosticInterfaceLimit::CompilationSymbolCount => "compilation symbol count",
        DiagnosticInterfaceLimit::FileSize => "file size",
        DiagnosticInterfaceLimit::SectionCount => "section count",
        DiagnosticInterfaceLimit::RecordCount => "record count",
        DiagnosticInterfaceLimit::StringLength => "string length",
        DiagnosticInterfaceLimit::BlobLength => "blob length",
        DiagnosticInterfaceLimit::DecodedAllocation => "decoded allocation",
        DiagnosticInterfaceLimit::SemanticTypeDepth => "semantic type depth",
        DiagnosticInterfaceLimit::TemplateGraphSize => "template graph size",
        DiagnosticInterfaceLimit::ExternalReferenceCount => "external reference count",
    }
}

pub(crate) const fn format_english_interface_section(
    section: DiagnosticInterfaceSection,
) -> &'static str {
    match section {
        DiagnosticInterfaceSection::Strings => "strings",
        DiagnosticInterfaceSection::PackageMetadata => "package metadata",
        DiagnosticInterfaceSection::Dependencies => "dependencies",
        DiagnosticInterfaceSection::SymbolIdentities => "symbol identities",
        DiagnosticInterfaceSection::Relationships => "relationships",
        DiagnosticInterfaceSection::ExportedLookup => "exported lookup",
        DiagnosticInterfaceSection::SymbolDirectory => "symbol directory",
        DiagnosticInterfaceSection::SemanticTypes => "semantic types",
        DiagnosticInterfaceSection::Constants => "constants",
        DiagnosticInterfaceSection::Contracts => "contracts",
        DiagnosticInterfaceSection::Declarations => "declarations",
        DiagnosticInterfaceSection::DeclarationTemplates => "declaration templates",
        DiagnosticInterfaceSection::Implementations => "implementations",
        DiagnosticInterfaceSection::TargetDependencies => "target dependencies",
        DiagnosticInterfaceSection::SourceProvenance => "source provenance",
        DiagnosticInterfaceSection::SupportGraph => "support graph",
    }
}
