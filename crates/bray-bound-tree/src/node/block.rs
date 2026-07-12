use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{LocalConstantSymbolId, TypeId};

use crate::{BoundExpressionId, BoundNodeOrigin, BoundPatternId};

/// One source-ordered semantic item inside a bound block.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum BoundBlockItem {
    /// A local binding declaration.
    LocalBinding(BoundLocalBinding),
    /// A local constant declaration.
    LocalConstant(BoundLocalConstant),
    /// A sequenced, trailing, or generator expression.
    Expression(BoundExpressionId),
}

impl BoundBlockItem {
    pub(crate) const fn expression(&self) -> Option<BoundExpressionId> {
        match self {
            Self::LocalBinding(binding) => Some(binding.initializer()),
            Self::LocalConstant(constant) => Some(constant.initializer()),
            Self::Expression(expression) => Some(*expression),
        }
    }

    pub(crate) const fn pattern(&self) -> Option<BoundPatternId> {
        match self {
            Self::LocalBinding(binding) => Some(binding.pattern()),
            Self::LocalConstant(_) | Self::Expression(_) => None,
        }
    }
}

/// A checked local binding declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundLocalBinding {
    origin: BoundNodeOrigin,
    pattern: BoundPatternId,
    declared_type: Option<TypeId>,
    initializer: BoundExpressionId,
    is_recovered: bool,
}

impl BoundLocalBinding {
    /// Creates one checked or error-aware local binding declaration.
    pub const fn new(
        origin: BoundNodeOrigin,
        pattern: BoundPatternId,
        declared_type: Option<TypeId>,
        initializer: BoundExpressionId,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            pattern,
            declared_type,
            initializer,
            is_recovered,
        }
    }

    /// Returns the source or synthesized declaration origin.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the irrefutable binding pattern.
    pub const fn pattern(self) -> BoundPatternId {
        self.pattern
    }

    /// Returns the explicit declared type when one was present.
    pub const fn declared_type(self) -> Option<TypeId> {
        self.declared_type
    }

    /// Returns the bound initializer expression.
    pub const fn initializer(self) -> BoundExpressionId {
        self.initializer
    }

    /// Returns whether syntax or semantic recovery contributed to this declaration.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

/// A checked block-local constant declaration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BoundLocalConstant {
    origin: BoundNodeOrigin,
    symbol: Option<LocalConstantSymbolId>,
    declared_type: TypeId,
    initializer: BoundExpressionId,
    is_recovered: bool,
}

impl BoundLocalConstant {
    /// Creates one checked or error-aware block-local constant declaration.
    pub const fn new(
        origin: BoundNodeOrigin,
        symbol: Option<LocalConstantSymbolId>,
        declared_type: TypeId,
        initializer: BoundExpressionId,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            symbol,
            declared_type,
            initializer,
            is_recovered,
        }
    }

    /// Returns the source or synthesized declaration origin.
    pub const fn origin(self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the block-local constant identity.
    pub const fn symbol(self) -> Option<LocalConstantSymbolId> {
        self.symbol
    }

    /// Returns the explicit declared type.
    pub const fn declared_type(self) -> TypeId {
        self.declared_type
    }

    /// Returns the bound initializer expression.
    pub const fn initializer(self) -> BoundExpressionId {
        self.initializer
    }

    /// Returns whether syntax or semantic recovery contributed to this declaration.
    pub const fn is_recovered(self) -> bool {
        self.is_recovered
    }
}

/// A source-shaped block with items in deterministic evaluation order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundBlock {
    origin: BoundNodeOrigin,
    items: Arc<[BoundBlockItem]>,
    is_recovered: bool,
}

impl BoundBlock {
    /// Creates a block from checked items in source-semantic order.
    pub fn new(
        origin: BoundNodeOrigin,
        items: impl IntoIterator<Item = BoundBlockItem>,
        is_recovered: bool,
    ) -> Self {
        Self {
            origin,
            items: shared_slice(items),
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin of this block.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns block items in deterministic source-semantic order.
    pub fn items(&self) -> &[BoundBlockItem] {
        &self.items
    }

    /// Returns whether recovery was required while checking this block.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}
