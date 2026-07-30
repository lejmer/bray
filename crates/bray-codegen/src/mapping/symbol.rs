use bray_ir::MirRuntimeReference;
use bray_runtime_interface::{BinarySymbolName, ProtectedAsyncFrameId, ProtectedFrameOperation};

use crate::{CodegenCallableSignature, CodegenInstanceKey, CodegenLinkage};

/// Stable semantic identity of one binary definition or reference.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenSymbolKey {
    /// One concrete Bray definition.
    Instance(CodegenInstanceKey),
    /// One external runtime ABI role.
    Runtime(MirRuntimeReference),
    /// One compiler-generated protected-frame operation.
    ProtectedFrame {
        /// Concrete protected-frame identity.
        frame: ProtectedAsyncFrameId,
        /// Operation emitted for the frame descriptor.
        operation: ProtectedFrameOperation,
    },
}

/// Exact binary spelling, linkage, and machine signature selected for one symbol.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenSymbolMapping {
    key: CodegenSymbolKey,
    name: BinarySymbolName,
    linkage: CodegenLinkage,
    signature: CodegenCallableSignature,
}

impl CodegenSymbolMapping {
    /// Creates one completed target symbol mapping.
    pub const fn new(
        key: CodegenSymbolKey,
        name: BinarySymbolName,
        linkage: CodegenLinkage,
        signature: CodegenCallableSignature,
    ) -> Self {
        Self {
            key,
            name,
            linkage,
            signature,
        }
    }

    /// Returns the semantic symbol identity.
    pub const fn key(&self) -> &CodegenSymbolKey {
        &self.key
    }

    /// Returns the exact binary symbol name.
    pub const fn name(&self) -> &BinarySymbolName {
        &self.name
    }

    /// Returns the selected binary linkage.
    pub const fn linkage(&self) -> CodegenLinkage {
        self.linkage
    }

    /// Returns the complete selected machine signature.
    pub const fn signature(&self) -> &CodegenCallableSignature {
        &self.signature
    }

    /// Consumes the mapping into its completed symbol contributions.
    pub fn into_parts(
        self,
    ) -> (
        CodegenSymbolKey,
        BinarySymbolName,
        CodegenLinkage,
        CodegenCallableSignature,
    ) {
        (self.key, self.name, self.linkage, self.signature)
    }
}
