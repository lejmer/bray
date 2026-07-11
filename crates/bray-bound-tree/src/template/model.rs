use std::sync::Arc;

use bray_base::shared_slice;
use bray_symbols::{
    ConstantTermId, DependencyContractTemplateId, ExternalSymbolKey, LifecycleObligationKind,
    SymbolOrdinal, TypeId,
};

use super::{CheckedTemplateInputId, CheckedTemplateNodeId, CheckedTemplateTemporaryId};

/// The declaration-owned semantic behavior represented by a checked template.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedTemplateKind {
    /// A parameter, field, or payload runtime default.
    RuntimeDefault,
    /// A compile-time constant definition.
    ConstantDefinition,
    /// A reusable predicate definition.
    PredicateDefinition,
    /// A generic declaration constraint.
    GenericConstraint,
    /// A callable precondition, postcondition, or static contract clause.
    CallableContract,
}

/// Whether checked construction completed without semantic recovery.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedTemplateCompletion {
    /// Every operation was checked successfully and can be serialized and lowered.
    Complete,
    /// Semantic recovery contributed to the representation.
    Recovered,
}

/// The closed role of one explicit checked-template input.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedTemplateInputKind {
    /// A generic type parameter addressed by stable semantic identity.
    GenericType(ExternalSymbolKey),
    /// A generic constant parameter addressed by stable semantic identity.
    GenericConstant(ExternalSymbolKey),
    /// The receiver accepted by a declaration-owned provider.
    Receiver,
    /// A callable parameter addressed by declaration-stable ordinal.
    Parameter(SymbolOrdinal),
    /// The result visible while checking a postcondition.
    PostconditionResult,
}

/// One typed contextual or generic input declared by a checked template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedTemplateInput {
    kind: CheckedTemplateInputKind,
    ty: TypeId,
}

impl CheckedTemplateInput {
    /// Creates an explicit typed template input.
    pub const fn new(kind: CheckedTemplateInputKind, ty: TypeId) -> Self {
        Self { kind, ty }
    }

    /// Returns the input's stable semantic role.
    pub const fn kind(&self) -> &CheckedTemplateInputKind {
        &self.kind
    }

    /// Returns the checked input type.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }
}

macro_rules! define_stable_requirement {
    ($name:ident, $documentation:literal) => {
        #[doc = $documentation]
        #[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name(ExternalSymbolKey);

        impl $name {
            /// Creates a requirement from a stable declaration identity.
            pub const fn new(declaration: ExternalSymbolKey) -> Self {
                Self(declaration)
            }

            /// Returns the required declaration's stable identity.
            pub const fn declaration(&self) -> &ExternalSymbolKey {
                &self.0
            }
        }
    };
}

define_stable_requirement!(
    CheckedTemplateEffect,
    "One checked effect required or produced while evaluating a template."
);
define_stable_requirement!(
    CheckedTemplateCapability,
    "One checked capability required while evaluating a template."
);
define_stable_requirement!(
    CheckedTemplateTrustedObligation,
    "One checked trusted obligation retained by a template."
);
define_stable_requirement!(
    CheckedTemplateWitness,
    "One selected implementation required by a checked template."
);

/// Portable checked behavior retained at a declaration boundary.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedTemplateBehavior {
    effects: Arc<[CheckedTemplateEffect]>,
    capabilities: Arc<[CheckedTemplateCapability]>,
    trusted_obligations: Arc<[CheckedTemplateTrustedObligation]>,
    lifecycle_obligations: Arc<[LifecycleObligationKind]>,
    dependency_contract: DependencyContractTemplateId,
    witnesses: Arc<[CheckedTemplateWitness]>,
}

impl CheckedTemplateBehavior {
    /// Creates portable behavior in deterministic published semantic order.
    pub fn new(
        effects: impl IntoIterator<Item = CheckedTemplateEffect>,
        capabilities: impl IntoIterator<Item = CheckedTemplateCapability>,
        trusted_obligations: impl IntoIterator<Item = CheckedTemplateTrustedObligation>,
        lifecycle_obligations: impl IntoIterator<Item = LifecycleObligationKind>,
        dependency_contract: DependencyContractTemplateId,
        witnesses: impl IntoIterator<Item = CheckedTemplateWitness>,
    ) -> Self {
        Self {
            effects: shared_slice(effects),
            capabilities: shared_slice(capabilities),
            trusted_obligations: shared_slice(trusted_obligations),
            lifecycle_obligations: shared_slice(lifecycle_obligations),
            dependency_contract,
            witnesses: shared_slice(witnesses),
        }
    }

    /// Returns checked effects in published semantic order.
    pub fn effects(&self) -> &[CheckedTemplateEffect] {
        &self.effects
    }

    /// Returns checked capabilities in published semantic order.
    pub fn capabilities(&self) -> &[CheckedTemplateCapability] {
        &self.capabilities
    }

    /// Returns trusted obligations in published semantic order.
    pub fn trusted_obligations(&self) -> &[CheckedTemplateTrustedObligation] {
        &self.trusted_obligations
    }

    /// Returns lifecycle obligations in published semantic order.
    pub fn lifecycle_obligations(&self) -> &[LifecycleObligationKind] {
        &self.lifecycle_obligations
    }

    /// Returns the normalized portable dependency contract.
    pub const fn dependency_contract(&self) -> DependencyContractTemplateId {
        self.dependency_contract
    }

    /// Returns selected implementation witnesses in published semantic order.
    pub fn witnesses(&self) -> &[CheckedTemplateWitness] {
        &self.witnesses
    }
}

/// The closed normalized operation vocabulary of a checked template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedTemplateOperation {
    /// Reads one explicitly declared contextual or generic input.
    Input(CheckedTemplateInputId),
    /// Materializes an already checked open or closed constant term.
    Constant(ConstantTermId),
    /// Reads a declaration-owned value through stable semantic identity.
    Declaration(ExternalSymbolKey),
    /// Calls one selected declaration with deterministic argument order.
    Call {
        /// The selected callable declaration.
        callable: ExternalSymbolKey,
        /// Arguments in exact evaluation and parameter order.
        arguments: Arc<[CheckedTemplateNodeId]>,
        /// The selected implementation witness when dispatch requires one.
        implementation: Option<ExternalSymbolKey>,
    },
    /// Applies an already checked semantic conversion.
    Convert {
        /// The converted value.
        value: CheckedTemplateNodeId,
        /// The checked destination type.
        target: TypeId,
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
        member: ExternalSymbolKey,
    },
    /// Reads one explicitly materialized template-local temporary.
    Temporary(CheckedTemplateTemporaryId),
}

impl CheckedTemplateOperation {
    /// Creates a selected call operation with stable argument order.
    pub fn call(
        callable: ExternalSymbolKey,
        arguments: impl IntoIterator<Item = CheckedTemplateNodeId>,
        implementation: Option<ExternalSymbolKey>,
    ) -> Self {
        Self::Call {
            callable,
            arguments: shared_slice(arguments),
            implementation,
        }
    }

    /// Creates an ordered tuple construction.
    pub fn tuple(elements: impl IntoIterator<Item = CheckedTemplateNodeId>) -> Self {
        Self::Tuple(shared_slice(elements))
    }

    /// Creates an ordered array construction.
    pub fn array(elements: impl IntoIterator<Item = CheckedTemplateNodeId>) -> Self {
        Self::Array(shared_slice(elements))
    }

    pub(crate) fn node_references(&self) -> &[CheckedTemplateNodeId] {
        match self {
            Self::Call { arguments, .. } | Self::Tuple(arguments) | Self::Array(arguments) => {
                arguments
            }
            Self::Convert { value, .. } => std::slice::from_ref(value),
            Self::Project { subject, .. } => std::slice::from_ref(subject),
            Self::Input(_) | Self::Constant(_) | Self::Declaration(_) | Self::Temporary(_) => &[],
        }
    }
}

/// One typed normalized operation in deterministic evaluation order.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedTemplateNode {
    operation: CheckedTemplateOperation,
    ty: TypeId,
}

impl CheckedTemplateNode {
    /// Creates one typed checked-template operation.
    pub const fn new(operation: CheckedTemplateOperation, ty: TypeId) -> Self {
        Self { operation, ty }
    }

    /// Returns the normalized operation.
    pub const fn operation(&self) -> &CheckedTemplateOperation {
        &self.operation
    }

    /// Returns the checked result type.
    pub const fn ty(&self) -> TypeId {
        self.ty
    }
}

/// One explicitly materialized template-local temporary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedTemplateTemporary {
    initializer: CheckedTemplateNodeId,
    ty: TypeId,
    dependency_contract: DependencyContractTemplateId,
}

impl CheckedTemplateTemporary {
    pub(crate) const fn new(
        initializer: CheckedTemplateNodeId,
        ty: TypeId,
        dependency_contract: DependencyContractTemplateId,
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
    pub const fn ty(self) -> TypeId {
        self.ty
    }

    /// Returns the dependencies carried by the stored value.
    pub const fn dependency_contract(self) -> DependencyContractTemplateId {
        self.dependency_contract
    }
}

/// An immutable source-independent checked declaration-owned template.
///
/// Canonical type, constant-term, and dependency-contract handles are resolved through their
/// semantic store when encoding. Their store-local numeric representations are never serialized.
/// All operation identities are template-local, and all declaration references use stable
/// external symbol keys.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedTemplate {
    kind: CheckedTemplateKind,
    inputs: Arc<[CheckedTemplateInput]>,
    nodes: Arc<[CheckedTemplateNode]>,
    temporaries: Arc<[CheckedTemplateTemporary]>,
    result: CheckedTemplateNodeId,
    behavior: CheckedTemplateBehavior,
}

impl CheckedTemplate {
    pub(crate) fn new(
        kind: CheckedTemplateKind,
        inputs: impl IntoIterator<Item = CheckedTemplateInput>,
        nodes: impl IntoIterator<Item = CheckedTemplateNode>,
        temporaries: impl IntoIterator<Item = CheckedTemplateTemporary>,
        result: CheckedTemplateNodeId,
        behavior: CheckedTemplateBehavior,
    ) -> Self {
        Self {
            kind,
            inputs: shared_slice(inputs),
            nodes: shared_slice(nodes),
            temporaries: shared_slice(temporaries),
            result,
            behavior,
        }
    }

    /// Returns the declaration-owned semantic category.
    pub const fn kind(&self) -> CheckedTemplateKind {
        self.kind
    }

    /// Returns explicit generic and contextual inputs in binding order.
    pub fn inputs(&self) -> &[CheckedTemplateInput] {
        &self.inputs
    }

    /// Returns normalized operations in deterministic evaluation order.
    pub fn nodes(&self) -> &[CheckedTemplateNode] {
        &self.nodes
    }

    /// Returns explicitly materialized temporaries in deterministic allocation order.
    pub fn temporaries(&self) -> &[CheckedTemplateTemporary] {
        &self.temporaries
    }

    /// Returns the operation producing the template result.
    pub const fn result(&self) -> CheckedTemplateNodeId {
        self.result
    }

    /// Returns effects, capabilities, dependencies, and implementation witnesses.
    pub const fn behavior(&self) -> &CheckedTemplateBehavior {
        &self.behavior
    }
}
