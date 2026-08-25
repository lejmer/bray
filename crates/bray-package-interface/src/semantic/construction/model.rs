use super::super::model::{
    InterfaceCallableInstanceId, InterfaceConstantTermId, InterfaceConstantValueId,
    InterfaceDependencyContractId, InterfaceGenericSubstitutionId,
    InterfaceImplementationInstanceId, InterfaceSemanticRecordKind, InterfaceSemantics,
    InterfaceTraitApplicationId, InterfaceTypeId,
};
use super::super::validation::{InterfaceTypeGraphError, interface_type_graph_depth};
use crate::InterfaceSymbolReference;

/// One artifact-local semantic table addressed by a fragment reference.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum InterfaceSemanticTableKind {
    GenericSubstitution,
    TraitApplication,
    CallableInstance,
    ImplementationInstance,
    DependencyContract,
    Type,
    ConstantValue,
    ConstantTerm,
    CheckedTemplate,
    SupportEntity,
}

impl InterfaceSemanticTableKind {
    /// Returns the stable machine key for this semantic table category.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GenericSubstitution => "generic_substitution",
            Self::TraitApplication => "trait_application",
            Self::CallableInstance => "callable_instance",
            Self::ImplementationInstance => "implementation_instance",
            Self::DependencyContract => "dependency_contract",
            Self::Type => "type",
            Self::ConstantValue => "constant_value",
            Self::ConstantTerm => "constant_term",
            Self::CheckedTemplate => "checked_template",
            Self::SupportEntity => "support_entity",
        }
    }
}

/// Failure while committing independently built semantic fragments.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub enum InterfaceSemanticCommitError {
    /// A fragment reference has no package-wide replacement.
    MissingReference {
        table: InterfaceSemanticTableKind,
        reference: u32,
    },
    /// A fragment contains records that require whole-package construction.
    UnexpectedPackageRecord,
    /// Two fragments provide different records for one declaration-owned semantic key.
    ConflictingRecord {
        owner: InterfaceSymbolReference,
        kind: InterfaceSemanticRecordKind,
    },
    /// A fragment semantic table contains a recursive artifact-local reference.
    CyclicReference(InterfaceSemanticTableKind),
    /// Appending a fragment would exceed an artifact-local identity range.
    IdentityOverflow(InterfaceSemanticTableKind),
}

/// Package-wide replacements for one fragment's artifact-local semantic IDs.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct InterfaceSemanticIdRemap {
    substitutions: Box<[InterfaceGenericSubstitutionId]>,
    trait_applications: Box<[InterfaceTraitApplicationId]>,
    callable_instances: Box<[InterfaceCallableInstanceId]>,
    implementation_instances: Box<[InterfaceImplementationInstanceId]>,
    dependency_contracts: Box<[InterfaceDependencyContractId]>,
    types: Box<[InterfaceTypeId]>,
    constant_values: Box<[InterfaceConstantValueId]>,
    constant_terms: Box<[InterfaceConstantTermId]>,
}

impl InterfaceSemanticIdRemap {
    /// Creates an empty remap for a fragment without value records.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces applied-declaration ID mappings in fragment table order.
    pub fn with_applications(
        mut self,
        substitutions: impl IntoIterator<Item = InterfaceGenericSubstitutionId>,
        trait_applications: impl IntoIterator<Item = InterfaceTraitApplicationId>,
        callable_instances: impl IntoIterator<Item = InterfaceCallableInstanceId>,
        implementation_instances: impl IntoIterator<Item = InterfaceImplementationInstanceId>,
    ) -> Self {
        self.substitutions = substitutions.into_iter().collect();
        self.trait_applications = trait_applications.into_iter().collect();
        self.callable_instances = callable_instances.into_iter().collect();
        self.implementation_instances = implementation_instances.into_iter().collect();

        self
    }

    /// Replaces value and dependency ID mappings in fragment table order.
    pub fn with_values(
        mut self,
        dependency_contracts: impl IntoIterator<Item = InterfaceDependencyContractId>,
        types: impl IntoIterator<Item = InterfaceTypeId>,
        constant_values: impl IntoIterator<Item = InterfaceConstantValueId>,
        constant_terms: impl IntoIterator<Item = InterfaceConstantTermId>,
    ) -> Self {
        self.dependency_contracts = dependency_contracts.into_iter().collect();
        self.types = types.into_iter().collect();
        self.constant_values = constant_values.into_iter().collect();
        self.constant_terms = constant_terms.into_iter().collect();

        self
    }

    pub(super) fn validate(
        &self,
        fragment: &InterfaceSemantics,
    ) -> Result<(), InterfaceSemanticCommitError> {
        let counts = [
            (
                fragment.substitutions.len(),
                self.substitutions.len(),
                InterfaceSemanticTableKind::GenericSubstitution,
            ),
            (
                fragment.trait_applications.len(),
                self.trait_applications.len(),
                InterfaceSemanticTableKind::TraitApplication,
            ),
            (
                fragment.callable_instances.len(),
                self.callable_instances.len(),
                InterfaceSemanticTableKind::CallableInstance,
            ),
            (
                fragment.implementation_instances.len(),
                self.implementation_instances.len(),
                InterfaceSemanticTableKind::ImplementationInstance,
            ),
            (
                fragment.dependency_contracts.len(),
                self.dependency_contracts.len(),
                InterfaceSemanticTableKind::DependencyContract,
            ),
            (
                fragment.types.len(),
                self.types.len(),
                InterfaceSemanticTableKind::Type,
            ),
            (
                fragment.constant_values.len(),
                self.constant_values.len(),
                InterfaceSemanticTableKind::ConstantValue,
            ),
            (
                fragment.constant_terms.len(),
                self.constant_terms.len(),
                InterfaceSemanticTableKind::ConstantTerm,
            ),
        ];

        for (records, mappings, table) in counts {
            if records != mappings {
                return Err(InterfaceSemanticCommitError::MissingReference {
                    table,
                    reference: u32::try_from(records.min(mappings)).unwrap_or(u32::MAX),
                });
            }
        }

        match interface_type_graph_depth(&fragment.types) {
            Ok(_) => {}
            Err(InterfaceTypeGraphError::MissingReference(reference)) => {
                return Err(InterfaceSemanticCommitError::MissingReference {
                    table: InterfaceSemanticTableKind::Type,
                    reference,
                });
            }
            Err(InterfaceTypeGraphError::Cycle) => {
                return Err(InterfaceSemanticCommitError::CyclicReference(
                    InterfaceSemanticTableKind::Type,
                ));
            }
        }

        Ok(())
    }

    pub(super) fn substitution(
        &self,
        id: InterfaceGenericSubstitutionId,
    ) -> Result<InterfaceGenericSubstitutionId, InterfaceSemanticCommitError> {
        mapped(
            &self.substitutions,
            id.raw(),
            InterfaceSemanticTableKind::GenericSubstitution,
        )
    }

    pub(super) fn trait_application(
        &self,
        id: InterfaceTraitApplicationId,
    ) -> Result<InterfaceTraitApplicationId, InterfaceSemanticCommitError> {
        mapped(
            &self.trait_applications,
            id.raw(),
            InterfaceSemanticTableKind::TraitApplication,
        )
    }

    pub(super) fn dependency_contract(
        &self,
        id: InterfaceDependencyContractId,
    ) -> Result<InterfaceDependencyContractId, InterfaceSemanticCommitError> {
        mapped(
            &self.dependency_contracts,
            id.raw(),
            InterfaceSemanticTableKind::DependencyContract,
        )
    }

    pub(super) fn ty(
        &self,
        id: InterfaceTypeId,
    ) -> Result<InterfaceTypeId, InterfaceSemanticCommitError> {
        mapped(&self.types, id.raw(), InterfaceSemanticTableKind::Type)
    }

    pub(super) fn constant_term(
        &self,
        id: InterfaceConstantTermId,
    ) -> Result<InterfaceConstantTermId, InterfaceSemanticCommitError> {
        mapped(
            &self.constant_terms,
            id.raw(),
            InterfaceSemanticTableKind::ConstantTerm,
        )
    }
}

fn mapped<T: Copy>(
    mappings: &[T],
    reference: u32,
    table: InterfaceSemanticTableKind,
) -> Result<T, InterfaceSemanticCommitError> {
    mappings
        .get(reference as usize)
        .copied()
        .ok_or(InterfaceSemanticCommitError::MissingReference { table, reference })
}
