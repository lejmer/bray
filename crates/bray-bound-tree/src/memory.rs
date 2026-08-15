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

    /// Returns the stable inspection spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Relaxed => "relaxed",
            Self::Acquire => "acquire",
            Self::Release => "release",
            Self::AcquireRelease => "acquire_release",
            Self::SequentiallyConsistent => "sequentially_consistent",
        }
    }

    /// Returns whether this ordering is valid for an atomic load.
    pub const fn valid_for_load(self) -> bool {
        !matches!(self, Self::Release | Self::AcquireRelease)
    }

    /// Returns whether this ordering is valid for an atomic store.
    pub const fn valid_for_store(self) -> bool {
        !matches!(self, Self::Acquire | Self::AcquireRelease)
    }

    /// Returns whether this success ordering permits the supplied failure ordering.
    pub const fn permits_failure(self, failure: Self) -> bool {
        match (self, failure) {
            (_, Self::Release | Self::AcquireRelease) => false,
            (Self::Relaxed | Self::Release, Self::Relaxed) => true,
            (Self::Acquire | Self::AcquireRelease, Self::Relaxed | Self::Acquire) => true,
            (
                Self::SequentiallyConsistent,
                Self::Relaxed | Self::Acquire | Self::SequentiallyConsistent,
            ) => true,
            _ => false,
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

/// One structurally parsed assembly constraint with its exact operand role.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InlineAssemblyConstraint<'constraint> {
    kind: InlineAssemblyOperandKind,
    class: &'constraint str,
    explicit: bool,
}

impl<'constraint> InlineAssemblyConstraint<'constraint> {
    /// Parses one complete structural constraint independently of target availability.
    pub fn try_parse(constraint: &'constraint str) -> Option<Self> {
        let (kind, class) = if constraint == "label" {
            (InlineAssemblyOperandKind::Label, constraint)
        } else if let Some(class) = constraint.strip_prefix("+&") {
            (InlineAssemblyOperandKind::EarlyInOut, class)
        } else if let Some(class) = constraint.strip_prefix('+') {
            (InlineAssemblyOperandKind::InOut, class)
        } else if let Some(class) = constraint.strip_prefix("=&") {
            (InlineAssemblyOperandKind::Output, class)
        } else if let Some(class) = constraint.strip_prefix('=') {
            (InlineAssemblyOperandKind::LateOutput, class)
        } else if constraint == "i" {
            (InlineAssemblyOperandKind::Immediate, constraint)
        } else if constraint == "s" {
            (InlineAssemblyOperandKind::Symbol, constraint)
        } else if constraint == "m" {
            (InlineAssemblyOperandKind::Memory, constraint)
        } else {
            (InlineAssemblyOperandKind::Input, constraint)
        };

        if kind == InlineAssemblyOperandKind::Label {
            return Some(Self {
                kind,
                class,
                explicit: false,
            });
        }

        if class.is_empty()
            || class.starts_with(['=', '+', '&', '*', '%'])
            || class.contains('\0')
        {
            return None;
        }

        let explicit = class.starts_with('{') || class.ends_with('}');

        let class = if explicit {
            class.strip_prefix('{')?.strip_suffix('}')?
        } else {
            class
        };

        if class.is_empty() || class.contains(['{', '}']) {
            return None;
        }

        if matches!(
            kind,
            InlineAssemblyOperandKind::Input
                | InlineAssemblyOperandKind::Output
                | InlineAssemblyOperandKind::LateOutput
                | InlineAssemblyOperandKind::InOut
                | InlineAssemblyOperandKind::EarlyInOut
        ) && !explicit
            && matches!(class, "m" | "i" | "s" | "label")
        {
            return None;
        }

        Some(Self {
            kind,
            class,
            explicit,
        })
    }

    /// Returns the exact operand role encoded by the constraint.
    pub const fn kind(self) -> InlineAssemblyOperandKind {
        self.kind
    }

    /// Returns the normalized register or reserved constraint class.
    pub const fn class(self) -> &'constraint str {
        self.class
    }

    /// Returns whether the class names one exact physical register.
    pub const fn explicit(self) -> bool {
        self.explicit
    }
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
    /// Retains one complete structurally validated assembly contract.
    #[expect(
        clippy::too_many_arguments,
        reason = "assembly validation joins the literal identities and structural descriptors"
    )]
    pub fn try_new(
        template: ConstantValueId,
        constraints: ConstantValueId,
        clobbers: ConstantValueId,
        features: ConstantValueId,
        options: ConstantValueId,
        operands: [Option<InlineAssemblyOperand>; MAX_INLINE_ASSEMBLY_OPERANDS],
        operand_count: u8,
        template_text: &str,
        constraint_text: &str,
    ) -> Option<Self> {
        let contract = Self {
            template,
            constraints,
            clobbers,
            features,
            options,
            operands,
            operand_count,
        };

        contract
            .structurally_valid(template_text, constraint_text)
            .then_some(contract)
    }

    /// Splits a constraint list into its exact nonempty, trimmed byte ranges.
    pub fn constraint_ranges(value: &str) -> Option<Vec<(usize, usize)>> {
        if value.is_empty() {
            return Some(Vec::new());
        }

        let mut ranges = Vec::new();
        let mut offset = 0_usize;

        for part in value.split(',') {
            let trimmed = part.trim();

            if trimmed.is_empty() {
                return None;
            }

            let leading = part.len() - part.trim_start().len();
            ranges.push((offset + leading, trimmed.len()));
            offset += part.len() + 1;
        }

        Some(ranges)
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

    /// Validates descriptor types against their selected structural operand types.
    pub fn operand_types_valid(
        self,
        inputs: &[TypeId],
        outputs: &[TypeId],
        labels: &[TypeId],
    ) -> bool {
        if !self.value_types_valid(inputs, outputs) {
            return false;
        }

        let mut label_index = 0_usize;

        let valid = self.operands().all(|operand| {
            let label_valid = if operand.kind() == InlineAssemblyOperandKind::Label {
                let valid = labels.get(label_index).copied() == Some(operand.ty());
                label_index += 1;

                valid
            } else {
                true
            };

            label_valid
        });

        valid && label_index == labels.len()
    }

    /// Validates input and output descriptor types against selected structural value types.
    pub fn value_types_valid(self, inputs: &[TypeId], outputs: &[TypeId]) -> bool {
        self.operands().all(|operand| {
            let input_valid = operand.runtime_input().is_none_or(|ordinal| {
                inputs.get(usize::from(ordinal)).copied() == Some(operand.ty())
            });

            let output_valid = operand.output().is_none_or(|ordinal| {
                outputs.get(usize::from(ordinal)).copied() == Some(operand.ty())
            });

            input_valid && output_valid
        }) && self
            .operands()
            .filter_map(InlineAssemblyOperand::runtime_input)
            .count()
            == inputs.len()
            && self
                .operands()
                .filter_map(InlineAssemblyOperand::output)
                .count()
                == outputs.len()
    }

    fn structurally_valid(self, template: &str, constraints: &str) -> bool {
        let count = usize::from(self.operand_count);

        if count > MAX_INLINE_ASSEMBLY_OPERANDS
            || self.operands[..count].contains(&None)
            || self.operands[count..].iter().any(Option::is_some)
        {
            return false;
        }

        let Some(ranges) = Self::constraint_ranges(constraints) else {
            return false;
        };

        if ranges.len() != count {
            return false;
        }

        let operands = self.operands().collect::<Vec<_>>();

        let ranges_valid = operands.iter().zip(ranges).all(|(operand, range)| {
            let Ok(start) = u16::try_from(range.0) else {
                return false;
            };

            let Ok(length) = u16::try_from(range.1) else {
                return false;
            };

            operand_fields_valid(*operand)
                && operand.constraint_range() == (start, length)
                && constraint_kind_valid(
                    operand.kind(),
                    &constraints[range.0..range.0 + range.1],
                )
        });

        ranges_valid
            && dense_ordinals(operands.iter().filter_map(|operand| operand.input()))
            && dense_ordinals(
                operands
                    .iter()
                    .filter_map(|operand| operand.runtime_input()),
            )
            && dense_ordinals(operands.iter().filter_map(|operand| operand.output()))
            && outputs_precede_inputs(&operands)
            && template_valid(template, llvm_operand_count(&operands))
    }
}

fn operand_fields_valid(operand: InlineAssemblyOperand) -> bool {
    let input = operand.input().is_some();
    let runtime = operand.runtime_input().is_some();
    let output = operand.output().is_some();
    let constant = operand.constant().is_some();

    match operand.kind() {
        InlineAssemblyOperandKind::Input | InlineAssemblyOperandKind::Memory => {
            input && runtime && !output && !constant && operand.symbol().is_none()
        }
        InlineAssemblyOperandKind::LateOutput | InlineAssemblyOperandKind::Output => {
            !input && !runtime && output && !constant && operand.symbol().is_none()
        }
        InlineAssemblyOperandKind::InOut | InlineAssemblyOperandKind::EarlyInOut => {
            input && runtime && output && !constant && operand.symbol().is_none()
        }
        InlineAssemblyOperandKind::Immediate => {
            input && !runtime && !output && constant && operand.symbol().is_none()
        }
        InlineAssemblyOperandKind::Symbol => input && !runtime && !output && !constant,
        InlineAssemblyOperandKind::Label => {
            !input && !runtime && !output && !constant && operand.symbol().is_none()
        }
    }
}

fn constraint_kind_valid(kind: InlineAssemblyOperandKind, constraint: &str) -> bool {
    InlineAssemblyConstraint::try_parse(constraint)
        .is_some_and(|constraint| constraint.kind() == kind)
}

fn dense_ordinals(ordinals: impl Iterator<Item = u16>) -> bool {
    let mut seen = 0_u64;
    let mut count = 0_u32;

    for ordinal in ordinals {
        let Some(bit) = 1_u64.checked_shl(u32::from(ordinal)) else {
            return false;
        };

        if seen & bit != 0 {
            return false;
        }

        seen |= bit;
        count += 1;
    }

    seen == 1_u64.checked_shl(count).unwrap_or(0).wrapping_sub(1)
}

fn outputs_precede_inputs(operands: &[InlineAssemblyOperand]) -> bool {
    let mut saw_pure_input = false;

    for operand in operands {
        let has_output = operand.output().is_some();

        if saw_pure_input && has_output {
            return false;
        }

        saw_pure_input |= !has_output;
    }

    true
}

fn llvm_operand_count(operands: &[InlineAssemblyOperand]) -> usize {
    operands
        .iter()
        .map(|operand| {
            usize::from(operand.output().is_some())
                + usize::from(operand.input().is_some())
                + usize::from(operand.kind() == InlineAssemblyOperandKind::Label)
        })
        .sum()
}

fn template_valid(template: &str, operand_count: usize) -> bool {
    if template.contains('\0') {
        return false;
    }

    let bytes = template.as_bytes();
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] != b'$' {
            index += 1;
            continue;
        }

        index += 1;

        if index < bytes.len() && bytes[index] == b'$' {
            index += 1;
            continue;
        }

        let braced = index < bytes.len() && bytes[index] == b'{';
        index += usize::from(braced);
        let start = index;

        while index < bytes.len() && bytes[index].is_ascii_digit() {
            index += 1;
        }

        if start == index {
            return false;
        }

        let Ok(ordinal) = template[start..index].parse::<usize>() else {
            return false;
        };

        if ordinal >= operand_count {
            return false;
        }

        if braced {
            if index >= bytes.len() || bytes[index] != b'}' {
                return false;
            }

            index += 1;
        }
    }

    true
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

/// Integer fetch behavior selected for protected atomic storage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum AtomicFetchKind {
    /// Wrapping integer addition.
    Add,
    /// Wrapping integer subtraction.
    Subtract,
    /// Bitwise conjunction.
    And,
    /// Bitwise disjunction.
    Or,
    /// Bitwise exclusive disjunction.
    Xor,
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
    /// Initialize protected atomic storage before it becomes shared.
    AtomicInitialize {
        /// Stored atomic-compatible value type.
        value: TypeId,
    },
    /// Atomically load protected storage.
    AtomicLoad {
        /// Stored atomic-compatible value type.
        value: TypeId,
        /// Compile-time load ordering.
        order: MemoryOrder,
    },
    /// Atomically store protected storage.
    AtomicStore {
        /// Stored atomic-compatible value type.
        value: TypeId,
        /// Compile-time store ordering.
        order: MemoryOrder,
    },
    /// Atomically exchange protected storage.
    AtomicExchange {
        /// Stored atomic-compatible value type.
        value: TypeId,
        /// Compile-time read-modify-write ordering.
        order: MemoryOrder,
    },
    /// Perform atomic compare-exchange.
    AtomicCompareExchange {
        /// Stored atomic-compatible value type.
        value: TypeId,
        /// Whether spurious failure is permitted.
        weak: bool,
        /// Compile-time success ordering.
        success: MemoryOrder,
        /// Compile-time failure ordering.
        failure: MemoryOrder,
    },
    /// Perform an integer atomic fetch operation.
    AtomicFetch {
        /// Stored integer value type.
        value: TypeId,
        /// Arithmetic or bitwise operation.
        kind: AtomicFetchKind,
        /// Compile-time read-modify-write ordering.
        order: MemoryOrder,
    },
    /// Wait while protected atomic storage equals one expected value.
    AtomicWait {
        /// Stored atomic-compatible value type.
        value: TypeId,
        /// Compile-time load ordering used by the wait.
        order: MemoryOrder,
    },
    /// Notify waiters observing protected atomic storage.
    AtomicNotify {
        /// Stored atomic-compatible value type.
        value: TypeId,
        /// Whether every waiter is notified instead of at most one.
        all: bool,
    },
}

impl CheckedMemoryOperationKind {
    /// Returns whether every atomic ordering carried by this operation is legal for its role.
    pub const fn has_valid_atomic_ordering(self) -> bool {
        match self {
            Self::AtomicLoad { order, .. } | Self::AtomicWait { order, .. } => {
                order.valid_for_load()
            }
            Self::AtomicStore { order, .. } => order.valid_for_store(),
            Self::AtomicCompareExchange {
                success, failure, ..
            } => success.permits_failure(failure),
            _ => true,
        }
    }

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
            | Self::CallbackState { .. }
            | Self::AtomicInitialize { .. }
            | Self::AtomicLoad { .. }
            | Self::AtomicNotify { .. } => 1,
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
            | Self::ByteBufferRead
            | Self::AtomicStore { .. }
            | Self::AtomicExchange { .. }
            | Self::AtomicFetch { .. }
            | Self::AtomicWait { .. } => 2,
            Self::VolatileWrite { .. } | Self::CompareAddress { .. } => 2,
            Self::Copy { .. }
            | Self::RawDeallocate
            | Self::ByteBufferFill
            | Self::AtomicCompareExchange { .. } => 3,
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
            | Self::AtomicInitialize { .. }
            | Self::AtomicLoad { .. }
            | Self::AtomicNotify { .. }
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
            | Self::CompareAddress { .. }
            | Self::AtomicStore { .. }
            | Self::AtomicExchange { .. }
            | Self::AtomicFetch { .. }
            | Self::AtomicWait { .. } => {
                if ordinal < 2 { Some(ordinal) } else { None }
            }
            Self::Copy { .. }
            | Self::RawDeallocate
            | Self::ByteBufferFill
            | Self::AtomicCompareExchange { .. } => {
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
                | Self::AtomicStore { .. }
                | Self::AtomicWait { .. }
                | Self::AtomicNotify { .. }
        )
    }
}

#[cfg(test)]
mod atomic_order_tests {
    use bray_symbols::{SemanticValueStore, TypeData};

    use super::{CheckedMemoryOperationKind, MemoryOrder};

    #[test]
    fn operation_specific_ordering_matrix_is_exact() {
        use MemoryOrder as Order;

        assert!(Order::Relaxed.valid_for_load());
        assert!(Order::Acquire.valid_for_load());
        assert!(!Order::Release.valid_for_load());
        assert!(!Order::AcquireRelease.valid_for_load());
        assert!(Order::SequentiallyConsistent.valid_for_load());

        assert!(Order::Relaxed.valid_for_store());
        assert!(!Order::Acquire.valid_for_store());
        assert!(Order::Release.valid_for_store());
        assert!(!Order::AcquireRelease.valid_for_store());
        assert!(Order::SequentiallyConsistent.valid_for_store());

        assert!(!Order::Relaxed.valid_for_fence());
        assert!(Order::Acquire.valid_for_fence());
        assert!(Order::Release.valid_for_fence());
        assert!(Order::AcquireRelease.valid_for_fence());
        assert!(Order::SequentiallyConsistent.valid_for_fence());
    }

    #[test]
    fn compare_exchange_failure_order_never_has_release_behavior() {
        use MemoryOrder as Order;

        for success in [
            Order::Relaxed,
            Order::Acquire,
            Order::Release,
            Order::AcquireRelease,
            Order::SequentiallyConsistent,
        ] {
            assert!(!success.permits_failure(Order::Release));
            assert!(!success.permits_failure(Order::AcquireRelease));
        }

        assert!(Order::Relaxed.permits_failure(Order::Relaxed));
        assert!(!Order::Relaxed.permits_failure(Order::Acquire));
        assert!(Order::AcquireRelease.permits_failure(Order::Acquire));
        assert!(Order::SequentiallyConsistent.permits_failure(Order::SequentiallyConsistent));
    }

    #[test]
    fn checked_atomic_kinds_apply_operation_specific_ordering_legality() {
        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must be available: {error:?}"));

        let value = values
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("test atomic type must intern: {error:?}"));

        assert!(
            CheckedMemoryOperationKind::AtomicLoad {
                value,
                order: MemoryOrder::Acquire,
            }
            .has_valid_atomic_ordering()
        );

        assert!(
            !CheckedMemoryOperationKind::AtomicLoad {
                value,
                order: MemoryOrder::Release,
            }
            .has_valid_atomic_ordering()
        );

        assert!(
            !CheckedMemoryOperationKind::AtomicStore {
                value,
                order: MemoryOrder::Acquire,
            }
            .has_valid_atomic_ordering()
        );

        assert!(
            !CheckedMemoryOperationKind::AtomicCompareExchange {
                value,
                weak: false,
                success: MemoryOrder::Relaxed,
                failure: MemoryOrder::Acquire,
            }
            .has_valid_atomic_ordering()
        );
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
        CheckedMemoryOperationsBuildError, InlineAssemblyConstraint, InlineAssemblyContract,
        InlineAssemblyOperand, InlineAssemblyOperandKind, MAX_INLINE_ASSEMBLY_OPERANDS,
        MemoryReadKind,
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

        let contract = InlineAssemblyContract::try_new(
            first, first, first, first, first, operands, 2, "", "i,i",
        )
        .unwrap_or_else(|| panic!("test assembly contract must validate"));

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
    fn inline_assembly_contract_validation_rejects_malformed_external_shapes() {
        assert_eq!(
            InlineAssemblyConstraint::try_parse("m").map(InlineAssemblyConstraint::kind),
            Some(InlineAssemblyOperandKind::Memory),
        );

        assert_eq!(
            InlineAssemblyConstraint::try_parse("{rax}").map(InlineAssemblyConstraint::class),
            Some("rax"),
        );

        assert_eq!(
            InlineAssemblyConstraint::try_parse("+{rax}")
                .map(InlineAssemblyConstraint::explicit),
            Some(true),
        );

        assert!(InlineAssemblyConstraint::try_parse("=m").is_none());
        assert!(InlineAssemblyConstraint::try_parse("{rax").is_none());

        let values = SemanticValueStore::try_new()
            .unwrap_or_else(|error| panic!("test semantic values must be available: {error:?}"));

        let ty = values
            .intern_type(TypeData::Error)
            .unwrap_or_else(|error| panic!("test assembly type must intern: {error:?}"));

        let constant = values
            .intern_constant_value(ConstantValueData::new(
                ty,
                ConstantValueKind::Boolean(false),
            ))
            .unwrap_or_else(|error| panic!("test assembly constant must intern: {error:?}"));

        let mut operands = [None; MAX_INLINE_ASSEMBLY_OPERANDS];

        operands[0] = Some(InlineAssemblyOperand::new(
            InlineAssemblyOperandKind::Input,
            ty,
            Some(0),
            Some(0),
            None,
            None,
            None,
            0,
            3,
        ));

        let valid = |operands, count, template, constraints| {
            InlineAssemblyContract::try_new(
                constant,
                constant,
                constant,
                constant,
                constant,
                operands,
                count,
                template,
                constraints,
            )
        };

        assert!(valid(operands, 1, "use $0", "reg").is_some());
        assert!(valid(operands, 1, "use $1", "reg").is_none());

        let mut reserved_register = operands;

        reserved_register[0] = Some(InlineAssemblyOperand::new(
            InlineAssemblyOperandKind::Input,
            ty,
            Some(0),
            Some(0),
            None,
            None,
            None,
            0,
            1,
        ));

        assert!(valid(reserved_register, 1, "", "m").is_none());

        let contract = valid(operands, 1, "", "reg")
            .unwrap_or_else(|| panic!("valid assembly contract must be available"));

        let other = values
            .intern_type(TypeData::tuple([]))
            .unwrap_or_else(|error| panic!("different assembly type must intern: {error:?}"));

        assert!(contract.operand_types_valid(&[ty], &[], &[]));
        assert!(!contract.operand_types_valid(&[other], &[], &[]));

        let mut sparse = operands;
        sparse[0] = None;
        sparse[1] = operands[0];
        assert!(valid(sparse, 2, "", "reg,reg").is_none());

        let mut duplicate_runtime = operands;

        duplicate_runtime[1] = operands[0].map(|operand| {
            InlineAssemblyOperand::new(
                operand.kind(),
                operand.ty(),
                Some(1),
                Some(0),
                None,
                None,
                None,
                4,
                3,
            )
        });

        assert!(valid(duplicate_runtime, 2, "", "reg,reg").is_none());
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
