use std::path::PathBuf;

use bray_source::SourceInputKind;

/// Exact origin selected for one compiler source input.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticSourceInputOrigin {
    /// Host filesystem path selected by a file input.
    File(PathBuf),
    /// Stable name selected by a virtual or generated input.
    Name(String),
    /// Document URI selected by an editor input.
    Uri(String),
    /// The rejected input did not provide the identity required by its category.
    Missing,
}

/// Complete source-input identity used by load and request diagnostics.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DiagnosticSourceInput {
    index: u64,
    kind: SourceInputKind,
    origin: DiagnosticSourceInputOrigin,
}

impl DiagnosticSourceInput {
    /// Creates source-input context from its request position, category, and exact origin.
    pub const fn new(
        index: u64,
        kind: SourceInputKind,
        origin: DiagnosticSourceInputOrigin,
    ) -> Self {
        Self {
            index,
            kind,
            origin,
        }
    }

    /// Returns the zero-based position in the compiler request.
    pub const fn index(&self) -> u64 {
        self.index
    }

    /// Returns the stable source-input category.
    pub const fn kind(&self) -> SourceInputKind {
        self.kind
    }

    /// Returns the exact selected origin or missing-origin marker.
    pub const fn origin(&self) -> &DiagnosticSourceInputOrigin {
        &self.origin
    }
}
