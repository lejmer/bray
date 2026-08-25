use bray_ir::MirRuntimeReference;
use bray_runtime_interface::{
    BinarySymbolName, ProtectedAsyncFrameId, ProtectedFrameOperation, RuntimeAbiRole,
};
use crate::{CodegenCallableSignature, CodegenInstanceKey, CodegenLinkage};

/// Runtime roles required by every native entry that invokes a Bray callable.
pub const FOREIGN_CALLBACK_RUNTIME_ROLES: [RuntimeAbiRole; 2] = [
    RuntimeAbiRole::ForeignCallbackExecution,
    RuntimeAbiRole::PanicReporting,
];

/// Exact binary spelling and linkage for a native entry into one Bray callable.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenNativeEntryMapping {
    name: BinarySymbolName,
    linkage: CodegenLinkage,
}

impl CodegenNativeEntryMapping {
    /// Creates one native entry mapping.
    pub const fn new(name: BinarySymbolName, linkage: CodegenLinkage) -> Self {
        Self { name, linkage }
    }

    /// Returns the exact binary symbol name.
    pub const fn name(&self) -> &BinarySymbolName {
        &self.name
    }

    /// Returns the selected binary linkage.
    pub const fn linkage(&self) -> CodegenLinkage {
        self.linkage
    }
}

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
    native_entry: Option<CodegenNativeEntryMapping>,
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
            native_entry: None,
        }
    }

    /// Adds the native entry through which external code invokes this Bray callable.
    pub fn with_native_entry(mut self, native_entry: CodegenNativeEntryMapping) -> Self {
        self.native_entry = Some(native_entry);

        self
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

    /// Returns the native entry when external code may invoke this Bray callable.
    pub const fn native_entry(&self) -> Option<&CodegenNativeEntryMapping> {
        self.native_entry.as_ref()
    }

    /// Consumes the mapping into its completed symbol contributions.
    pub fn into_parts(
        self,
    ) -> (
        CodegenSymbolKey,
        BinarySymbolName,
        CodegenLinkage,
        CodegenCallableSignature,
        Option<CodegenNativeEntryMapping>,
    ) {
        (
            self.key,
            self.name,
            self.linkage,
            self.signature,
            self.native_entry,
        )
    }
}
