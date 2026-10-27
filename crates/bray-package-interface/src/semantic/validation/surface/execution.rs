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

    // rust-style: allow(function-too-large, reason = "one callable contract boundary checks domains and supplied evidence against the same signature and proof graph")
    fn validate_execution_contract(
        &self,
        contract: &InterfaceCallableContract,
        surface: &PackageInterfaceSurface,
        graph: &mut BTreeMap<(u32, Obligation), BTreeSet<(u32, Obligation)>>,
    ) -> Result<(), InterfaceValidationError> {
        if contract.execution_contract.domains.is_empty()
            && contract.execution_contract.evidence.is_empty()
            && contract.invocation_behavior.execution_properties.is_empty()
            && contract.invocation_behavior.execution_domains.is_empty()
            && contract
                .deferred_execution_behavior
                .as_ref()
                .is_none_or(|behavior| {
                    behavior.execution_properties.is_empty()
                        && behavior.execution_domains.is_empty()
                })
        {
            return Ok(());
        }

        let owner = local_symbol(&contract.owner)?.raw();

        let signature = self
            .callable_signatures
            .binary_search_by(|signature| signature.owner.cmp(&contract.owner))
            .ok()
            .map(|index| &self.callable_signatures[index])
            .ok_or_else(invalid)?;

        let input_count = signature.parameters.len() + usize::from(signature.receiver.is_some());

        for behavior in std::iter::once(&contract.invocation_behavior)
            .chain(contract.deferred_execution_behavior.as_ref())
        {
            for domain in &*behavior.execution_domains {
                for predicate in &*domain.entry {
                    if let Some(term) = predicate.condition {
                        self.validate_execution_term(term, input_count, false, surface)?;
                    }
                }
            }
        }

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
            let internal = proof.obligation.is_internal()
                && matches!(
                    proof.origin,
                    CallableExecutionOrigin::CheckedBody
                        | CallableExecutionOrigin::CompilerIntrinsic
                );

            if (!internal && !declared.contains(&proof.obligation))
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
                    CallableExecutionTarget::Requirement {
                        callable,
                        requirement: (subject, application),
                    } => {
                        if proof.origin != CallableExecutionOrigin::CheckedBody
                            || !(internal && required.is_internal()
                                || matches!(required, Obligation::Property(_, None)))
                            || subject
                                .to_index()
                                .is_none_or(|index| index >= self.types.len())
                        {
                            return Err(invalid());
                        }

                        let instance = callable
                            .to_index()
                            .and_then(|index| self.callable_instances.get(index))
                            .ok_or_else(invalid)?;

                        let application = application
                            .to_index()
                            .and_then(|index| self.trait_applications.get(index))
                            .ok_or_else(invalid)?;

                        if !validate_symbol_kind(&instance.definition, surface)?.is_callable()
                            || !super::reference::reference_is_owned_by(
                                &instance.definition,
                                &application.definition,
                                surface,
                            )?
                        {
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

                        // This is a premise, not evidence for the abstract member. Its exact selected
                        // implementation is discharged when the checked provider is instantiated.
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
                            || !(invocation_behavior
                                .execution_properties
                                .contains(&ExecutionProperty::Pure)
                                || invocation_behavior.execution_domains.iter().any(|domain| {
                                    domain.properties.contains(&ExecutionProperty::Pure)
                                }))
                        {
                            return Err(invalid());
                        }
                    }
                }
            }

            match proof.origin {
                CallableExecutionOrigin::CompilerIntrinsic => {
                    if signature.has_body
                        || !proof.dependencies.is_empty()
                        || !matches!(
                            proof.obligation,
                            Obligation::Observation
                                | Obligation::Termination
                                | Obligation::FiniteIteration
                                | Obligation::Property(
                                    ExecutionProperty::Pure | ExecutionProperty::Total,
                                    _
                                )
                        )
                    {
                        return Err(invalid());
                    }

                    // The consumer resolves the exact recognized intrinsic before using this leaf.
                    graph.insert((owner, proof.obligation), dependencies);
                }
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

        let requirements = contract
            .invocation_preconditions
            .iter()
            .filter_map(|clause| {
                clause
                    .predicate()
                    .and_then(|predicate| predicate.condition())
            })
            .collect::<BTreeSet<_>>();

        for behavior in std::iter::once(&contract.invocation_behavior)
            .chain(contract.deferred_execution_behavior.as_ref())
        {
            for promise in &*behavior.execution_domains {
                let mut entry = requirements.clone();

                for predicate in &*promise.entry {
                    entry.insert(predicate.condition.ok_or_else(invalid)?);
                }

                for property in &*promise.properties {
                    if !execution.domains.iter().any(|domain| {
                        domain.properties.contains(property)
                            && domain.entry.iter().copied().collect::<BTreeSet<_>>() == entry
                    }) {
                        return Err(invalid());
                    }
                }
            }
        }

        let public = supplied
            .into_iter()
            .filter(|obligation| !obligation.is_internal())
            .collect();

        if declared != public {
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
            .binary_search_by(|contract| contract.owner.cmp(&signature.owner))
            .ok()
            .map(|index| &self.callable_contracts[index])
        else {
            return if invocation_behavior.execution_properties.is_empty()
                && invocation_behavior.execution_domains.is_empty()
                && deferred_execution_behavior.as_ref().is_none_or(|behavior| {
                    behavior.execution_properties.is_empty()
                        && behavior.execution_domains.is_empty()
                }) {
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

        for (signature_phase, contract_phase) in
            std::iter::once((invocation_behavior.as_ref(), &contract.invocation_behavior)).chain(
                deferred_execution_behavior
                    .as_deref()
                    .zip(contract.deferred_execution_behavior.as_ref()),
            )
        {
            if signature_phase
                .execution_domains
                .iter()
                .any(|domain| !contract_phase.execution_domains.contains(domain))
            {
                return Err(invalid());
            }
        }

        let behavior = contract
            .deferred_execution_behavior
            .as_ref()
            .unwrap_or(&contract.invocation_behavior);

        let requirements = contract
            .invocation_preconditions
            .iter()
            .filter_map(|clause| {
                clause
                    .predicate()
                    .and_then(|predicate| predicate.condition())
            })
            .collect::<BTreeSet<_>>();

        for property in &*behavior.execution_properties {
            if !contract.execution_contract.domains.iter().any(|domain| {
                domain.properties.contains(property) && domain.requires_only(&requirements)
            }) {
                return Err(invalid());
            }
        }

        Ok(())
    }

    pub(in crate::semantic::validation) fn validate_returned_input(
        &self,
        mut term: InterfaceConstantTermId,
        inputs: usize,
        surface: &PackageInterfaceSurface,
    ) -> Result<(), InterfaceValidationError> {
        for _ in 0..bray_symbols::EXECUTION_CONDITION_WORK_LIMIT {
            match term
                .to_index()
                .and_then(|index| self.constant_terms.get(index))
                .ok_or_else(invalid)?
            {
                InterfaceConstantTerm::CallableArgument(ordinal) => {
                    return if usize::try_from(ordinal.raw()).is_ok_and(|index| index < inputs) {
                        Ok(())
                    } else {
                        Err(invalid())
                    };
                }
                InterfaceConstantTerm::Projection { subject, kind } => {
                    validate_projection(kind, surface)?;
                    term = *subject;
                }
                _ => return Err(invalid()),
            }
        }

        Err(invalid())
    }

    pub(in crate::semantic::validation) fn validate_execution_term(
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
                InterfaceConstantTerm::Parameter(parameter) => {
                    if validate_symbol_kind(parameter, surface)?
                        != bray_symbols::SymbolKind::GenericConstParameter
                    {
                        return Err(invalid());
                    }
                }
                InterfaceConstantTerm::NullablePresent(value) => pending.push(*value),
                InterfaceConstantTerm::CallableArgument(ordinal) => {
                    let index = usize::try_from(ordinal.raw()).map_err(|_| invalid())?;

                    if index >= inputs && !(result && index == inputs) {
                        return Err(invalid());
                    }
                }
                InterfaceConstantTerm::VariantTest { subject, variant } => {
                    if validate_symbol_kind(variant, surface)?
                        != bray_symbols::SymbolKind::UnionVariant
                    {
                        return Err(invalid());
                    }

                    pending.push(*subject);
                }
                InterfaceConstantTerm::Unary { operand, .. } => pending.push(*operand),
                InterfaceConstantTerm::Binary { left, right, .. } => {
                    pending.extend([*left, *right])
                }
                InterfaceConstantTerm::Call {
                    callable,
                    selected_implementation: None,
                    arguments,
                } => {
                    let instance = callable
                        .to_index()
                        .and_then(|index| self.callable_instances.get(index))
                        .ok_or_else(invalid)?;

                    let substitution = instance
                        .substitution
                        .to_index()
                        .and_then(|index| self.substitutions.get(index))
                        .ok_or_else(invalid)?;

                    if !validate_symbol_kind(&instance.definition, surface)?.is_callable()
                        || substitution.owner != instance.definition
                    {
                        return Err(invalid());
                    }

                    pending.extend(arguments.iter().copied());
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
                    validate_projection(kind, surface)?;

                    pending.push(*subject);
                }
                _ => {
                    return Err(invalid());
                }
            }
        }

        Ok(())
    }
}

fn invalid() -> InterfaceValidationError {
    crate::semantic::codec::invalid_value(InterfaceValidationField::Reference)
}

fn validate_projection(
    kind: &crate::InterfaceConstantProjection,
    surface: &PackageInterfaceSurface,
) -> Result<(), InterfaceValidationError> {
    match kind {
        crate::InterfaceConstantProjection::ProductField(field) => {
            if validate_symbol_kind(field, surface)? != bray_symbols::SymbolKind::StructField {
                return Err(invalid());
            }
        }
        crate::InterfaceConstantProjection::UnionPayloadField(field) => {
            if validate_symbol_kind(field, surface)? != bray_symbols::SymbolKind::UnionPayloadField
            {
                return Err(invalid());
            }
        }
        crate::InterfaceConstantProjection::NullableValue
        | crate::InterfaceConstantProjection::TupleElement(_)
        | crate::InterfaceConstantProjection::ArrayElementOrdinal(_) => {}
        _ => return Err(invalid()),
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::{InterfaceConstantTerm, InterfaceConstantTermId, InterfaceSemantics};
    use bray_symbols::{ConstantBinaryOperation, SymbolOrdinal};

    #[test]
    fn returned_input_metadata_rejects_computation_results_and_cycles() {
        let bundle = crate::test_support::package_interface_export_bundle();
        let first = InterfaceConstantTermId::new(0);
        let second = InterfaceConstantTermId::new(1);
        let input = InterfaceConstantTerm::CallableArgument(SymbolOrdinal::new(0));

        for (term, valid) in [
            (input.clone(), true),
            (
                InterfaceConstantTerm::CallableArgument(SymbolOrdinal::new(1)),
                false,
            ),
            (
                InterfaceConstantTerm::Projection {
                    subject: first,
                    kind: crate::InterfaceConstantProjection::NullableValue,
                },
                true,
            ),
            (
                InterfaceConstantTerm::Projection {
                    subject: second,
                    kind: crate::InterfaceConstantProjection::NullableValue,
                },
                false,
            ),
            (
                InterfaceConstantTerm::Binary {
                    operation: ConstantBinaryOperation::Equal,
                    left: first,
                    right: first,
                },
                false,
            ),
        ] {
            let semantics =
                InterfaceSemantics::new().with_values([], [], [], [input.clone(), term]);

            assert_eq!(
                semantics
                    .validate_returned_input(second, 1, bundle.surface())
                    .is_ok(),
                valid
            );
        }
    }
}
