use std::sync::Arc;

use bray_base::NonEmptySharedStr;
use bray_bound_tree::{CheckedTemplate, CheckedTemplateKind};
use bray_compiler_known::CompilerKnownDeclarationKey;
use bray_symbols::{
    AnySymbolId, CallableContractSet, CallableInstanceId, CallableSymbolId, CheckedConstraint,
    ConstantTermId, ConstantValueId, DependencyContractTemplateId, GenericOwnerId,
    GenericSubstitutionId, ImplementationCoherenceEvidence, ImplementationInstanceId,
    ImplementationSubject, ImplementationSymbolId, SemanticValueStore, SemanticValueStoreError,
    SymbolKeyData, TargetPropertyDependency, TraitApplicationId, TypeId,
};

use crate::{InterfaceSemanticRecordKind, InterfaceSemantics, InterfaceSymbolReference};

use super::InternState;
use super::declaration_model::{
    ImportedCallableParameterDefault, ImportedCallableSignature, ImportedGenericDeclaration,
    ImportedPredicateDefinition,
};

/// Resolves artifact-local and dependency symbol references into one compilation snapshot.
pub trait InterfaceSymbolResolver {
    /// Resolves one validated interface symbol reference.
    fn resolve(&self, reference: &InterfaceSymbolReference) -> Option<AnySymbolId>;

    /// Resolves one validated interface symbol reference to its stable semantic identity.
    fn symbol_key(&self, reference: &InterfaceSymbolReference) -> Option<bray_symbols::SymbolKey>;
}

/// Failure while publishing decoded interface semantics into a compilation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum InterfaceSemanticInternError {
    /// A validated interface reference was not present in the imported symbol maps.
    UnresolvedSymbol(InterfaceSymbolReference),
    /// A resolved symbol had a category incompatible with the semantic record.
    InvalidSymbolKind(InterfaceSymbolReference),
    /// The decoded semantic graph cannot be represented by the current canonical store.
    UnresolvedValueGraph,
    /// The canonical semantic store rejected a decoded value.
    SemanticStore(SemanticValueStoreError),
    /// A checked-template graph failed ordinary template validation after remapping.
    InvalidTemplate(bray_bound_tree::CheckedTemplateBuildError),
    /// A declaration-owned template references an incompatible private support entity.
    InvalidSupportEntity(bray_symbols::InterfaceSupportEntityId),
}

impl From<SemanticValueStoreError> for InterfaceSemanticInternError {
    fn from(error: SemanticValueStoreError) -> Self {
        Self::SemanticStore(error)
    }
}

/// Immutable compilation-local IDs produced from one decoded semantic interface graph.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ImportedSemantics {
    pub(super) interface_semantics: Arc<InterfaceSemantics>,
    pub(super) types: Arc<[TypeId]>,
    pub(super) constant_values: Arc<[ConstantValueId]>,
    pub(super) constant_terms: Arc<[ConstantTermId]>,
    pub(super) dependency_contracts: Arc<[DependencyContractTemplateId]>,
    pub(super) trait_applications: Arc<[TraitApplicationId]>,
    pub(super) substitutions: Arc<[GenericSubstitutionId]>,
    pub(super) implementation_instances: Arc<[ImplementationInstanceId]>,
    pub(super) callable_instances: Arc<[CallableInstanceId]>,
    pub(super) callable_signatures: Arc<[ImportedCallableSignature]>,
    pub(super) generic_declarations: Arc<[ImportedGenericDeclaration]>,
    pub(super) callable_parameter_defaults: Arc<[ImportedCallableParameterDefault]>,
    pub(super) predicate_definitions: Arc<[ImportedPredicateDefinition]>,
    pub(super) declared_types: Arc<[ImportedDeclaredType]>,
    pub(super) type_representations: Arc<[bray_symbols::DeclaredTypeRepresentation]>,
    pub(super) declaration_templates: Arc<[ImportedDeclarationTemplate]>,
    pub(super) constraints: Arc<[ImportedConstraint]>,
    pub(super) callable_contracts: Arc<[ImportedCallableContract]>,
    pub(super) implementations: Arc<[ImportedImplementation]>,
    pub(super) coherence: Arc<[ImplementationCoherenceEvidence]>,
    pub(super) target_dependencies: Arc<[ImportedTargetProperty]>,
    pub(super) abi_dependencies: Arc<[ImportedAbiDependency]>,
    pub(super) runtime_requirements: Arc<[ImportedRuntimeRequirement]>,
    pub(super) provenance: Arc<[ImportedSourceProvenance]>,
}

/// One imported declaration-owned checked template and its exact semantic owner.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct ImportedDeclarationTemplate {
    pub(super) owner: AnySymbolId,
    pub(super) kind: CheckedTemplateKind,
    pub(super) ordinal: bray_symbols::SymbolOrdinal,
    pub(super) entity: bray_symbols::InterfaceSupportEntityId,
    pub(super) template: Arc<CheckedTemplate>,
}

/// One exact imported symbol-owned semantic record.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum ImportedSemanticRecord {
    /// One callable signature template.
    CallableSignature(ImportedCallableSignature),
    /// One generic declaration template.
    GenericDeclaration(ImportedGenericDeclaration),
    /// One callable parameter default template.
    CallableParameterDefault(ImportedCallableParameterDefault),
    /// One predicate definition state.
    PredicateDefinition(ImportedPredicateDefinition),
    /// One declaration-owned checked type.
    DeclaredType(ImportedDeclaredType),
    /// One declared type representation contract.
    TypeRepresentation(bray_symbols::DeclaredTypeRepresentation),
    /// One checked generic constraint.
    GenericConstraint(ImportedConstraint),
    /// One complete callable contract set.
    CallableContracts(ImportedCallableContract),
    /// One source-independent checked declaration-owned template.
    DeclarationTemplate(ImportedDeclarationTemplate),
    /// One public implementation surface.
    Implementation(ImportedImplementation),
    /// One required target property value.
    TargetProperty(TargetPropertyDependency),
    /// One required callable ABI.
    Abi(ImportedAbiDependency),
    /// One required private runtime ABI surface.
    Runtime(ImportedRuntimeRequirement),
}

impl ImportedSemanticRecord {
    /// Returns the stable semantic record category retained by this imported value.
    pub const fn kind(&self) -> crate::InterfaceSemanticRecordKind {
        use crate::InterfaceSemanticRecordKind as Kind;

        match self {
            Self::CallableSignature(_) => Kind::CallableSignature,
            Self::GenericDeclaration(_) => Kind::GenericDeclaration,
            Self::CallableParameterDefault(_) => Kind::CallableParameterDefault,
            Self::PredicateDefinition(_) => Kind::PredicateDefinition,
            Self::DeclaredType(_) => Kind::DeclaredType,
            Self::TypeRepresentation(_) => Kind::TypeRepresentation,
            Self::GenericConstraint(_) => Kind::GenericConstraint,
            Self::CallableContracts(_) => Kind::CallableContracts,
            Self::DeclarationTemplate(_) => Kind::DeclarationTemplate,
            Self::Implementation(_) => Kind::Implementation,
            Self::TargetProperty(_) => Kind::TargetProperty,
            Self::Abi(_) => Kind::Abi,
            Self::Runtime(_) => Kind::Runtime,
        }
    }
}

/// One imported declaration-owned checked type.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ImportedDeclaredType {
    pub(super) owner: AnySymbolId,
    pub(super) ty: TypeId,
}

impl ImportedDeclaredType {
    /// Returns the declaration owning this type.
    pub const fn owner(self) -> AnySymbolId {
        self.owner
    }

    /// Returns the checked declared type.
    pub const fn ty(self) -> TypeId {
        self.ty
    }
}

impl ImportedDeclarationTemplate {
    /// Returns the declaration that owns this template.
    pub const fn owner(&self) -> AnySymbolId {
        self.owner
    }

    /// Returns the declaration-owned template category.
    pub const fn kind(&self) -> CheckedTemplateKind {
        self.kind
    }

    /// Returns the stable ordinal within the owner and template category.
    pub const fn ordinal(&self) -> bray_symbols::SymbolOrdinal {
        self.ordinal
    }

    /// Returns the private support entity containing the checked template.
    pub const fn entity(&self) -> bray_symbols::InterfaceSupportEntityId {
        self.entity
    }

    /// Returns the immutable source-independent checked template.
    pub fn template(&self) -> &CheckedTemplate {
        &self.template
    }
}

/// One imported generic constraint and its exact owning declaration.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedConstraint {
    pub(super) owner: GenericOwnerId,
    pub(super) constraint: CheckedConstraint,
}

impl ImportedConstraint {
    /// Returns the declaration that owns this constraint.
    pub const fn owner(self) -> GenericOwnerId {
        self.owner
    }

    /// Returns the checked imported constraint.
    pub const fn constraint(self) -> CheckedConstraint {
        self.constraint
    }
}

/// One imported callable contract set and its exact owning declaration.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedCallableContract {
    pub(super) owner: CallableSymbolId,
    pub(super) contract: CallableContractSet,
}

impl ImportedCallableContract {
    /// Returns the callable that owns this contract set.
    pub const fn owner(&self) -> CallableSymbolId {
        self.owner
    }

    /// Returns the checked imported callable contract set.
    pub const fn contract(&self) -> &CallableContractSet {
        &self.contract
    }
}

/// One imported implementation surface.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedImplementation {
    pub(super) implementation: ImplementationSymbolId,
    pub(super) subject: ImplementationSubject,
    pub(super) trait_application: Option<TraitApplicationId>,
    pub(super) coherence: Option<ImplementationCoherenceEvidence>,
    pub(super) constraints: Arc<[CheckedConstraint]>,
    pub(super) target_dependencies: Arc<[TargetPropertyDependency]>,
}

impl ImportedImplementation {
    /// Returns the implementation declaration.
    pub const fn implementation(&self) -> ImplementationSymbolId {
        self.implementation
    }

    /// Returns the implemented subject type.
    pub const fn subject(&self) -> ImplementationSubject {
        self.subject
    }

    /// Returns the implemented trait application for trait implementations.
    pub const fn trait_application(&self) -> Option<TraitApplicationId> {
        self.trait_application
    }

    /// Returns the exported coherence set containing this trait implementation.
    pub const fn coherence(&self) -> Option<&ImplementationCoherenceEvidence> {
        self.coherence.as_ref()
    }

    /// Returns the implementation's checked generic constraints.
    pub fn constraints(&self) -> &[CheckedConstraint] {
        &self.constraints
    }

    /// Returns the target semantics required by this implementation header.
    pub fn target_dependencies(&self) -> &[TargetPropertyDependency] {
        &self.target_dependencies
    }
}

/// One imported target requirement and the semantic record that consumes it.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedTargetProperty {
    pub(super) owner: AnySymbolId,
    pub(super) dependency: TargetPropertyDependency,
}

impl ImportedTargetProperty {
    /// Returns the semantic record that consumes this requirement.
    pub const fn owner(&self) -> AnySymbolId {
        self.owner
    }

    /// Returns the required target property and value.
    pub const fn dependency(&self) -> &TargetPropertyDependency {
        &self.dependency
    }
}

/// One callable ABI dependency decoded into local symbol identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedAbiDependency {
    pub(super) symbol: CallableSymbolId,
    pub(super) abi: bray_symbols::CallableAbi,
}

/// One imported runtime requirement and its exact semantic owner.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedRuntimeRequirement {
    pub(super) owner: AnySymbolId,
    pub(super) frames: Arc<[bray_runtime_interface::ProtectedAsyncFrameId]>,
    pub(super) requirements: bray_runtime_interface::RuntimeRequirements,
}

impl ImportedRuntimeRequirement {
    /// Returns the semantic declaration or support entity that owns this requirement.
    pub const fn owner(&self) -> AnySymbolId {
        self.owner
    }

    /// Returns the hidden protected-frame identities in canonical order.
    pub fn frames(&self) -> &[bray_runtime_interface::ProtectedAsyncFrameId] {
        &self.frames
    }

    /// Returns target-specific private runtime requirements.
    pub const fn requirements(&self) -> &bray_runtime_interface::RuntimeRequirements {
        &self.requirements
    }
}

impl ImportedAbiDependency {
    /// Returns the declaration exposing the ABI dependency.
    pub const fn symbol(self) -> CallableSymbolId {
        self.symbol
    }

    /// Returns the required callable ABI.
    pub const fn abi(self) -> bray_symbols::CallableAbi {
        self.abi
    }
}

/// Optional source provenance resolved to one local symbol identity.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ImportedSourceProvenance {
    pub(super) symbol: AnySymbolId,
    pub(super) document: NonEmptySharedStr,
    pub(super) start: u32,
    pub(super) end: u32,
}

impl ImportedSourceProvenance {
    /// Returns the resolved symbol correlated with this source range.
    pub const fn symbol(&self) -> AnySymbolId {
        self.symbol
    }

    /// Returns the normalized source document identity.
    pub fn document(&self) -> &str {
        self.document.as_str()
    }

    /// Returns the half-open source range.
    pub const fn range(&self) -> std::ops::Range<u32> {
        self.start..self.end
    }
}

impl ImportedSemantics {
    /// Resolves one separately stored checked template against this imported semantic graph.
    pub fn intern_checked_template(
        &self,
        template: &crate::InterfaceCheckedTemplate,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<CheckedTemplate, InterfaceSemanticInternError> {
        InternState::from_imported(self).convert_template(
            template,
            &self.interface_semantics,
            symbols,
        )
    }

    /// Returns the exact semantics selected by symbol owner and category.
    pub fn symbol_semantics(
        &self,
        owner: AnySymbolId,
        kind: InterfaceSemanticRecordKind,
    ) -> Vec<ImportedSemanticRecord> {
        // Exact results own shallow Arc-backed record views independently of the shared graph.
        match kind {
            InterfaceSemanticRecordKind::CallableSignature => self
                .callable_signatures
                .iter()
                .filter(|record| record.owner().into_any() == owner)
                .cloned()
                .map(ImportedSemanticRecord::CallableSignature)
                .collect(),
            InterfaceSemanticRecordKind::GenericDeclaration => self
                .generic_declarations
                .iter()
                .filter(|record| record.owner().symbol() == owner)
                .cloned()
                .map(ImportedSemanticRecord::GenericDeclaration)
                .collect(),
            InterfaceSemanticRecordKind::CallableParameterDefault => self
                .callable_parameter_defaults
                .iter()
                .copied()
                .filter(|record| AnySymbolId::from(record.parameter()) == owner)
                .map(ImportedSemanticRecord::CallableParameterDefault)
                .collect(),
            InterfaceSemanticRecordKind::PredicateDefinition => self
                .predicate_definitions
                .iter()
                .cloned()
                .filter(|record| record.owner().into_any() == owner)
                .map(ImportedSemanticRecord::PredicateDefinition)
                .collect(),
            InterfaceSemanticRecordKind::DeclaredType => self
                .declared_types
                .iter()
                .copied()
                .filter(|record| record.owner() == owner)
                .map(ImportedSemanticRecord::DeclaredType)
                .collect(),
            InterfaceSemanticRecordKind::TypeRepresentation => self
                .type_representations
                .iter()
                .filter(|record| record.subject().into_any() == owner)
                .cloned()
                .map(ImportedSemanticRecord::TypeRepresentation)
                .collect(),
            InterfaceSemanticRecordKind::GenericConstraint => self
                .constraints
                .iter()
                .copied()
                .filter(|record| record.owner().symbol() == owner)
                .map(ImportedSemanticRecord::GenericConstraint)
                .collect(),
            InterfaceSemanticRecordKind::CallableContracts => self
                .callable_contracts
                .iter()
                .filter(|record| record.owner().into_any() == owner)
                .cloned()
                .map(ImportedSemanticRecord::CallableContracts)
                .collect(),
            InterfaceSemanticRecordKind::DeclarationTemplate => self
                .declaration_templates
                .iter()
                .filter(|record| record.owner() == owner)
                .cloned()
                .map(ImportedSemanticRecord::DeclarationTemplate)
                .collect(),
            InterfaceSemanticRecordKind::Implementation => self
                .implementations
                .iter()
                .filter(|record| record.implementation().into_any() == owner)
                .cloned()
                .map(ImportedSemanticRecord::Implementation)
                .collect(),
            InterfaceSemanticRecordKind::TargetProperty => self
                .target_dependencies
                .iter()
                .filter(|record| record.owner() == owner)
                .map(|record| record.dependency().clone())
                .map(ImportedSemanticRecord::TargetProperty)
                .collect(),
            InterfaceSemanticRecordKind::Abi => self
                .abi_dependencies
                .iter()
                .copied()
                .filter(|record| record.symbol().into_any() == owner)
                .map(ImportedSemanticRecord::Abi)
                .collect(),
            InterfaceSemanticRecordKind::Runtime => self
                .runtime_requirements
                .iter()
                .filter(|record| record.owner() == owner)
                .cloned()
                .map(ImportedSemanticRecord::Runtime)
                .collect(),
        }
    }

    /// Returns canonical semantic types in interface table order.
    pub fn types(&self) -> &[TypeId] {
        &self.types
    }

    /// Returns canonical constant values in interface table order.
    pub fn constant_values(&self) -> &[ConstantValueId] {
        &self.constant_values
    }

    /// Returns the durable constant values corresponding to the imported IDs.
    pub(crate) fn interface_constant_values(&self) -> &[crate::InterfaceConstantValue] {
        self.interface_semantics.constant_values()
    }

    /// Returns canonical assembly elements for a tuple or the language unit type.
    pub(crate) fn assembly_value_element_types(&self, ty: TypeId) -> Option<Vec<TypeId>> {
        let elements = match self.interface_type(ty)? {
            crate::InterfaceType::Tuple(elements) => elements,
            crate::InterfaceType::Named {
                definition: crate::InterfaceSymbolReference::CompilerKnown(reference),
                ..
            } if matches!(
                reference.key().data(),
                SymbolKeyData::CompilerKnownDeclaration { key, .. }
                    if CompilerKnownDeclarationKey::try_new("Unit").as_ref() == Some(key)
            ) =>
            {
                return Some(Vec::new());
            }
            _ => return None,
        };

        self.resolve_interface_types(elements)
    }

    /// Returns the exact element types when `ty` is a structural tuple.
    pub(crate) fn tuple_element_types(&self, ty: TypeId) -> Option<Vec<TypeId>> {
        let crate::InterfaceType::Tuple(elements) = self.interface_type(ty)? else {
            return None;
        };

        self.resolve_interface_types(elements)
    }

    /// Returns the contained type when `ty` is structurally nullable.
    pub(crate) fn nullable_element_type(&self, ty: TypeId) -> Option<TypeId> {
        let crate::InterfaceType::Nullable(element) = self.interface_type(ty)? else {
            return None;
        };

        self.types.get(element.to_index()?).copied()
    }

    /// Returns whether `ty` names one exact compiler-known declaration.
    pub(crate) fn is_compiler_known_type(
        &self,
        ty: TypeId,
        expected: &CompilerKnownDeclarationKey,
    ) -> bool {
        let Some(crate::InterfaceType::Named {
            definition: crate::InterfaceSymbolReference::CompilerKnown(reference),
            ..
        }) = self.interface_type(ty)
        else {
            return false;
        };

        matches!(
            reference.key().data(),
            SymbolKeyData::CompilerKnownDeclaration { key, .. }
                if key == expected
        )
    }

    pub(crate) fn compiler_known_type_argument(
        &self,
        ty: TypeId,
        expected: &CompilerKnownDeclarationKey,
    ) -> Option<TypeId> {
        let crate::InterfaceType::Named {
            definition,
            substitution,
        } = self.interface_type(ty)?
        else {
            return None;
        };

        let crate::InterfaceSymbolReference::CompilerKnown(reference) = definition else {
            return None;
        };

        if !matches!(
            reference.key().data(),
            SymbolKeyData::CompilerKnownDeclaration { key, .. } if key == expected
        ) {
            return None;
        }

        let substitution = self
            .interface_semantics
            .substitutions
            .get(substitution.to_index()?)?;

        if &substitution.owner != definition {
            return None;
        }

        let [binding] = substitution.bindings.as_ref() else {
            return None;
        };

        let crate::InterfaceGenericArgument::Type(argument) = binding.argument else {
            return None;
        };

        self.types.get(argument.to_index()?).copied()
    }

    pub(crate) fn borrow_target(
        &self,
        ty: TypeId,
        expected: bray_symbols::BorrowKind,
    ) -> Option<TypeId> {
        let crate::InterfaceType::Borrow { kind, target } = self.interface_type(ty)? else {
            return None;
        };

        if *kind != expected {
            return None;
        }

        self.types.get(target.to_index()?).copied()
    }

    fn interface_type(&self, ty: TypeId) -> Option<&crate::InterfaceType> {
        let slot = self.types.iter().position(|candidate| *candidate == ty)?;

        self.interface_semantics.types().get(slot)
    }

    fn resolve_interface_types(&self, types: &[crate::InterfaceTypeId]) -> Option<Vec<TypeId>> {
        types
            .iter()
            .map(|ty| self.types.get(ty.to_index()?).copied())
            .collect()
    }

    /// Returns canonical open constant terms in interface table order.
    pub fn constant_terms(&self) -> &[ConstantTermId] {
        &self.constant_terms
    }

    /// Returns canonical dependency contracts in interface table order.
    pub fn dependency_contracts(&self) -> &[DependencyContractTemplateId] {
        &self.dependency_contracts
    }

    /// Returns canonical trait applications in interface table order.
    pub fn trait_applications(&self) -> &[TraitApplicationId] {
        &self.trait_applications
    }

    /// Returns canonical generic substitutions in interface table order.
    pub fn substitutions(&self) -> &[GenericSubstitutionId] {
        &self.substitutions
    }

    /// Returns canonical implementation instances in interface table order.
    pub fn implementation_instances(&self) -> &[ImplementationInstanceId] {
        &self.implementation_instances
    }

    /// Returns canonical callable instances in interface table order.
    pub fn callable_instances(&self) -> &[CallableInstanceId] {
        &self.callable_instances
    }

    /// Returns imported callable signatures in interface order.
    pub fn callable_signatures(&self) -> &[ImportedCallableSignature] {
        &self.callable_signatures
    }

    /// Returns imported generic declaration templates in interface order.
    pub fn generic_declarations(&self) -> &[ImportedGenericDeclaration] {
        &self.generic_declarations
    }

    /// Returns imported callable parameter defaults in interface order.
    pub fn callable_parameter_defaults(&self) -> &[ImportedCallableParameterDefault] {
        &self.callable_parameter_defaults
    }

    /// Returns imported predicate definition states in interface order.
    pub fn predicate_definitions(&self) -> &[ImportedPredicateDefinition] {
        &self.predicate_definitions
    }

    /// Returns imported declaration-owned checked types in interface order.
    pub fn declared_types(&self) -> &[ImportedDeclaredType] {
        &self.declared_types
    }

    /// Returns imported declared type representation contracts in interface order.
    pub fn type_representations(&self) -> &[bray_symbols::DeclaredTypeRepresentation] {
        &self.type_representations
    }

    /// Returns imported declaration-owned templates in canonical interface order.
    pub fn declaration_templates(&self) -> &[ImportedDeclarationTemplate] {
        &self.declaration_templates
    }

    /// Returns imported constraints in interface order.
    pub fn constraints(&self) -> &[ImportedConstraint] {
        &self.constraints
    }

    /// Returns imported callable contracts in interface order.
    pub fn callable_contracts(&self) -> &[ImportedCallableContract] {
        &self.callable_contracts
    }

    /// Returns imported implementation surfaces in canonical order.
    pub fn implementations(&self) -> &[ImportedImplementation] {
        &self.implementations
    }

    /// Returns imported coherence semantics in canonical order.
    pub fn coherence(&self) -> &[ImplementationCoherenceEvidence] {
        &self.coherence
    }

    /// Returns imported target-property dependencies in canonical order.
    pub fn target_dependencies(&self) -> &[ImportedTargetProperty] {
        &self.target_dependencies
    }

    /// Returns imported ABI dependencies in canonical order.
    pub fn abi_dependencies(&self) -> &[ImportedAbiDependency] {
        &self.abi_dependencies
    }

    /// Returns imported private runtime requirements in canonical owner order.
    pub fn runtime_requirements(&self) -> &[ImportedRuntimeRequirement] {
        &self.runtime_requirements
    }

    /// Returns optional imported source provenance.
    pub fn provenance(&self) -> &[ImportedSourceProvenance] {
        &self.provenance
    }
}

impl InterfaceSemantics {
    /// Resolves and validates this complete semantic graph into canonical semantic values.
    pub fn intern(
        &self,
        store: &SemanticValueStore,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<ImportedSemantics, InterfaceSemanticInternError> {
        let mut state = InternState::new(self);

        while state.has_pending() {
            let before = state.resolved_count();

            state.intern_pass(self, store, symbols)?;

            if state.resolved_count() == before {
                return Err(InterfaceSemanticInternError::UnresolvedValueGraph);
            }
        }

        state.finish(self, symbols)
    }
}
