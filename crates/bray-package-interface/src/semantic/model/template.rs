use std::sync::Arc;

use bray_base::sorted_unique_shared_slice;
use bray_bound_tree::{CheckedTemplateKind, CheckedTemplateNodeId};
use bray_symbols::{
    CurrentRunCancellation, InterfaceSupportEntityId, LifecycleObligationKind, SymbolOrdinal,
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
    result_dependencies: InterfaceDependencyContractId,
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
        result_dependencies: InterfaceDependencyContractId,
        witnesses: impl IntoIterator<Item = InterfaceImplementationReference>,
    ) -> Self {
        Self {
            effects: sorted_unique_shared_slice(effects),
            capabilities: sorted_unique_shared_slice(capabilities),
            trusted_obligations: sorted_unique_shared_slice(trusted_obligations),
            execution,
            lifecycle_obligations: sorted_unique_shared_slice(lifecycle_obligations),
            dependency_contract,
            result_dependencies,
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

    /// Returns dependencies retained by the produced value after evaluation completes.
    pub const fn result_dependencies(&self) -> InterfaceDependencyContractId {
        self.result_dependencies
    }
}

/// A checked-template operation whose semantic references belong to the interface graph.
pub type InterfaceCheckedTemplateOperation = bray_bound_tree::CheckedTemplateOperation<
    InterfaceConstantTermId,
    InterfaceTypeId,
    InterfaceTemplateReference,
    InterfaceGenericSubstitutionId,
    InterfaceImplementationReference,
>;

/// The checked custom-index selection using interface-local semantic references.
pub type InterfaceCheckedTemplateIndexCall = bray_bound_tree::CheckedTemplateIndexCall<
    InterfaceTemplateReference,
    InterfaceGenericSubstitutionId,
    InterfaceImplementationReference,
    InterfaceTypeId,
>;

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

    /// Returns temporaries in allocation order with nondecreasing initializer nodes.
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

/// One declaration-owned template addressable through the record directory.
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
