use crate::{CodegenCallableSignature, CodegenInstanceKey, CodegenLinkage, CodegenUnitKey};
use bray_ir::MirRuntimeReference;
use bray_runtime_interface::{
    BinarySymbolName, ProtectedAsyncFrameId, ProtectedFrameOperation, RuntimeAbiRole,
};

/// Runtime roles required by every native entry that invokes a Bray callable.
pub const FOREIGN_CALLBACK_RUNTIME_ROLES: [RuntimeAbiRole; 2] = [
    RuntimeAbiRole::ForeignCallbackExecution,
    RuntimeAbiRole::PanicReporting,
];

/// The authority and execution boundary established by a generated native entry.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodegenNativeEntryKind {
    /// Enter an ordinary foreign callback through the runtime attachment service.
    ForeignCallback,
    /// Finish an existing exact-thread owner without recursively attaching.
    ThreadStorageExit,
    /// Contain bootstrap failures and return the native runtime failure status.
    RuntimeStatus,
}

impl CodegenNativeEntryKind {
    /// Selects the non-attaching entry boundary required by a bootstrap role.
    pub const fn for_runtime_role(role: RuntimeAbiRole) -> Option<Self> {
        match role {
            RuntimeAbiRole::ThreadStorageExit => Some(Self::ThreadStorageExit),
            RuntimeAbiRole::RuntimeInitialization
            | RuntimeAbiRole::ThreadAttachmentEnter
            | RuntimeAbiRole::ThreadAttachmentLeave
            | RuntimeAbiRole::ThreadAttachmentMarkMain
            | RuntimeAbiRole::ThreadAttachmentCleanup
            | RuntimeAbiRole::StructuredShutdown => Some(Self::RuntimeStatus),
            _ => None,
        }
    }
}

/// Exact binary spelling, linkage, and entry contract for one native callback.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenNativeEntryMapping {
    name: BinarySymbolName,
    linkage: CodegenLinkage,
    kind: CodegenNativeEntryKind,
}

impl CodegenNativeEntryMapping {
    /// Creates one native entry mapping.
    pub const fn new(name: BinarySymbolName, linkage: CodegenLinkage) -> Self {
        Self {
            name,
            linkage,
            kind: CodegenNativeEntryKind::ForeignCallback,
        }
    }

    /// Selects the validated native entry contract.
    pub const fn with_kind(mut self, kind: CodegenNativeEntryKind) -> Self {
        self.kind = kind;

        self
    }

    /// Returns the validated native entry contract.
    pub const fn kind(&self) -> CodegenNativeEntryKind {
        self.kind
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
    /// Concrete cleanup backing layout emitted beside an asynchronous callable.
    CleanupStorage(CodegenInstanceKey),
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

    /// Whether this unit emits the mapped definition rather than a reference.
    pub fn defines_in(&self, unit: &CodegenUnitKey) -> bool {
        match &self.key {
            CodegenSymbolKey::Instance(instance) | CodegenSymbolKey::CleanupStorage(instance) => {
                unit.instances().contains(instance) && self.linkage != CodegenLinkage::Import
            }
            CodegenSymbolKey::Runtime(_) | CodegenSymbolKey::ProtectedFrame { .. } => false,
        }
    }

    /// Returns the complete selected machine signature.
    pub const fn signature(&self) -> &CodegenCallableSignature {
        &self.signature
    }

    /// Returns the native entry when external code may invoke this Bray callable.
    pub const fn native_entry(&self) -> Option<&CodegenNativeEntryMapping> {
        self.native_entry.as_ref()
    }

    /// Returns the symbol whose address crosses a native callable boundary.
    pub const fn callable_address_name(&self) -> &BinarySymbolName {
        match &self.native_entry {
            Some(entry) => entry.name(),
            None => &self.name,
        }
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
