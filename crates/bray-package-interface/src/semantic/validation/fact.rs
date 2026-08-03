use std::collections::BTreeSet;

use crate::validation::is_strictly_sorted;
use crate::{
    InterfaceLimit, InterfaceSemanticFacts, InterfaceSymbolReference, InterfaceValidationError,
    InterfaceValidationLimits, PackageInterfaceSurface,
};
use bray_symbols::SymbolKind;

use super::declaration::validate_predicate_definition;
use super::saturating_u64;
use crate::semantic::model::{
    InterfaceCallableContract, InterfaceCallableContractClause, InterfaceCallablePhaseBehavior,
    InterfaceConstraintKind,
};

impl InterfaceSemanticFacts {
    pub(super) fn validate_surface_facts(
        &self,
        surface: &PackageInterfaceSurface,
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
                .callable_signatures
                .windows(2)
                .all(|pair| pair[0].owner < pair[1].owner)
            || !self
                .generic_declarations
                .windows(2)
                .all(|pair| pair[0].owner < pair[1].owner)
            || !self
                .callable_parameter_defaults
                .windows(2)
                .all(|pair| pair[0].parameter < pair[1].parameter)
            || !self
                .predicate_definitions
                .windows(2)
                .all(|pair| pair[0].owner < pair[1].owner)
            || !self
                .declared_types
                .windows(2)
                .all(|pair| pair[0].owner < pair[1].owner)
            || !self
                .type_representations
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
            || !self
                .runtime_requirements
                .windows(2)
                .all(|pair| pair[0].owner < pair[1].owner)
            || !is_strictly_sorted(&self.provenance)
        {
            return Err(InterfaceValidationError::Malformed);
        }

        for constraint in &*self.constraints {
            validate_symbol(&constraint.owner, symbol_count, dependency_count)?;

            match constraint.kind {
                InterfaceConstraintKind::Predicate(predicate) => validate_index(
                    predicate.dependency_contract.to_index(),
                    self.dependency_contracts.len(),
                )?,
                InterfaceConstraintKind::TraitSatisfaction {
                    subject,
                    application,
                } => {
                    validate_index(subject.to_index(), self.types.len())?;
                    validate_index(application.to_index(), self.trait_applications.len())?;
                }
            }
        }

        for contract in &*self.callable_contracts {
            self.validate_callable_contract(contract, symbol_count, dependency_count)?;
        }

        for signature in &*self.callable_signatures {
            self.validate_callable_signature(signature, surface)?;
        }

        for declaration in &*self.generic_declarations {
            self.validate_generic_declaration(declaration, surface)?;
        }

        for default in &*self.callable_parameter_defaults {
            if validate_symbol_kind(&default.parameter, surface)? != SymbolKind::CallableParameter {
                return Err(InterfaceValidationError::Malformed);
            }

            let parameter = local_symbol(&default.parameter)?;

            let has_provider = surface.relationships().iter().any(|relationship| {
                relationship.kind() == bray_symbols::SymbolRelationshipKind::DefaultProvider
                    && relationship.owner() == parameter
            });

            if default.is_present != has_provider {
                return Err(InterfaceValidationError::Malformed);
            }
        }

        for definition in &*self.predicate_definitions {
            validate_predicate_definition(definition, surface)?;
        }

        for declared in &*self.declared_types {
            validate_symbol(&declared.owner, symbol_count, dependency_count)?;
            validate_index(declared.ty.to_index(), self.types.len())?;
        }

        for representation in &*self.type_representations {
            let owner = local_symbol(&representation.owner)?;
            let owner_kind = validate_symbol_kind(&representation.owner, surface)?;

            if !matches!(owner_kind, SymbolKind::Struct | SymbolKind::Union)
                || representation
                    .alignment
                    .is_some_and(|value| !value.is_power_of_two())
                || representation
                    .packing
                    .is_some_and(|value| !value.is_power_of_two())
            {
                return Err(InterfaceValidationError::Malformed);
            }

            if let Some(tag_type) = representation.union_tag_type {
                validate_index(tag_type.to_index(), self.types.len())?;
            }

            if owner_kind != SymbolKind::Union
                && (representation.union_tag_type.is_some()
                    || !representation.union_tags.is_empty())
            {
                return Err(InterfaceValidationError::Malformed);
            }

            for tag in &*representation.union_tags {
                let variant = local_symbol(&tag.variant)?;

                if validate_symbol_kind(&tag.variant, surface)? != SymbolKind::UnionVariant
                    || !surface.relationships().iter().any(|relationship| {
                        relationship.kind() == bray_symbols::SymbolRelationshipKind::UnionVariant
                            && relationship.owner() == owner
                            && relationship.member() == variant
                    })
                {
                    return Err(InterfaceValidationError::Malformed);
                }
            }

            if representation.copy == bray_symbols::DeclaredCopyContract::Conditional {
                if representation.copy_dependencies.is_empty()
                    || !is_strictly_sorted(&representation.copy_dependencies)
                {
                    return Err(InterfaceValidationError::Malformed);
                }
            } else if !representation.copy_dependencies.is_empty() {
                return Err(InterfaceValidationError::Malformed);
            }

            for dependency in &*representation.copy_dependencies {
                let parameter = local_symbol(dependency)?;

                if validate_symbol_kind(dependency, surface)? != SymbolKind::GenericTypeParameter
                    || !surface.relationships().iter().any(|relationship| {
                        relationship.kind()
                            == bray_symbols::SymbolRelationshipKind::GenericParameter
                            && relationship.owner() == owner
                            && relationship.member() == parameter
                    })
                {
                    return Err(InterfaceValidationError::Malformed);
                }
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
            validate_symbol(&target.owner, symbol_count, dependency_count)?;
            validate_symbol(&target.fact, symbol_count, dependency_count)?;
            validate_index(target.value.to_index(), self.constant_values.len())?;
        }

        for dependency in &*self.abi_dependencies {
            validate_symbol(&dependency.symbol, symbol_count, dependency_count)?;
        }

        self.validate_runtime_requirements(symbol_count, dependency_count, limits)?;

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

    fn validate_runtime_requirements(
        &self,
        symbol_count: usize,
        dependency_count: usize,
        limits: InterfaceValidationLimits,
    ) -> Result<(), InterfaceValidationError> {
        for runtime in &*self.runtime_requirements {
            validate_symbol(runtime.owner(), symbol_count, dependency_count)?;

            if runtime.requirements().runtime().is_some()
                || !runtime.requirements().roles().is_empty()
                || runtime.frame().is_some() && runtime.requirements().frame_abi().is_none()
            {
                return Err(InterfaceValidationError::Malformed);
            }

            for value in [
                runtime.requirements().target().as_str(),
                runtime.requirements().panic_abi().as_str(),
            ] {
                limits.check(InterfaceLimit::StringLength, saturating_u64(value.len()))?;
            }

            if let Some(identity) = runtime.requirements().runtime() {
                limits.check(
                    InterfaceLimit::StringLength,
                    saturating_u64(identity.as_str().len()),
                )?;
            }
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
            return Err(InterfaceValidationError::Malformed);
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
            return Err(InterfaceValidationError::Malformed);
        };

        if parameters.len() != signature.parameters.len() || *result != signature.result {
            return Err(InterfaceValidationError::Malformed);
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
            return Err(InterfaceValidationError::Malformed);
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
            _ => return Err(InterfaceValidationError::Malformed),
        };

        if signature
            .receiver
            .as_ref()
            .map(|receiver| &receiver.parameter)
            != expected_receiver
        {
            return Err(InterfaceValidationError::Malformed);
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
            return Err(InterfaceValidationError::Malformed);
        }

        for parameter in &*declaration.parameters {
            let parameter_kind = validate_symbol_kind(parameter, surface)?;

            if !bray_symbols::SymbolRelationshipKind::GenericParameter
                .supports(owner_kind, parameter_kind)
                || reference_owner(parameter, surface)?
                    != Some(reference_key(&declaration.owner, surface)?)
            {
                return Err(InterfaceValidationError::Malformed);
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
            return Err(InterfaceValidationError::Malformed);
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
                        return Err(InterfaceValidationError::Malformed);
                    }

                    validate_index(subject.to_index(), self.types.len())?;
                    validate_index(application.to_index(), self.trait_applications.len())?;
                }
            }
        }

        Ok(())
    }

    pub(super) fn validate_callable_behavior(
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

fn validate_owned_parameter(
    owner: &InterfaceSymbolReference,
    parameter: &InterfaceSymbolReference,
    expected_kind: SymbolKind,
    surface: &PackageInterfaceSurface,
) -> Result<(), InterfaceValidationError> {
    if validate_symbol_kind(parameter, surface)? != expected_kind
        || !reference_is_owned_by(parameter, owner, surface)?
    {
        return Err(InterfaceValidationError::Malformed);
    }

    Ok(())
}

fn reference_is_owned_by(
    member: &InterfaceSymbolReference,
    owner: &InterfaceSymbolReference,
    surface: &PackageInterfaceSurface,
) -> Result<bool, InterfaceValidationError> {
    match (member, owner) {
        (
            InterfaceSymbolReference::CompilerKnown(_),
            InterfaceSymbolReference::CompilerKnown(_),
        ) => Ok(true),
        (InterfaceSymbolReference::CompilerKnown(_), _)
        | (_, InterfaceSymbolReference::CompilerKnown(_)) => Ok(false),
        _ => Ok(reference_owner(member, surface)? == Some(reference_key(owner, surface)?)),
    }
}

pub(super) fn local_symbol(
    reference: &InterfaceSymbolReference,
) -> Result<bray_symbols::InterfaceSymbolId, InterfaceValidationError> {
    match reference {
        InterfaceSymbolReference::Local(symbol) => Ok(*symbol),
        InterfaceSymbolReference::Dependency { .. }
        | InterfaceSymbolReference::CompilerKnown(_) => Err(InterfaceValidationError::Malformed),
    }
}

fn relationship_members(
    surface: &PackageInterfaceSurface,
    owner: bray_symbols::InterfaceSymbolId,
    relationship_kind: bray_symbols::SymbolRelationshipKind,
    member_kind: SymbolKind,
) -> Vec<InterfaceSymbolReference> {
    surface
        .relationships()
        .iter()
        .filter(|relationship| {
            relationship.kind() == relationship_kind && relationship.owner() == owner
        })
        .filter_map(|relationship| {
            let member = surface.symbols().symbol(relationship.member())?;

            (member.kind() == member_kind)
                .then_some(InterfaceSymbolReference::Local(relationship.member()))
        })
        .collect::<Vec<_>>()
}

fn reference_owner<'surface>(
    reference: &'surface InterfaceSymbolReference,
    surface: &'surface PackageInterfaceSurface,
) -> Result<Option<&'surface bray_symbols::ExternalSymbolKey>, InterfaceValidationError> {
    Ok(reference_key(reference, surface)?.owner())
}

fn reference_key<'surface>(
    reference: &'surface InterfaceSymbolReference,
    surface: &'surface PackageInterfaceSurface,
) -> Result<&'surface bray_symbols::ExternalSymbolKey, InterfaceValidationError> {
    match reference {
        InterfaceSymbolReference::Local(id) => surface
            .symbols()
            .symbol(*id)
            .map(|symbol| symbol.key())
            .ok_or(InterfaceValidationError::Malformed),
        InterfaceSymbolReference::Dependency { key, .. } => Ok(key),
        InterfaceSymbolReference::CompilerKnown(_) => Err(InterfaceValidationError::Malformed),
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
        InterfaceSymbolReference::CompilerKnown(_) => Ok(()),
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
        InterfaceSymbolReference::CompilerKnown(reference) => Ok(reference.kind()),
    }
}

pub(super) fn validate_index(
    index: Option<usize>,
    length: usize,
) -> Result<(), InterfaceValidationError> {
    super::checked_index(index, length).map(|_| ())
}
