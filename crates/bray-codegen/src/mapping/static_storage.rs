use std::sync::Arc;

use bray_runtime_interface::BinarySymbolName;
use bray_symbols::{ConstantValueId, StaticStorageDuration, SymbolKey};

use crate::{CodegenImplementationWitness, CodegenInstanceKey, CodegenSpecialization};

/// One implementation selected for one ordered static requirement.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenStaticWitness {
    requirement: SymbolKey,
    implementation: CodegenImplementationWitness,
}

impl CodegenStaticWitness {
    /// Creates one requirement-to-implementation selection.
    pub const fn new(requirement: SymbolKey, implementation: CodegenImplementationWitness) -> Self {
        Self {
            requirement,
            implementation,
        }
    }

    /// Returns the required trait identity.
    pub const fn requirement(&self) -> &SymbolKey {
        &self.requirement
    }

    /// Returns the selected concrete implementation.
    pub const fn implementation(&self) -> &CodegenImplementationWitness {
        &self.implementation
    }
}

/// Stable same-product identity of one closed Bray-owned static realization.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenStaticInstanceKey {
    declaration: SymbolKey,
    specialization: CodegenSpecialization,
    witnesses: Arc<[CodegenStaticWitness]>,
    target: bray_ir::MirTargetContract,
    duration: StaticStorageDuration,
}

impl CodegenStaticInstanceKey {
    /// Creates one canonical target realization identity.
    pub fn new(
        declaration: SymbolKey,
        specialization: CodegenSpecialization,
        witnesses: impl IntoIterator<Item = CodegenStaticWitness>,
        target: bray_ir::MirTargetContract,
        duration: StaticStorageDuration,
    ) -> Self {
        Self {
            declaration,
            specialization,
            witnesses: bray_base::shared_slice(witnesses),
            target,
            duration,
        }
    }

    /// Returns the portable declaration identity.
    pub const fn declaration(&self) -> &SymbolKey {
        &self.declaration
    }

    /// Returns the normalized closed generic specialization.
    pub const fn specialization(&self) -> &CodegenSpecialization {
        &self.specialization
    }

    /// Returns selected implementations in canonical requirement order.
    pub fn witnesses(&self) -> &[CodegenStaticWitness] {
        &self.witnesses
    }

    /// Returns the target profile participating in identity.
    pub const fn target(&self) -> &bray_ir::MirTargetContract {
        &self.target
    }

    /// Returns the storage owner domain.
    pub const fn duration(&self) -> StaticStorageDuration {
        self.duration
    }
}

/// Native realization selected for one static storage use in a concrete MIR instance.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenStaticStorageMapping {
    owner: CodegenInstanceKey,
    storage: bray_ir::MirStorageId,
    ty: bray_symbols::TypeId,
    instance: CodegenStaticInstanceKey,
    symbol: BinarySymbolName,
    initial_value: ConstantValueId,
    cleanup: Option<CodegenInstanceKey>,
}

impl CodegenStaticStorageMapping {
    /// Creates one exact storage-use mapping.
    pub const fn new(
        owner: CodegenInstanceKey,
        storage: bray_ir::MirStorageId,
        ty: bray_symbols::TypeId,
        instance: CodegenStaticInstanceKey,
        symbol: BinarySymbolName,
        initial_value: ConstantValueId,
        cleanup: Option<CodegenInstanceKey>,
    ) -> Self {
        Self {
            owner,
            storage,
            ty,
            instance,
            symbol,
            initial_value,
            cleanup,
        }
    }

    /// Returns the concrete MIR instance containing the use.
    pub const fn owner(&self) -> &CodegenInstanceKey {
        &self.owner
    }

    /// Returns the unit-local static storage root.
    pub const fn storage(&self) -> bray_ir::MirStorageId {
        self.storage
    }

    /// Returns the MIR template type stored by this instance.
    pub const fn ty(&self) -> bray_symbols::TypeId {
        self.ty
    }

    /// Returns the canonical same-product static identity.
    pub const fn instance(&self) -> &CodegenStaticInstanceKey {
        &self.instance
    }

    /// Returns the deterministic internal realization symbol.
    pub const fn symbol(&self) -> &BinarySymbolName {
        &self.symbol
    }

    /// Returns the product-formed constant value stored before execution begins.
    pub const fn initial_value(&self) -> ConstantValueId {
        self.initial_value
    }

    /// Returns lifecycle resolution for this storage when its type owns cleanup work.
    pub const fn cleanup(&self) -> Option<&CodegenInstanceKey> {
        self.cleanup.as_ref()
    }

    /// Returns the coalesced accessor symbol paired with this storage symbol.
    pub fn accessor_name(&self) -> String {
        format!("{}.access", self.symbol.as_str())
    }

    /// Returns the linked host-table record paired with this static realization.
    pub fn host_name(&self) -> String {
        format!("bray.static.host.{}", self.symbol.as_str())
    }

    /// Returns the attachment identity cell paired with exact-thread storage.
    pub fn attachment_name(&self) -> String {
        format!("{}.attachment", self.symbol.as_str())
    }

    /// Returns the attachment cleanup entry paired with exact-thread storage.
    pub fn cleanup_name(&self) -> String {
        format!("{}.cleanup", self.symbol.as_str())
    }
}
