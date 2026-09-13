use std::sync::Arc;

use bray_base::{shared_slice, sorted_unique_shared_slice};
use bray_symbols::{
    BorrowKind, ConstantBinaryOperation, ConstantTermId, ConstantUnaryOperation,
    CurrentRunCancellation, DependencyContractTemplateId, GenericSubstitutionId,
    LifecycleObligationKind, SymbolKey, SymbolKind, SymbolOrdinal, TypeId,
};

use super::{CheckedTemplateInputId, CheckedTemplateNodeId, CheckedTemplateTemporaryId};

/// The declaration-owned semantic behavior represented by a checked template.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedTemplateKind {
    /// A parameter, field, or payload runtime default.
    RuntimeDefault,
    /// A compile-time constant definition.
    ConstantDefinition,
    /// A product-static constant initializer.
    ProductStaticInitializer,
    /// An exact-thread static constant initializer.
    ThreadLocalStaticInitializer,
    /// A reusable predicate definition.
    PredicateDefinition,
    /// A generic declaration constraint.
    GenericConstraint,
    /// A callable precondition, postcondition, or static contract clause.
    CallableContract,
    /// A checked const-callable body retained for cross-package evaluation.
    ConstantCallableBody,
}

impl CheckedTemplateKind {
    /// Returns whether this template category can be owned by the supplied declaration kind.
    pub const fn accepts_owner(self, owner: SymbolKind) -> bool {
        match self {
            Self::RuntimeDefault => matches!(
                owner,
                SymbolKind::CallableParameterDefaultProvider
                    | SymbolKind::StructFieldDefaultProvider
                    | SymbolKind::UnionPayloadDefaultProvider
            ),
            Self::ConstantDefinition => matches!(
                owner,
                SymbolKind::Constant
                    | SymbolKind::TraitConstantMember
                    | SymbolKind::TraitConstantFulfillment
            ),
            Self::ProductStaticInitializer | Self::ThreadLocalStaticInitializer => {
                matches!(owner, SymbolKind::Static)
            }
            Self::PredicateDefinition => matches!(
                owner,
                SymbolKind::Predicate
                    | SymbolKind::TraitPredicateMember
                    | SymbolKind::TraitPredicateFulfillment
            ),
            Self::GenericConstraint => owner.can_be_source_declared(),
            Self::CallableContract => {
                matches!(owner, SymbolKind::CallableContract) || owner.is_callable()
            }
            Self::ConstantCallableBody => owner.is_callable(),
        }
    }
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
    GenericType(SymbolKey),
    /// A generic constant parameter addressed by stable semantic identity.
    GenericConstant(SymbolKey),
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
        pub struct $name(SymbolKey);

        impl $name {
            /// Creates a requirement from a stable declaration identity.
            pub const fn new(declaration: SymbolKey) -> Self {
                Self(declaration)
            }

            /// Returns the required declaration's stable identity.
            pub const fn declaration(&self) -> &SymbolKey {
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
    CheckedTemplateExecutionRequirement,
    "One checked execution-lane predicate required while evaluating a template."
);

/// Execution-context behavior retained by a checked template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedTemplateExecution {
    requirements: Arc<[CheckedTemplateExecutionRequirement]>,
    current_run_cancellation: CurrentRunCancellation,
}

impl CheckedTemplateExecution {
    /// Creates normalized execution-context behavior.
    pub fn new(
        requirements: impl IntoIterator<Item = CheckedTemplateExecutionRequirement>,
        current_run_cancellation: CurrentRunCancellation,
    ) -> Self {
        Self {
            requirements: sorted_unique_shared_slice(requirements),
            current_run_cancellation,
        }
    }

    /// Returns execution-lane requirements in canonical semantic-set order.
    pub fn requirements(&self) -> &[CheckedTemplateExecutionRequirement] {
        &self.requirements
    }

    /// Returns whether evaluation can enter cancellation for its current run.
    pub const fn current_run_cancellation(&self) -> CurrentRunCancellation {
        self.current_run_cancellation
    }
}
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
    execution: CheckedTemplateExecution,
    lifecycle_obligations: Arc<[LifecycleObligationKind]>,
    dependency_contract: DependencyContractTemplateId,
    result_dependencies: DependencyContractTemplateId,
    witnesses: Arc<[CheckedTemplateWitness]>,
}

impl CheckedTemplateBehavior {
    /// Creates portable behavior as canonical sorted semantic sets.
    pub fn new(
        effects: impl IntoIterator<Item = CheckedTemplateEffect>,
        capabilities: impl IntoIterator<Item = CheckedTemplateCapability>,
        trusted_obligations: impl IntoIterator<Item = CheckedTemplateTrustedObligation>,
        execution: CheckedTemplateExecution,
        lifecycle_obligations: impl IntoIterator<Item = LifecycleObligationKind>,
        dependency_contract: DependencyContractTemplateId,
        result_dependencies: DependencyContractTemplateId,
        witnesses: impl IntoIterator<Item = CheckedTemplateWitness>,
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
    pub fn effects(&self) -> &[CheckedTemplateEffect] {
        &self.effects
    }

    /// Returns checked capabilities in canonical semantic-set order.
    pub fn capabilities(&self) -> &[CheckedTemplateCapability] {
        &self.capabilities
    }

    /// Returns trusted obligations in canonical semantic-set order.
    pub fn trusted_obligations(&self) -> &[CheckedTemplateTrustedObligation] {
        &self.trusted_obligations
    }

    /// Returns execution-lane requirements in canonical semantic-set order.
    pub fn execution_requirements(&self) -> &[CheckedTemplateExecutionRequirement] {
        self.execution.requirements()
    }

    /// Returns lifecycle obligations in canonical semantic-set order.
    pub fn lifecycle_obligations(&self) -> &[LifecycleObligationKind] {
        &self.lifecycle_obligations
    }

    /// Returns the normalized portable dependency contract.
    pub const fn dependency_contract(&self) -> DependencyContractTemplateId {
        self.dependency_contract
    }

    /// Returns whether evaluation can enter cancellation for its current run.
    pub const fn current_run_cancellation(&self) -> CurrentRunCancellation {
        self.execution.current_run_cancellation()
    }

    /// Returns selected implementation witnesses in canonical semantic-set order.
    pub fn witnesses(&self) -> &[CheckedTemplateWitness] {
        &self.witnesses
    }

    /// Returns dependencies retained by the produced value after evaluation completes.
    pub const fn result_dependencies(&self) -> DependencyContractTemplateId {
        self.result_dependencies
    }
}

/// The exact lazy boolean evaluation rule of a short-circuit operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedTemplateShortCircuitKind {
    /// Evaluates the right operand only when the left operand is true.
    And,
    /// Evaluates the right operand only when the left operand is false.
    Or,
}

/// Deterministic materialization work retained with one checked constant term.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CheckedTemplateConstantUsage {
    aggregate_elements: u64,
    literal_bytes: u64,
    expansions: u64,
}

impl CheckedTemplateConstantUsage {
    /// Creates an explicit constant materialization summary.
    pub const fn new(aggregate_elements: u64, literal_bytes: u64, expansions: u64) -> Self {
        Self {
            aggregate_elements,
            literal_bytes,
            expansions,
        }
    }

    /// Returns aggregate elements materialized while producing the term.
    pub const fn aggregate_elements(self) -> u64 {
        self.aggregate_elements
    }

    /// Returns source literal bytes decoded while producing the term.
    pub const fn literal_bytes(self) -> u64 {
        self.literal_bytes
    }

    /// Returns elements produced through constant expansion.
    pub const fn expansions(self) -> u64 {
        self.expansions
    }
}

/// The closed normalized operation vocabulary of a checked template.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CheckedTemplateOperation {
    /// Reads one explicitly declared contextual or generic input.
    Input(CheckedTemplateInputId),
    /// Materializes an already checked open or closed constant term.
    Constant {
        /// The checked open or closed constant term.
        term: ConstantTermId,
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
    /// Borrows one evaluated place with its checked capability.
    Borrow {
        /// Exact borrow capability.
        kind: BorrowKind,
        /// Place evaluated before creating the borrow.
        operand: CheckedTemplateNodeId,
    },
    /// Reads a declaration-owned value through stable semantic identity.
    Declaration {
        /// Selected declaration.
        declaration: SymbolKey,
        /// Exact closed generic application when the declaration is selected explicitly.
        substitution: Option<GenericSubstitutionId>,
    },
    /// Applies one selected callable or predicate with deterministic argument order.
    Call {
        /// The selected callable or predicate declaration.
        callable: SymbolKey,
        /// Ordered generic arguments applied to the callable declaration.
        substitution: GenericSubstitutionId,
        /// Arguments in exact evaluation and parameter order.
        arguments: Arc<[CheckedTemplateNodeId]>,
        /// The selected implementation witness when dispatch requires one.
        implementation: Option<(SymbolKey, GenericSubstitutionId)>,
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
        member: SymbolKey,
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
    /// Evaluates the left operand and evaluates the right operand only when required.
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

impl CheckedTemplateOperation {
    /// Creates a selected callable or predicate application with stable argument order.
    pub fn call(
        callable: SymbolKey,
        substitution: GenericSubstitutionId,
        arguments: impl IntoIterator<Item = CheckedTemplateNodeId>,
        implementation: Option<(SymbolKey, GenericSubstitutionId)>,
    ) -> Self {
        Self::Call {
            callable,
            substitution,
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

    pub(crate) fn try_for_each_node_reference<E>(
        &self,
        mut visit: impl FnMut(CheckedTemplateNodeId) -> Result<(), E>,
    ) -> Result<(), E> {
        match self {
            Self::Call { arguments, .. } | Self::Tuple(arguments) | Self::Array(arguments) => {
                for argument in arguments.iter() {
                    visit(*argument)?;
                }
            }
            Self::Convert { value, .. } => visit(*value)?,
            Self::Unary { operand, .. } | Self::Borrow { operand, .. } => visit(*operand)?,
            Self::Binary { left, right, .. } => {
                visit(*left)?;
                visit(*right)?;
            }
            Self::Project { subject, .. } => visit(*subject)?,
            Self::Conditional {
                condition,
                when_true,
                when_false,
            } => {
                visit(*condition)?;
                visit(*when_true)?;
                visit(*when_false)?;
            }
            Self::ShortCircuit { left, right, .. } => {
                visit(*left)?;
                visit(*right)?;
            }
            Self::Input(_)
            | Self::Constant { .. }
            | Self::Declaration { .. }
            | Self::Temporary(_) => {}
        }

        Ok(())
    }
}

/// One typed normalized operation in deterministic dependency order.
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
/// stable symbol keys.
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

    /// Returns normalized operations in deterministic dependency order.
    ///
    /// Consumers begin with [`Self::result`] and follow operation semantics. Table order alone is
    /// not an evaluation schedule because conditional and short-circuit children are lazy.
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

#[cfg(test)]
mod tests {
    use bray_symbols::{
        DependencyContractTemplateData, LifecycleObligationKind, PackageIdentity,
        SemanticValueStore, SymbolKey,
    };

    use super::{
        CheckedTemplateBehavior, CheckedTemplateCapability, CheckedTemplateEffect,
        CheckedTemplateExecution, CheckedTemplateExecutionRequirement,
        CheckedTemplateTrustedObligation, CheckedTemplateWitness,
    };

    #[test]
    fn behavior_sets_are_canonical_across_permutations_and_duplicates() {
        let dependencies = dependencies();
        let first_key = external_key("example.first");
        let second_key = external_key("example.second");

        let first = CheckedTemplateBehavior::new(
            [
                CheckedTemplateEffect::new(second_key.clone()),
                CheckedTemplateEffect::new(first_key.clone()),
                CheckedTemplateEffect::new(first_key.clone()),
            ],
            [
                CheckedTemplateCapability::new(second_key.clone()),
                CheckedTemplateCapability::new(first_key.clone()),
                CheckedTemplateCapability::new(second_key.clone()),
            ],
            [
                CheckedTemplateTrustedObligation::new(first_key.clone()),
                CheckedTemplateTrustedObligation::new(second_key.clone()),
                CheckedTemplateTrustedObligation::new(first_key.clone()),
            ],
            CheckedTemplateExecution::new(
                [
                    CheckedTemplateExecutionRequirement::new(second_key.clone()),
                    CheckedTemplateExecutionRequirement::new(first_key.clone()),
                    CheckedTemplateExecutionRequirement::new(second_key.clone()),
                ],
                bray_symbols::CurrentRunCancellation::MayEnter,
            ),
            [
                LifecycleObligationKind::Joining,
                LifecycleObligationKind::Destruction,
                LifecycleObligationKind::Joining,
            ],
            dependencies,
            dependencies,
            [
                CheckedTemplateWitness::new(second_key.clone()),
                CheckedTemplateWitness::new(first_key.clone()),
                CheckedTemplateWitness::new(second_key.clone()),
            ],
        );

        let second = CheckedTemplateBehavior::new(
            [
                CheckedTemplateEffect::new(first_key.clone()),
                CheckedTemplateEffect::new(second_key.clone()),
            ],
            [
                CheckedTemplateCapability::new(first_key.clone()),
                CheckedTemplateCapability::new(second_key.clone()),
            ],
            [
                CheckedTemplateTrustedObligation::new(second_key.clone()),
                CheckedTemplateTrustedObligation::new(first_key.clone()),
            ],
            CheckedTemplateExecution::new(
                [
                    CheckedTemplateExecutionRequirement::new(first_key.clone()),
                    CheckedTemplateExecutionRequirement::new(second_key.clone()),
                ],
                bray_symbols::CurrentRunCancellation::MayEnter,
            ),
            [
                LifecycleObligationKind::Destruction,
                LifecycleObligationKind::Joining,
            ],
            dependencies,
            dependencies,
            [
                CheckedTemplateWitness::new(first_key),
                CheckedTemplateWitness::new(second_key),
            ],
        );

        assert_eq!(first, second);
        assert_eq!(first.effects().len(), 2);
        assert_eq!(first.execution_requirements().len(), 2);
        assert_eq!(first.capabilities().len(), 2);
        assert_eq!(first.trusted_obligations().len(), 2);
        assert_eq!(first.lifecycle_obligations().len(), 2);
        assert_eq!(first.witnesses().len(), 2);
    }

    fn dependencies() -> bray_symbols::DependencyContractTemplateId {
        let Ok(store) = SemanticValueStore::try_new() else {
            panic!("semantic value store identity must be available");
        };

        let Ok(dependencies) =
            store.intern_dependency_contract_template(DependencyContractTemplateData::new([]))
        else {
            panic!("empty dependency contract must be valid");
        };

        dependencies
    }

    fn external_key(package: &str) -> SymbolKey {
        let Some(package) = PackageIdentity::try_new(package) else {
            panic!("test package identity must be valid");
        };

        SymbolKey::external(bray_symbols::ExternalSymbolKey::package(package))
    }
}
