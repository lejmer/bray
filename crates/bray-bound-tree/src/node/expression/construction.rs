use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{SymbolName, TypeId};

use crate::{BoundExpressionId, BoundNodeOrigin};

/// One named field initializer in a struct construction.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundStructFieldInitializer {
    name: Option<SymbolName>,
    expression: BoundExpressionId,
    is_recovered: bool,
}

impl BoundStructFieldInitializer {
    /// Creates one source field initializer.
    pub const fn new(
        name: Option<SymbolName>,
        expression: BoundExpressionId,
        is_recovered: bool,
    ) -> Self {
        Self {
            name,
            expression,
            is_recovered,
        }
    }

    /// Returns the canonical field name when present.
    pub const fn name(&self) -> Option<&SymbolName> {
        self.name.as_ref()
    }

    /// Returns the field value expression.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns whether recovery contributed to this initializer.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

/// A struct construction with an optional explicit type head.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BoundStructConstructionExpression {
    origin: BoundNodeOrigin,
    head: Option<BoundExpressionId>,
    fields: Arc<[BoundStructFieldInitializer]>,
    operands: Arc<[BoundExpressionId]>,
    ty: Option<TypeId>,
    is_recovered: bool,
}

impl BoundStructConstructionExpression {
    /// Creates a struct construction preserving field associations.
    pub fn new(
        origin: BoundNodeOrigin,
        head: Option<BoundExpressionId>,
        fields: impl IntoIterator<Item = BoundStructFieldInitializer>,
        ty: Option<TypeId>,
        is_recovered: bool,
    ) -> Self {
        let fields = shared_slice(fields);
        let operands = shared_slice(
            head.into_iter()
                .chain(fields.iter().map(BoundStructFieldInitializer::expression)),
        );

        Self {
            origin,
            head,
            fields,
            operands,
            ty,
            is_recovered,
        }
    }

    /// Returns the source or synthesized origin.
    pub const fn origin(&self) -> BoundNodeOrigin {
        self.origin
    }

    /// Returns the explicit type head when present.
    pub const fn head(&self) -> Option<BoundExpressionId> {
        self.head
    }

    /// Returns field initializers in source order.
    pub fn fields(&self) -> &[BoundStructFieldInitializer] {
        &self.fields
    }

    /// Returns the resolved type when available.
    pub const fn ty(&self) -> Option<TypeId> {
        self.ty
    }

    /// Returns whether recovery contributed to this construction.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }

    pub(crate) fn operands(&self) -> &[BoundExpressionId] {
        &self.operands
    }
}
