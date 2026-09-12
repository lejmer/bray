use std::collections::{BTreeMap, BTreeSet};

use bray_symbols::{
    CallableExecutionObligation, CallableExecutionOrigin, CallableExecutionTarget,
    ExecutionProperty, SymbolOrdinal, check_execution_proof_dependencies,
};

use super::reference::{local_symbol, reference_owner, validate_symbol_kind};
use crate::validation::is_strictly_sorted;
use crate::{
    InterfaceCallableContract, InterfaceCallableSignature, InterfaceConstantTerm,
    InterfaceConstantTermId, InterfaceSemantics, InterfaceSymbolReference, InterfaceType,
    InterfaceValidationError, InterfaceValidationField, PackageInterfaceSurface,
};

type Obligation = CallableExecutionObligation<SymbolOrdinal>;

impl InterfaceSemantics {
    pub(super) fn validate_execution_contracts(
        &self,
        surface: &PackageInterfaceSurface,
    ) -> Result<(), InterfaceValidationError> {
        let mut graph = BTreeMap::new();

        for (index, contract) in self.callable_contracts.iter().enumerate() {
            self.validate_execution_contract(contract, surface, &mut graph)
                .map_err(|error| {
                    super::context::semantic_record_error(
                        error,
                        crate::InterfaceSemanticRecordKind::CallableContracts,
                        index,
                    )
                })?;
        }

        if let Some(((owner, _), failure)) = check_execution_proof_dependencies(&graph)
            .into_iter()
            .next()
        {
            let cause = match failure {
                bray_symbols::ExecutionProofFailure::CircularCompletion(target) => {
                    crate::InterfaceMalformedCause::Cycle {
                        field: InterfaceValidationField::Dependency,
                        index: u64::from(target),
                    }
                }
                bray_symbols::ExecutionProofFailure::MissingCandidate(_) => {
                    crate::InterfaceMalformedCause::Missing {
                        field: InterfaceValidationField::Dependency,
                    }
                }
            };

            let error = crate::InterfaceValidationError::Malformed {
                context: crate::InterfaceValidationContext::Section(
                    crate::InterfaceSectionTag::SemanticRecordDirectory,
                ),
                cause,
            };

            let error = match self.callable_contracts.iter().position(|contract| matches!(&contract.owner, InterfaceSymbolReference::Local(symbol) if symbol.raw() == owner)) {
                Some(index) => super::context::semantic_record_error(error, crate::InterfaceSemanticRecordKind::CallableContracts, index),
                None => error,
            };

            return Err(error);
        }

        Ok(())
    }

    fn validate_execution_contract(
        &self,
        contract: &InterfaceCallableContract,
        surface: &PackageInterfaceSurface,
        graph: &mut BTreeMap<(u32, Obligation), BTreeSet<(u32, Obligation)>>,
    ) -> Result<(), InterfaceValidationError> {
        if contract.execution_contract.domains.is_empty()
            && contract.execution_contract.evidence.is_empty()
            && contract.invocation_behavior.execution_properties.is_empty()
            && contract
                .deferred_execution_behavior
                .as_ref()
                .is_none_or(|behavior| behavior.execution_properties.is_empty())
        {
            return Ok(());
        }

        let owner = local_symbol(&contract.owner)?.raw();

        let signature = self
            .callable_signatures
            .iter()
            .find(|signature| signature.owner == contract.owner)
            .ok_or_else(invalid)?;

        let input_count = signature.parameters.len() + usize::from(signature.receiver.is_some());

        let execution = &contract.execution_contract;
        let mut declared = BTreeSet::new();
        let mut posts = BTreeSet::new();

        for (index, domain) in execution.domains.iter().enumerate() {
            if usize::try_from(domain.ordinal.raw()).ok() != Some(index)
                || !is_strictly_sorted(&domain.properties)
            {
                return Err(invalid());
            }

            for term in &*domain.entry {
                self.validate_execution_term(*term, input_count, false, surface)?;
            }

            for property in &*domain.properties {
                declared.insert(Obligation::Property(*property, Some(domain.ordinal)));
            }

            for (ordinal, term) in &*domain.postconditions {
                if usize::try_from(ordinal.raw()).ok() != Some(posts.len())
                    || !posts.insert(*ordinal)
                {
                    return Err(invalid());
                }

                self.validate_execution_term(*term, input_count, true, surface)?;
                declared.insert(Obligation::Postcondition(*ordinal));
            }
        }

        if !execution
            .evidence
            .windows(2)
            .all(|pair| pair[0].obligation < pair[1].obligation)
        {
            return Err(invalid());
        }

        let mut supplied = BTreeSet::new();

        for proof in &*execution.evidence {
            if !declared.contains(&proof.obligation)
                || !supplied.insert(proof.obligation)
                || !is_strictly_sorted(&proof.dependencies)
            {
                return Err(invalid());
            }

            let mut dependencies = BTreeSet::new();

            for (target, required) in &*proof.dependencies {
                match target {
                    CallableExecutionTarget::Callable(instance) => {
                        let instance = instance
                            .to_index()
                            .and_then(|index| self.callable_instances.get(index))
                            .ok_or_else(invalid)?;

                        if !validate_symbol_kind(&instance.definition, surface)?.is_callable() {
                            return Err(invalid());
                        }

                        let substitution = instance
                            .substitution
                            .to_index()
                            .and_then(|index| self.substitutions.get(index))
                            .ok_or_else(invalid)?;

                        if substitution.owner != instance.definition {
                            return Err(invalid());
                        }

                        match &instance.definition {
                            InterfaceSymbolReference::Local(target) => {
                                dependencies.insert((target.raw(), *required));
                            }
                            InterfaceSymbolReference::Dependency { .. }
                            | InterfaceSymbolReference::CompilerKnown(_) => {
                                // The consumer validates provider evidence against its exact loaded dependency.
                            }
                        }
                    }
                    CallableExecutionTarget::Indirect(ty) => {
                        let Some(InterfaceType::Callable {
                            invocation_behavior,
                            ..
                        }) = ty.to_index().and_then(|index| self.types.get(index))
                        else {
                            return Err(invalid());
                        };

                        if *required != Obligation::Property(ExecutionProperty::Pure, None)
                            || !invocation_behavior
                                .execution_properties
                                .contains(&ExecutionProperty::Pure)
                        {
                            return Err(invalid());
                        }
                    }
                }
            }

            match proof.origin {
                CallableExecutionOrigin::Requirement => {
                    if signature.has_body
                        || reference_owner(&contract.owner, surface)?.map(|owner| owner.kind())
                            != Some(bray_symbols::SymbolKind::Trait)
                        || !dependencies.is_empty()
                        || !proof.dependencies.is_empty()
                    {
                        return Err(invalid());
                    }
                }
                CallableExecutionOrigin::ForeignAssertion => {
                    if signature.has_body || !proof.dependencies.is_empty() {
                        return Err(invalid());
                    }

                    let Some(InterfaceType::Callable {
                        trust: bray_symbols::CallableTrust::Trusted,
                        abi,
                        deferred_execution_behavior: None,
                        ..
                    }) = signature
                        .callable_type
                        .to_index()
                        .and_then(|index| self.types.get(index))
                    else {
                        return Err(invalid());
                    };

                    if *abi == bray_symbols::CallableAbi::Bray || !contract.invocation_behavior.trusted_capabilities.iter().any(|requirement| {
                            matches!(&requirement.capability, InterfaceSymbolReference::CompilerKnown(reference)
                                if matches!(reference.key().data(), bray_symbols::SymbolKeyData::CompilerKnownDeclaration { key, .. }
                                    if key.as_str() == "ForeignCall"))
                        }) { return Err(invalid()); }

                    graph.insert((owner, proof.obligation), dependencies);
                }
                CallableExecutionOrigin::CheckedBody => {
                    if !signature.has_body {
                        return Err(invalid());
                    }

                    graph.insert((owner, proof.obligation), dependencies);
                }
            }
        }

        if declared != supplied {
            return Err(invalid());
        }

        Ok(())
    }

    pub(super) fn validate_execution_phase_promises(
        &self,
        signature: &InterfaceCallableSignature,
    ) -> Result<(), InterfaceValidationError> {
        let Some(InterfaceType::Callable {
            invocation_behavior,
            deferred_execution_behavior,
            ..
        }) = signature
            .callable_type
            .to_index()
            .and_then(|index| self.types.get(index))
        else {
            return Err(invalid());
        };

        let Some(contract) = self
            .callable_contracts
            .iter()
            .find(|contract| contract.owner == signature.owner)
        else {
            return if invocation_behavior.execution_properties.is_empty()
                && deferred_execution_behavior
                    .as_ref()
                    .is_none_or(|behavior| behavior.execution_properties.is_empty())
            {
                Ok(())
            } else {
                Err(invalid())
            };
        };

        if invocation_behavior.execution_properties
            != contract.invocation_behavior.execution_properties
            || deferred_execution_behavior
                .as_ref()
                .map(|behavior| &behavior.execution_properties)
                != contract
                    .deferred_execution_behavior
                    .as_ref()
                    .map(|behavior| &behavior.execution_properties)
        {
            return Err(invalid());
        }

        let behavior = contract
            .deferred_execution_behavior
            .as_ref()
            .unwrap_or(&contract.invocation_behavior);

        for property in &*behavior.execution_properties {
            if !contract
                .execution_contract
                .domains
                .iter()
                .any(|domain| domain.entry.is_empty() && domain.properties.contains(property))
            {
                return Err(invalid());
            }
        }

        Ok(())
    }

    fn validate_execution_term(
        &self,
        term: InterfaceConstantTermId,
        inputs: usize,
        result: bool,
        surface: &PackageInterfaceSurface,
    ) -> Result<(), InterfaceValidationError> {
        let mut pending = vec![term];
        let mut remaining = bray_symbols::EXECUTION_CONDITION_WORK_LIMIT;

        while let Some(term) = pending.pop() {
            remaining = remaining.checked_sub(1).ok_or_else(invalid)?;

            let term = term
                .to_index()
                .and_then(|index| self.constant_terms.get(index))
                .ok_or_else(invalid)?;

            match term {
                InterfaceConstantTerm::Typed { term, .. } => pending.push(*term),
                InterfaceConstantTerm::Value(_) => {}
                InterfaceConstantTerm::CallableArgument(ordinal) => {
                    let index = usize::try_from(ordinal.raw()).map_err(|_| invalid())?;

                    if index >= inputs && !(result && index == inputs) {
                        return Err(invalid());
                    }
                }
                InterfaceConstantTerm::Unary { operand, .. } => pending.push(*operand),
                InterfaceConstantTerm::Binary { left, right, .. } => {
                    pending.extend([*left, *right])
                }
                InterfaceConstantTerm::PredicateCall {
                    predicate: definition,
                    substitution,
                    arguments,
                } => {
                    if !matches!(
                        validate_symbol_kind(definition, surface)?,
                        bray_symbols::SymbolKind::Predicate
                            | bray_symbols::SymbolKind::TraitPredicateMember
                            | bray_symbols::SymbolKind::TraitPredicateFulfillment
                    ) {
                        return Err(invalid());
                    }

                    let substitution = substitution
                        .to_index()
                        .and_then(|index| self.substitutions.get(index))
                        .ok_or_else(invalid)?;

                    if substitution.owner != *definition {
                        return Err(invalid());
                    }

                    pending.extend(arguments.iter().copied())
                }
                InterfaceConstantTerm::Projection { subject, kind } => {
                    let (field, expected) = match kind {
                        crate::InterfaceConstantProjection::ProductField(field) => {
                            (field, bray_symbols::SymbolKind::StructField)
                        }
                        crate::InterfaceConstantProjection::UnionPayloadField(field) => {
                            (field, bray_symbols::SymbolKind::UnionPayloadField)
                        }
                        _ => return Err(invalid()),
                    };

                    if validate_symbol_kind(field, surface)? != expected {
                        return Err(invalid());
                    }

                    pending.push(*subject);
                }
                _ => return Err(invalid()),
            }
        }

        Ok(())
    }
}

fn invalid() -> InterfaceValidationError {
    crate::semantic::codec::invalid_value(InterfaceValidationField::Reference)
}
