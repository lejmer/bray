use std::sync::Arc;

use bray_base::{NonEmptySharedStr, shared_slice};

use crate::{CallableAbi, FunctionSymbolId};

/// Whether a native symbol enters or leaves the current Bray product.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ForeignCallableDirection {
    /// The callable body is supplied by another linked artifact.
    Import,
    /// The callable body is supplied by the current Bray product.
    Export,
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
    symbol: NonEmptySharedStr,
    links: Arc<[NativeLinkRequirement]>,
}

impl ForeignCallableContract {
    /// Creates one complete native boundary contract.
    pub fn new(
        callable: FunctionSymbolId,
        direction: ForeignCallableDirection,
        abi: CallableAbi,
        symbol: NonEmptySharedStr,
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

    /// Returns the exact native symbol spelling.
    pub fn symbol(&self) -> &str {
        self.symbol.as_str()
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
            symbol,
            [NativeLinkRequirement::new(library, NativeLinkKind::Dynamic)],
        );

        assert_eq!(contract.callable(), callable);
        assert_eq!(contract.direction(), ForeignCallableDirection::Import);
        assert_eq!(contract.abi(), CallableAbi::C);
        assert_eq!(contract.symbol(), "native_run");
        assert_eq!(contract.links()[0].name(), "runtime");
        assert_eq!(contract.links()[0].kind(), NativeLinkKind::Dynamic);
    }
}
