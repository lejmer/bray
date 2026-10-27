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

        for ty in &*self.types {
            if let crate::InterfaceType::Callable {
                parameters,
                invocation_behavior,
                deferred_execution_behavior,
                ..
            } = ty
            {
                for behavior in std::iter::once(invocation_behavior.as_ref())
                    .chain(deferred_execution_behavior.as_deref())
                {
                    for domain in &*behavior.execution_domains {
                        for predicate in &*domain.entry {
                            if let Some(term) = predicate.condition {
                                self.validate_execution_term(
                                    term,
                                    parameters.len(),
                                    false,
                                    surface,
                                )?;
                            }
                        }
                    }
                }
            }
        }

        self.validate_execution_contracts(surface)?;

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

        validate_index(
            signature.result_dependencies.to_index(),
            self.dependency_contracts.len(),
        )?;

        let result_dependencies = &self.dependency_contracts[signature
            .result_dependencies
            .to_index()
            .expect("validated dependency index")];

        if let Some(term) = result_dependencies.observed_result {
            self.validate_execution_term(
                term,
                signature.parameters.len() + usize::from(signature.receiver.is_some()),
                false,
                surface,
            )?;
        }

        if let Some(input) = result_dependencies.returned_input {
            self.validate_returned_input(
                input,
                signature.parameters.len() + usize::from(signature.receiver.is_some()),
                surface,
            )?;
        }

        for carrier in &*result_dependencies.carriers {
            self.validate_execution_term(
                carrier.guarantee(),
                signature.parameters.len() + usize::from(signature.receiver.is_some()),
                true,
                surface,
            )?;
        }

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

        self.validate_execution_phase_promises(signature)
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
            .relationships_for(
                owner,
                bray_symbols::SymbolRelationshipKind::GenericParameter,
            )
            .iter()
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
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ));
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

    pub(super) fn validate_predicate_summary(
        &self,
        predicate: crate::InterfacePredicateSummary,
    ) -> Result<(), InterfaceValidationError> {
        let invalid =
            || crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Reference);

        validate_index(
            predicate.dependency_contract.to_index(),
            self.dependency_contracts.len(),
        )?;

        if let Some(term) = predicate.condition {
            validate_index(term.to_index(), self.constant_terms.len())?;
        }

        let execution = self.predicate_uses_execution(predicate.condition)?;

        if execution == (predicate.domain == bray_symbols::PredicateDomain::Execution) {
            Ok(())
        } else {
            Err(invalid())
        }
    }

    fn predicate_uses_execution(
        &self,
        condition: Option<crate::InterfaceConstantTermId>,
    ) -> Result<bool, InterfaceValidationError> {
        use crate::InterfaceConstantTerm as Term;

        let invalid =
            || crate::semantic::codec::invalid_value(crate::InterfaceValidationField::Reference);

        let mut pending = condition
            .into_iter()
            .map(|term| (term, false))
            .collect::<Vec<_>>();

        let mut active = BTreeSet::new();
        let mut complete = BTreeSet::new();
        let mut execution = false;

        // This boundary can run before the value graph is validated. Traverse each
        // reachable term once, retaining the active path to reject malformed cycles.
        while let Some((id, exit)) = pending.pop() {
            if exit {
                active.remove(&id);
                complete.insert(id);
                continue;
            }

            if complete.contains(&id) {
                continue;
            }

            if !active.insert(id) {
                return Err(invalid());
            }

            let term = self
                .constant_terms
                .get(id.to_index().ok_or_else(invalid)?)
                .ok_or_else(invalid)?;

            pending.push((id, true));

            let mut child = |id| pending.push((id, false));

            match term {
                Term::Typed { term, .. } => child(*term),
                Term::Unary { operand, .. } | Term::Conversion { operand, .. } => child(*operand),
                Term::NullablePresent(value) => child(*value),
                Term::Binary { left, right, .. } => {
                    child(*left);
                    child(*right);
                }
                Term::VariantTest { subject, .. } => child(*subject),
                Term::Tuple(values) | Term::Array(values) => values.iter().copied().for_each(child),
                Term::Product(fields) | Term::Union { fields, .. } => {
                    fields.iter().map(|field| *field.value()).for_each(child);
                }
                Term::Call { arguments, .. } => arguments.iter().copied().for_each(child),
                Term::PredicateCall {
                    predicate,
                    arguments,
                    ..
                } => {
                    let known = match predicate {
                        crate::InterfaceSymbolReference::CompilerKnown(reference) => match reference.key().data() {
                            bray_symbols::SymbolKeyData::CompilerKnownDeclaration { key, .. } =>
                                bray_compiler_known::COMPILER_KNOWN_CATALOG.compiler_known_declaration_by_key(key)
                                    .and_then(|declaration| declaration.implementation_hook())
                                    .is_some_and(bray_compiler_known::ImplementationHook::is_execution_requirement),
                            _ => false,
                        },
                        _ => false,
                    };

                    if known {
                        if !arguments.is_empty()
                            || !matches!(predicate, crate::InterfaceSymbolReference::CompilerKnown(reference) if reference.kind() == SymbolKind::Predicate)
                        {
                            return Err(invalid());
                        }

                        execution = true;
                    } else if let Some(definition) = self
                        .predicate_definitions
                        .iter()
                        .find(|definition| &definition.owner == predicate)
                        && let Some(condition) = definition.condition
                    {
                        child(condition);
                    }

                    arguments.iter().copied().for_each(child);
                }
                Term::Projection { subject, kind } => {
                    child(*subject);
                    kind.term_references().for_each(child);
                }
                Term::Value(_)
                | Term::IntegerLiteral { .. }
                | Term::CallableArgument(_)
                | Term::Parameter(_)
                | Term::TargetProperty(_)
                | Term::DefinitionApplication { .. } => {}
            }
        }

        Ok(execution)
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
        for predicates in [
            &behavior.predicate_requirements,
            &behavior.predicate_guarantees,
        ] {
            if !is_strictly_sorted(predicates) {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ));
            }

            for predicate in &**predicates {
                self.validate_predicate_summary(*predicate)?;
            }
        }

        for (index, domain) in behavior.execution_domains.iter().enumerate() {
            if usize::try_from(domain.ordinal.raw()).ok() != Some(index)
                || domain.properties.is_empty()
                || !is_strictly_sorted(&domain.properties)
                || !domain.postconditions.is_empty()
            {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ));
            }

            for predicate in &*domain.entry {
                self.validate_predicate_summary(*predicate)?;
            }
        }

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

        for ty in &*behavior.receiver_cleanup {
            validate_index(ty.to_index(), self.types.len())?;
        }

        if !is_strictly_sorted(&behavior.receiver_cleanup)
            || !is_strictly_sorted(&behavior.lifecycle_obligations)
            || !is_strictly_sorted(&behavior.execution_properties)
        {
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

    let has_provider = !surface
        .relationships_for(
            parameter,
            bray_symbols::SymbolRelationshipKind::DefaultProvider,
        )
        .is_empty();

    if default.is_present != has_provider {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Reference,
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn execution_domains_require_the_actual_compiler_predicate() {
        use crate::{
            InterfaceConstantTerm, InterfaceConstantTermId, InterfaceDependencyContract,
            InterfaceDependencyContractId, InterfacePredicateSummary,
        };

        use bray_symbols::{PredicateDomain, SymbolOrdinal};

        let key = bray_symbols::SymbolKey::compiler_known_declaration(
            bray_compiler_known::CompilerKnownDeclarationKey::try_new("BlockingExecution")
                .expect("fixture compiler known declaration key must be valid"),
            bray_symbols::SymbolKind::Predicate,
        )
        .expect("fixture `bray_symbols::SymbolKey::compiler_known_declaration` call must succeed");

        let predicate = crate::InterfaceSymbolReference::CompilerKnown(
            crate::CompilerKnownSymbolReference::try_new(key)
                .expect("fixture compiler known symbol reference must be valid"),
        );

        let semantics = crate::InterfaceSemantics::new().with_values(
            [InterfaceDependencyContract::new([])],
            [crate::InterfaceType::Tuple([].into())],
            [],
            [
                InterfaceConstantTerm::CallableArgument(SymbolOrdinal::new(0)),
                InterfaceConstantTerm::PredicateCall {
                    predicate: predicate.clone(),
                    substitution: crate::InterfaceGenericSubstitutionId::new(0),
                    arguments: [].into(),
                },
                InterfaceConstantTerm::Typed {
                    term: InterfaceConstantTermId::new(1),
                    ty: crate::InterfaceTypeId::new(0),
                },
                InterfaceConstantTerm::Typed {
                    term: InterfaceConstantTermId::new(3),
                    ty: crate::InterfaceTypeId::new(0),
                },
                InterfaceConstantTerm::PredicateCall {
                    predicate,
                    substitution: crate::InterfaceGenericSubstitutionId::new(0),
                    arguments: [InterfaceConstantTermId::new(0)].into(),
                },
                InterfaceConstantTerm::Binary {
                    operation: bray_symbols::ConstantBinaryOperation::LogicalOr,
                    left: InterfaceConstantTermId::new(0),
                    right: InterfaceConstantTermId::new(2),
                },
                InterfaceConstantTerm::Binary {
                    operation: bray_symbols::ConstantBinaryOperation::LogicalAnd,
                    left: InterfaceConstantTermId::new(2),
                    right: InterfaceConstantTermId::new(2),
                },
                InterfaceConstantTerm::Unary {
                    operation: bray_symbols::ConstantUnaryOperation::LogicalNot,
                    operand: InterfaceConstantTermId::new(4),
                },
            ],
        );

        for (term, domain, valid) in [
            (None, PredicateDomain::Value, true),
            (None, PredicateDomain::Execution, false),
            (Some(0), PredicateDomain::Value, true),
            (Some(0), PredicateDomain::Execution, false),
            (Some(1), PredicateDomain::Execution, true),
            (Some(1), PredicateDomain::Value, false),
            (Some(1), PredicateDomain::Trusted, false),
            (Some(0), PredicateDomain::Trusted, true),
            (Some(2), PredicateDomain::Execution, true),
            (Some(3), PredicateDomain::Execution, false),
            (Some(4), PredicateDomain::Execution, false),
            (Some(5), PredicateDomain::Execution, true),
            (Some(5), PredicateDomain::Value, false),
            (Some(6), PredicateDomain::Execution, true),
            (Some(7), PredicateDomain::Execution, false),
        ] {
            let summary = InterfacePredicateSummary::new(InterfaceDependencyContractId::new(0))
                .with_condition(term.map(InterfaceConstantTermId::new), domain);

            assert_eq!(
                semantics.validate_predicate_summary(summary).is_ok(),
                valid,
                "{term:?} {domain:?}"
            );
        }
    }
}
