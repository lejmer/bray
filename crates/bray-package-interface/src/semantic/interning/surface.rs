use bray_symbols::{
    CallableContractClause, CallableContractSet, CallableInstanceId, CallableSymbolId,
    CheckedConstraint, ConstantTermId, ConstantValueId, DependencyContractTemplateId,
    GenericOwnerId, GenericSubstitutionId, ImplementationInstanceId, ImplementationSubject,
    ImplementationSymbolId, PredicateSemanticSummary, SymbolOrdinal, TraitApplicationId,
    TrustedCapabilityRequirement, TypeId,
};

use crate::{
    InterfaceCallableInstanceId, InterfaceConstantTermId, InterfaceConstantValueId,
    InterfaceDependencyContractId, InterfaceGenericSubstitutionId,
    InterfaceImplementationInstanceId, InterfaceSemanticFacts, InterfaceTraitApplicationId,
    InterfaceTypeId,
};

use super::common::{
    finish_table, invalid_symbol, lookup, resolve_exact, resolve_family, resolve_symbol,
};
use super::{
    ImportedAbiDependency, ImportedCallableContractFact, ImportedCoherenceFact,
    ImportedConstraintFact, ImportedImplementationFact, ImportedSemanticFacts,
    ImportedSourceProvenance, ImportedTargetFactDependency, InterfaceSemanticInternError,
    InterfaceSymbolResolver, InternState,
};

impl InternState {
    pub(super) fn finish(
        self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<ImportedSemanticFacts, InterfaceSemanticInternError> {
        let constraints = self.convert_constraints(facts, symbols)?;
        let callable_contracts = self.convert_callable_contracts(facts, symbols)?;
        let implementations = self.convert_implementations(facts, symbols)?;
        let coherence = self.convert_coherence(facts, symbols)?;
        let target_dependencies = self.convert_target_dependencies(facts, symbols)?;
        let abi_dependencies = self.convert_abi_dependencies(facts, symbols)?;
        let provenance = self.convert_provenance(facts, symbols)?;

        Ok(ImportedSemanticFacts {
            types: finish_table(self.types)?,
            constant_values: finish_table(self.constant_values)?,
            constant_terms: finish_table(self.constant_terms)?,
            dependency_contracts: finish_table(self.dependency_contracts)?,
            trait_applications: finish_table(self.trait_applications)?,
            substitutions: finish_table(self.substitutions)?,
            implementation_instances: finish_table(self.implementation_instances)?,
            callable_instances: finish_table(self.callable_instances)?,
            constraints: constraints.into(),
            callable_contracts: callable_contracts.into(),
            implementations: implementations.into(),
            coherence: coherence.into(),
            target_dependencies: target_dependencies.into(),
            abi_dependencies: abi_dependencies.into(),
            provenance: provenance.into(),
        })
    }

    pub(super) fn convert_constraints(
        &self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedConstraintFact>, InterfaceSemanticInternError> {
        facts
            .constraints
            .iter()
            .map(|input| {
                let owner = resolve_symbol(symbols, &input.owner)?;

                let Some(owner) = GenericOwnerId::try_new(owner) else {
                    return Err(invalid_symbol(&input.owner));
                };

                let dependency = self
                    .dependency_contract_id(input.predicate.dependency_contract)
                    .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

                Ok(ImportedConstraintFact {
                    owner,
                    constraint: CheckedConstraint::new(
                        input.ordinal,
                        PredicateSemanticSummary::new(dependency),
                    ),
                })
            })
            .collect()
    }

    pub(super) fn convert_callable_contracts(
        &self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedCallableContractFact>, InterfaceSemanticInternError> {
        facts
            .callable_contracts
            .iter()
            .map(|input| {
                let mut clauses = Vec::with_capacity(input.clauses.len());

                for clause in &*input.clauses {
                    let dependency = self
                        .dependency_contract_id(clause.predicate.dependency_contract)
                        .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

                    clauses.push(CallableContractClause::new(
                        clause.ordinal,
                        clause.kind,
                        PredicateSemanticSummary::new(dependency),
                    ));
                }

                let capabilities = input
                    .trusted_capabilities
                    .iter()
                    .enumerate()
                    .map(|(index, reference)| {
                        let ordinal = u32::try_from(index)
                            .map(SymbolOrdinal::new)
                            .map_err(|_| InterfaceSemanticInternError::UnresolvedValueGraph)?;

                        Ok(TrustedCapabilityRequirement::new(
                            ordinal,
                            resolve_symbol(symbols, reference)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, InterfaceSemanticInternError>>()?;

                let dependency = self
                    .dependency_contract_id(input.dependency_contract)
                    .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

                Ok(ImportedCallableContractFact {
                    owner: resolve_family::<CallableSymbolId>(symbols, &input.owner)?,
                    contract: CallableContractSet::new(clauses, capabilities, dependency),
                })
            })
            .collect()
    }

    pub(super) fn convert_implementations(
        &self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedImplementationFact>, InterfaceSemanticInternError> {
        facts
            .implementations
            .iter()
            .map(|input| {
                let implementation = resolve_family(symbols, &input.implementation)?;

                let trait_application = input
                    .trait_application
                    .map(|id| {
                        self.trait_application_id(id)
                            .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)
                    })
                    .transpose()?;

                if matches!(implementation, ImplementationSymbolId::Inherent(_))
                    != trait_application.is_none()
                {
                    return Err(invalid_symbol(&input.implementation));
                }

                Ok(ImportedImplementationFact {
                    implementation,
                    subject: ImplementationSubject::new(
                        self.type_id(input.subject)
                            .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?,
                    ),
                    trait_application,
                })
            })
            .collect()
    }

    pub(super) fn convert_coherence(
        &self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedCoherenceFact>, InterfaceSemanticInternError> {
        facts
            .coherence
            .iter()
            .map(|input| {
                let implementations = input
                    .implementations
                    .iter()
                    .map(|reference| {
                        let implementation = resolve_family(symbols, reference)?;

                        if matches!(implementation, ImplementationSymbolId::Inherent(_)) {
                            return Err(invalid_symbol(reference));
                        }

                        Ok(implementation)
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                Ok(ImportedCoherenceFact {
                    subject: self
                        .type_id(input.subject)
                        .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?,
                    trait_application: self
                        .trait_application_id(input.trait_application)
                        .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?,
                    implementations: implementations.into(),
                })
            })
            .collect()
    }

    pub(super) fn convert_target_dependencies(
        &self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedTargetFactDependency>, InterfaceSemanticInternError> {
        facts
            .target_dependencies
            .iter()
            .map(|input| {
                Ok(ImportedTargetFactDependency {
                    fact: resolve_exact(symbols, &input.fact)?,
                    value: self
                        .constant_value_id(input.value)
                        .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?,
                })
            })
            .collect()
    }

    pub(super) fn convert_abi_dependencies(
        &self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedAbiDependency>, InterfaceSemanticInternError> {
        facts
            .abi_dependencies
            .iter()
            .map(|input| {
                Ok(ImportedAbiDependency {
                    symbol: resolve_family::<CallableSymbolId>(symbols, &input.symbol)?,
                    abi: input.abi,
                })
            })
            .collect()
    }

    pub(super) fn convert_provenance(
        &self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<ImportedSourceProvenance>, InterfaceSemanticInternError> {
        facts
            .provenance
            .iter()
            .map(|input| {
                Ok(ImportedSourceProvenance {
                    symbol: resolve_symbol(symbols, &input.symbol)?,
                    // Imported facts retain the Arc-backed document identity independently.
                    document: input.document.clone(),
                    start: input.start,
                    end: input.end,
                })
            })
            .collect()
    }

    pub(super) fn type_id(&self, id: InterfaceTypeId) -> Option<TypeId> {
        lookup(&self.types, id.to_index())
    }

    pub(super) fn constant_value_id(
        &self,
        id: InterfaceConstantValueId,
    ) -> Option<ConstantValueId> {
        lookup(&self.constant_values, id.to_index())
    }

    pub(super) fn constant_term_id(&self, id: InterfaceConstantTermId) -> Option<ConstantTermId> {
        lookup(&self.constant_terms, id.to_index())
    }

    pub(super) fn dependency_contract_id(
        &self,
        id: InterfaceDependencyContractId,
    ) -> Option<DependencyContractTemplateId> {
        lookup(&self.dependency_contracts, id.to_index())
    }

    pub(super) fn substitution_id(
        &self,
        id: InterfaceGenericSubstitutionId,
    ) -> Option<GenericSubstitutionId> {
        lookup(&self.substitutions, id.to_index())
    }

    pub(super) fn trait_application_id(
        &self,
        id: InterfaceTraitApplicationId,
    ) -> Option<TraitApplicationId> {
        lookup(&self.trait_applications, id.to_index())
    }

    pub(super) fn callable_instance_id(
        &self,
        id: InterfaceCallableInstanceId,
    ) -> Option<CallableInstanceId> {
        lookup(&self.callable_instances, id.to_index())
    }

    pub(super) fn implementation_instance_id(
        &self,
        id: InterfaceImplementationInstanceId,
    ) -> Option<ImplementationInstanceId> {
        lookup(&self.implementation_instances, id.to_index())
    }
}
