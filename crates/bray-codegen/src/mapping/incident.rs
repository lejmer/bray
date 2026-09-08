use bray_ir::MirSourceAnchor;
use bray_symbols::TypeId;

use crate::CodegenInstanceKey;

/// Concrete ownership and reporting contract for one erased cleanup error.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenCleanupIncident {
    ty: TypeId,
    type_identity: [u8; 32],
    source: Option<MirSourceAnchor>,
    cleanup: CodegenInstanceKey,
    allocation: CodegenInstanceKey,
    deallocation: CodegenInstanceKey,
}

impl CodegenCleanupIncident {
    /// Creates the complete payload contract with its paired memory and cleanup operations.
    pub const fn new(
        ty: TypeId,
        type_identity: [u8; 32],
        source: Option<MirSourceAnchor>,
        cleanup: CodegenInstanceKey,
        allocation: CodegenInstanceKey,
        deallocation: CodegenInstanceKey,
    ) -> Self {
        Self {
            ty,
            type_identity,
            source,
            cleanup,
            allocation,
            deallocation,
        }
    }

    /// Returns the concrete error type.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }

    /// Returns the stable identity retained by the erased payload.
    pub const fn type_identity(&self) -> [u8; 32] {
        self.type_identity
    }

    /// Returns the cleanup source location when available.
    pub const fn source(&self) -> Option<&MirSourceAnchor> {
        self.source.as_ref()
    }

    /// Returns abandonment cleanup for the owned payload.
    pub const fn cleanup(&self) -> &CodegenInstanceKey {
        &self.cleanup
    }

    /// Returns the Bray allocation operation.
    pub const fn allocation(&self) -> &CodegenInstanceKey {
        &self.allocation
    }

    /// Returns the matching Bray deallocation operation.
    pub const fn deallocation(&self) -> &CodegenInstanceKey {
        &self.deallocation
    }

    /// Returns every callable required to own and release the erased payload.
    pub const fn dependencies(&self) -> [&CodegenInstanceKey; 3] {
        [&self.cleanup, &self.allocation, &self.deallocation]
    }
}
