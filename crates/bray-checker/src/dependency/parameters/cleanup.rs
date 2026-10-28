use bray_symbols::{DependencyRequirement, TypeData, TypeId, TypeLifecycle};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

pub(super) fn has_conditions(requirements: &[DependencyRequirement]) -> bool {
    requirements.iter().any(|requirement| match requirement {
        DependencyRequirement::CleanupDependencies { .. } => true,
        DependencyRequirement::Guarded(guarded) => has_conditions(guarded.requirements()),
        DependencyRequirement::FixedPoint {
            definitions,
            result,
        } => definitions.iter().any(|items| has_conditions(items)) || has_conditions(result),
        DependencyRequirement::ResultCall { inputs, .. } => inputs
            .iter()
            .any(|input| has_conditions(input.values()) || has_conditions(input.storage())),
        _ => false,
    })
}

pub(super) fn exact_requirements<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    requirements: &[DependencyRequirement],
) -> Result<Vec<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
    super::super::equations::map_requirements(requirements, 0, &mut |requirement, _| {
        let DependencyRequirement::CleanupDependencies { ty, requirements } = requirement else {
            return None;
        };

        Some((|| {
            let mut result = exact_requirements(request, requirements)?;

            result.extend(exact_predicate_requirements(request, *ty)?);

            Ok(result)
        })())
    })
}

/// Structural cleanup depends on parameter cleanup presence, while projections
/// that select a type still need their exact implementation-selection arguments.
pub(super) fn exact_predicate_requirements<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> Result<Vec<DependencyRequirement>, CheckerQueryError<C::UpstreamError>> {
    let values = request.semantic_values();

    if values.type_is_concrete(ty)
        || matches!(
            values.type_data(ty).as_ref(),
            TypeData::TypeParameter(_) | TypeData::OwnedIndirection { .. }
        )
    {
        return Ok(Vec::new());
    }

    let mut cleanup = crate::asynchronous::CleanupShapeResolver::new(request);

    let parts = crate::asynchronous::full_cleanup_parts(
        &mut cleanup,
        ty,
        request.unit().key().source(),
        false,
    )?;

    if parts.diagnostics().has_errors() {
        return Ok(vec![DependencyRequirement::cleanup_dependencies(ty, [])]);
    }

    let mut result = Vec::new();

    for (part_ty, part) in parts.value() {
        if part
            .as_ref()
            .is_some_and(|part| part.release().is_some() || part.phases().includes_cancellation())
        {
            // An owned release or run makes cleanup unconditional for this type.
            return Ok(Vec::new());
        }

        match values.type_data(*part_ty).as_ref() {
            TypeData::TypeParameter(_) => {}
            TypeData::Named { definition, .. } => {
                if *request.declared_type_lifecycle(*definition)?.value() != TypeLifecycle::None {
                    return Ok(Vec::new());
                }

                result.push(DependencyRequirement::cleanup_dependencies(*part_ty, []));
            }
            _ => result.push(DependencyRequirement::cleanup_dependencies(*part_ty, [])),
        }
    }

    Ok(result)
}
