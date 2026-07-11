use crate::{
    InterfaceLimit, InterfaceSemanticFacts, InterfaceSymbolReference, InterfaceValidationError,
    InterfaceValidationLimits,
};

use super::saturating_u64;

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
            || !self.coherence.windows(2).all(|pair| pair[0] < pair[1])
            || !self
                .target_dependencies
                .windows(2)
                .all(|pair| pair[0].fact < pair[1].fact)
            || !self
                .abi_dependencies
                .windows(2)
                .all(|pair| pair[0].symbol < pair[1].symbol)
            || !self.provenance.windows(2).all(|pair| pair[0] < pair[1])
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
            validate_symbol(&contract.owner, symbol_count, dependency_count)?;
            validate_index(
                contract.dependency_contract.to_index(),
                self.dependency_contracts.len(),
            )?;

            if !contract
                .clauses
                .windows(2)
                .all(|pair| pair[0].ordinal < pair[1].ordinal)
            {
                return Err(InterfaceValidationError::Malformed);
            }

            for clause in &*contract.clauses {
                validate_index(
                    clause.predicate.dependency_contract.to_index(),
                    self.dependency_contracts.len(),
                )?;
            }

            for capability in &*contract.trusted_capabilities {
                validate_symbol(capability, symbol_count, dependency_count)?;
            }
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
                saturating_u64(provenance.document.len()),
            )?;

            if provenance.document.is_empty() || provenance.start > provenance.end {
                return Err(InterfaceValidationError::Malformed);
            }
        }

        Ok(())
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

pub(super) fn validate_index(
    index: Option<usize>,
    length: usize,
) -> Result<(), InterfaceValidationError> {
    if index.is_none_or(|index| index >= length) {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(())
}
