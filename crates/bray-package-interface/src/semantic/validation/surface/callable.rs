use bray_symbols::CallableConditions;

use std::collections::BTreeSet;

use bray_symbols::SymbolKind;

use crate::semantic::model::{
    InterfaceCallableContract, InterfaceCallableContractClause, InterfaceCallablePhaseBehavior,
};
use crate::validation::is_strictly_sorted;
use crate::{
    InterfaceSemanticRecordKind, InterfaceSemantics, InterfaceSymbolReference,
    InterfaceValidationError, PackageInterfaceSurface,
};

use super::context::semantic_record_error;
use super::reference::{
    local_symbol, reference_key, reference_owner, relationship_members, validate_index,
    validate_owned_parameter, validate_symbol, validate_symbol_kind,
};

impl InterfaceSemantics {
    pub(super) fn validate_callable_surface(
        &self,
        surface: &PackageInterfaceSurface,
        symbol_count: usize,
        dependency_count: usize,
    ) -> Result<(), InterfaceValidationError> {
        for (index, contract) in self.callable_contracts.iter().enumerate() {
            self.validate_callable_contract(contract, symbol_count, dependency_count)
                .map_err(|error| {
                    semantic_record_error(
                        error,
                        InterfaceSemanticRecordKind::CallableContracts,
                        index,
                    )
                })?;
        }

        for (index, signature) in self.callable_signatures.iter().enumerate() {
            self.validate_callable_signature(signature, surface)
                .map_err(|error| {
                    semantic_record_error(
                        error,
                        InterfaceSemanticRecordKind::CallableSignature,
                        index,
                    )
                })?;
        }

        for (index, declaration) in self.generic_declarations.iter().enumerate() {
            self.validate_generic_declaration(declaration, surface)
                .map_err(|error| {
                    semantic_record_error(
                        error,
                        InterfaceSemanticRecordKind::GenericDeclaration,
                        index,
                    )
                })?;
        }

        self.validate_callable_parameter_defaults(surface)
    }

    fn validate_callable_parameter_defaults(
        &self,
        surface: &PackageInterfaceSurface,
    ) -> Result<(), InterfaceValidationError> {
        for (index, default) in self.callable_parameter_defaults.iter().enumerate() {
            validate_callable_parameter_default(default, surface).map_err(|error| {
                semantic_record_error(
                    error,
                    InterfaceSemanticRecordKind::CallableParameterDefault,
                    index,
                )
            })?;
        }

        Ok(())
    }

    fn validate_callable_signature(
        &self,
        signature: &crate::InterfaceCallableSignature,
        surface: &PackageInterfaceSurface,
    ) -> Result<(), InterfaceValidationError> {
        let owner = local_symbol(&signature.owner)?;
        let owner_kind = validate_symbol_kind(&signature.owner, surface)?;

        if !owner_kind.is_callable() {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        validate_index(signature.callable_type.to_index(), self.types.len())?;
        validate_index(signature.result.to_index(), self.types.len())?;

        let Some(crate::InterfaceType::Callable {
            parameters, result, ..
        }) = signature
            .callable_type
            .to_index()
            .and_then(|index| self.types.get(index))
        else {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        };

        if parameters.len() != signature.parameters.len() || *result != signature.result {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        if let Some(receiver) = &signature.receiver {
            validate_owned_parameter(
                &signature.owner,
                &receiver.parameter,
                SymbolKind::ReceiverParameter,
                surface,
            )?;

            validate_index(receiver.ty.to_index(), self.types.len())?;
        }

        for parameter in &*signature.parameters {
            validate_owned_parameter(
                &signature.owner,
                parameter,
                SymbolKind::CallableParameter,
                surface,
            )?;
        }

        let relationship_parameters = relationship_members(
            surface,
            owner,
            bray_symbols::SymbolRelationshipKind::CallableParameter,
            SymbolKind::CallableParameter,
        );

        if signature.parameters.as_ref() != relationship_parameters.as_slice() {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        let relationship_receivers = relationship_members(
            surface,
            owner,
            bray_symbols::SymbolRelationshipKind::CallableParameter,
            SymbolKind::ReceiverParameter,
        );

        let expected_receiver = match relationship_receivers.as_slice() {
            [] => None,
            [receiver] => Some(receiver),
            _ => {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ));
            }
        };

        if signature
            .receiver
            .as_ref()
            .map(|receiver| &receiver.parameter)
            != expected_receiver
        {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        Ok(())
    }

    fn validate_generic_declaration(
        &self,
        declaration: &crate::InterfaceGenericDeclaration,
        surface: &PackageInterfaceSurface,
    ) -> Result<(), InterfaceValidationError> {
        let owner = local_symbol(&declaration.owner)?;
        let owner_kind = validate_symbol_kind(&declaration.owner, surface)?;

        if !owner_kind.supports_generic_substitutions()
            || !declaration.parameters.is_empty() && !owner_kind.admits_generic_parameters()
        {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        for parameter in &*declaration.parameters {
            let parameter_kind = validate_symbol_kind(parameter, surface)?;

            if !bray_symbols::SymbolRelationshipKind::GenericParameter
                .supports(owner_kind, parameter_kind)
                || reference_owner(parameter, surface)?
                    != Some(reference_key(&declaration.owner, surface)?)
            {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ));
            }
        }

        let relationship_parameters = surface
            .relationships()
            .iter()
            .filter(|relationship| {
                relationship.kind() == bray_symbols::SymbolRelationshipKind::GenericParameter
                    && relationship.owner() == owner
            })
            .map(|relationship| InterfaceSymbolReference::Local(relationship.member()))
            .collect::<Vec<_>>();

        if declaration.parameters.as_ref() != relationship_parameters.as_slice() {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
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
        self.validate_callable_conditions(contract.conditions())?;
        self.validate_callable_evidence(contract, symbol_count, dependency_count)?;

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

    fn validate_callable_evidence(
        &self,
        contract: &InterfaceCallableContract,
        symbol_count: usize,
        dependency_count: usize,
    ) -> Result<(), InterfaceValidationError> {
        if !contract
            .evidence()
            .windows(2)
            .all(|pair| pair[0].obligation() < pair[1].obligation())
        {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        for proof in contract.evidence() {
            if contract
                .evidence()
                .first()
                .is_some_and(|first| first.origin() != proof.origin())
            {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ));
            }

            validate_evidence_obligation(contract, proof.obligation())?;

            for (target, obligation) in proof.dependencies() {
                validate_index(target.callable().to_index(), self.callable_instances.len())?;

                let callable = target
                    .callable()
                    .to_index()
                    .and_then(|index| self.callable_instances.get(index))
                    .ok_or_else(|| {
                        crate::semantic::codec::invalid_value(
                            crate::InterfaceValidationField::Reference,
                        )
                    })?;

                let symbol = &callable.definition;

                validate_symbol(symbol, symbol_count, dependency_count)?;

                if let Some((subject, application)) = target.dispatch() {
                    validate_index(subject.to_index(), self.types.len())?;
                    validate_index(application.to_index(), self.trait_applications.len())?;
                }

                if let InterfaceSymbolReference::Local(_) = symbol {
                    let dependency = self
                        .callable_contracts
                        .binary_search_by(|candidate| candidate.owner().cmp(symbol))
                        .ok()
                        .and_then(|index| self.callable_contracts.get(index));

                    let Some(dependency) = dependency else {
                        return Err(crate::semantic::codec::invalid_value(
                            crate::InterfaceValidationField::Reference,
                        ));
                    };

                    validate_evidence_obligation(dependency, *obligation)?;

                    if target.dispatch().is_none()
                        && dependency
                            .evidence()
                            .binary_search_by_key(
                                obligation,
                                bray_symbols::CallableContractEvidence::obligation,
                            )
                            .is_err()
                    {
                        return Err(crate::semantic::codec::invalid_value(
                            crate::InterfaceValidationField::Reference,
                        ));
                    }
                }
            }
        }

        Ok(())
    }

    pub(in crate::semantic::validation) fn validate_callable_conditions(
        &self,
        contract: &bray_symbols::CallableConditionSet<InterfaceCallableContractClause>,
    ) -> Result<(), InterfaceValidationError> {
        self.validate_callable_clauses(contract.invocation_preconditions())?;
        self.validate_callable_clauses(contract.static_constraints())?;
        self.validate_callable_clauses(contract.normal_completion_postconditions())?;
        self.validate_callable_clauses(contract.entry_guards())?;
        self.validate_callable_clauses(contract.guarded_postconditions())?;

        let mut clause_ordinals = BTreeSet::new();

        for clause in contract.clauses() {
            if !clause_ordinals.insert(clause.ordinal) {
                return Err(crate::semantic::codec::malformed(
                    crate::InterfaceMalformedCause::Duplicate {
                        field: crate::InterfaceValidationField::Reference,
                        index: u64::from(clause.ordinal.raw()),
                    },
                ));
            }
        }

        validate_guard_references(contract)?;

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
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        for clause in clauses {
            match clause.value {
                crate::InterfaceCallableContractClauseValue::Predicate(predicate) => {
                    self.validate_predicate_summary(predicate)?;
                }
                crate::InterfaceCallableContractClauseValue::TraitSatisfaction {
                    subject,
                    application,
                } => {
                    if clause.kind != bray_symbols::CallableContractClauseKind::Static {
                        return Err(crate::semantic::codec::invalid_value(
                            crate::InterfaceValidationField::Reference,
                        ));
                    }

                    validate_index(subject.to_index(), self.types.len())?;
                    validate_index(application.to_index(), self.trait_applications.len())?;
                }
            }
        }

        Ok(())
    }

    pub(in crate::semantic::validation) fn validate_callable_behavior(
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
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ));
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
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        for requirement in &*behavior.trusted_capabilities {
            validate_symbol(&requirement.capability, symbol_count, dependency_count)?;
        }

        if !is_strictly_sorted(&behavior.lifecycle_obligations) {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        validate_index(
            behavior.dependency_contract.to_index(),
            self.dependency_contracts.len(),
        )
    }
}

fn validate_evidence_obligation(
    contract: &InterfaceCallableContract,
    obligation: bray_symbols::CallableContractObligation,
) -> Result<(), InterfaceValidationError> {
    let declared = match obligation {
        bray_symbols::CallableContractObligation::Execution(guarantee) => {
            contract.execution_guarantees().contains(&guarantee)
        }
        bray_symbols::CallableContractObligation::Postcondition(ordinal) => {
            contract.clauses().any(|clause| {
                clause.kind == bray_symbols::CallableContractClauseKind::Ensures
                    && clause.ordinal == ordinal
            })
        }
    };

    if declared {
        Ok(())
    } else {
        Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Reference,
        ))
    }
}

fn validate_callable_parameter_default(
    default: &crate::InterfaceCallableParameterDefault,
    surface: &PackageInterfaceSurface,
) -> Result<(), InterfaceValidationError> {
    if validate_symbol_kind(&default.parameter, surface)? != SymbolKind::CallableParameter {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Reference,
        ));
    }

    let parameter = local_symbol(&default.parameter)?;

    let has_provider = surface.relationships().iter().any(|relationship| {
        relationship.kind() == bray_symbols::SymbolRelationshipKind::DefaultProvider
            && relationship.owner() == parameter
    });

    if default.is_present != has_provider {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Reference,
        ));
    }

    Ok(())
}

fn validate_guard_references(
    contract: &impl CallableConditions<Clause = InterfaceCallableContractClause>,
) -> Result<(), InterfaceValidationError> {
    use crate::semantic::codec::malformed;
    use crate::{InterfaceMalformedCause, InterfaceValidationField};

    let guards = contract
        .entry_guards()
        .iter()
        .map(|clause| clause.ordinal)
        .collect::<BTreeSet<_>>();

    for clause in contract.clauses() {
        let Some(guard) = clause.guard else {
            continue;
        };

        if !matches!(
            clause.kind,
            bray_symbols::CallableContractClauseKind::Guard
                | bray_symbols::CallableContractClauseKind::Ensures
        ) {
            return Err(malformed(InterfaceMalformedCause::ValueMismatch {
                field: InterfaceValidationField::Reference,
                expected: 0,
                actual: 1,
            }));
        }

        validate_entry_guard(guard, &guards)?;

        if guard >= clause.ordinal {
            return Err(malformed(InterfaceMalformedCause::OrderingViolation {
                field: InterfaceValidationField::Reference,
                previous: u64::from(guard.raw()),
                actual: u64::from(clause.ordinal.raw()),
            }));
        }
    }

    for guarantee in contract.execution_guarantees() {
        if let Some(guard) = guarantee.guard() {
            validate_entry_guard(guard, &guards)?;
        }
    }

    Ok(())
}

fn validate_entry_guard(
    guard: bray_symbols::SymbolOrdinal,
    guards: &BTreeSet<bray_symbols::SymbolOrdinal>,
) -> Result<(), InterfaceValidationError> {
    if guards.contains(&guard) {
        return Ok(());
    }

    // Distinct u32 ordinals bound this count below the u64 range.
    let available = guards.iter().map(|_| 1_u64).sum();

    Err(crate::semantic::codec::malformed(
        crate::InterfaceMalformedCause::InvalidReference {
            field: crate::InterfaceValidationField::Reference,
            index: u64::from(guard.raw()),
            available,
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::validate_guard_references;
    use crate::{
        InterfaceCallableContract, InterfaceCallableContractClause, InterfaceDependencyContractId,
        InterfacePredicateSummary, InterfaceSymbolReference,
    };
    use bray_symbols::CallableConditions;
    use bray_symbols::{
        CallableContractClauseKind, CallableExecutionGuarantee, ExecutionProperty,
        InterfaceSymbolId, SymbolOrdinal,
    };

    fn clause(
        ordinal: u32,
        kind: CallableContractClauseKind,
        guard: Option<u32>,
    ) -> InterfaceCallableContractClause {
        InterfaceCallableContractClause::new(
            SymbolOrdinal::new(ordinal),
            kind,
            InterfacePredicateSummary::new(InterfaceDependencyContractId::new(0)),
        )
        .with_guard(guard.map(SymbolOrdinal::new))
    }

    fn contract(
        clauses: impl IntoIterator<Item = InterfaceCallableContractClause>,
    ) -> InterfaceCallableContract {
        InterfaceCallableContract::new(
            InterfaceSymbolReference::Local(InterfaceSymbolId::new(0)),
            clauses,
            crate::test_support::callable_phase_behavior(),
            None,
        )
    }

    #[test]
    fn a_callable_has_one_evidence_authority_and_one_record_per_promise() {
        use CallableEvidenceOrigin::{CheckedBody, ForeignAssertion};

        use bray_symbols::{
            CallableContractEvidence, CallableContractObligation, CallableEvidenceOrigin,
        };

        let guarantees = [
            CallableExecutionGuarantee::new(ExecutionProperty::Pure, None),
            CallableExecutionGuarantee::new(ExecutionProperty::Total, None),
        ];

        for (origins, accepted) in [
            ([CheckedBody, CheckedBody], true),
            ([ForeignAssertion, ForeignAssertion], true),
            ([CheckedBody, ForeignAssertion], false),
        ] {
            let evidence = guarantees
                .into_iter()
                .zip(origins)
                .map(|(guarantee, origin)| {
                    let obligation = CallableContractObligation::Execution(guarantee);

                    match origin {
                        CheckedBody => CallableContractEvidence::new(obligation, []),
                        ForeignAssertion => CallableContractEvidence::foreign_assertion(obligation),
                    }
                });

            let contract = contract([])
                .with_execution_guarantees(guarantees)
                .with_evidence(evidence);

            assert_eq!(
                crate::InterfaceSemantics::new()
                    .validate_callable_evidence(&contract, 1, 0)
                    .is_ok(),
                accepted
            );
        }

        let proof =
            CallableContractEvidence::new(CallableContractObligation::Execution(guarantees[0]), []);

        let duplicate = contract([])
            .with_execution_guarantees(guarantees)
            .with_evidence([proof.clone(), proof]);

        assert!(
            crate::InterfaceSemantics::new()
                .validate_callable_evidence(&duplicate, 1, 0)
                .is_err()
        );
    }

    #[test]
    fn body_evidence_requires_the_exact_declared_promise() {
        use bray_symbols::CallableContractObligation::{Execution, Postcondition};

        let guarded =
            CallableExecutionGuarantee::new(ExecutionProperty::Pure, Some(SymbolOrdinal::new(0)));

        let contract = contract([
            clause(0, CallableContractClauseKind::Guard, None),
            clause(1, CallableContractClauseKind::Ensures, Some(0)),
        ])
        .with_execution_guarantees([guarded]);

        for (obligation, accepted) in [
            (Execution(guarded), true),
            (
                Execution(CallableExecutionGuarantee::new(
                    ExecutionProperty::Pure,
                    None,
                )),
                false,
            ),
            (
                Execution(CallableExecutionGuarantee::new(
                    ExecutionProperty::Total,
                    Some(SymbolOrdinal::new(0)),
                )),
                false,
            ),
            (Postcondition(SymbolOrdinal::new(0)), false),
            (Postcondition(SymbolOrdinal::new(1)), true),
            (Postcondition(SymbolOrdinal::new(2)), false),
        ] {
            assert_eq!(
                super::validate_evidence_obligation(&contract, obligation).is_ok(),
                accepted
            );
        }
    }

    #[test]
    fn guard_references_reject_missing_self_and_forward_domains() {
        use CallableContractClauseKind::{Ensures, Guard, Requires};

        for clauses in [
            vec![clause(0, Ensures, Some(9))],
            vec![clause(0, Guard, Some(0))],
            vec![clause(0, Guard, Some(1)), clause(1, Guard, None)],
            vec![clause(0, Requires, None), clause(1, Ensures, Some(0))],
            vec![clause(0, Guard, None), clause(1, Requires, Some(0))],
        ] {
            assert!(validate_guard_references(&contract(clauses)).is_err());
        }
    }

    #[test]
    fn execution_property_guards_must_name_entry_conditions() {
        let invalid = contract([]).with_execution_guarantees([CallableExecutionGuarantee::new(
            ExecutionProperty::Pure,
            Some(SymbolOrdinal::new(0)),
        )]);

        assert!(validate_guard_references(&invalid).is_err());

        let valid = contract([
            clause(0, CallableContractClauseKind::Guard, None),
            clause(1, CallableContractClauseKind::Guard, Some(0)),
            clause(2, CallableContractClauseKind::Ensures, Some(1)),
        ])
        .with_execution_guarantees([CallableExecutionGuarantee::new(
            ExecutionProperty::Total,
            Some(SymbolOrdinal::new(1)),
        )]);

        assert_eq!(validate_guard_references(&valid), Ok(()));
    }
}
