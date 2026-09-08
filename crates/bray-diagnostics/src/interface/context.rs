use bray_package_interface_model::InterfaceSemanticRecordKind;

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
        kind: InterfaceSemanticRecordKind,
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
