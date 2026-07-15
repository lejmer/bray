use bray_symbols::TypeId;

use super::{CheckedTemplateInputId, CheckedTemplateNodeId, CheckedTemplateTemporaryId};

/// A typed failure while validating a source-independent checked template.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CheckedTemplateBuildError {
    /// A template-local identity table cannot represent another entry.
    CapacityExceeded,
    /// Semantic recovery contributed to a template that must be portable and lowerable.
    RecoveredTemplate,
    /// An operation references an input that is not declared by the template.
    MissingInput(CheckedTemplateInputId),
    /// Two inputs declare the same contextual or generic role.
    DuplicateInput {
        /// The first declaration of the input role.
        first: CheckedTemplateInputId,
        /// The repeated declaration of the input role.
        duplicate: CheckedTemplateInputId,
    },
    /// An input read produces a type different from its declaration.
    InputTypeMismatch {
        /// The operation reading the input.
        node: CheckedTemplateNodeId,
        /// The input whose declared type must be preserved.
        input: CheckedTemplateInputId,
        /// The type declared by the input.
        expected: TypeId,
        /// The type produced by the operation.
        actual: TypeId,
    },
    /// An operation or result references a node outside the template.
    MissingNode(CheckedTemplateNodeId),
    /// An operation references a node that has not yet been declared in dependency order.
    ForwardNodeReference {
        /// The operation containing the invalid reference.
        node: CheckedTemplateNodeId,
        /// The referenced node at or after the operation in dependency order.
        referenced: CheckedTemplateNodeId,
    },
    /// An operation references a temporary that is not declared by the template.
    MissingTemporary(CheckedTemplateTemporaryId),
    /// A temporary is read before its initializer has been evaluated.
    UninitializedTemporary {
        /// The operation reading the temporary.
        node: CheckedTemplateNodeId,
        /// The temporary whose initializer is not available.
        temporary: CheckedTemplateTemporaryId,
    },
    /// A temporary declaration differs from its initializer result type.
    TemporaryInitializerTypeMismatch {
        /// The operation initializing the temporary.
        initializer: CheckedTemplateNodeId,
        /// The initializer result type.
        expected: TypeId,
        /// The type declared by the temporary.
        actual: TypeId,
    },
    /// A temporary read produces a type different from the temporary.
    TemporaryTypeMismatch {
        /// The operation reading the temporary.
        node: CheckedTemplateNodeId,
        /// The temporary whose stored type must be preserved.
        temporary: CheckedTemplateTemporaryId,
        /// The type carried by the temporary.
        expected: TypeId,
        /// The type produced by the operation.
        actual: TypeId,
    },
    /// A conversion result differs from its checked destination type.
    ConversionTypeMismatch {
        /// The conversion operation.
        node: CheckedTemplateNodeId,
        /// The checked conversion target.
        expected: TypeId,
        /// The type produced by the operation.
        actual: TypeId,
    },
    /// Conditional branches do not produce one coherent result type.
    ConditionalBranchTypeMismatch {
        /// The conditional operation.
        node: CheckedTemplateNodeId,
        /// The true branch result type.
        when_true: TypeId,
        /// The false branch result type.
        when_false: TypeId,
    },
    /// A conditional result differs from its coherent branch type.
    ConditionalResultTypeMismatch {
        /// The conditional operation.
        node: CheckedTemplateNodeId,
        /// The coherent branch result type.
        expected: TypeId,
        /// The type produced by the operation.
        actual: TypeId,
    },
    /// Short-circuit operands do not have one coherent type.
    ShortCircuitOperandTypeMismatch {
        /// The short-circuit operation.
        node: CheckedTemplateNodeId,
        /// The left operand type.
        left: TypeId,
        /// The right operand type.
        right: TypeId,
    },
    /// A short-circuit result differs from its coherent operand type.
    ShortCircuitResultTypeMismatch {
        /// The short-circuit operation.
        node: CheckedTemplateNodeId,
        /// The coherent operand type.
        expected: TypeId,
        /// The type produced by the operation.
        actual: TypeId,
    },
    /// Array construction children do not have one coherent element type.
    ArrayElementTypeMismatch {
        /// The array construction operation.
        node: CheckedTemplateNodeId,
        /// The element whose type differs from the first element.
        element: CheckedTemplateNodeId,
        /// The first element type.
        expected: TypeId,
        /// The mismatched element type.
        actual: TypeId,
    },
}
