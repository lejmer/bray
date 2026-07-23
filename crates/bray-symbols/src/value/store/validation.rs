use std::collections::HashSet;

use super::{
    super::{
        CallableInstanceData, ConstantProjectionKind, ConstantTermData, ConstantValueData,
        ConstantValueKind, DependencyContractTemplateData, DependencyGuard, DependencyProjection,
        DependencyRequirement, DependencySubject, DependencySubjectRoot, GenericArgument,
        GenericSubstitutionData, GenericSubstitutionId, ImplementationInstanceData,
        SemanticValueStoreError, SemanticValueStoreId, TraitApplicationData, TraitApplicationId,
        TypeData, TypeId,
    },
    table::SemanticTables,
};

pub(super) fn validate_type_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: &TypeData,
) -> Result<(), SemanticValueStoreError> {
    match data {
        TypeData::Error | TypeData::TypeParameter(_) | TypeData::ContextualSelf(_) => {}
        TypeData::Named { substitution, .. } => {
            tables.substitutions.get(store, *substitution)?;
        }
        TypeData::AssociatedTypeProjection {
            subject,
            application,
            ..
        } => {
            tables.types.get(store, *subject)?;
            tables.trait_applications.get(store, *application)?;
        }
        TypeData::TraitView(application) => {
            tables.trait_applications.get(store, *application)?;
        }
        TypeData::Tuple(elements) => validate_types(tables, store, elements)?,
        TypeData::Array { element, length } => {
            tables.types.get(store, *element)?;
            tables.constant_terms.get(store, *length)?;
        }
        TypeData::Slice(target) | TypeData::Nullable(target) | TypeData::Borrow { target, .. } => {
            tables.types.get(store, *target)?;
        }
        TypeData::OwnedIndirection { storage, target } => {
            tables.types.get(store, *storage)?;
            tables.types.get(store, *target)?;
        }
        TypeData::Callable(callable) => {
            for parameter in callable.parameters() {
                tables.types.get(store, parameter.ty())?;
            }

            tables.types.get(store, callable.result())?;

            let dependencies = callable.dependency_contracts();

            tables
                .dependency_contracts
                .get(store, dependencies.invocation())?;

            if let Some(deferred) = dependencies.deferred_execution() {
                tables.dependency_contracts.get(store, deferred)?;
            }
        }
    }

    Ok(())
}

pub(super) fn validate_constant_value_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: &ConstantValueData,
) -> Result<(), SemanticValueStoreError> {
    tables.types.get(store, data.ty())?;

    match data.kind() {
        ConstantValueKind::NullablePresent(value) => {
            tables.constant_values.get(store, *value)?;
        }
        ConstantValueKind::Tuple(values)
        | ConstantValueKind::Array(values)
        | ConstantValueKind::Product(values) => validate_values(tables, store, values)?,
        ConstantValueKind::Union { fields, .. } => validate_values(tables, store, fields)?,
        ConstantValueKind::Error
        | ConstantValueKind::Boolean(_)
        | ConstantValueKind::Character(_)
        | ConstantValueKind::Integer(_)
        | ConstantValueKind::Real(_)
        | ConstantValueKind::Complex { .. }
        | ConstantValueKind::String(_)
        | ConstantValueKind::Unit
        | ConstantValueKind::NullableAbsent => {}
    }

    Ok(())
}

pub(super) fn validate_constant_term_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: &ConstantTermData,
) -> Result<(), SemanticValueStoreError> {
    match data {
        ConstantTermData::Value(value) => {
            tables.constant_values.get(store, *value)?;
        }
        ConstantTermData::IntegerLiteral { .. } => {}
        ConstantTermData::Parameter(_) | ConstantTermData::TargetFact(_) => {}
        ConstantTermData::Unary { operand, .. } => {
            tables.constant_terms.get(store, *operand)?;
        }
        ConstantTermData::Binary { left, right, .. } => {
            tables.constant_terms.get(store, *left)?;
            tables.constant_terms.get(store, *right)?;
        }
        ConstantTermData::Conversion { operand, target } => {
            tables.constant_terms.get(store, *operand)?;
            tables.types.get(store, *target)?;
        }
        ConstantTermData::DefinitionApplication {
            definition,
            substitution,
            selected_implementation,
        } => {
            let Some(expected) = super::super::GenericOwnerId::try_new(definition.into_any())
            else {
                return Err(SemanticValueStoreError::OpenSubstitution);
            };

            validate_application_owner(tables, store, expected, *substitution)?;

            if let Some(implementation) = selected_implementation {
                tables
                    .implementation_instances
                    .get(store, *implementation)?;
            }
        }
        ConstantTermData::Call {
            callable,
            selected_implementation,
            arguments,
        } => {
            tables.callable_instances.get(store, *callable)?;

            if let Some(implementation) = selected_implementation {
                tables
                    .implementation_instances
                    .get(store, *implementation)?;
            }

            for argument in arguments.iter().copied() {
                tables.constant_terms.get(store, argument)?;
            }
        }
        ConstantTermData::Projection(projection) => {
            tables.constant_terms.get(store, projection.subject())?;

            if let ConstantProjectionKind::ArrayElement(index) = projection.kind() {
                tables.constant_terms.get(store, index)?;
            }
        }
    }

    Ok(())
}

pub(super) fn validate_substitution_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: &GenericSubstitutionData,
) -> Result<(), SemanticValueStoreError> {
    for binding in data.bindings() {
        match binding.argument() {
            GenericArgument::Type(ty) => {
                tables.types.get(store, ty)?;
            }
            GenericArgument::Constant(term) => {
                tables.constant_terms.get(store, term)?;
            }
        }
    }

    Ok(())
}

pub(super) fn validate_trait_application_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: TraitApplicationData,
) -> Result<(), SemanticValueStoreError> {
    let symbol = crate::AnySymbolId::from(data.definition());

    let Some(expected) = super::super::GenericOwnerId::try_new(symbol) else {
        return Err(SemanticValueStoreError::OpenSubstitution);
    };

    validate_application_owner(tables, store, expected, data.substitution())
}

pub(super) fn validate_callable_instance_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: CallableInstanceData,
) -> Result<(), SemanticValueStoreError> {
    let Some(expected) = super::super::GenericOwnerId::try_new(data.definition().symbol()) else {
        return Err(SemanticValueStoreError::OpenSubstitution);
    };

    validate_application_owner(tables, store, expected, data.substitution())
}

pub(super) fn validate_implementation_instance_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: ImplementationInstanceData,
) -> Result<(), SemanticValueStoreError> {
    let expected_symbol = data.definition().into_any();

    let Some(expected) = super::super::GenericOwnerId::try_new(expected_symbol) else {
        return Err(SemanticValueStoreError::OpenSubstitution);
    };

    validate_application_owner(tables, store, expected, data.substitution())
}

fn validate_application_owner(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    expected: super::super::GenericOwnerId,
    substitution: GenericSubstitutionId,
) -> Result<(), SemanticValueStoreError> {
    let substitution = tables.substitutions.get(store, substitution)?;
    let actual = substitution.owner();

    if actual != expected {
        return Err(SemanticValueStoreError::GenericOwnerMismatch { expected, actual });
    }

    Ok(())
}

pub(super) fn validate_dependency_template_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: &DependencyContractTemplateData,
) -> Result<(), SemanticValueStoreError> {
    let mut pending: Vec<_> = data.requirements().iter().collect();

    while let Some(requirement) = pending.pop() {
        match requirement {
            DependencyRequirement::Direct { subject, .. } => {
                validate_dependency_subject(tables, store, subject)?;
            }
            DependencyRequirement::Guarded(guarded) => {
                validate_dependency_guard(tables, store, guarded.guard())?;
                pending.extend(guarded.requirements());
            }
        }
    }

    Ok(())
}

fn validate_dependency_guard(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    guard: &DependencyGuard,
) -> Result<(), SemanticValueStoreError> {
    match guard {
        DependencyGuard::NullablePresent(subject)
        | DependencyGuard::ActiveUnionVariant { subject, .. } => {
            validate_dependency_subject(tables, store, subject)
        }
    }
}

fn validate_dependency_subject(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    subject: &DependencySubject,
) -> Result<(), SemanticValueStoreError> {
    if let DependencySubjectRoot::ImplementationWitness(instance) = subject.subject_root() {
        tables.implementation_instances.get(store, instance)?;
    }

    for projection in subject.projections() {
        if let DependencyProjection::Element(index) = projection {
            tables.constant_terms.get(store, *index)?;
        }
    }

    Ok(())
}

pub(super) fn validate_concrete_substitution(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    root: GenericSubstitutionId,
) -> Result<(), SemanticValueStoreError> {
    let mut pending = vec![ConcreteWork::Substitution(root)];

    let mut types = HashSet::new();
    let mut substitutions = HashSet::new();
    let mut trait_applications = HashSet::new();

    while let Some(work) = pending.pop() {
        match work {
            ConcreteWork::Type(id) if types.insert(id) => {
                let data = tables.types.get(store, id)?;

                match data {
                    TypeData::Error
                    | TypeData::TypeParameter(_)
                    | TypeData::ContextualSelf(_)
                    | TypeData::AssociatedTypeProjection { .. } => {
                        return Err(SemanticValueStoreError::OpenSubstitution);
                    }
                    TypeData::Named { substitution, .. } => {
                        pending.push(ConcreteWork::Substitution(*substitution));
                    }
                    TypeData::Tuple(elements) => {
                        pending.extend(elements.iter().copied().map(ConcreteWork::Type));
                    }
                    TypeData::Array { element, length } => {
                        pending.push(ConcreteWork::Type(*element));

                        validate_closed_term(tables, store, *length, &mut pending)?;
                    }
                    TypeData::Slice(target)
                    | TypeData::Nullable(target)
                    | TypeData::Borrow { target, .. } => {
                        pending.push(ConcreteWork::Type(*target));
                    }
                    TypeData::TraitView(application) => {
                        pending.push(ConcreteWork::TraitApplication(*application));
                    }
                    TypeData::OwnedIndirection { storage, target } => {
                        pending.push(ConcreteWork::Type(*storage));
                        pending.push(ConcreteWork::Type(*target));
                    }
                    TypeData::Callable(callable) => {
                        pending.extend(
                            callable
                                .parameters()
                                .iter()
                                .map(|parameter| ConcreteWork::Type(parameter.ty())),
                        );

                        pending.push(ConcreteWork::Type(callable.result()));
                    }
                }
            }
            ConcreteWork::Substitution(id) if substitutions.insert(id) => {
                let substitution = tables.substitutions.get(store, id)?;

                for binding in substitution.bindings() {
                    match binding.argument() {
                        GenericArgument::Type(ty) => pending.push(ConcreteWork::Type(ty)),
                        GenericArgument::Constant(term) => {
                            validate_closed_term(tables, store, term, &mut pending)?;
                        }
                    }
                }
            }
            ConcreteWork::TraitApplication(id) if trait_applications.insert(id) => {
                let application = tables.trait_applications.get(store, id)?;

                pending.push(ConcreteWork::Substitution(application.substitution()));
            }
            ConcreteWork::Type(_)
            | ConcreteWork::Substitution(_)
            | ConcreteWork::TraitApplication(_) => {}
        }
    }

    Ok(())
}

fn validate_closed_term(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    term: super::super::ConstantTermId,
    pending: &mut Vec<ConcreteWork>,
) -> Result<(), SemanticValueStoreError> {
    let ConstantTermData::Value(value) = tables.constant_terms.get(store, term)? else {
        return Err(SemanticValueStoreError::OpenSubstitution);
    };

    let value = tables.constant_values.get(store, *value)?;

    if matches!(value.kind(), ConstantValueKind::Error) {
        return Err(SemanticValueStoreError::OpenSubstitution);
    }

    pending.push(ConcreteWork::Type(value.ty()));

    Ok(())
}

enum ConcreteWork {
    Type(TypeId),
    Substitution(GenericSubstitutionId),
    TraitApplication(TraitApplicationId),
}

fn validate_types(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    types: &[TypeId],
) -> Result<(), SemanticValueStoreError> {
    for ty in types.iter().copied() {
        tables.types.get(store, ty)?;
    }

    Ok(())
}

fn validate_values(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    values: &[super::super::ConstantValueId],
) -> Result<(), SemanticValueStoreError> {
    for value in values.iter().copied() {
        tables.constant_values.get(store, value)?;
    }

    Ok(())
}
