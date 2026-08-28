use std::sync::Arc;

use bray_runtime_interface::{BinarySymbolName, ExecutableEntryResult};
use bray_symbols::{CallableExecution, ConstantValueId, StaticStorageDuration, SymbolKey};

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

/// One exact native relocation retained by a closed static initializer.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenStaticRelocation {
    value: ConstantValueId,
    instance: CodegenStaticInstanceKey,
    symbol: BinarySymbolName,
    ty: bray_symbols::TypeId,
}

impl CodegenStaticRelocation {
    /// Creates one initializer value to static-storage relocation.
    pub fn new(
        value: ConstantValueId,
        instance: CodegenStaticInstanceKey,
        symbol: BinarySymbolName,
        ty: bray_symbols::TypeId,
    ) -> Self {
        Self {
            value,
            instance,
            symbol,
            ty,
        }
    }

    /// Returns the initializer leaf represented by this relocation.
    pub const fn value(&self) -> ConstantValueId {
        self.value
    }

    /// Returns the exact target static instance.
    pub const fn instance(&self) -> &CodegenStaticInstanceKey {
        &self.instance
    }

    /// Returns the target storage symbol.
    pub const fn symbol(&self) -> &BinarySymbolName {
        &self.symbol
    }

    /// Returns the target storage type.
    pub const fn ty(&self) -> bray_symbols::TypeId {
        self.ty
    }
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
    native_binding: Option<bray_symbols::NativeSymbolBinding>,
    defines_storage: bool,
    initial_value: ConstantValueId,
    relocations: Arc<[CodegenStaticRelocation]>,
    finalization: Option<CodegenStaticFinalization>,
    destroy: Option<CodegenInstanceKey>,
}

/// One nontrivial static finalizer and its closed completion contract.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenStaticFinalization {
    execution: CallableExecution,
    instance: CodegenInstanceKey,
    result: ExecutableEntryResult,
    error_type_identity: Option<[u8; 32]>,
    source: Option<bray_ir::MirSourceAnchor>,
    incident_cleanup: Option<CodegenInstanceKey>,
    incident_memory: Option<CodegenStaticIncidentMemory>,
}

/// Bray allocation helpers retained for one fallible static-finalization incident.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenStaticIncidentMemory {
    allocation: CodegenInstanceKey,
    deallocation: CodegenInstanceKey,
}

impl CodegenStaticIncidentMemory {
    /// Creates one paired allocation and deallocation contract.
    pub const fn new(
        allocation: CodegenInstanceKey,
        deallocation: CodegenInstanceKey,
    ) -> Self {
        Self {
            allocation,
            deallocation,
        }
    }

    /// Returns the Bray allocation helper.
    pub const fn allocation(&self) -> &CodegenInstanceKey {
        &self.allocation
    }

    /// Returns the Bray deallocation helper.
    pub const fn deallocation(&self) -> &CodegenInstanceKey {
        &self.deallocation
    }
}

impl CodegenStaticFinalization {
    /// Creates one closed finalizer mapping.
    pub const fn new(
        execution: CallableExecution,
        instance: CodegenInstanceKey,
        result: ExecutableEntryResult,
        error_type_identity: Option<[u8; 32]>,
        source: Option<bray_ir::MirSourceAnchor>,
        incident_cleanup: Option<CodegenInstanceKey>,
        incident_memory: Option<CodegenStaticIncidentMemory>,
    ) -> Self {
        Self {
            execution,
            instance,
            result,
            error_type_identity,
            source,
            incident_cleanup,
            incident_memory,
        }
    }

    /// Returns whether finalization executes immediately or through a protected frame.
    pub const fn execution(&self) -> CallableExecution {
        self.execution
    }

    /// Returns the generated static-finalizer instance.
    pub const fn instance(&self) -> &CodegenInstanceKey {
        &self.instance
    }

    /// Returns the permitted finalizer completion shape.
    pub const fn result(&self) -> ExecutableEntryResult {
        self.result
    }

    /// Returns the concrete error type identity for a fallible completion.
    pub const fn error_type_identity(&self) -> Option<[u8; 32]> {
        self.error_type_identity
    }

    /// Returns the selected finalizer source location when locally available.
    pub const fn source(&self) -> Option<&bray_ir::MirSourceAnchor> {
        self.source.as_ref()
    }

    /// Returns abandonment cleanup for an owned incident payload.
    pub const fn incident_cleanup(&self) -> Option<&CodegenInstanceKey> {
        self.incident_cleanup.as_ref()
    }

    /// Returns the Bray memory helpers used for a fallible incident payload.
    pub const fn incident_memory(&self) -> Option<&CodegenStaticIncidentMemory> {
        self.incident_memory.as_ref()
    }
}

impl CodegenStaticStorageMapping {
    /// Creates one exact storage-use mapping.
    pub fn new(
        owner: CodegenInstanceKey,
        storage: bray_ir::MirStorageId,
        ty: bray_symbols::TypeId,
        instance: CodegenStaticInstanceKey,
        symbol: BinarySymbolName,
        native_binding: Option<bray_symbols::NativeSymbolBinding>,
        defines_storage: bool,
        initial_value: ConstantValueId,
        relocations: impl IntoIterator<Item = CodegenStaticRelocation>,
        finalization: Option<CodegenStaticFinalization>,
        destroy: Option<CodegenInstanceKey>,
    ) -> Self {
        Self {
            owner,
            storage,
            ty,
            instance,
            symbol,
            native_binding,
            defines_storage,
            initial_value,
            relocations: bray_base::shared_slice(relocations),
            finalization,
            destroy,
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

    /// Returns the native export strength selected for this storage.
    pub const fn native_binding(&self) -> Option<bray_symbols::NativeSymbolBinding> {
        self.native_binding
    }

    /// Returns whether this unit owns the storage definition.
    pub const fn defines_storage(&self) -> bool {
        self.defines_storage
    }

    /// Returns the product-formed constant value stored before execution begins.
    pub const fn initial_value(&self) -> ConstantValueId {
        self.initial_value
    }

    /// Returns exact native address relocations retained by the initializer.
    pub fn relocations(&self) -> &[CodegenStaticRelocation] {
        &self.relocations
    }

    /// Returns the relocation represented by one initializer value.
    pub fn relocation(&self, value: ConstantValueId) -> Option<&CodegenStaticRelocation> {
        self.relocations
            .binary_search_by_key(&value, CodegenStaticRelocation::value)
            .ok()
            .and_then(|index| self.relocations.get(index))
    }

    /// Returns graceful finalization for this storage when its type defines it.
    pub const fn finalization(&self) -> Option<&CodegenStaticFinalization> {
        self.finalization.as_ref()
    }

    /// Returns destruction for this storage when its representation requires it.
    pub const fn destroy(&self) -> Option<&CodegenInstanceKey> {
        self.destroy.as_ref()
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

    /// Returns the cleanup preparation entry paired with this storage symbol.
    pub fn prepare_name(&self) -> String {
        format!("{}.prepare", self.symbol.as_str())
    }

    /// Returns the finalization entry paired with this storage symbol.
    pub fn finalize_name(&self) -> String {
        format!("{}.finalize", self.symbol.as_str())
    }

    /// Returns the destruction entry paired with this storage symbol.
    pub fn destroy_name(&self) -> String {
        format!("{}.destroy", self.symbol.as_str())
    }

    /// Returns the exact-thread detach entry paired with this storage symbol.
    pub fn detach_name(&self) -> String {
        format!("{}.detach", self.symbol.as_str())
    }
}
