use bray_binder::{BinderFactContext, SymbolFactProvider};
use bray_diagnostics::DiagnosticBag;
use std::collections::BTreeSet;

use bray_symbols::{
    AnySymbolId, CallableParameterData, CallableTypeData, CheckedConstraint, CheckedConstraintKind,
    GenericArgument, GenericConstraintsFact, GenericOwnerId, GenericSubstitutionData,
    SemanticValueStore, SymbolFactRequest, TraitApplicationData, TypeData, TypeId,
};

use crate::compilation::binder::{CompilationBinderFacts, binder_fact_error};
use crate::fact::FactQueryError;

pub(super) fn enclosing_generic_constraints(
    facts: &CompilationBinderFacts<'_>,
    mut owner: AnySymbolId,
    diagnostics: &mut DiagnosticBag,
) -> Result<Vec<(GenericOwnerId, CheckedConstraint)>, FactQueryError> {
    let mut result = Vec::new();

    loop {
        if let Some(generic_owner) = GenericOwnerId::try_new(owner) {
            let constraints = facts
                .symbol_fact(SymbolFactRequest::<GenericConstraintsFact>::new(
                    generic_owner,
                ))
                .map_err(binder_fact_error)?;

            *diagnostics = diagnostics.merged(constraints.diagnostics());

            result.extend(
                constraints
                    .value()
                    .constraints()
                    .iter()
                    .copied()
                    .map(|constraint| (generic_owner, constraint)),
            );
        }

        let Some(container) = facts.symbols().containing_symbol(owner) else {
            break;
        };

        owner = container;
    }

    Ok(result)
}

pub(super) fn normalize_type_equalities(
    values: &SemanticValueStore,
    ty: TypeId,
    constraints: &[(GenericOwnerId, CheckedConstraint)],
) -> Result<TypeId, FactQueryError> {
    let mut active = BTreeSet::new();

    normalize_type(values, ty, constraints, &mut active)
}

fn normalize_type(
    values: &SemanticValueStore,
    ty: TypeId,
    constraints: &[(GenericOwnerId, CheckedConstraint)],
    active: &mut BTreeSet<TypeId>,
) -> Result<TypeId, FactQueryError> {
    if !active.insert(ty) {
        return Ok(ty);
    }

    let canonical = canonical_equal_type(values, ty, constraints)?;

    if canonical != ty {
        let normalized = normalize_type(values, canonical, constraints, active)?;

        active.remove(&ty);

        return Ok(normalized);
    }

    let data = values
        .type_data(ty)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let normalized = match data.as_ref() {
        TypeData::Error | TypeData::TypeParameter(_) | TypeData::ContextualSelf(_) => ty,
        TypeData::Named {
            definition,
            substitution,
        } => values
            .intern_type(TypeData::Named {
                definition: *definition,
                substitution: normalize_substitution(values, *substitution, constraints, active)?,
            })
            .map_err(|_| FactQueryError::InfrastructureFailure)?,
        TypeData::TypeValuedMemberProjection {
            subject,
            application,
            member,
        } => values
            .intern_type(TypeData::TypeValuedMemberProjection {
                subject: normalize_type(values, *subject, constraints, active)?,
                application: normalize_application(values, *application, constraints, active)?,
                member: *member,
            })
            .map_err(|_| FactQueryError::InfrastructureFailure)?,
        TypeData::Tuple(elements) => {
            let elements = elements
                .iter()
                .copied()
                .map(|element| normalize_type(values, element, constraints, active))
                .collect::<Result<Vec<_>, _>>()?;

            values
                .intern_type(TypeData::tuple(elements))
                .map_err(|_| FactQueryError::InfrastructureFailure)?
        }
        TypeData::Array { element, length } => values
            .intern_type(TypeData::Array {
                element: normalize_type(values, *element, constraints, active)?,
                length: *length,
            })
            .map_err(|_| FactQueryError::InfrastructureFailure)?,
        TypeData::Slice(element) => values
            .intern_type(TypeData::Slice(normalize_type(
                values,
                *element,
                constraints,
                active,
            )?))
            .map_err(|_| FactQueryError::InfrastructureFailure)?,
        TypeData::Generator(element) => values
            .intern_type(TypeData::Generator(normalize_type(
                values,
                *element,
                constraints,
                active,
            )?))
            .map_err(|_| FactQueryError::InfrastructureFailure)?,
        TypeData::Nullable(target) => values
            .intern_type(TypeData::Nullable(normalize_type(
                values,
                *target,
                constraints,
                active,
            )?))
            .map_err(|_| FactQueryError::InfrastructureFailure)?,
        TypeData::Borrow { kind, target } => values
            .intern_type(TypeData::Borrow {
                kind: *kind,
                target: normalize_type(values, *target, constraints, active)?,
            })
            .map_err(|_| FactQueryError::InfrastructureFailure)?,
        TypeData::TraitView(application) => values
            .intern_type(TypeData::TraitView(normalize_application(
                values,
                *application,
                constraints,
                active,
            )?))
            .map_err(|_| FactQueryError::InfrastructureFailure)?,
        TypeData::OwnedIndirection { storage, target } => values
            .intern_type(TypeData::OwnedIndirection {
                storage: normalize_type(values, *storage, constraints, active)?,
                target: normalize_type(values, *target, constraints, active)?,
            })
            .map_err(|_| FactQueryError::InfrastructureFailure)?,
        TypeData::Callable(callable) => {
            let parameters = callable
                .parameters()
                .iter()
                .map(|parameter| {
                    normalize_type(values, parameter.ty(), constraints, active).map(|ty| {
                        CallableParameterData::new(
                            parameter.name().clone(),
                            parameter.position(),
                            parameter.mode(),
                            ty,
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?;

            let callable = CallableTypeData::new(
                parameters,
                normalize_type(values, callable.result(), constraints, active)?,
                callable.constness(),
                callable.trust(),
                callable.abi(),
                callable.dependency_contracts(),
            )
            .with_phase_behaviors(callable.phase_behaviors().clone());

            values
                .intern_type(TypeData::Callable(callable))
                .map_err(|_| FactQueryError::InfrastructureFailure)?
        }
    };

    active.remove(&ty);

    Ok(normalized)
}

fn canonical_equal_type(
    values: &SemanticValueStore,
    ty: TypeId,
    constraints: &[(GenericOwnerId, CheckedConstraint)],
) -> Result<TypeId, FactQueryError> {
    let mut component = BTreeSet::from([ty]);
    let mut changed = true;

    while changed {
        changed = false;

        for (_, constraint) in constraints {
            let CheckedConstraintKind::TypeEquality { left, right } = constraint.kind() else {
                continue;
            };

            if component.contains(&left) || component.contains(&right) {
                changed |= component.insert(left);
                changed |= component.insert(right);
            }
        }
    }

    component
        .into_iter()
        .map(|candidate| {
            let data = values
                .type_data(candidate)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let is_projection =
                matches!(data.as_ref(), TypeData::TypeValuedMemberProjection { .. });

            Ok(((is_projection, candidate), candidate))
        })
        .collect::<Result<Vec<_>, FactQueryError>>()?
        .into_iter()
        .min_by_key(|(key, _)| *key)
        .map(|(_, candidate)| candidate)
        .ok_or(FactQueryError::InfrastructureFailure)
}

fn normalize_application(
    values: &SemanticValueStore,
    application: bray_symbols::TraitApplicationId,
    constraints: &[(GenericOwnerId, CheckedConstraint)],
    active: &mut BTreeSet<TypeId>,
) -> Result<bray_symbols::TraitApplicationId, FactQueryError> {
    let application = values
        .trait_application_data(application)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let substitution =
        normalize_substitution(values, application.substitution(), constraints, active)?;

    values
        .intern_trait_application(TraitApplicationData::new(
            application.definition(),
            substitution,
        ))
        .map_err(|_| FactQueryError::InfrastructureFailure)
}

fn normalize_substitution(
    values: &SemanticValueStore,
    substitution: bray_symbols::GenericSubstitutionId,
    constraints: &[(GenericOwnerId, CheckedConstraint)],
    active: &mut BTreeSet<TypeId>,
) -> Result<bray_symbols::GenericSubstitutionId, FactQueryError> {
    let substitution = values
        .generic_substitution_data(substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let (parameters, arguments): (Vec<_>, Vec<_>) = substitution
        .bindings()
        .iter()
        .map(|binding| {
            let argument = match binding.argument() {
                GenericArgument::Type(ty) => {
                    GenericArgument::Type(normalize_type(values, ty, constraints, active)?)
                }
                GenericArgument::Constant(term) => GenericArgument::Constant(term),
            };

            Ok((binding.parameter(), argument))
        })
        .collect::<Result<Vec<_>, FactQueryError>>()?
        .into_iter()
        .unzip();

    let substitution =
        GenericSubstitutionData::try_new(substitution.owner(), parameters, arguments)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

    values
        .intern_generic_substitution(substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)
}
