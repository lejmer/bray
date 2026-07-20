use bray_symbols::{
    CallableCapabilityRequirement, CallableContractClause, CallableContractSet,
    CallableEffectRequirement, CallableExecutionRequirement, CallableInstanceId,
    CallablePhaseBehavior, CallableSymbolId, CheckedConstraint, ConstantTermId, ConstantValueId,
    DependencyContractTemplateId, GenericOwnerId, GenericSubstitutionId,
    ImplementationCandidateSetKey, ImplementationCoherenceEvidence,
    ImplementationCoherenceParticipant, ImplementationInstanceId, ImplementationSubject,
    ImplementationSymbolId, PredicateSemanticSummary, TargetFactDependency, TraitApplicationId,
    TrustedCapabilityRequirement, TypeId,
};

use crate::semantic::model::InterfaceCallablePhaseBehavior;
use crate::{
    InterfaceCallableInstanceId, InterfaceConstantTermId, InterfaceConstantValueId,
    InterfaceDependencyContractId, InterfaceGenericSubstitutionId,
    InterfaceImplementationInstanceId, InterfaceSemanticFacts, InterfaceTraitApplicationId,
    InterfaceTypeId,
};

use super::common::{
    finish_table, invalid_symbol, lookup, resolve_exact, resolve_family, resolve_stable_symbol_key,
    resolve_symbol,
};
use super::{
    ImportedAbiDependency, ImportedCallableContractFact, ImportedConstraintFact,
    ImportedImplementationFact, ImportedSemanticFacts, ImportedSourceProvenance,
    InterfaceSemanticInternError, InterfaceSymbolResolver, InternState,
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
        let declaration_templates = self.convert_declaration_templates(facts, symbols)?;

        Ok(ImportedSemanticFacts {
            types: finish_table(self.types)?,
            constant_values: finish_table(self.constant_values)?,
            constant_terms: finish_table(self.constant_terms)?,
            dependency_contracts: finish_table(self.dependency_contracts)?,
            trait_applications: finish_table(self.trait_applications)?,
            substitutions: finish_table(self.substitutions)?,
            implementation_instances: finish_table(self.implementation_instances)?,
            callable_instances: finish_table(self.callable_instances)?,
            declaration_templates: declaration_templates.into(),
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
                let clause_count = input.invocation_preconditions.len()
                    + input.static_constraints.len()
                    + input.normal_completion_postconditions.len();

                let mut clauses = Vec::with_capacity(clause_count);

                for clause in input
                    .invocation_preconditions
                    .iter()
                    .chain(input.static_constraints.iter())
                    .chain(input.normal_completion_postconditions.iter())
                {
                    let dependency = self
                        .dependency_contract_id(clause.predicate.dependency_contract)
                        .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

                    clauses.push(CallableContractClause::new(
                        clause.ordinal,
                        clause.kind,
                        PredicateSemanticSummary::new(dependency),
                    ));
                }

                let invocation_behavior =
                    self.convert_callable_behavior(&input.invocation_behavior, symbols)?;

                let deferred_execution_behavior = input
                    .deferred_execution_behavior
                    .as_ref()
                    .map(|behavior| self.convert_callable_behavior(behavior, symbols))
                    .transpose()?;

                Ok(ImportedCallableContractFact {
                    owner: resolve_family::<CallableSymbolId>(symbols, &input.owner)?,
                    contract: CallableContractSet::new(
                        clauses,
                        invocation_behavior,
                        deferred_execution_behavior,
                    ),
                })
            })
            .collect()
    }

    fn convert_callable_behavior(
        &self,
        input: &InterfaceCallablePhaseBehavior,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<CallablePhaseBehavior, InterfaceSemanticInternError> {
        let effects = input
            .effects
            .iter()
            .map(|reference| resolve_symbol(symbols, reference).map(CallableEffectRequirement::new))
            .collect::<Result<Vec<_>, _>>()?;

        let capabilities = input
            .capabilities
            .iter()
            .map(|reference| {
                resolve_symbol(symbols, reference).map(CallableCapabilityRequirement::new)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let trusted_capabilities = input
            .trusted_capabilities
            .iter()
            .map(|requirement| {
                Ok(TrustedCapabilityRequirement::new(
                    requirement.ordinal,
                    resolve_symbol(symbols, &requirement.capability)?,
                ))
            })
            .collect::<Result<Vec<_>, InterfaceSemanticInternError>>()?;

        let execution_requirements = input
            .execution_requirements
            .iter()
            .map(|reference| {
                resolve_symbol(symbols, reference).map(CallableExecutionRequirement::new)
            })
            .collect::<Result<Vec<_>, _>>()?;

        let dependency = self
            .dependency_contract_id(input.dependency_contract)
            .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

        Ok(CallablePhaseBehavior::new(
            effects,
            capabilities,
            trusted_capabilities,
            execution_requirements,
            input.lifecycle_obligations.iter().copied(),
            dependency,
            input.current_run_cancellation,
        ))
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
    ) -> Result<Vec<ImplementationCoherenceEvidence>, InterfaceSemanticInternError> {
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

                        let key = resolve_stable_symbol_key(symbols, reference)?;

                        Ok(ImplementationCoherenceParticipant::new(key, implementation))
                    })
                    .collect::<Result<Vec<_>, _>>()?;

                let subject = self
                    .type_id(input.subject)
                    .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

                let trait_application = self
                    .trait_application_id(input.trait_application)
                    .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

                ImplementationCoherenceEvidence::try_new(
                    ImplementationCandidateSetKey::new(subject, trait_application),
                    implementations,
                )
                .map_err(|_| InterfaceSemanticInternError::UnresolvedValueGraph)
            })
            .collect()
    }

    pub(super) fn convert_target_dependencies(
        &self,
        facts: &InterfaceSemanticFacts,
        symbols: &impl InterfaceSymbolResolver,
    ) -> Result<Vec<TargetFactDependency>, InterfaceSemanticInternError> {
        facts
            .target_dependencies
            .iter()
            .map(|input| {
                let fact = resolve_exact(symbols, &input.fact)?;

                let key = resolve_stable_symbol_key(symbols, &input.fact)?;

                let value = self
                    .constant_value_id(input.value)
                    .ok_or(InterfaceSemanticInternError::UnresolvedValueGraph)?;

                Ok(TargetFactDependency::new(key, fact, value))
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
