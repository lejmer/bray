use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;
use bray_bound_tree::{
    CheckedTemplateConstantUsage, CheckedTemplateInputId, CheckedTemplateKind,
    CheckedTemplateNodeId, CheckedTemplateShortCircuitKind, CheckedTemplateTemporaryId,
};
use bray_symbols::{
    ConstantBinaryOperation, ConstantUnaryOperation, CurrentRunCancellation,
    InterfaceSupportEntityId, LifecycleObligationKind, SymbolOrdinal,
};

use super::{
    InterfaceConstantTermId, InterfaceDependencyContractId, InterfaceGenericSubstitutionId,
    InterfaceImplementationReference, InterfaceTemplateReference, InterfaceTypeId,
};
use crate::InterfaceSymbolReference;

/// The closed role of one explicit interface checked-template input.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceCheckedTemplateInputKind {
    /// A generic type parameter addressed through the interface graph.
    GenericType(InterfaceSymbolReference),
    /// A generic constant parameter addressed through the interface graph.
    GenericConstant(InterfaceSymbolReference),
    /// The receiver accepted by a declaration-owned provider.
    Receiver,
    /// A callable parameter addressed by declaration-stable ordinal.
    Parameter(SymbolOrdinal),
    /// The result visible while checking a postcondition.
    PostconditionResult,
}

/// One typed contextual or generic input declared by an interface template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCheckedTemplateInput {
    kind: InterfaceCheckedTemplateInputKind,
    ty: InterfaceTypeId,
}

impl InterfaceCheckedTemplateInput {
    /// Creates one explicit typed template input.
    pub const fn new(kind: InterfaceCheckedTemplateInputKind, ty: InterfaceTypeId) -> Self {
        Self { kind, ty }
    }

    /// Returns the input's stable semantic role.
    pub const fn kind(&self) -> &InterfaceCheckedTemplateInputKind {
        &self.kind
    }

    /// Returns the checked input type.
    pub const fn ty(&self) -> InterfaceTypeId {
        self.ty
    }
}

/// Portable checked behavior retained by one interface template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCheckedTemplateExecution {
    requirements: Arc<[InterfaceSymbolReference]>,
    current_run_cancellation: CurrentRunCancellation,
}

impl InterfaceCheckedTemplateExecution {
    /// Creates normalized execution-context behavior.
    pub fn new(
        requirements: impl IntoIterator<Item = InterfaceSymbolReference>,
        current_run_cancellation: CurrentRunCancellation,
    ) -> Self {
        Self {
            requirements: sorted_unique_shared_slice(requirements),
            current_run_cancellation,
        }
    }

    /// Returns execution-lane requirements in canonical semantic-set order.
    pub fn requirements(&self) -> &[InterfaceSymbolReference] {
        &self.requirements
    }

    /// Returns whether evaluation can enter cancellation for its current run.
    pub const fn current_run_cancellation(&self) -> CurrentRunCancellation {
        self.current_run_cancellation
    }
}

/// Portable checked behavior retained by one interface template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCheckedTemplateBehavior {
    effects: Arc<[InterfaceSymbolReference]>,
    capabilities: Arc<[InterfaceSymbolReference]>,
    trusted_obligations: Arc<[InterfaceSymbolReference]>,
    execution: InterfaceCheckedTemplateExecution,
    lifecycle_obligations: Arc<[LifecycleObligationKind]>,
    dependency_contract: InterfaceDependencyContractId,
    witnesses: Arc<[InterfaceImplementationReference]>,
}

impl InterfaceCheckedTemplateBehavior {
    /// Creates checked behavior in the caller's canonical semantic-set order.
    pub fn new(
        effects: impl IntoIterator<Item = InterfaceSymbolReference>,
        capabilities: impl IntoIterator<Item = InterfaceSymbolReference>,
        trusted_obligations: impl IntoIterator<Item = InterfaceSymbolReference>,
        execution: InterfaceCheckedTemplateExecution,
        lifecycle_obligations: impl IntoIterator<Item = LifecycleObligationKind>,
        dependency_contract: InterfaceDependencyContractId,
        witnesses: impl IntoIterator<Item = InterfaceImplementationReference>,
    ) -> Self {
        Self {
            effects: sorted_unique_shared_slice(effects),
            capabilities: sorted_unique_shared_slice(capabilities),
            trusted_obligations: sorted_unique_shared_slice(trusted_obligations),
            execution,
            lifecycle_obligations: sorted_unique_shared_slice(lifecycle_obligations),
            dependency_contract,
            witnesses: sorted_unique_shared_slice(witnesses),
        }
    }

    /// Returns checked effects in canonical semantic-set order.
    pub fn effects(&self) -> &[InterfaceSymbolReference] {
        &self.effects
    }

    /// Returns checked capabilities in canonical semantic-set order.
    pub fn capabilities(&self) -> &[InterfaceSymbolReference] {
        &self.capabilities
    }

    /// Returns trusted obligations in canonical semantic-set order.
    pub fn trusted_obligations(&self) -> &[InterfaceSymbolReference] {
        &self.trusted_obligations
    }

    /// Returns execution-lane requirements in canonical semantic-set order.
    pub fn execution_requirements(&self) -> &[InterfaceSymbolReference] {
        self.execution.requirements()
    }

    /// Returns lifecycle obligations in canonical semantic-set order.
    pub fn lifecycle_obligations(&self) -> &[LifecycleObligationKind] {
        &self.lifecycle_obligations
    }

    /// Returns the normalized portable dependency contract.
    pub const fn dependency_contract(&self) -> InterfaceDependencyContractId {
        self.dependency_contract
    }

    /// Returns whether evaluation can enter cancellation for its current run.
    pub const fn current_run_cancellation(&self) -> CurrentRunCancellation {
        self.execution.current_run_cancellation()
    }

    /// Returns selected implementations in canonical semantic-set order.
    pub fn witnesses(&self) -> &[InterfaceImplementationReference] {
        &self.witnesses
    }
}

/// The closed normalized operation vocabulary of an interface template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceCheckedTemplateOperation {
    /// Reads one explicitly declared contextual or generic input.
    Input(CheckedTemplateInputId),
    /// Materializes an already checked open or closed constant term.
    Constant {
        /// The checked open or closed constant term.
        term: InterfaceConstantTermId,
        /// Materialization work no longer recoverable from a closed value.
        usage: CheckedTemplateConstantUsage,
    },
    /// Applies a selected unary constant operation.
    Unary {
        /// Exact checked operation.
        operation: ConstantUnaryOperation,
        /// Operand evaluated before the operation.
        operand: CheckedTemplateNodeId,
    },
    /// Applies a selected binary constant operation.
    Binary {
        /// Exact checked operation.
        operation: ConstantBinaryOperation,
        /// Left operand evaluated first.
        left: CheckedTemplateNodeId,
        /// Right operand evaluated second unless the operation short-circuits.
        right: CheckedTemplateNodeId,
    },
    /// Reads a declaration-owned value.
    Declaration(InterfaceTemplateReference),
    /// Calls one selected declaration with deterministic argument order.
    Call {
        /// The selected callable or predicate declaration.
        callable: InterfaceTemplateReference,
        /// Ordered generic arguments applied to the callable declaration.
        substitution: InterfaceGenericSubstitutionId,
        /// Arguments in exact evaluation and parameter order.
        arguments: Arc<[CheckedTemplateNodeId]>,
        /// The selected implementation witness when dispatch requires one.
        implementation: Option<(
            InterfaceImplementationReference,
            InterfaceGenericSubstitutionId,
        )>,
    },
    /// Applies an already checked semantic conversion.
    Convert {
        /// The converted value.
        value: CheckedTemplateNodeId,
        /// The checked destination type.
        target: InterfaceTypeId,
    },
    /// Constructs a tuple from values in element order.
    Tuple(Arc<[CheckedTemplateNodeId]>),
    /// Constructs an array from values in element order.
    Array(Arc<[CheckedTemplateNodeId]>),
    /// Projects a selected declaration-owned member from a value.
    Project {
        /// The projected subject.
        subject: CheckedTemplateNodeId,
        /// The selected field, payload, or associated declaration.
        member: InterfaceTemplateReference,
    },
    /// Evaluates a condition once and then exactly one selected branch.
    Conditional {
        /// The condition evaluated before either branch.
        condition: CheckedTemplateNodeId,
        /// The result evaluated only when the condition is true.
        when_true: CheckedTemplateNodeId,
        /// The result evaluated only when the condition is false.
        when_false: CheckedTemplateNodeId,
    },
    /// Evaluates the left operand and conditionally evaluates the right operand.
    ShortCircuit {
        /// The exact conjunction or disjunction evaluation rule.
        kind: CheckedTemplateShortCircuitKind,
        /// The operand evaluated first.
        left: CheckedTemplateNodeId,
        /// The operand evaluated conditionally.
        right: CheckedTemplateNodeId,
    },
    /// Reads one explicitly materialized template-local temporary.
    Temporary(CheckedTemplateTemporaryId),
}

impl InterfaceCheckedTemplateOperation {
    /// Creates a selected call operation with stable argument order.
    pub fn call(
        callable: InterfaceTemplateReference,
        substitution: InterfaceGenericSubstitutionId,
        arguments: impl IntoIterator<Item = CheckedTemplateNodeId>,
        implementation: Option<(
            InterfaceImplementationReference,
            InterfaceGenericSubstitutionId,
        )>,
    ) -> Self {
        Self::Call {
            callable,
            substitution,
            arguments: arguments.into_iter().collect(),
            implementation,
        }
    }

    /// Creates an ordered tuple construction.
    pub fn tuple(elements: impl IntoIterator<Item = CheckedTemplateNodeId>) -> Self {
        Self::Tuple(elements.into_iter().collect())
    }

    /// Creates an ordered array construction.
    pub fn array(elements: impl IntoIterator<Item = CheckedTemplateNodeId>) -> Self {
        Self::Array(elements.into_iter().collect())
    }
}

/// One typed normalized operation in deterministic dependency order.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCheckedTemplateNode {
    operation: InterfaceCheckedTemplateOperation,
    ty: InterfaceTypeId,
}

impl InterfaceCheckedTemplateNode {
    /// Creates one typed interface-template operation.
    pub const fn new(operation: InterfaceCheckedTemplateOperation, ty: InterfaceTypeId) -> Self {
        Self { operation, ty }
    }

    /// Returns the normalized operation.
    pub const fn operation(&self) -> &InterfaceCheckedTemplateOperation {
        &self.operation
    }

    /// Returns the checked result type.
    pub const fn ty(&self) -> InterfaceTypeId {
        self.ty
    }
}

/// One explicitly materialized interface-template temporary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCheckedTemplateTemporary {
    initializer: CheckedTemplateNodeId,
    ty: InterfaceTypeId,
    dependency_contract: InterfaceDependencyContractId,
}

impl InterfaceCheckedTemplateTemporary {
    /// Creates one typed temporary initialized by an earlier template node.
    pub const fn new(
        initializer: CheckedTemplateNodeId,
        ty: InterfaceTypeId,
        dependency_contract: InterfaceDependencyContractId,
    ) -> Self {
        Self {
            initializer,
            ty,
            dependency_contract,
        }
    }

    /// Returns the node whose value initializes this temporary.
    pub const fn initializer(self) -> CheckedTemplateNodeId {
        self.initializer
    }

    /// Returns the checked temporary type.
    pub const fn ty(self) -> InterfaceTypeId {
        self.ty
    }

    /// Returns the dependencies carried by the stored value.
    pub const fn dependency_contract(self) -> InterfaceDependencyContractId {
        self.dependency_contract
    }
}

/// An immutable source-independent checked template using artifact-local references.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceCheckedTemplate {
    kind: CheckedTemplateKind,
    inputs: Arc<[InterfaceCheckedTemplateInput]>,
    nodes: Arc<[InterfaceCheckedTemplateNode]>,
    temporaries: Arc<[InterfaceCheckedTemplateTemporary]>,
    result: CheckedTemplateNodeId,
    behavior: InterfaceCheckedTemplateBehavior,
}

impl InterfaceCheckedTemplate {
    /// Creates one complete checked interface template.
    pub fn new(
        kind: CheckedTemplateKind,
        inputs: impl IntoIterator<Item = InterfaceCheckedTemplateInput>,
        nodes: impl IntoIterator<Item = InterfaceCheckedTemplateNode>,
        temporaries: impl IntoIterator<Item = InterfaceCheckedTemplateTemporary>,
        result: CheckedTemplateNodeId,
        behavior: InterfaceCheckedTemplateBehavior,
    ) -> Self {
        Self {
            kind,
            inputs: inputs.into_iter().collect(),
            nodes: nodes.into_iter().collect(),
            temporaries: temporaries.into_iter().collect(),
            result,
            behavior,
        }
    }

    /// Returns the declaration-owned semantic category.
    pub const fn kind(&self) -> CheckedTemplateKind {
        self.kind
    }

    /// Returns explicit generic and contextual inputs in binding order.
    pub fn inputs(&self) -> &[InterfaceCheckedTemplateInput] {
        &self.inputs
    }

    /// Returns normalized operations in deterministic dependency order.
    pub fn nodes(&self) -> &[InterfaceCheckedTemplateNode] {
        &self.nodes
    }

    /// Returns explicitly materialized temporaries in deterministic allocation order.
    pub fn temporaries(&self) -> &[InterfaceCheckedTemplateTemporary] {
        &self.temporaries
    }

    /// Returns the operation producing the template result.
    pub const fn result(&self) -> CheckedTemplateNodeId {
        self.result
    }

    /// Returns effects, capabilities, dependencies, and implementation witnesses.
    pub const fn behavior(&self) -> &InterfaceCheckedTemplateBehavior {
        &self.behavior
    }
}

/// One declaration-owned template addressable through the fact directory.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InterfaceDeclarationTemplate {
    owner: InterfaceSymbolReference,
    kind: CheckedTemplateKind,
    ordinal: SymbolOrdinal,
    entity: InterfaceSupportEntityId,
}

impl InterfaceDeclarationTemplate {
    /// Creates one stable declaration-to-template support edge.
    pub const fn new(
        owner: InterfaceSymbolReference,
        kind: CheckedTemplateKind,
        ordinal: SymbolOrdinal,
        entity: InterfaceSupportEntityId,
    ) -> Self {
        Self {
            owner,
            kind,
            ordinal,
            entity,
        }
    }

    /// Returns the declaration that owns this template.
    pub const fn owner(&self) -> &InterfaceSymbolReference {
        &self.owner
    }

    /// Returns the declaration-owned semantic category.
    pub const fn kind(&self) -> CheckedTemplateKind {
        self.kind
    }

    /// Returns the stable ordinal within the owner and template category.
    pub const fn ordinal(&self) -> SymbolOrdinal {
        self.ordinal
    }

    /// Returns the private support entity containing the checked template.
    pub const fn entity(&self) -> InterfaceSupportEntityId {
        self.entity
    }
}
