use std::sync::Arc;

use super::{
    InterfaceAbiDependency, InterfaceCallableContract, InterfaceCallableInstance,
    InterfaceCheckedTemplate, InterfaceCoherenceRecord, InterfaceConstantTerm,
    InterfaceConstantValue, InterfaceConstraint, InterfaceDeclarationTemplate,
    InterfaceDependencyContract, InterfaceGenericSubstitution, InterfaceImplementationInstance,
    InterfaceImplementationRecord, InterfaceSemanticFactEntry, InterfaceSemanticFactKind,
    InterfaceSourceProvenance, InterfaceSupportEntity, InterfaceTargetFactDependency,
    InterfaceTraitApplication, InterfaceType,
};

/// Complete immutable semantic fact tables ready for package-interface encoding.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InterfaceSemanticFacts {
    pub(crate) substitutions: Arc<[InterfaceGenericSubstitution]>,
    pub(crate) trait_applications: Arc<[InterfaceTraitApplication]>,
    pub(crate) callable_instances: Arc<[InterfaceCallableInstance]>,
    pub(crate) implementation_instances: Arc<[InterfaceImplementationInstance]>,
    pub(crate) dependency_contracts: Arc<[InterfaceDependencyContract]>,
    pub(crate) types: Arc<[InterfaceType]>,
    pub(crate) constant_values: Arc<[InterfaceConstantValue]>,
    pub(crate) constant_terms: Arc<[InterfaceConstantTerm]>,
    pub(crate) constraints: Arc<[InterfaceConstraint]>,
    pub(crate) callable_contracts: Arc<[InterfaceCallableContract]>,
    pub(crate) checked_templates: Arc<[InterfaceCheckedTemplate]>,
    pub(crate) declaration_templates: Arc<[InterfaceDeclarationTemplate]>,
    pub(crate) support_entities: Arc<[InterfaceSupportEntity]>,
    pub(crate) implementations: Arc<[InterfaceImplementationRecord]>,
    pub(crate) coherence: Arc<[InterfaceCoherenceRecord]>,
    pub(crate) target_dependencies: Arc<[InterfaceTargetFactDependency]>,
    pub(crate) abi_dependencies: Arc<[InterfaceAbiDependency]>,
    pub(crate) provenance: Arc<[InterfaceSourceProvenance]>,
}

impl InterfaceSemanticFacts {
    /// Creates an empty semantic fact bundle.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces substitutions and applied declaration instances.
    pub fn with_applications(
        mut self,
        substitutions: impl IntoIterator<Item = InterfaceGenericSubstitution>,
        trait_applications: impl IntoIterator<Item = InterfaceTraitApplication>,
        callable_instances: impl IntoIterator<Item = InterfaceCallableInstance>,
        implementation_instances: impl IntoIterator<Item = InterfaceImplementationInstance>,
    ) -> Self {
        self.substitutions = substitutions.into_iter().collect();
        self.trait_applications = trait_applications.into_iter().collect();
        self.callable_instances = callable_instances.into_iter().collect();
        self.implementation_instances = implementation_instances.into_iter().collect();

        self
    }

    /// Replaces canonical types, constants, and portable dependency contracts.
    pub fn with_values(
        mut self,
        dependency_contracts: impl IntoIterator<Item = InterfaceDependencyContract>,
        types: impl IntoIterator<Item = InterfaceType>,
        constant_values: impl IntoIterator<Item = InterfaceConstantValue>,
        constant_terms: impl IntoIterator<Item = InterfaceConstantTerm>,
    ) -> Self {
        self.dependency_contracts = dependency_contracts.into_iter().collect();
        self.types = types.into_iter().collect();
        self.constant_values = constant_values.into_iter().collect();
        self.constant_terms = constant_terms.into_iter().collect();

        self
    }

    /// Replaces declaration-owned constraint and callable-contract facts.
    pub fn with_contracts(
        mut self,
        constraints: impl IntoIterator<Item = InterfaceConstraint>,
        callable_contracts: impl IntoIterator<Item = InterfaceCallableContract>,
    ) -> Self {
        self.constraints = constraints.into_iter().collect();
        self.callable_contracts = callable_contracts.into_iter().collect();

        self
    }

    /// Replaces checked declaration-owned templates and their private support graph.
    pub fn with_templates(
        mut self,
        checked_templates: impl IntoIterator<Item = InterfaceCheckedTemplate>,
        declaration_templates: impl IntoIterator<Item = InterfaceDeclarationTemplate>,
        support_entities: impl IntoIterator<Item = InterfaceSupportEntity>,
    ) -> Self {
        self.checked_templates = checked_templates.into_iter().collect();
        self.declaration_templates = declaration_templates.into_iter().collect();
        self.support_entities = support_entities.into_iter().collect();

        self
    }

    /// Replaces implementation and coherence facts.
    pub fn with_implementations(
        mut self,
        implementations: impl IntoIterator<Item = InterfaceImplementationRecord>,
        coherence: impl IntoIterator<Item = InterfaceCoherenceRecord>,
    ) -> Self {
        self.implementations = implementations.into_iter().collect();
        self.coherence = coherence.into_iter().collect();

        self
    }

    /// Replaces target and ABI compatibility facts.
    pub fn with_target_dependencies(
        mut self,
        target_dependencies: impl IntoIterator<Item = InterfaceTargetFactDependency>,
        abi_dependencies: impl IntoIterator<Item = InterfaceAbiDependency>,
    ) -> Self {
        self.target_dependencies = target_dependencies.into_iter().collect();
        self.abi_dependencies = abi_dependencies.into_iter().collect();

        self
    }

    /// Replaces optional source provenance excluded from semantic identity.
    pub fn with_provenance(
        mut self,
        provenance: impl IntoIterator<Item = InterfaceSourceProvenance>,
    ) -> Self {
        self.provenance = provenance.into_iter().collect();

        self
    }

    /// Returns the canonical semantic type table.
    pub fn types(&self) -> &[InterfaceType] {
        &self.types
    }

    /// Returns the canonical constant value table.
    pub fn constant_values(&self) -> &[InterfaceConstantValue] {
        &self.constant_values
    }

    /// Returns the canonical constant term table.
    pub fn constant_terms(&self) -> &[InterfaceConstantTerm] {
        &self.constant_terms
    }

    /// Returns generic constraints in canonical owner and ordinal order.
    pub fn constraints(&self) -> &[InterfaceConstraint] {
        &self.constraints
    }

    /// Returns callable contracts in canonical owner order.
    pub fn callable_contracts(&self) -> &[InterfaceCallableContract] {
        &self.callable_contracts
    }

    /// Returns source-independent checked templates in artifact-local ID order.
    pub fn checked_templates(&self) -> &[InterfaceCheckedTemplate] {
        &self.checked_templates
    }

    /// Returns declaration-owned template facts in canonical owner order.
    pub fn declaration_templates(&self) -> &[InterfaceDeclarationTemplate] {
        &self.declaration_templates
    }

    /// Returns the private support graph in deterministic dependency order.
    pub fn support_entities(&self) -> &[InterfaceSupportEntity] {
        &self.support_entities
    }

    /// Returns public implementation records in canonical order.
    pub fn implementations(&self) -> &[InterfaceImplementationRecord] {
        &self.implementations
    }

    /// Returns coherence records in canonical key order.
    pub fn coherence(&self) -> &[InterfaceCoherenceRecord] {
        &self.coherence
    }

    /// Returns target-fact dependencies in canonical fact order.
    pub fn target_dependencies(&self) -> &[InterfaceTargetFactDependency] {
        &self.target_dependencies
    }

    /// Returns ABI dependencies in canonical symbol order.
    pub fn abi_dependencies(&self) -> &[InterfaceAbiDependency] {
        &self.abi_dependencies
    }

    /// Returns optional source provenance.
    pub fn provenance(&self) -> &[InterfaceSourceProvenance] {
        &self.provenance
    }

    /// Returns the canonical symbol-fact directory.
    pub fn fact_directory(&self) -> Arc<[InterfaceSemanticFactEntry]> {
        // References retain Arc-backed external keys so directory ownership stays shallow.
        let constraints =
            self.constraints
                .iter()
                .enumerate()
                .map(|(index, fact)| InterfaceSemanticFactEntry {
                    owner: fact.owner.clone(),
                    kind: InterfaceSemanticFactKind::GenericConstraint,
                    section: crate::InterfaceSectionTag::Contracts,
                    record: checked_record(index),
                });

        let callable_contracts = self
            .callable_contracts
            .iter()
            .enumerate()
            .map(|(index, fact)| InterfaceSemanticFactEntry {
                owner: fact.owner.clone(),
                kind: InterfaceSemanticFactKind::CallableContracts,
                section: crate::InterfaceSectionTag::Contracts,
                record: checked_record(index),
            });

        let declaration_templates =
            self.declaration_templates
                .iter()
                .enumerate()
                .map(|(index, fact)| InterfaceSemanticFactEntry {
                    owner: fact.owner().clone(),
                    kind: InterfaceSemanticFactKind::DeclarationTemplate,
                    section: crate::InterfaceSectionTag::DeclarationTemplates,
                    record: checked_record(index),
                });

        let implementations = self
            .implementations
            .iter()
            .enumerate()
            .map(|(index, fact)| InterfaceSemanticFactEntry {
                owner: fact.implementation.clone(),
                kind: InterfaceSemanticFactKind::Implementation,
                section: crate::InterfaceSectionTag::Implementations,
                record: checked_record(index),
            });

        let targets = self
            .target_dependencies
            .iter()
            .enumerate()
            .map(|(index, fact)| InterfaceSemanticFactEntry {
                owner: fact.owner.clone(),
                kind: InterfaceSemanticFactKind::TargetFact,
                section: crate::InterfaceSectionTag::TargetDependencies,
                record: checked_record(index),
            });

        let abis = self
            .abi_dependencies
            .iter()
            .enumerate()
            .map(|(index, fact)| InterfaceSemanticFactEntry {
                owner: fact.symbol.clone(),
                kind: InterfaceSemanticFactKind::Abi,
                section: crate::InterfaceSectionTag::TargetDependencies,
                record: checked_record(index),
            });

        let mut entries: Vec<_> = constraints
            .chain(callable_contracts)
            .chain(declaration_templates)
            .chain(implementations)
            .chain(targets)
            .chain(abis)
            .collect();

        entries.sort();
        entries.into()
    }
}

fn checked_record(index: usize) -> u32 {
    match u32::try_from(index) {
        Ok(index) => index,
        Err(_) => unreachable!("validated interface record counts fit the wire representation"),
    }
}
