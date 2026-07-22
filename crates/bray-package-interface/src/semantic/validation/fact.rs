use std::collections::BTreeSet;

use crate::validation::is_strictly_sorted;
use crate::{
    InterfaceLimit, InterfaceSemanticFacts, InterfaceSymbolReference, InterfaceValidationError,
    InterfaceValidationLimits, PackageInterfaceSurface,
};
use bray_symbols::SymbolKind;

use super::saturating_u64;
use crate::semantic::model::{
    InterfaceCallableContract, InterfaceCallableContractClause, InterfaceCallablePhaseBehavior,
};

impl InterfaceSemanticFacts {
    pub(super) fn validate_surface_facts(
        &self,
        symbol_count: usize,
        dependency_count: usize,
        limits: InterfaceValidationLimits,
    ) -> Result<(), InterfaceValidationError> {
        if !self
            .constraints
            .windows(2)
            .all(|pair| (&pair[0].owner, pair[0].ordinal) < (&pair[1].owner, pair[1].ordinal))
            || !self
                .callable_contracts
                .windows(2)
                .all(|pair| pair[0].owner < pair[1].owner)
            || !self
                .implementations
                .windows(2)
                .all(|pair| pair[0].implementation < pair[1].implementation)
            || !is_strictly_sorted(&self.coherence)
            || !self
                .target_dependencies
                .windows(2)
                .all(|pair| (&pair[0].owner, &pair[0].fact) < (&pair[1].owner, &pair[1].fact))
            || !self
                .abi_dependencies
                .windows(2)
                .all(|pair| pair[0].symbol < pair[1].symbol)
            || !is_strictly_sorted(&self.provenance)
        {
            return Err(InterfaceValidationError::Malformed);
        }

        for constraint in &*self.constraints {
            validate_symbol(&constraint.owner, symbol_count, dependency_count)?;
            validate_index(
                constraint.predicate.dependency_contract.to_index(),
                self.dependency_contracts.len(),
            )?;
        }

        for contract in &*self.callable_contracts {
            self.validate_callable_contract(contract, symbol_count, dependency_count)?;
        }

        for implementation in &*self.implementations {
            validate_symbol(
                &implementation.implementation,
                symbol_count,
                dependency_count,
            )?;

            validate_index(implementation.subject.to_index(), self.types.len())?;

            if let Some(id) = implementation.trait_application {
                validate_index(id.to_index(), self.trait_applications.len())?;
            }
        }

        for coherence in &*self.coherence {
            validate_index(coherence.subject.to_index(), self.types.len())?;

            validate_index(
                coherence.trait_application.to_index(),
                self.trait_applications.len(),
            )?;

            if coherence.implementations.is_empty() {
                return Err(InterfaceValidationError::Malformed);
            }

            if !coherence
                .implementations
                .windows(2)
                .all(|pair| pair[0] < pair[1])
            {
                return Err(InterfaceValidationError::Malformed);
            }

            for implementation in &*coherence.implementations {
                validate_symbol(implementation, symbol_count, dependency_count)?;
            }
        }

        for target in &*self.target_dependencies {
            validate_symbol(&target.owner, symbol_count, dependency_count)?;
            validate_symbol(&target.fact, symbol_count, dependency_count)?;
            validate_index(target.value.to_index(), self.constant_values.len())?;
        }

        for dependency in &*self.abi_dependencies {
            validate_symbol(&dependency.symbol, symbol_count, dependency_count)?;
        }

        for provenance in &*self.provenance {
            validate_symbol(&provenance.symbol, symbol_count, dependency_count)?;

            limits.check(
                InterfaceLimit::StringLength,
                saturating_u64(provenance.document.as_str().len()),
            )?;

            if provenance.start > provenance.end {
                return Err(InterfaceValidationError::Malformed);
            }
        }

        Ok(())
    }

    fn validate_callable_contract(
        &self,
        contract: &InterfaceCallableContract,
        symbol_count: usize,
        dependency_count: usize,
    ) -> Result<(), InterfaceValidationError> {
        validate_symbol(&contract.owner, symbol_count, dependency_count)?;
        self.validate_callable_clauses(&contract.invocation_preconditions)?;
        self.validate_callable_clauses(&contract.static_constraints)?;
        self.validate_callable_clauses(&contract.normal_completion_postconditions)?;

        let mut clause_ordinals = BTreeSet::new();

        for clause in contract
            .invocation_preconditions
            .iter()
            .chain(contract.static_constraints.iter())
            .chain(contract.normal_completion_postconditions.iter())
        {
            if !clause_ordinals.insert(clause.ordinal) {
                return Err(InterfaceValidationError::Malformed);
            }
        }

        self.validate_callable_behavior(
            &contract.invocation_behavior,
            symbol_count,
            dependency_count,
        )?;

        if let Some(behavior) = &contract.deferred_execution_behavior {
            self.validate_callable_behavior(behavior, symbol_count, dependency_count)?;
        }

        Ok(())
    }

    fn validate_callable_clauses(
        &self,
        clauses: &[InterfaceCallableContractClause],
    ) -> Result<(), InterfaceValidationError> {
        if !clauses
            .windows(2)
            .all(|pair| pair[0].ordinal < pair[1].ordinal)
        {
            return Err(InterfaceValidationError::Malformed);
        }

        for clause in clauses {
            validate_index(
                clause.predicate.dependency_contract.to_index(),
                self.dependency_contracts.len(),
            )?;
        }

        Ok(())
    }

    fn validate_callable_behavior(
        &self,
        behavior: &InterfaceCallablePhaseBehavior,
        symbol_count: usize,
        dependency_count: usize,
    ) -> Result<(), InterfaceValidationError> {
        for requirements in [
            &behavior.effects,
            &behavior.capabilities,
            &behavior.execution_requirements,
        ] {
            if !is_strictly_sorted(requirements) {
                return Err(InterfaceValidationError::Malformed);
            }

            for requirement in &**requirements {
                validate_symbol(requirement, symbol_count, dependency_count)?;
            }
        }

        if !behavior
            .trusted_capabilities
            .windows(2)
            .all(|pair| pair[0].ordinal < pair[1].ordinal)
        {
            return Err(InterfaceValidationError::Malformed);
        }

        for requirement in &*behavior.trusted_capabilities {
            validate_symbol(&requirement.capability, symbol_count, dependency_count)?;
        }

        if !is_strictly_sorted(&behavior.lifecycle_obligations) {
            return Err(InterfaceValidationError::Malformed);
        }

        validate_index(
            behavior.dependency_contract.to_index(),
            self.dependency_contracts.len(),
        )
    }
}

pub(super) fn validate_symbol(
    reference: &InterfaceSymbolReference,
    symbol_count: usize,
    dependency_count: usize,
) -> Result<(), InterfaceValidationError> {
    match reference {
        InterfaceSymbolReference::Local(id) => validate_index(id.to_index(), symbol_count),
        InterfaceSymbolReference::Dependency { dependency, .. } => {
            validate_index(dependency.to_index(), dependency_count)
        }
    }
}

pub(super) fn validate_symbol_kind(
    reference: &InterfaceSymbolReference,
    surface: &PackageInterfaceSurface,
) -> Result<SymbolKind, InterfaceValidationError> {
    match reference {
        InterfaceSymbolReference::Local(id) => surface
            .symbols()
            .symbol(*id)
            .map(|symbol| symbol.kind())
            .ok_or(InterfaceValidationError::Malformed),
        InterfaceSymbolReference::Dependency { dependency, key } => {
            let Some(dependency) = dependency
                .to_index()
                .and_then(|index| surface.dependencies().get(index))
            else {
                return Err(InterfaceValidationError::Malformed);
            };

            if key.package_identity() != dependency.package() {
                return Err(InterfaceValidationError::Malformed);
            }

            Ok(key.kind())
        }
    }
}

pub(super) fn validate_index(
    index: Option<usize>,
    length: usize,
) -> Result<(), InterfaceValidationError> {
    super::checked_index(index, length).map(|_| ())
}
