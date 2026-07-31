use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::TypeId;

use crate::{BoundExpressionId, BoundUnitId, BoundUnitKind};

/// Whether an address operation exposes shared or mutable storage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MemoryAddressKind {
    /// Form a raw pointer from shared storage.
    Shared,
    /// Form a raw pointer from mutable storage.
    Mutable,
}

/// Unit used by a checked raw-pointer offset.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MemoryOffsetUnit {
    /// Offset by elements of the pointer's element type.
    Element,
    /// Offset by bytes.
    Byte,
}

/// Ownership effect of reading a value from raw storage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MemoryReadKind {
    /// Copy the value while preserving the source initialization fact.
    Copy,
    /// Move the value and invalidate the source initialization fact.
    Move,
}

/// Whether a representation-level memory copy permits overlap.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MemoryCopyKind {
    /// Source and destination ranges must not overlap.
    NonOverlapping,
    /// Source and destination ranges may overlap.
    Overlapping,
}

/// Target-layout value requested by a compiler-provided memory declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MemoryLayoutQueryKind {
    /// Size of one initialized value in bytes.
    Size,
    /// Required alignment of a value in bytes.
    Alignment,
    /// Distance between adjacent values in contiguous storage.
    Stride,
    /// Allocation layout for a requested element count.
    Layout,
}

/// Checked compiler-provided memory behavior at one call expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedMemoryOperationKind {
    /// Form a raw pointer from an ordinary storage reference.
    Address {
        /// Authority exposed by the source reference.
        kind: MemoryAddressKind,
        /// Pointed-to value type.
        pointee: TypeId,
    },
    /// Produce a null raw pointer.
    Null {
        /// Pointed-to value type.
        pointee: TypeId,
    },
    /// Test a raw pointer for null.
    IsNull {
        /// Pointed-to value type.
        pointee: TypeId,
    },
    /// Offset a raw pointer.
    Offset {
        /// Offset unit selected by the declaration.
        unit: MemoryOffsetUnit,
        /// Pointed-to value type.
        pointee: TypeId,
    },
    /// Reinterpret a raw pointer's element type.
    Reinterpret {
        /// Source element type.
        source: TypeId,
        /// Target element type.
        target: TypeId,
    },
    /// Read one value from raw storage.
    Read {
        /// Read value type.
        pointee: TypeId,
        /// Ownership effect selected from the type's copy contract.
        kind: MemoryReadKind,
    },
    /// Write one value into raw storage and establish its initialization fact.
    Write {
        /// Written value type.
        pointee: TypeId,
    },
    /// Copy a representation-level range and establish destination initialization facts.
    Copy {
        /// Copied element type.
        pointee: TypeId,
        /// Checked overlap policy.
        kind: MemoryCopyKind,
    },
    /// Request one selected-target layout value.
    LayoutQuery {
        /// Queried value type.
        ty: TypeId,
        /// Requested layout property.
        kind: MemoryLayoutQueryKind,
    },
    /// Create a distinct owned writable allocation.
    Allocate,
    /// Release an allocation and invalidate its dependent facts.
    Deallocate,
}

/// One source-correlated checked memory operation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedMemoryOperation {
    expression: BoundExpressionId,
    kind: CheckedMemoryOperationKind,
    arguments: Arc<[BoundExpressionId]>,
}

impl CheckedMemoryOperation {
    /// Creates one checked memory operation.
    pub fn new(
        expression: BoundExpressionId,
        kind: CheckedMemoryOperationKind,
        arguments: impl IntoIterator<Item = BoundExpressionId>,
    ) -> Self {
        Self {
            expression,
            kind,
            arguments: shared_slice(arguments),
        }
    }

    /// Returns the source-correlated call expression.
    pub const fn expression(&self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the exact memory behavior selected for the call.
    pub const fn kind(&self) -> CheckedMemoryOperationKind {
        self.kind
    }

    /// Returns evaluated source arguments in declaration-parameter order.
    pub fn arguments(&self) -> &[BoundExpressionId] {
        &self.arguments
    }
}

/// A malformed checked memory-operation table.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum CheckedMemoryOperationsBuildError {
    /// One operation belongs to another bound unit.
    ForeignUnit,
    /// More than one operation describes the same expression.
    DuplicateExpression,
}

/// Flow-sensitive outcome of one compiler-provided memory operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MemoryOperationStatus {
    /// Control flow cannot reach the operation.
    Unreachable,
    /// The operation's obligations and memory-state transition are valid.
    Valid,
    /// Recovery prevents a complete decision.
    Recovered,
    /// No live trusted fact source acknowledges the operation's caller obligations.
    MissingTrustedFacts,
    /// The operation reaches an allocation invalidated by deallocation.
    InvalidatedAllocation,
    /// The operation reads raw storage without an initialized value of the required type.
    UninitializedRawStorage,
    /// Deallocation would discard live initialized values or dependent obligations.
    OutstandingObligations,
}

/// One durable flow decision for a compiler-provided memory operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MemoryOperationDecision {
    expression: BoundExpressionId,
    status: MemoryOperationStatus,
}

impl MemoryOperationDecision {
    /// Creates one source-correlated memory-flow decision.
    pub const fn new(expression: BoundExpressionId, status: MemoryOperationStatus) -> Self {
        Self { expression, status }
    }

    /// Returns the checked operation occurrence.
    pub const fn expression(self) -> BoundExpressionId {
        self.expression
    }

    /// Returns the flow-sensitive outcome.
    pub const fn status(self) -> MemoryOperationStatus {
        self.status
    }
}

/// Compiler-provided memory operations selected within one checked bound unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CheckedMemoryOperations {
    unit: BoundUnitId,
    kind: BoundUnitKind,
    operations: Arc<[CheckedMemoryOperation]>,
    is_recovered: bool,
}

impl CheckedMemoryOperations {
    /// Validates and creates one immutable operation table.
    pub fn try_new(
        unit: BoundUnitId,
        kind: BoundUnitKind,
        operations: impl IntoIterator<Item = CheckedMemoryOperation>,
        is_recovered: bool,
    ) -> Result<Self, CheckedMemoryOperationsBuildError> {
        let mut operations = operations.into_iter().collect::<Vec<_>>();

        operations.sort_unstable_by_key(|operation| operation.expression());

        if operations
            .iter()
            .any(|operation| operation.expression().unit() != unit)
        {
            return Err(CheckedMemoryOperationsBuildError::ForeignUnit);
        }

        if operations
            .windows(2)
            .any(|pair| pair[0].expression() == pair[1].expression())
        {
            return Err(CheckedMemoryOperationsBuildError::DuplicateExpression);
        }

        Ok(Self {
            unit,
            kind,
            operations: shared_slice(operations),
            is_recovered,
        })
    }

    /// Returns the checked bound unit.
    pub const fn unit(&self) -> BoundUnitId {
        self.unit
    }

    /// Returns the checked unit category.
    pub const fn kind(&self) -> BoundUnitKind {
        self.kind
    }

    /// Returns operations in bound-expression order.
    pub fn operations(&self) -> &[CheckedMemoryOperation] {
        &self.operations
    }

    /// Returns the operation selected for one expression.
    pub fn operation(&self, expression: BoundExpressionId) -> Option<&CheckedMemoryOperation> {
        self.operations
            .binary_search_by_key(&expression, |operation| operation.expression())
            .ok()
            .map(|index| &self.operations[index])
    }

    /// Returns whether recovery prevented complete memory-operation checking.
    pub const fn is_recovered(&self) -> bool {
        self.is_recovered
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CheckedMemoryOperation, CheckedMemoryOperationKind, CheckedMemoryOperations,
        CheckedMemoryOperationsBuildError, MemoryReadKind,
    };
    use crate::{BoundExpressionId, BoundUnitId, BoundUnitKind};
    use crate::test_support::error_type;

    #[test]
    fn operation_tables_sort_and_index_expressions() {
        let unit = BoundUnitId::new(3);
        let first = BoundExpressionId::from_slot(unit, 1);
        let second = BoundExpressionId::from_slot(unit, 2);

        let first_operation = CheckedMemoryOperation::new(
            first,
            CheckedMemoryOperationKind::Read {
                pointee: error_type(),
                kind: MemoryReadKind::Move,
            },
            [],
        );

        let second_operation = CheckedMemoryOperation::new(
            second,
            CheckedMemoryOperationKind::Allocate,
            [],
        );

        let table = CheckedMemoryOperations::try_new(
            unit,
            BoundUnitKind::CallableBody,
            [second_operation.clone(), first_operation.clone()],
            false,
        )
        .unwrap_or_else(|error| panic!("memory operation table must build: {error:?}"));

        assert_eq!(
            table.operations(),
            [first_operation.clone(), second_operation]
        );

        assert_eq!(table.operation(first), Some(&first_operation));
    }

    #[test]
    fn operation_tables_reject_duplicate_and_foreign_expressions() {
        let unit = BoundUnitId::new(4);
        let expression = BoundExpressionId::from_slot(unit, 1);

        let operation = CheckedMemoryOperation::new(
            expression,
            CheckedMemoryOperationKind::Allocate,
            [],
        );

        assert_eq!(
            CheckedMemoryOperations::try_new(
                unit,
                BoundUnitKind::CallableBody,
                [operation.clone(), operation],
                false,
            ),
            Err(CheckedMemoryOperationsBuildError::DuplicateExpression)
        );

        let foreign = CheckedMemoryOperation::new(
            BoundExpressionId::from_slot(BoundUnitId::new(5), 1),
            CheckedMemoryOperationKind::Deallocate,
            [],
        );

        assert_eq!(
            CheckedMemoryOperations::try_new(
                unit,
                BoundUnitKind::CallableBody,
                [foreign],
                false,
            ),
            Err(CheckedMemoryOperationsBuildError::ForeignUnit)
        );
    }
}
