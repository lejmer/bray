use crate::{MirCallableReference, MirOperand, MirPlace};
use bray_base::shared_slice;
use bray_bound_tree::{CheckedMemoryOperationKind, ConstructionInputId, ConstructionTarget};
use bray_symbols::{ConstantTermId, TypeId};
use std::sync::Arc;

/// The normalized representation built by one aggregate operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirAggregateKind {
    /// A tuple value.
    Tuple,
    /// A fixed array with one operand per element.
    Array,
    /// A fixed array with one repeated-value operand and an extent carried by its result type.
    RepeatedArray,
    /// A bounded half-open range with lower and upper operands.
    Range,
    /// The present state of a nullable value.
    NullablePresent,
}

/// The normalized result accumulated by one generator expression.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirGeneratorKind {
    /// A fixed array whose checked count determines its final extent.
    Array,
    /// A lazy generator value.
    General,
}

/// Compiler-provided UTF-8 text behavior selected for one call.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum MirTextOperationKind {
    /// Count Unicode scalar values.
    ScalarCount,
    /// Test whether text is empty.
    IsEmpty,
    /// Compare text values for equality.
    Equals,
    /// Select one Unicode scalar by scalar index.
    ScalarAt,
    /// Copy one half-open scalar range into owned text.
    ScalarSlice,
    /// Borrow the underlying valid UTF-8 bytes.
    Utf8,
    /// Validate and copy borrowed UTF-8 bytes into owned text.
    FromUtf8,
    /// Return a character's Unicode scalar value.
    CharacterScalarValue,
    /// Construct a character from a valid Unicode scalar value.
    CharacterFromScalarValue,
    /// Return a character's UTF-8 encoded length.
    CharacterUtf8Length,
    /// Return one byte from a character's UTF-8 encoding.
    CharacterUtf8Byte,
    /// Test whether a character is alphabetic.
    CharacterIsAlphabetic,
    /// Test whether a character is numeric.
    CharacterIsNumeric,
    /// Test whether a character is whitespace.
    CharacterIsWhitespace,
    /// Release one owned text storage reference.
    Release,
}

/// One explicit UTF-8 text operation with evaluated operands.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirTextOperation {
    kind: MirTextOperationKind,
    operands: Arc<[MirOperand]>,
    operand_types: Arc<[TypeId]>,
    result_type: Option<TypeId>,
}

impl MirTextOperation {
    /// Creates one text operation in evaluation order.
    pub fn new(
        kind: MirTextOperationKind,
        operands: impl IntoIterator<Item = MirOperand>,
        operand_types: impl IntoIterator<Item = TypeId>,
        result_type: Option<TypeId>,
    ) -> Self {
        Self {
            kind,
            operands: shared_slice(operands),
            operand_types: shared_slice(operand_types),
            result_type,
        }
    }

    /// Returns the selected text behavior.
    pub const fn kind(&self) -> MirTextOperationKind {
        self.kind
    }

    /// Returns evaluated operands in declaration order.
    pub fn operands(&self) -> &[MirOperand] {
        &self.operands
    }

    /// Returns selected operand types in declaration order.
    pub fn operand_types(&self) -> &[TypeId] {
        &self.operand_types
    }

    /// Returns the exact operation result type.
    pub const fn result_type(&self) -> Option<TypeId> {
        self.result_type
    }
}

/// One explicit compiler-provided memory operation with evaluated operands.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirMemoryOperation {
    kind: CheckedMemoryOperationKind,
    operands: Arc<[MirOperand]>,
    operand_types: Arc<[TypeId]>,
    result_type: Option<TypeId>,
    inline_assembly_symbols: Arc<[MirCallableReference]>,
}

impl MirMemoryOperation {
    /// Creates one memory operation in evaluation order.
    pub fn new(
        kind: CheckedMemoryOperationKind,
        operands: impl IntoIterator<Item = MirOperand>,
        operand_types: impl IntoIterator<Item = TypeId>,
        result_type: Option<TypeId>,
    ) -> Self {
        Self {
            kind,
            operands: shared_slice(operands),
            operand_types: shared_slice(operand_types),
            result_type,
            inline_assembly_symbols: Arc::from([]),
        }
    }

    /// Retains closed callable symbols referenced by assembly operands in descriptor order.
    pub fn with_inline_assembly_symbols(
        mut self,
        symbols: impl IntoIterator<Item = MirCallableReference>,
    ) -> Self {
        self.inline_assembly_symbols = shared_slice(symbols);

        self
    }

    /// Returns the checked memory behavior.
    pub const fn kind(&self) -> CheckedMemoryOperationKind {
        self.kind
    }

    /// Returns evaluated operands in declaration order.
    pub fn operands(&self) -> &[MirOperand] {
        &self.operands
    }

    /// Returns the selected parameter types in declaration order.
    pub fn operand_types(&self) -> &[TypeId] {
        &self.operand_types
    }

    /// Returns the selected result type when the operation produces a value.
    pub const fn result_type(&self) -> Option<TypeId> {
        self.result_type
    }

    /// Returns closed callable symbols referenced by assembly operands in descriptor order.
    pub fn inline_assembly_symbols(&self) -> &[MirCallableReference] {
        &self.inline_assembly_symbols
    }
}

/// One explicit generator accumulation step.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum MirGeneratorOperation {
    /// Initialize generator result storage.
    Begin {
        /// Result representation being accumulated.
        kind: MirGeneratorKind,
        /// Destination retaining the in-progress result.
        destination: MirPlace,
        /// Checked yielded-element type whose layout governs accumulation.
        element: TypeId,
        /// Exact element count when checking proved one.
        exact_count: Option<ConstantTermId>,
    },
    /// Append one yielded element.
    Push {
        /// In-progress result storage.
        destination: MirPlace,
        /// Element yielded by the generator body.
        value: MirOperand,
    },
    /// Finish accumulation and produce the generator expression value.
    Finish {
        /// Completed result storage.
        destination: MirPlace,
    },
}

/// One aggregate construction with operands in evaluation order.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirAggregate {
    kind: MirAggregateKind,
    operands: Arc<[MirOperand]>,
}

impl MirAggregate {
    /// Creates one normalized aggregate construction.
    pub fn new(kind: MirAggregateKind, operands: impl IntoIterator<Item = MirOperand>) -> Self {
        Self {
            kind,
            operands: shared_slice(operands),
        }
    }

    /// Returns the aggregate representation.
    pub const fn kind(&self) -> MirAggregateKind {
        self.kind
    }

    /// Returns aggregate operands in evaluation order.
    pub fn operands(&self) -> &[MirOperand] {
        &self.operands
    }
}

/// One supplied or defaulted input of a normalized construction operation.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirConstructionInput {
    input: ConstructionInputId,
    ordinal: u32,
    value: MirOperand,
}

impl MirConstructionInput {
    /// Retains an evaluated input whose default and failure handling are already explicit.
    pub const fn new(input: ConstructionInputId, ordinal: u32, value: MirOperand) -> Self {
        Self {
            input,
            ordinal,
            value,
        }
    }

    /// Returns the initialized field or parameter.
    pub const fn input(&self) -> ConstructionInputId {
        self.input
    }

    /// Returns the input's declaration-order ordinal.
    pub const fn ordinal(&self) -> u32 {
        self.ordinal
    }

    /// Returns the evaluated input transferred by construction.
    pub const fn value(&self) -> &MirOperand {
        &self.value
    }
}

/// One normalized struct, union-variant, or type-form construction.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct MirConstruction {
    target: ConstructionTarget,
    inputs: Arc<[MirConstructionInput]>,
}

impl MirConstruction {
    /// Creates one construction from its checked target and ordered inputs.
    pub fn new(
        target: ConstructionTarget,
        inputs: impl IntoIterator<Item = MirConstructionInput>,
    ) -> Self {
        Self {
            target,
            inputs: shared_slice(inputs),
        }
    }

    /// Returns the exact construction target.
    pub const fn target(&self) -> ConstructionTarget {
        self.target
    }

    /// Returns evaluated inputs in their source and default evaluation order.
    pub fn inputs(&self) -> &[MirConstructionInput] {
        &self.inputs
    }
}
