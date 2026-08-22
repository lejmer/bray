use std::sync::Arc;

use bray_base::{NonEmptySharedStr, shared_slice};

use crate::{CallableAbi, FunctionSymbolId, StaticSymbolId};

/// Whether a native symbol enters or leaves the current Bray product.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ForeignCallableDirection {
    /// The callable body is supplied by another linked artifact.
    Import,
    /// The callable body is supplied by the current Bray product.
    Export,
}

/// One native symbol identity selected independently of its resolution policy.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeSymbolIdentity {
    /// An exact target symbol spelling.
    Name(NonEmptySharedStr),
    /// A target-supported symbol ordinal.
    Ordinal(u64),
}

impl NativeSymbolIdentity {
    /// Returns the exact target symbol spelling when this identity is name based.
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Name(name) => Some(name.as_str()),
            Self::Ordinal(_) => None,
        }
    }

    /// Returns the target symbol ordinal when this identity is ordinal based.
    pub const fn ordinal(&self) -> Option<u64> {
        match self {
            Self::Name(_) => None,
            Self::Ordinal(ordinal) => Some(*ordinal),
        }
    }
}

/// Native link-selection strength for one symbol.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeSymbolBinding {
    /// Ordinary strong symbol selection.
    Strong,
    /// Target weak symbol selection.
    Weak,
}

/// Whether native product formation requires one imported symbol to resolve.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeSymbolPresence {
    /// Product formation requires the symbol.
    Required,
    /// An unresolved imported data symbol produces a null address.
    Optional,
}

/// Target symbol identity and resolution policy selected by `@symbol(...)`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeSymbolContract {
    identity: NativeSymbolIdentity,
    version: Option<NonEmptySharedStr>,
    binding: NativeSymbolBinding,
    presence: NativeSymbolPresence,
}

impl NativeSymbolContract {
    /// Creates one validated native symbol contract.
    pub const fn new(
        identity: NativeSymbolIdentity,
        version: Option<NonEmptySharedStr>,
        binding: NativeSymbolBinding,
        presence: NativeSymbolPresence,
    ) -> Self {
        Self {
            identity,
            version,
            binding,
            presence,
        }
    }

    /// Creates the ordinary strong and required contract for one exact name.
    pub const fn required_name(name: NonEmptySharedStr) -> Self {
        Self::new(
            NativeSymbolIdentity::Name(name),
            None,
            NativeSymbolBinding::Strong,
            NativeSymbolPresence::Required,
        )
    }

    /// Returns the exact target symbol identity.
    pub const fn identity(&self) -> &NativeSymbolIdentity {
        &self.identity
    }

    /// Returns the optional target symbol version.
    pub fn version(&self) -> Option<&str> {
        self.version.as_ref().map(NonEmptySharedStr::as_str)
    }

    /// Returns the native link-selection strength.
    pub const fn binding(&self) -> NativeSymbolBinding {
        self.binding
    }

    /// Returns whether product formation requires the symbol to resolve.
    pub const fn presence(&self) -> NativeSymbolPresence {
        self.presence
    }
}

/// A language-defined native link-input category.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum NativeLinkKind {
    /// A dynamically linked library.
    Dynamic,
    /// A statically linked library or archive.
    Static,
    /// A system library selected by the target toolchain.
    System,
    /// A target platform framework.
    Framework,
}

impl NativeLinkKind {
    /// Returns the canonical language spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Dynamic => "dynamic",
            Self::Static => "static",
            Self::System => "system",
            Self::Framework => "framework",
        }
    }

    /// Returns the link kind with the supplied canonical language spelling.
    pub fn for_name(name: &str) -> Option<Self> {
        match name {
            "dynamic" => Some(Self::Dynamic),
            "static" => Some(Self::Static),
            "system" => Some(Self::System),
            "framework" => Some(Self::Framework),
            _ => None,
        }
    }
}

/// One validated native artifact requirement selected by `@link(...)`.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NativeLinkRequirement {
    name: NonEmptySharedStr,
    kind: NativeLinkKind,
}

impl NativeLinkRequirement {
    /// Creates one requirement from a nonempty native artifact name and link category.
    pub const fn new(name: NonEmptySharedStr, kind: NativeLinkKind) -> Self {
        Self { name, kind }
    }

    /// Returns the canonical native artifact name.
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Returns the requested native link category.
    pub const fn kind(&self) -> NativeLinkKind {
        self.kind
    }
}

/// The validated native boundary contract of one source function.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ForeignCallableContract {
    callable: FunctionSymbolId,
    direction: ForeignCallableDirection,
    abi: CallableAbi,
    symbol: NativeSymbolContract,
    links: Arc<[NativeLinkRequirement]>,
}

impl ForeignCallableContract {
    /// Creates one complete native boundary contract.
    pub fn new(
        callable: FunctionSymbolId,
        direction: ForeignCallableDirection,
        abi: CallableAbi,
        symbol: NativeSymbolContract,
        links: impl IntoIterator<Item = NativeLinkRequirement>,
    ) -> Self {
        Self {
            callable,
            direction,
            abi,
            symbol,
            links: shared_slice(links),
        }
    }

    /// Returns the source function that owns this boundary contract.
    pub const fn callable(&self) -> FunctionSymbolId {
        self.callable
    }

    /// Returns whether the native symbol is imported or exported.
    pub const fn direction(&self) -> ForeignCallableDirection {
        self.direction
    }

    /// Returns the selected foreign callable ABI.
    pub const fn abi(&self) -> CallableAbi {
        self.abi
    }

    /// Returns the exact native symbol identity and resolution policy.
    pub const fn symbol(&self) -> &NativeSymbolContract {
        &self.symbol
    }

    /// Returns native artifact requirements in source order.
    pub fn links(&self) -> &[NativeLinkRequirement] {
        &self.links
    }
}

/// The validated native boundary contract of one static declaration.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ForeignStaticContract {
    declaration: StaticSymbolId,
    direction: ForeignCallableDirection,
    symbol: NativeSymbolContract,
    mutable: bool,
    links: Arc<[NativeLinkRequirement]>,
}

impl ForeignStaticContract {
    /// Creates one complete native data boundary contract.
    pub fn new(
        declaration: StaticSymbolId,
        direction: ForeignCallableDirection,
        symbol: NativeSymbolContract,
        mutable: bool,
        links: impl IntoIterator<Item = NativeLinkRequirement>,
    ) -> Self {
        Self {
            declaration,
            direction,
            symbol,
            mutable,
            links: shared_slice(links),
        }
    }

    /// Returns the static declaration that owns this boundary contract.
    pub const fn declaration(&self) -> StaticSymbolId {
        self.declaration
    }

    /// Returns whether the native data symbol is imported or exported.
    pub const fn direction(&self) -> ForeignCallableDirection {
        self.direction
    }

    /// Returns the exact native symbol identity and resolution policy.
    pub const fn symbol(&self) -> &NativeSymbolContract {
        &self.symbol
    }

    /// Returns whether the native storage contract permits mutation.
    pub const fn is_mutable(&self) -> bool {
        self.mutable
    }

    /// Returns native artifact requirements in source order.
    pub fn links(&self) -> &[NativeLinkRequirement] {
        &self.links
    }
}

#[cfg(test)]
mod tests {
    use bray_base::NonEmptySharedStr;

    use super::{
        ForeignCallableContract, ForeignCallableDirection, NativeLinkKind, NativeLinkRequirement,
    };
    use crate::{CallableAbi, FunctionSymbolId, SymbolId};

    #[test]
    fn foreign_callable_contracts_preserve_typed_boundary_inputs() {
        let callable = FunctionSymbolId::from_symbol_id(SymbolId::new(3));

        let Some(symbol) = NonEmptySharedStr::try_new("native_run") else {
            panic!("test symbol name must be valid");
        };

        let Some(library) = NonEmptySharedStr::try_new("runtime") else {
            panic!("test library name must be valid");
        };

        let contract = ForeignCallableContract::new(
            callable,
            ForeignCallableDirection::Import,
            CallableAbi::C,
            super::NativeSymbolContract::required_name(symbol),
            [NativeLinkRequirement::new(library, NativeLinkKind::Dynamic)],
        );

        assert_eq!(contract.callable(), callable);
        assert_eq!(contract.direction(), ForeignCallableDirection::Import);
        assert_eq!(contract.abi(), CallableAbi::C);
        assert_eq!(contract.symbol().identity().name(), Some("native_run"));
        assert_eq!(contract.links()[0].name(), "runtime");
        assert_eq!(contract.links()[0].kind(), NativeLinkKind::Dynamic);
    }
}
