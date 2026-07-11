use std::sync::Arc;

use bray_diagnostics::DiagnosticKind;
use bray_syntax::SyntaxKind;

use super::{CatalogDeclarationKind, CatalogKind, CatalogSourceAnchor};
use crate::{ImplementationHook, RepresentationRole};

/// Catalog entry category used by structural diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CatalogEntryKind {
    /// A declaration surface entry.
    Declaration,
    /// A language-known value entry.
    Value,
}

/// Known field names in the private catalog language.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CatalogField {
    /// A declaration owner stable key.
    Owner,
    /// A target availability rule.
    Availability,
    /// A protected representation role.
    Representation,
    /// A compiler implementation hook.
    Implementation,
    /// An embedded Bray declaration surface.
    Surface,
    /// A language-known token spelling.
    Spelling,
    /// An embedded Bray type-expression surface.
    Type,
}

/// Typed metadata namespace used when a spelling is unknown.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CatalogMetadataKind {
    /// Target availability metadata.
    Availability,
    /// Protected representation metadata.
    Representation,
    /// Compiler implementation metadata.
    Implementation,
}

/// Stable-key namespace used by duplicate and relationship diagnostics.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CatalogKeyDomain {
    /// Compiler-known scope keys.
    CompilerKnownScope,
    /// Compiler-known declaration keys.
    CompilerKnownDeclaration,
    /// Compiler-known special value keys.
    CompilerKnownValue,
    /// Recognized standard-library scope keys.
    RecognizedStandardLibraryScope,
    /// Recognized standard-library declaration keys.
    RecognizedStandardLibraryDeclaration,
}

/// Token or grammar shape expected by the private catalog parser.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CatalogExpectation {
    /// The exact catalog word carried by this value.
    Word(&'static str),
    /// A stable catalog key identifier.
    StableKey,
    /// A metadata identifier.
    Identifier,
    /// A scope location.
    ScopeLocation,
    /// A declaration or value entry.
    Entry,
    /// An entry field.
    Field,
    /// One token spelling.
    TokenSpelling,
    /// An opening brace.
    OpenBrace,
    /// A closing brace.
    CloseBrace,
    /// A semicolon.
    Semicolon,
    /// The end of the catalog source.
    EndOfFile,
}

/// A related stable key retained without erasing its identity domain.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogRelatedKey {
    domain: CatalogKeyDomain,
    spelling: Arc<str>,
}

impl CatalogRelatedKey {
    pub(super) fn new(domain: CatalogKeyDomain, spelling: impl Into<Arc<str>>) -> Self {
        Self {
            domain,
            spelling: spelling.into(),
        }
    }

    /// Returns the stable-key namespace.
    pub const fn domain(&self) -> CatalogKeyDomain {
        self.domain
    }

    /// Returns the exact stable-key spelling.
    pub fn spelling(&self) -> &str {
        &self.spelling
    }
}

/// Locale-neutral category and typed arguments for one catalog defect.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CatalogDiagnosticKind {
    /// The Bray lexer rejected catalog source text.
    Lexical(DiagnosticKind),
    /// Catalog source text exceeds compact source-range storage.
    SourceTooLarge { bytes: usize },
    /// A non-EOF token did not match the catalog grammar.
    UnexpectedToken {
        expected: CatalogExpectation,
        actual: SyntaxKind,
    },
    /// The catalog source ended before a required grammar element.
    UnexpectedEndOfFile { expected: CatalogExpectation },
    /// The declared catalog family does not match the source inventory.
    CatalogKindMismatch {
        expected: CatalogKind,
        actual: CatalogKind,
    },
    /// A catalog family spelling is not recognized.
    UnknownCatalogKind { spelling: Arc<str> },
    /// An entry category spelling is not recognized.
    UnknownEntryKind { spelling: Arc<str> },
    /// An entry field is not part of the private catalog language.
    UnknownField {
        entry: CatalogEntryKind,
        spelling: Arc<str>,
    },
    /// A singleton entry field appears more than once.
    DuplicateField { field: CatalogField },
    /// A required entry field is absent.
    MissingField { field: CatalogField },
    /// An entry category is not supported by the catalog family.
    UnsupportedEntry {
        catalog: CatalogKind,
        entry: CatalogEntryKind,
    },
    /// A scope location is not supported by the catalog family.
    UnsupportedScopeLocation { catalog: CatalogKind },
    /// Contributions to one scope key disagree about location.
    ConflictingScopeLocation,
    /// A stable key appears more than once in one identity domain.
    DuplicateKey { domain: CatalogKeyDomain },
    /// A typed metadata spelling is not recognized.
    UnknownMetadata {
        metadata: CatalogMetadataKind,
        spelling: Arc<str>,
    },
    /// A declaration owner stable key does not exist.
    UnknownOwner,
    /// The declaration ownership graph contains a cycle.
    OwnershipCycle,
    /// The child declaration category is invalid for its resolved owner.
    InvalidDeclarationContext {
        owner: Option<CatalogDeclarationKind>,
        child: CatalogDeclarationKind,
    },
    /// A representation role was assigned to more than one compiler-known entry.
    DuplicateRepresentationRole { role: RepresentationRole },
    /// A representation role cannot describe the selected entry category.
    IncompatibleRepresentationRole {
        role: RepresentationRole,
        entry: CatalogEntryKind,
    },
    /// An implementation hook cannot describe the declaration category.
    IncompatibleImplementationHook {
        hook: ImplementationHook,
        declaration: CatalogDeclarationKind,
    },
    /// A recognized catalog entry attempted to define protected representation.
    RecognizedRepresentation,
    /// A descriptor identity domain exceeded compact ID storage.
    DescriptorLimitExceeded {
        domain: CatalogKeyDomain,
        count: usize,
    },
    /// Validated data could not be represented by the descriptor contract.
    DescriptorConstructionInvariant { domain: CatalogKeyDomain },
    /// An embedded Bray declaration fragment is malformed or recovered.
    InvalidDeclarationSurface,
    /// An embedded Bray type-expression fragment is malformed or recovered.
    InvalidTypeSurface,
}

/// One structured defect in checked-in catalog input.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CatalogDiagnostic {
    anchor: CatalogSourceAnchor,
    kind: CatalogDiagnosticKind,
    related_keys: Arc<[CatalogRelatedKey]>,
}

impl CatalogDiagnostic {
    /// Creates a diagnostic at an exact catalog source anchor.
    pub fn new(anchor: CatalogSourceAnchor, kind: CatalogDiagnosticKind) -> Self {
        Self {
            anchor,
            kind,
            related_keys: Arc::from([]),
        }
    }

    /// Adds typed related stable keys.
    pub(super) fn with_related_keys(
        mut self,
        related_keys: impl IntoIterator<Item = CatalogRelatedKey>,
    ) -> Self {
        self.related_keys = related_keys.into_iter().collect();
        self
    }

    /// Returns the exact catalog source location.
    pub const fn anchor(&self) -> CatalogSourceAnchor {
        self.anchor
    }

    /// Returns the locale-neutral error category and typed arguments.
    pub const fn kind(&self) -> &CatalogDiagnosticKind {
        &self.kind
    }

    /// Returns stable keys related to the defect.
    pub fn related_keys(&self) -> &[CatalogRelatedKey] {
        &self.related_keys
    }
}

/// Deterministically ordered catalog diagnostics.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CatalogDiagnostics(Arc<[CatalogDiagnostic]>);

impl CatalogDiagnostics {
    /// Creates a deterministically ordered, deduplicated diagnostic collection.
    pub fn new(diagnostics: impl IntoIterator<Item = CatalogDiagnostic>) -> Self {
        Self::from_unsorted(diagnostics.into_iter().collect())
    }

    pub(super) fn from_unsorted(mut diagnostics: Vec<CatalogDiagnostic>) -> Self {
        diagnostics.sort();
        diagnostics.dedup();
        Self(diagnostics.into())
    }

    /// Returns all diagnostics in deterministic source and category order.
    pub fn diagnostics(&self) -> &[CatalogDiagnostic] {
        &self.0
    }

    /// Returns whether no catalog defects were recorded.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl From<CatalogDiagnostic> for CatalogDiagnostics {
    fn from(diagnostic: CatalogDiagnostic) -> Self {
        Self(Arc::from([diagnostic]))
    }
}
