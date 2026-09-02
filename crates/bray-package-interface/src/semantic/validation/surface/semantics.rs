use bray_symbols::SymbolKind;

use crate::semantic::model::InterfaceConstraintKind;
use crate::validation::is_strictly_sorted;
use crate::{
    InterfaceLimit, InterfaceSemanticRecordKind, InterfaceSemantics, InterfaceSymbolReference,
    InterfaceValidationError, InterfaceValidationLimits, PackageInterfaceSurface,
};

use super::super::declaration::validate_predicate_definition;
use super::super::saturating_u64;
use super::context::semantic_record_error;
use super::reference::{local_symbol, validate_index, validate_symbol, validate_symbol_kind};

impl InterfaceSemantics {
    pub(in crate::semantic::validation) fn validate_surface_semantics(
        &self,
        surface: &PackageInterfaceSurface,
        symbol_count: usize,
        dependency_count: usize,
        limits: InterfaceValidationLimits,
    ) -> Result<(), InterfaceValidationError> {
        self.validate_surface_ordering()?;
        self.validate_constraints(symbol_count, dependency_count)?;
        self.validate_callable_surface(surface, symbol_count, dependency_count)?;
        self.validate_declarations(surface, symbol_count, dependency_count)?;
        self.validate_type_representations(surface)?;
        self.validate_implementations(symbol_count, dependency_count)?;

        self.validate_environment(symbol_count, dependency_count, limits)
    }

    fn validate_surface_ordering(&self) -> Result<(), InterfaceValidationError> {
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
            || !self.target_dependencies.windows(2).all(|pair| {
                (&pair[0].owner, &pair[0].property) < (&pair[1].owner, &pair[1].property)
            })
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
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }

        Ok(())
    }

    fn validate_constraints(
        &self,
        symbol_count: usize,
        dependency_count: usize,
    ) -> Result<(), InterfaceValidationError> {
        for (index, constraint) in self.constraints.iter().enumerate() {
            let validation = (|| {
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
                    InterfaceConstraintKind::TypeEquality { left, right } => {
                        validate_index(left.to_index(), self.types.len())?;
                        validate_index(right.to_index(), self.types.len())?;
                    }
                }

                Ok(())
            })();

            validation.map_err(|error| {
                semantic_record_error(error, InterfaceSemanticRecordKind::GenericConstraint, index)
            })?;
        }

        Ok(())
    }

    fn validate_declarations(
        &self,
        surface: &PackageInterfaceSurface,
        symbol_count: usize,
        dependency_count: usize,
    ) -> Result<(), InterfaceValidationError> {
        for (index, definition) in self.predicate_definitions.iter().enumerate() {
            validate_predicate_definition(definition, surface).map_err(|error| {
                semantic_record_error(
                    error,
                    InterfaceSemanticRecordKind::PredicateDefinition,
                    index,
                )
            })?;
        }

        for (index, declared) in self.declared_types.iter().enumerate() {
            let validation = (|| {
                validate_symbol(&declared.owner, symbol_count, dependency_count)?;

                validate_index(declared.ty.to_index(), self.types.len())
            })();

            validation.map_err(|error| {
                semantic_record_error(error, InterfaceSemanticRecordKind::DeclaredType, index)
            })?;
        }

        Ok(())
    }

    fn validate_type_representations(
        &self,
        surface: &PackageInterfaceSurface,
    ) -> Result<(), InterfaceValidationError> {
        for (index, representation) in self.type_representations.iter().enumerate() {
            let validation = (|| {
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
                    return Err(crate::semantic::codec::invalid_value(
                        crate::InterfaceValidationField::Reference,
                    ));
                }

                if let Some(tag_type) = representation.union_tag_type {
                    validate_index(tag_type.to_index(), self.types.len())?;
                }

                if owner_kind != SymbolKind::Union
                    && (representation.union_tag_type.is_some()
                        || !representation.union_tags.is_empty())
                {
                    return Err(crate::semantic::codec::invalid_value(
                        crate::InterfaceValidationField::Reference,
                    ));
                }

                for tag in &*representation.union_tags {
                    let variant = local_symbol(&tag.variant)?;

                    if validate_symbol_kind(&tag.variant, surface)? != SymbolKind::UnionVariant
                        || !surface.relationships().iter().any(|relationship| {
                            relationship.kind()
                                == bray_symbols::SymbolRelationshipKind::UnionVariant
                                && relationship.owner() == owner
                                && relationship.member() == variant
                        })
                    {
                        return Err(crate::semantic::codec::invalid_value(
                            crate::InterfaceValidationField::Reference,
                        ));
                    }
                }

                validate_storage_shape(
                    &representation.storage,
                    owner,
                    owner_kind,
                    self.types.len(),
                    surface,
                )?;

                if representation.copy == bray_symbols::DeclaredCopyContract::Conditional {
                    if representation.copy_dependencies.is_empty()
                        || !is_strictly_sorted(&representation.copy_dependencies)
                    {
                        return Err(crate::semantic::codec::invalid_value(
                            crate::InterfaceValidationField::Reference,
                        ));
                    }
                } else if !representation.copy_dependencies.is_empty() {
                    return Err(crate::semantic::codec::invalid_value(
                        crate::InterfaceValidationField::Reference,
                    ));
                }

                for dependency in &*representation.copy_dependencies {
                    let parameter = local_symbol(dependency)?;

                    if validate_symbol_kind(dependency, surface)?
                        != SymbolKind::GenericTypeParameter
                        || !surface.relationships().iter().any(|relationship| {
                            relationship.kind()
                                == bray_symbols::SymbolRelationshipKind::GenericParameter
                                && relationship.owner() == owner
                                && relationship.member() == parameter
                        })
                    {
                        return Err(crate::semantic::codec::invalid_value(
                            crate::InterfaceValidationField::Reference,
                        ));
                    }
                }

                Ok(())
            })();

            validation.map_err(|error| {
                semantic_record_error(
                    error,
                    InterfaceSemanticRecordKind::TypeRepresentation,
                    index,
                )
            })?;
        }

        Ok(())
    }

    fn validate_implementations(
        &self,
        symbol_count: usize,
        dependency_count: usize,
    ) -> Result<(), InterfaceValidationError> {
        for (index, implementation) in self.implementations.iter().enumerate() {
            let validation = (|| {
                validate_symbol(
                    &implementation.implementation,
                    symbol_count,
                    dependency_count,
                )?;

                validate_index(implementation.subject.to_index(), self.types.len())?;

                if let Some(id) = implementation.trait_application {
                    validate_index(id.to_index(), self.trait_applications.len())?;
                }

                Ok(())
            })();

            validation.map_err(|error| {
                semantic_record_error(error, InterfaceSemanticRecordKind::Implementation, index)
            })?;
        }

        for coherence in &*self.coherence {
            validate_index(coherence.subject.to_index(), self.types.len())?;

            validate_index(
                coherence.trait_application.to_index(),
                self.trait_applications.len(),
            )?;

            if coherence.implementations.is_empty()
                || !coherence
                    .implementations
                    .windows(2)
                    .all(|pair| pair[0] < pair[1])
            {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ));
            }

            for implementation in &*coherence.implementations {
                validate_symbol(implementation, symbol_count, dependency_count)?;
            }
        }

        Ok(())
    }

    fn validate_environment(
        &self,
        symbol_count: usize,
        dependency_count: usize,
        limits: InterfaceValidationLimits,
    ) -> Result<(), InterfaceValidationError> {
        for (index, target) in self.target_dependencies.iter().enumerate() {
            let validation = (|| {
                validate_symbol(&target.owner, symbol_count, dependency_count)?;
                validate_symbol(&target.property, symbol_count, dependency_count)?;

                validate_index(target.value.to_index(), self.constant_values.len())
            })();

            validation.map_err(|error| {
                semantic_record_error(error, InterfaceSemanticRecordKind::TargetProperty, index)
            })?;
        }

        for (index, dependency) in self.abi_dependencies.iter().enumerate() {
            validate_symbol(&dependency.symbol, symbol_count, dependency_count).map_err(
                |error| semantic_record_error(error, InterfaceSemanticRecordKind::Abi, index),
            )?;
        }

        self.validate_runtime_requirements(symbol_count, dependency_count, limits)?;

        for provenance in &*self.provenance {
            validate_symbol(&provenance.symbol, symbol_count, dependency_count)?;

            limits.check(
                InterfaceLimit::StringLength,
                saturating_u64(provenance.document.as_str().len()),
            )?;

            if provenance.start > provenance.end {
                return Err(crate::semantic::codec::invalid_value(
                    crate::InterfaceValidationField::Reference,
                ));
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
        for (index, runtime) in self.runtime_requirements.iter().enumerate() {
            let validation = (|| {
                validate_symbol(runtime.owner(), symbol_count, dependency_count)?;

                limits.check(
                    InterfaceLimit::RecordCount,
                    saturating_u64(runtime.frames().len()),
                )?;

                if runtime.requirements().runtime().is_some()
                    || !runtime.requirements().roles().is_empty()
                    || runtime.frames().is_empty() != runtime.requirements().frame_abi().is_none()
                {
                    return Err(crate::semantic::codec::invalid_value(
                        crate::InterfaceValidationField::Reference,
                    ));
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

                Ok(())
            })();

            validation.map_err(|error| {
                semantic_record_error(error, InterfaceSemanticRecordKind::Runtime, index)
            })?;
        }

        Ok(())
    }
}

fn validate_storage_shape(
    storage: &crate::InterfaceStorageShape,
    owner: bray_symbols::InterfaceSymbolId,
    owner_kind: SymbolKind,
    type_count: usize,
    surface: &PackageInterfaceSurface,
) -> Result<(), InterfaceValidationError> {
    match storage {
        crate::InterfaceStorageShape::Structure(members) if owner_kind == SymbolKind::Struct => {
            for member in &**members {
                validate_index(member.ty().to_index(), type_count)?;

                if let Some(field) = member.field() {
                    validate_storage_member(
                        field,
                        owner,
                        SymbolKind::StructField,
                        bray_symbols::SymbolRelationshipKind::StructField,
                        surface,
                    )?;
                }
            }
        }
        crate::InterfaceStorageShape::Union(variants) if owner_kind == SymbolKind::Union => {
            for variant in &**variants {
                let variant_id = local_symbol(variant.variant())?;

                validate_storage_member(
                    variant.variant(),
                    owner,
                    SymbolKind::UnionVariant,
                    bray_symbols::SymbolRelationshipKind::UnionVariant,
                    surface,
                )?;

                for member in variant.members() {
                    validate_index(member.ty().to_index(), type_count)?;

                    if let Some(field) = member.field() {
                        validate_storage_member(
                            field,
                            variant_id,
                            SymbolKind::UnionPayloadField,
                            bray_symbols::SymbolRelationshipKind::UnionPayloadField,
                            surface,
                        )?;
                    }
                }
            }
        }
        crate::InterfaceStorageShape::Structure(_) | crate::InterfaceStorageShape::Union(_) => {
            return Err(crate::semantic::codec::invalid_value(
                crate::InterfaceValidationField::Reference,
            ));
        }
    }

    Ok(())
}

fn validate_storage_member(
    member: &InterfaceSymbolReference,
    owner: bray_symbols::InterfaceSymbolId,
    expected_kind: SymbolKind,
    relationship_kind: bray_symbols::SymbolRelationshipKind,
    surface: &PackageInterfaceSurface,
) -> Result<(), InterfaceValidationError> {
    let member_id = local_symbol(member)?;

    if validate_symbol_kind(member, surface)? != expected_kind
        || !surface.relationships().iter().any(|relationship| {
            relationship.kind() == relationship_kind
                && relationship.owner() == owner
                && relationship.member() == member_id
        })
    {
        return Err(crate::semantic::codec::invalid_value(
            crate::InterfaceValidationField::Reference,
        ));
    }

    Ok(())
}
