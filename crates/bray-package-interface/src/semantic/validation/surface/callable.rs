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
                    validate_index(
                        predicate.dependency_contract.to_index(),
                        self.dependency_contracts.len(),
                    )?;
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

        if !is_strictly_sorted(&behavior.lifecycle_obligations)
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
