use bray_diagnostics::DiagnosticBag;

use crate::chunk::DeclarationChunk;
use crate::table::DeclarationTable;

/// Immutable declarations and diagnostics discovered from one source unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationChunkResult {
    chunk: DeclarationChunk,
    diagnostics: DiagnosticBag,
}

impl DeclarationChunkResult {
    pub(crate) const fn new(chunk: DeclarationChunk, diagnostics: DiagnosticBag) -> Self {
        Self { chunk, diagnostics }
    }

    /// Returns the source-unit declaration chunk.
    pub const fn chunk(&self) -> &DeclarationChunk {
        &self.chunk
    }

    /// Returns diagnostics produced while discovering this source unit.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Consumes the result and returns its declaration chunk and diagnostics.
    pub fn into_parts(self) -> (DeclarationChunk, DiagnosticBag) {
        (self.chunk, self.diagnostics)
    }
}

/// Immutable merged declaration table and declaration-discovery diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeclarationTableResult {
    table: DeclarationTable,
    diagnostics: DiagnosticBag,
}

impl DeclarationTableResult {
    pub(crate) const fn new(table: DeclarationTable, diagnostics: DiagnosticBag) -> Self {
        Self { table, diagnostics }
    }

    /// Returns the merged declaration table.
    pub const fn table(&self) -> &DeclarationTable {
        &self.table
    }

    /// Returns diagnostics produced across declaration discovery and merge.
    pub const fn diagnostics(&self) -> &DiagnosticBag {
        &self.diagnostics
    }

    /// Consumes the result and returns its declaration table and diagnostics.
    pub fn into_parts(self) -> (DeclarationTable, DiagnosticBag) {
        (self.table, self.diagnostics)
    }
}

#[cfg(test)]
mod tests {
    use super::{DeclarationChunkResult, DeclarationTableResult};

    #[test]
    fn declaration_results_are_send_and_sync() {
        fn assert_send_sync<T: Send + Sync>() {}

        assert_send_sync::<DeclarationChunkResult>();
        assert_send_sync::<DeclarationTableResult>();
    }
}
