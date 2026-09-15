use super::{
    super::{
        CallableInstanceData, ConstantTermData, ConstantValueData, ConstantValueKind,
        DependencyContractTemplateData, DependencyGuard, DependencyProjection,
        DependencyRequirement, DependencySubject, DependencySubjectRoot, GenericArgument,
        GenericSubstitutionData, GenericSubstitutionId, ImplementationInstanceData,
        SemanticValueStoreId, TraitApplicationData, TypeData, TypeId,
    },
    table::SemanticTables,
};

pub(super) fn validate_type_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: &TypeData,
) {
    match data {
        TypeData::Error | TypeData::TypeParameter(_) | TypeData::ContextualSelf(_) => {}
        TypeData::Named { substitution, .. } => {
            tables.substitutions.get(store, *substitution);
        }
        TypeData::TypeValuedMemberProjection {
            subject,
            application,
            ..
        } => {
            tables.types.get(store, *subject);
            tables.trait_applications.get(store, *application);
        }
        TypeData::TraitView(application) => {
            tables.trait_applications.get(store, *application);
        }
        TypeData::Tuple(elements) => validate_types(tables, store, elements),
        TypeData::Array { element, length } => {
            tables.types.get(store, *element);
            tables.constant_terms.get(store, *length);
        }
        TypeData::FlexibleArray(target)
        | TypeData::Slice(target)
        | TypeData::Generator(target)
        | TypeData::Nullable(target)
        | TypeData::Borrow { target, .. } => {
            tables.types.get(store, *target);
        }
        TypeData::OwnedIndirection { storage, target } => {
            tables.types.get(store, *storage);
            tables.types.get(store, *target);
        }
        TypeData::Callable(callable) => {
            for parameter in callable.parameters() {
                tables.types.get(store, parameter.ty());
            }

            tables.types.get(store, callable.result());

            let dependencies = callable.dependency_contracts();

            tables
                .dependency_contracts
                .get(store, dependencies.invocation());

            if let Some(deferred) = dependencies.deferred_execution() {
                tables.dependency_contracts.get(store, deferred);
            }
        }
    }
}

pub(super) fn validate_constant_value_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: &ConstantValueData,
) {
    tables.types.get(store, data.ty());

    match data.kind() {
        ConstantValueKind::NullablePresent(value) => {
            tables.constant_values.get(store, *value);
        }
        ConstantValueKind::Tuple(values) | ConstantValueKind::Array(values) => {
            validate_values(tables, store, values);
        }
        ConstantValueKind::Product(fields) => {
            validate_field_values(tables, store, fields);
        }
        ConstantValueKind::Union { fields, .. } => {
            validate_field_values(tables, store, fields);
        }
        ConstantValueKind::Error
        | ConstantValueKind::Boolean(_)
        | ConstantValueKind::Character(_)
        | ConstantValueKind::Integer(_)
        | ConstantValueKind::Real(_)
        | ConstantValueKind::Complex { .. }
        | ConstantValueKind::String(_)
        | ConstantValueKind::StaticAddress(_)
        | ConstantValueKind::Unit
        | ConstantValueKind::NullableAbsent => {}
    }
}

pub(super) fn validate_constant_term_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: &ConstantTermData,
) {
    match data {
        ConstantTermData::Typed { term, ty } => {
            tables.constant_terms.get(store, *term);
            tables.types.get(store, *ty);
        }
        ConstantTermData::Value(value) => {
            tables.constant_values.get(store, *value);
        }
        ConstantTermData::IntegerLiteral { .. } => {}
        ConstantTermData::CallableArgument(_)
        | ConstantTermData::Parameter(_)
        | ConstantTermData::TargetProperty(_) => {}
        ConstantTermData::Unary { operand, .. } => {
            tables.constant_terms.get(store, *operand);
        }
        ConstantTermData::Binary { left, right, .. } => {
            tables.constant_terms.get(store, *left);
            tables.constant_terms.get(store, *right);
        }
        ConstantTermData::Conversion { operand, target } => {
            tables.constant_terms.get(store, *operand);
            tables.types.get(store, *target);
        }
        ConstantTermData::NullablePresent(value) => {
            tables.constant_terms.get(store, *value);
        }
        ConstantTermData::Tuple(values) | ConstantTermData::Array(values) => {
            for value in values.iter() {
                tables.constant_terms.get(store, *value);
            }
        }
        ConstantTermData::Product(fields) => {
            validate_field_terms(tables, store, fields);
        }
        ConstantTermData::Union { fields, .. } => {
            validate_field_terms(tables, store, fields);
        }
        ConstantTermData::DefinitionApplication {
            definition,
            substitution,
            selected_implementation,
        } => {
            let expected = super::super::GenericOwnerId::try_new(definition.into_any())
                .expect("application definition must be a generic owner");

            validate_application_owner(tables, store, expected, *substitution);

            if let Some(implementation) = selected_implementation {
                tables.implementation_instances.get(store, *implementation);
            }
        }
        ConstantTermData::Call {
            callable,
            selected_implementation,
            arguments,
        } => {
            tables.callable_instances.get(store, *callable);

            if let Some(implementation) = selected_implementation {
                tables.implementation_instances.get(store, *implementation);
            }

            for argument in arguments.iter().copied() {
                tables.constant_terms.get(store, argument);
            }
        }
        ConstantTermData::PredicateCall {
            predicate,
            arguments,
        } => {
            let expected = super::super::GenericOwnerId::try_new(predicate.definition().into_any())
                .expect("application definition must be a generic owner");

            validate_application_owner(tables, store, expected, predicate.substitution());

            for argument in arguments.iter().copied() {
                tables.constant_terms.get(store, argument);
            }
        }
        ConstantTermData::Projection(projection) => {
            tables.constant_terms.get(store, projection.subject());

            for term in projection.kind().term_references() {
                tables.constant_terms.get(store, term);
            }
        }
    }
}

fn validate_field_values<I>(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    fields: &[super::super::ConstantField<I, super::super::ConstantValueId>],
) {
    for field in fields {
        tables.constant_values.get(store, *field.value());
    }
}

fn validate_field_terms<I>(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    fields: &[super::super::ConstantField<I, super::super::ConstantTermId>],
) {
    for field in fields {
        tables.constant_terms.get(store, *field.value());
    }
}

pub(super) fn validate_substitution_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: &GenericSubstitutionData,
) {
    for binding in data.bindings() {
        match binding.argument() {
            GenericArgument::Type(ty) => {
                tables.types.get(store, ty);
            }
            GenericArgument::Constant(term) => {
                tables.constant_terms.get(store, term);
            }
        }
    }
}

pub(super) fn validate_trait_application_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: TraitApplicationData,
) {
    let symbol = crate::AnySymbolId::from(data.definition());

    let expected = super::super::GenericOwnerId::try_new(symbol)
        .expect("application definition must be a generic owner");

    validate_application_owner(tables, store, expected, data.substitution())
}

pub(super) fn validate_callable_instance_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: CallableInstanceData,
) {
    let expected = super::super::GenericOwnerId::try_new(data.definition().symbol())
        .expect("application definition must be a generic owner");

    validate_application_owner(tables, store, expected, data.substitution())
}

pub(super) fn validate_implementation_instance_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: ImplementationInstanceData,
) {
    let expected_symbol = data.definition().into_any();

    let expected = super::super::GenericOwnerId::try_new(expected_symbol)
        .expect("application definition must be a generic owner");

    validate_application_owner(tables, store, expected, data.substitution())
}

fn validate_application_owner(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    expected: super::super::GenericOwnerId,
    substitution: GenericSubstitutionId,
) {
    let substitution = tables.substitutions.get(store, substitution);
    let actual = substitution.owner();

    assert_eq!(actual, expected, "application substitution owner mismatch");
}

pub(super) fn validate_dependency_template_data(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    data: &DependencyContractTemplateData,
) {
    validate_dependency_variables(data.requirements(), &mut Vec::new());
    let mut pending: Vec<_> = data.requirements().iter().collect();

    while let Some(requirement) = pending.pop() {
        match requirement {
            DependencyRequirement::FixedPoint {
                definitions,
                result,
            } => {
                pending.extend(
                    definitions
                        .iter()
                        .flat_map(|definition| definition.iter())
                        .chain(result.iter()),
                );
            }
            DependencyRequirement::Variable { .. } => {}
            DependencyRequirement::ResultCall {
                callable,
                requirement,
                inputs,
            } => {
                tables.callable_instances.get(store, *callable);

                if let Some(requirement) = requirement {
                    tables.types.get(store, requirement.subject());

                    tables
                        .trait_applications
                        .get(store, requirement.trait_application());
                }

                for input in inputs.iter() {
                    pending.extend(input.values());
                    pending.extend(input.storage());
                }
            }
            DependencyRequirement::Direct { subject, .. } => {
                validate_dependency_subject(tables, store, subject);
            }
            DependencyRequirement::Guarded(guarded) => {
                validate_dependency_guard(tables, store, guarded.guard());
                pending.extend(guarded.requirements());
            }
        }
    }
}

fn validate_dependency_variables(requirements: &[DependencyRequirement], scopes: &mut Vec<usize>) {
    for requirement in requirements {
        match requirement {
            DependencyRequirement::Variable { depth, ordinal } => {
                let count = usize::try_from(*depth)
                    .ok()
                    .and_then(|depth| scopes.iter().rev().nth(depth))
                    .copied();

                if !count.is_some_and(|count| u64::from(ordinal.raw()) < count as u64) {
                    panic!(
                        "dependency variable {depth}:{} is outside its enclosing equations",
                        ordinal.raw()
                    );
                }
            }
            DependencyRequirement::FixedPoint {
                definitions,
                result,
            } => {
                scopes.push(definitions.len());

                for definition in definitions.iter().chain(std::iter::once(result)) {
                    validate_dependency_variables(definition, scopes);
                }

                scopes.pop();
            }
            DependencyRequirement::ResultCall { inputs, .. } => {
                for input in inputs.iter() {
                    validate_dependency_variables(input.values(), scopes);
                    validate_dependency_variables(input.storage(), scopes);
                }
            }
            DependencyRequirement::Guarded(guarded) => {
                validate_dependency_variables(guarded.requirements(), scopes)
            }
            DependencyRequirement::Direct { .. } => {}
        }
    }
}

fn validate_dependency_guard(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    guard: &DependencyGuard,
) {
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
) {
    if let DependencySubjectRoot::ImplementationWitness(instance) = subject.subject_root() {
        tables.implementation_instances.get(store, instance);
    }

    for projection in subject.projections() {
        if let DependencyProjection::Element(index) = projection {
            tables.constant_terms.get(store, *index);
        }
    }
}

fn validate_types(tables: &SemanticTables, store: SemanticValueStoreId, types: &[TypeId]) {
    for ty in types.iter().copied() {
        tables.types.get(store, ty);
    }
}

fn validate_values(
    tables: &SemanticTables,
    store: SemanticValueStoreId,
    values: &[super::super::ConstantValueId],
) {
    for value in values.iter().copied() {
        tables.constant_values.get(store, value);
    }
}

#[cfg(test)]
mod tests {
    use crate::DependencyRequirement;
    use crate::SymbolOrdinal;

    #[test]
    fn dependency_variable_validation_retains_the_invalid_reference() {
        for (depth, ordinal, valid) in [(0, 0, true), (1, 0, false), (0, 1, false)] {
            let variable = DependencyRequirement::variable(depth, SymbolOrdinal::new(ordinal));
            let graph = DependencyRequirement::fixed_point([vec![variable.clone()]], [variable]);

            let result = std::panic::catch_unwind(|| {
                super::validate_dependency_variables(&[graph], &mut Vec::new())
            });

            assert_eq!(result.is_ok(), valid);
        }
    }
}
