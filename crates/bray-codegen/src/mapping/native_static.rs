use bray_runtime_interface::BinarySymbolName;
use bray_symbols::{
    ForeignCallableDirection, NativeSymbolBinding, NativeSymbolPresence, StaticStorageDuration,
};

use crate::CodegenInstanceKey;

/// One native data-symbol address used by a concrete MIR instance.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenNativeStaticMapping {
    owner: CodegenInstanceKey,
    storage: bray_ir::MirStorageId,
    pointer_type: bray_symbols::TypeId,
    pointee_type: bray_symbols::TypeId,
    symbol: BinarySymbolName,
    direction: ForeignCallableDirection,
    binding: NativeSymbolBinding,
    presence: NativeSymbolPresence,
    duration: StaticStorageDuration,
}

impl CodegenNativeStaticMapping {
    /// Creates one complete native data-symbol use mapping.
    #[expect(
        clippy::too_many_arguments,
        reason = "the mapping retains each independently checked native storage contract field"
    )]
    pub const fn new(
        owner: CodegenInstanceKey,
        storage: bray_ir::MirStorageId,
        pointer_type: bray_symbols::TypeId,
        pointee_type: bray_symbols::TypeId,
        symbol: BinarySymbolName,
        direction: ForeignCallableDirection,
        binding: NativeSymbolBinding,
        presence: NativeSymbolPresence,
        duration: StaticStorageDuration,
    ) -> Self {
        Self {
            owner,
            storage,
            pointer_type,
            pointee_type,
            symbol,
            direction,
            binding,
            presence,
            duration,
        }
    }

    /// Returns the concrete MIR instance containing this use.
    pub const fn owner(&self) -> &CodegenInstanceKey {
        &self.owner
    }

    /// Returns the unit-local native static storage root.
    pub const fn storage(&self) -> bray_ir::MirStorageId {
        self.storage
    }

    /// Returns the raw pointer type exposed to Bray code.
    pub const fn pointer_type(&self) -> bray_symbols::TypeId {
        self.pointer_type
    }

    /// Returns the provider-owned stored type.
    pub const fn pointee_type(&self) -> bray_symbols::TypeId {
        self.pointee_type
    }

    /// Returns the exact binary symbol spelling.
    pub const fn symbol(&self) -> &BinarySymbolName {
        &self.symbol
    }

    /// Returns whether this data symbol is imported or exported.
    pub const fn direction(&self) -> ForeignCallableDirection {
        self.direction
    }

    /// Returns the native link-selection strength.
    pub const fn binding(&self) -> NativeSymbolBinding {
        self.binding
    }

    /// Returns whether product formation requires the symbol.
    pub const fn presence(&self) -> NativeSymbolPresence {
        self.presence
    }

    /// Returns the provider storage owner domain.
    pub const fn duration(&self) -> StaticStorageDuration {
        self.duration
    }
}
