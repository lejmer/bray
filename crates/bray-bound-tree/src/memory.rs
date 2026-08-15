// rust-style: allow(module-too-large, reason = "the checked memory operation and target-control contracts form one exhaustive typed protocol inventory")

use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{CallableAbi, CallableInstanceData, ConstantValueId, TypeId};

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

/// Address space selected for an explicit volatile operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum VolatileAddressSpace {
    /// Ordinary host-visible storage.
    Host,
    /// Target device storage with device-memory semantics.
    Device,
}

/// Provenance-losing comparison performed on exposed pointer addresses.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum PointerAddressComparison {
    /// Compare exposed addresses for equality.
    Equal,
    /// Compare exposed addresses using unsigned ordering.
    Less,
}

/// Ordering strength shared by hardware and compiler synchronization fences.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MemoryOrder {
    /// No ordering beyond atomicity.
    Relaxed,
    /// Prevent later operations from moving before the fence.
    Acquire,
    /// Prevent earlier operations from moving after the fence.
    Release,
    /// Apply both acquire and release ordering.
    AcquireRelease,
    /// Participate in the single sequentially consistent order.
    SequentiallyConsistent,
}

impl MemoryOrder {
    /// Returns whether the order is meaningful for a synchronization fence.
    pub const fn valid_for_fence(self) -> bool {
        !matches!(self, Self::Relaxed)
    }

    /// Returns the stable package-interface encoding.
    pub const fn to_u32(self) -> u32 {
        match self {
            Self::Relaxed => 0,
            Self::Acquire => 1,
            Self::Release => 2,
            Self::AcquireRelease => 3,
            Self::SequentiallyConsistent => 4,
        }
    }

    /// Decodes one stable package-interface value.
    pub const fn from_u64(value: u64) -> Option<Self> {
        match value {
            0 => Some(Self::Relaxed),
            1 => Some(Self::Acquire),
            2 => Some(Self::Release),
            3 => Some(Self::AcquireRelease),
            4 => Some(Self::SequentiallyConsistent),
            _ => None,
        }
    }
}

/// Semantic role of one exactly checked inline-assembly operand.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InlineAssemblyOperandKind {
    /// A register-class or explicit-register input.
    Input,
    /// An output that may overlap an unrelated input.
    LateOutput,
    /// An early-clobber output.
    Output,
    /// A tied input and late output.
    InOut,
    /// A tied input and early-clobber output.
    EarlyInOut,
    /// A compile-time integer immediate.
    Immediate,
    /// A closed callable symbol address.
    Symbol,
    /// A pointer naming an addressable memory operand.
    Memory,
    /// A typed alternate control-flow destination.
    Label,
}

/// One exact callable symbol retained for assembly relocation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InlineAssemblySymbol {
    instance: CallableInstanceData,
    abi: CallableAbi,
}

impl InlineAssemblySymbol {
    /// Creates a closed callable symbol identity with its checked calling convention.
    pub const fn new(instance: CallableInstanceData, abi: CallableAbi) -> Self {
        Self { instance, abi }
    }

    /// Returns the fully substituted callable instance.
    pub const fn instance(self) -> CallableInstanceData {
        self.instance
    }

    /// Returns the callable calling convention.
    pub const fn abi(self) -> CallableAbi {
        self.abi
    }
}

/// One typed operand descriptor retained after exact target checking.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InlineAssemblyOperand {
    kind: InlineAssemblyOperandKind,
    ty: TypeId,
    input: Option<u16>,
    runtime_input: Option<u16>,
    output: Option<u16>,
    constant: Option<ConstantValueId>,
    symbol: Option<InlineAssemblySymbol>,
    constraint_start: u16,
    constraint_length: u16,
}

impl InlineAssemblyOperand {
    /// Creates one descriptor from its checked structural positions and constraint spelling.
    #[expect(
        clippy::too_many_arguments,
        reason = "an assembly operand retains each checked structural fact explicitly"
    )]
    pub const fn new(
        kind: InlineAssemblyOperandKind,
        ty: TypeId,
        input: Option<u16>,
        runtime_input: Option<u16>,
        output: Option<u16>,
        constant: Option<ConstantValueId>,
        symbol: Option<InlineAssemblySymbol>,
        constraint_start: u16,
        constraint_length: u16,
    ) -> Self {
        Self {
            kind,
            ty,
            input,
            runtime_input,
            output,
            constant,
            symbol,
            constraint_start,
            constraint_length,
        }
    }

    /// Returns the exact operand role.
    pub const fn kind(self) -> InlineAssemblyOperandKind {
        self.kind
    }

    /// Returns the checked source type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    /// Returns the input-tuple ordinal consumed by this operand.
    pub const fn input(self) -> Option<u16> {
        self.input
    }

    /// Returns the compact runtime input ordinal when this operand is evaluated into MIR.
    pub const fn runtime_input(self) -> Option<u16> {
        self.runtime_input
    }

    /// Returns the output-tuple ordinal initialized by this operand.
    pub const fn output(self) -> Option<u16> {
        self.output
    }

    /// Returns the checked constant identity required by an immediate operand.
    pub const fn constant(self) -> Option<ConstantValueId> {
        self.constant
    }

    /// Returns the checked callable relocation identity required by a symbol operand.
    pub const fn symbol(self) -> Option<InlineAssemblySymbol> {
        self.symbol
    }

    /// Removes the checker-only symbol identity after it has been lowered to MIR.
    pub const fn without_symbol(mut self) -> Self {
        self.symbol = None;

        self
    }

    /// Returns the byte range of this operand's target constraint class.
    pub const fn constraint_range(self) -> (u16, u16) {
        (self.constraint_start, self.constraint_length)
    }
}

/// Maximum descriptor count retained in the compact checked operation contract.
pub const MAX_INLINE_ASSEMBLY_OPERANDS: usize = 32;

const BASE_INLINE_ASSEMBLY_CONSTANTS: usize = 5;
const MAX_INLINE_ASSEMBLY_CONTRACT_CONSTANTS: usize =
    BASE_INLINE_ASSEMBLY_CONSTANTS + MAX_INLINE_ASSEMBLY_OPERANDS;

/// Compile-time identities forming one exactly checked inline-assembly contract.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InlineAssemblyContract {
    template: ConstantValueId,
    constraints: ConstantValueId,
    clobbers: ConstantValueId,
    features: ConstantValueId,
    options: ConstantValueId,
    operands: [Option<InlineAssemblyOperand>; MAX_INLINE_ASSEMBLY_OPERANDS],
    operand_count: u8,
}

impl InlineAssemblyContract {
    /// Retains the checked literal identities used to lower one assembly operation.
    pub const fn new(
        template: ConstantValueId,
        constraints: ConstantValueId,
        clobbers: ConstantValueId,
        features: ConstantValueId,
        options: ConstantValueId,
        operands: [Option<InlineAssemblyOperand>; MAX_INLINE_ASSEMBLY_OPERANDS],
        operand_count: u8,
    ) -> Self {
        Self {
            template,
            constraints,
            clobbers,
            features,
            options,
            operands,
            operand_count,
        }
    }

    /// Returns the checked assembly-template literal identity.
    pub const fn template(self) -> ConstantValueId {
        self.template
    }

    /// Returns the checked operand-constraint literal identity.
    pub const fn constraints(self) -> ConstantValueId {
        self.constraints
    }

    /// Returns the checked clobber literal identity.
    pub const fn clobbers(self) -> ConstantValueId {
        self.clobbers
    }

    /// Returns the checked target-feature literal identity.
    pub const fn features(self) -> ConstantValueId {
        self.features
    }

    /// Returns the checked option literal identity.
    pub const fn options(self) -> ConstantValueId {
        self.options
    }

    /// Iterates the canonical checked operands in constraint order.
    pub fn operands(self) -> impl Iterator<Item = InlineAssemblyOperand> {
        self.operands
            .into_iter()
            .take(usize::from(self.operand_count))
            .flatten()
    }

    /// Returns the same structural contract after checker symbol identities enter MIR.
    pub fn without_symbols(mut self) -> Self {
        for operand in self.operands.iter_mut().flatten() {
            *operand = operand.without_symbol();
        }

        self
    }

    /// Iterates the complete checked compile-time contract in declaration order.
    pub fn constant_values(self) -> impl Iterator<Item = ConstantValueId> {
        let mut values = [None; MAX_INLINE_ASSEMBLY_CONTRACT_CONSTANTS];
        let mut count = 0;

        for value in [
            self.template,
            self.constraints,
            self.clobbers,
            self.features,
            self.options,
        ]
        .into_iter()
        .chain(self.operands().filter_map(InlineAssemblyOperand::constant))
        {
            if !values[..count].contains(&Some(value)) {
                values[count] = Some(value);
                count += 1;
            }
        }

        values.into_iter().take(count).flatten()
    }
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
    /// Create raw storage from separate byte count and alignment values.
    RawAllocate,
    /// Release raw storage described by separate pointer and layout values.
    RawDeallocate,
    /// Create a distinct owned writable allocation from a layout value.
    Allocate,
    /// Release an owned allocation and invalidate its dependent facts.
    Deallocate,
    /// Read a raw buffer's capacity.
    RawBufferCapacity,
    /// Read a raw buffer's initialized element count.
    RawBufferInitializedCount,
    /// Read a raw buffer's storage pointer.
    RawBufferPointer,
    /// Borrow a raw buffer's initialized elements.
    RawBufferInitializedSlice,
    /// Mutably borrow a raw buffer's initialized elements.
    RawBufferInitializedSliceMut,
    /// Produce a pointer to a raw buffer's spare storage.
    RawBufferSparePointer {
        /// Buffered element type governing pointer arithmetic.
        element: TypeId,
    },
    /// Update a raw buffer's initialized element count.
    RawBufferSetInitializedCount,
    /// Destroy initialized elements and release a raw buffer in place.
    RawBufferRelease {
        /// Buffered element type governing cleanup and allocation layout.
        element: TypeId,
    },
    /// Release a destination raw buffer and transfer a source owner into it.
    RawBufferReplace {
        /// Buffered element type governing cleanup and allocation layout.
        element: TypeId,
    },
    /// Relocate initialized elements between distinct raw-buffer owners.
    RawBufferRelocate {
        /// Buffered element type governing representation and alignment.
        element: TypeId,
    },
    /// Initialize a byte-buffer range to one repeated byte.
    ByteBufferFill,
    /// Copy an initialized byte slice into distinct writable storage.
    ByteSliceCopy,
    /// Read one initialized byte from byte-buffer storage.
    ByteBufferRead,
    /// Read the element count carried by a slice.
    SliceLength,
    /// Reconstruct a state borrow from a checked foreign-callback context parameter.
    CallbackState {
        /// Borrowed callback state type.
        state: TypeId,
    },
    /// Read initialized storage with volatile access semantics.
    VolatileRead {
        /// Read value type.
        pointee: TypeId,
        /// Address space governing the access.
        address_space: VolatileAddressSpace,
        /// Ownership effect selected from the value's copy contract.
        kind: MemoryReadKind,
    },
    /// Write storage with volatile access semantics.
    VolatileWrite {
        /// Written value type.
        pointee: TypeId,
        /// Address space governing the access.
        address_space: VolatileAddressSpace,
    },
    /// Expose a raw pointer's target address as a provenance-free integer.
    ExposeAddress {
        /// Pointed-to value type.
        pointee: TypeId,
    },
    /// Reconstruct a raw pointer from a provenance-free target address.
    FromExposedAddress {
        /// Pointed-to value type.
        pointee: TypeId,
    },
    /// Compare two provenance-free pointer addresses.
    CompareAddress {
        /// Pointed-to value type.
        pointee: TypeId,
        /// Exact comparison performed after exposure.
        comparison: PointerAddressComparison,
    },
    /// Apply one checked hardware or compiler synchronization fence.
    Fence {
        /// Whether the barrier applies only to compiler reordering.
        compiler_only: bool,
        /// Checked synchronization ordering.
        order: MemoryOrder,
    },
    /// Terminate the product catastrophically without source cleanup.
    CatastrophicAbort,
    /// Request a debugger trap and continue when the debugger resumes.
    DebuggerTrap,
    /// Terminate a path whose reachability violates a trusted contract.
    UnreachableTermination,
    /// Emit the selected target's spin-loop hint.
    SpinLoopHint,
    /// Test one statically named target instruction feature.
    TargetFeatureEnabled {
        /// Checked target-feature literal identity.
        feature: ConstantValueId,
    },
    /// Execute exactly checked trusted target-gated inline assembly.
    InlineAssembly {
        /// Structural tuple of typed input operands.
        inputs: TypeId,
        /// Structural tuple of typed outputs, absent when assembly cannot continue normally.
        output: Option<TypeId>,
        /// Structural tuple of typed label callables for alternate control flow.
        labels: Option<TypeId>,
        /// Checked compile-time assembly contract.
        contract: InlineAssemblyContract,
    },
}

impl CheckedMemoryOperationKind {
    /// Iterates compile-time values that form part of the checked operation contract.
    pub fn contract_constants(self) -> impl Iterator<Item = ConstantValueId> {
        let mut values = [None; MAX_INLINE_ASSEMBLY_CONTRACT_CONSTANTS];

        match self {
            Self::TargetFeatureEnabled { feature } => values[0] = Some(feature),
            Self::InlineAssembly { contract, .. } => {
                for (destination, value) in values.iter_mut().zip(contract.constant_values()) {
                    *destination = Some(value);
                }
            }
            _ => {}
        }

        values.into_iter().flatten()
    }

    /// Returns the exact number of runtime operands required by this operation.
    pub const fn operand_count(self) -> usize {
        match self {
            Self::Address { .. }
            | Self::IsNull { .. }
            | Self::Reinterpret { .. }
            | Self::Read { .. }
            | Self::Allocate
            | Self::Deallocate
            | Self::RawBufferCapacity
            | Self::RawBufferInitializedCount
            | Self::RawBufferPointer
            | Self::RawBufferInitializedSlice
            | Self::RawBufferInitializedSliceMut
            | Self::RawBufferSparePointer { .. }
            | Self::RawBufferRelease { .. }
            | Self::SliceLength
            | Self::CallbackState { .. } => 1,
            Self::VolatileRead { .. }
            | Self::ExposeAddress { .. }
            | Self::FromExposedAddress { .. } => 1,
            Self::Offset { .. }
            | Self::Write { .. }
            | Self::RawAllocate
            | Self::RawBufferReplace { .. }
            | Self::RawBufferRelocate { .. }
            | Self::ByteSliceCopy
            | Self::RawBufferSetInitializedCount
            | Self::ByteBufferRead => 2,
            Self::VolatileWrite { .. } | Self::CompareAddress { .. } => 2,
            Self::Copy { .. }
            | Self::RawDeallocate
            | Self::ByteBufferFill => 3,
            Self::LayoutQuery {
                kind: MemoryLayoutQueryKind::Layout,
                ..
            } => 1,
            Self::InlineAssembly { .. } => 1,
            Self::Null { .. }
            | Self::LayoutQuery { .. }
            | Self::Fence { .. }
            | Self::CatastrophicAbort
            | Self::DebuggerTrap
            | Self::UnreachableTermination
            | Self::SpinLoopHint
            | Self::TargetFeatureEnabled { .. } => 0,
        }
    }

    /// Maps a selected source-argument ordinal to its runtime MIR operand index.
    pub const fn runtime_argument_index(self, ordinal: usize) -> Option<usize> {
        match self {
            Self::Address { .. }
            | Self::IsNull { .. }
            | Self::Reinterpret { .. }
            | Self::Read { .. }
            | Self::Allocate
            | Self::Deallocate
            | Self::RawBufferCapacity
            | Self::RawBufferInitializedCount
            | Self::RawBufferPointer
            | Self::RawBufferInitializedSlice
            | Self::RawBufferInitializedSliceMut
            | Self::RawBufferSparePointer { .. }
            | Self::RawBufferRelease { .. }
            | Self::SliceLength
            | Self::CallbackState { .. }
            | Self::VolatileRead { .. }
            | Self::ExposeAddress { .. }
            | Self::FromExposedAddress { .. }
            | Self::LayoutQuery {
                kind: MemoryLayoutQueryKind::Layout,
                ..
            } => {
                if ordinal == 0 { Some(0) } else { None }
            }
            Self::Offset { .. }
            | Self::Write { .. }
            | Self::RawAllocate
            | Self::RawBufferReplace { .. }
            | Self::RawBufferRelocate { .. }
            | Self::ByteSliceCopy
            | Self::RawBufferSetInitializedCount
            | Self::ByteBufferRead
            | Self::VolatileWrite { .. }
            | Self::CompareAddress { .. } => {
                if ordinal < 2 { Some(ordinal) } else { None }
            }
            Self::Copy { .. } | Self::RawDeallocate | Self::ByteBufferFill => {
                if ordinal < 3 { Some(ordinal) } else { None }
            }
            Self::InlineAssembly { .. } => {
                if ordinal == 5 { Some(0) } else { None }
            }
            Self::Null { .. }
            | Self::LayoutQuery { .. }
            | Self::Fence { .. }
            | Self::CatastrophicAbort
            | Self::DebuggerTrap
            | Self::UnreachableTermination
            | Self::SpinLoopHint
            | Self::TargetFeatureEnabled { .. } => None,
        }
    }

    /// Returns whether the operation produces a value instead of only changing memory state.
    pub const fn produces_value(self) -> bool {
        !matches!(
            self,
            Self::Write { .. }
                | Self::Copy { .. }
                | Self::RawDeallocate
                | Self::Deallocate
                | Self::RawBufferSetInitializedCount
                | Self::RawBufferRelease { .. }
                | Self::RawBufferReplace { .. }
                | Self::RawBufferRelocate { .. }
                | Self::ByteBufferFill
                | Self::ByteSliceCopy
                | Self::VolatileWrite { .. }
                | Self::Fence { .. }
                | Self::CatastrophicAbort
                | Self::DebuggerTrap
                | Self::UnreachableTermination
                | Self::SpinLoopHint
                | Self::InlineAssembly { output: None, .. }
        )
    }
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
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
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
        CheckedMemoryOperationsBuildError, InlineAssemblyContract, InlineAssemblyOperand,
        InlineAssemblyOperandKind, MAX_INLINE_ASSEMBLY_OPERANDS, MemoryReadKind,
    };
    use crate::test_support::error_type;
    use crate::{BoundExpressionId, BoundUnitId, BoundUnitKind};
    use bray_symbols::{ConstantValueData, ConstantValueKind, SemanticValueStore, TypeData};

    #[test]
    fn inline_assembly_contract_constants_include_unique_immediates() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must be available: {error:?}"));

        let ty = values
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("test assembly type must intern: {error:?}"));

        let first = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Boolean(false),
            ))
            .unwrap_or_else(|error| panic!("first assembly constant must intern: {error:?}"));

        let second = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Boolean(true),
            ))
            .unwrap_or_else(|error| panic!("second assembly constant must intern: {error:?}"));

        let mut operands = [None; MAX_INLINE_ASSEMBLY_OPERANDS];

        operands[0] = Some(InlineAssemblyOperand::new(
            InlineAssemblyOperandKind::Immediate,
            ty,
            Some(0),
            None,
            None,
            Some(first),
            None,
            0,
            1,
        ));

        operands[1] = Some(InlineAssemblyOperand::new(
            InlineAssemblyOperandKind::Immediate,
            ty,
            Some(1),
            None,
            None,
            Some(second),
            None,
            2,
            1,
        ));

        let contract = InlineAssemblyContract::new(
            first, first, first, first, first, operands, 2,
        );

        let operation = CheckedMemoryOperationKind::InlineAssembly {
            inputs: ty,
            output: None,
            labels: None,
            contract,
        };

        assert_eq!(contract.constant_values().collect::<Vec<_>>(), [first, second]);
        assert_eq!(operation.contract_constants().collect::<Vec<_>>(), [first, second]);
    }

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

        let second_operation =
            CheckedMemoryOperation::new(second, CheckedMemoryOperationKind::Allocate, []);

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

        let operation =
            CheckedMemoryOperation::new(expression, CheckedMemoryOperationKind::Allocate, []);

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
            CheckedMemoryOperations::try_new(unit, BoundUnitKind::CallableBody, [foreign], false,),
            Err(CheckedMemoryOperationsBuildError::ForeignUnit)
        );
    }
}
