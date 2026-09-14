use bray_symbols::{DependencyCallInput, DependencyRequirement, SemanticValueStoreError};

pub(super) fn map_requirements<E>(
    requirements: &[DependencyRequirement],
    depth: u32,
    transform: &mut impl FnMut(
        &DependencyRequirement,
        u32,
    ) -> Option<Result<Vec<DependencyRequirement>, E>>,
) -> Result<Vec<DependencyRequirement>, E> {
    // Rebuilt requirements share unchanged subjects and guards with their input.
    requirements
        .iter()
        .map(|requirement| {
            if let Some(mapped) = transform(requirement, depth) {
                return mapped;
            }

            let mapped = match requirement {
                DependencyRequirement::FixedPoint {
                    definitions,
                    result,
                } => DependencyRequirement::fixed_point(
                    definitions
                        .iter()
                        .map(|definition| {
                            map_requirements(definition, depth.saturating_add(1), transform)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                    map_requirements(result, depth.saturating_add(1), transform)?,
                ),
                DependencyRequirement::ResultCall {
                    callable,
                    requirement,
                    inputs,
                } => DependencyRequirement::result_call(
                    *callable,
                    *requirement,
                    map_inputs(inputs, depth, transform)?,
                ),
                DependencyRequirement::Guarded(guarded) => DependencyRequirement::guarded(
                    guarded.guard().clone(),
                    map_requirements(guarded.requirements(), depth, transform)?,
                ),
                // Unchanged atomic requirements retain their immutable data in the rebuilt equation group.
                DependencyRequirement::Direct { .. } | DependencyRequirement::Variable { .. } => {
                    requirement.clone()
                }
            };

            Ok(vec![mapped])
        })
        .collect::<Result<Vec<_>, _>>()
        .map(|groups| groups.into_iter().flatten().collect())
}

fn map_inputs<E>(
    inputs: &[DependencyCallInput],
    depth: u32,
    transform: &mut impl FnMut(
        &DependencyRequirement,
        u32,
    ) -> Option<Result<Vec<DependencyRequirement>, E>>,
) -> Result<Vec<DependencyCallInput>, E> {
    map_input_requirements(inputs, &mut |requirements| {
        map_requirements(requirements, depth, transform)
    })
}

pub(super) fn map_input_requirements<E>(
    inputs: &[DependencyCallInput],
    transform: &mut impl FnMut(&[DependencyRequirement]) -> Result<Vec<DependencyRequirement>, E>,
) -> Result<Vec<DependencyCallInput>, E> {
    inputs
        .iter()
        .map(|input| {
            Ok(DependencyCallInput::new(
                input.root(),
                transform(input.values())?,
                transform(input.storage())?,
            ))
        })
        .collect()
}
pub(super) fn shift_variables(
    requirements: &[DependencyRequirement],
    amount: i64,
) -> Result<Vec<DependencyRequirement>, SemanticValueStoreError> {
    map_requirements(requirements, 0, &mut |requirement, binders| {
        let DependencyRequirement::Variable { depth, ordinal } = requirement else {
            return None;
        };

        if *depth < binders {
            return None;
        }

        let shifted = i64::from(*depth)
            .checked_add(amount)
            .and_then(|value| u32::try_from(value).ok());

        Some(
            shifted
                .map(|depth| vec![DependencyRequirement::variable(depth, *ordinal)])
                .ok_or(SemanticValueStoreError::InvalidDependencyVariable {
                    depth: *depth,
                    ordinal: ordinal.raw(),
                }),
        )
    })
}

pub(super) fn has_variables(requirements: &[DependencyRequirement], local_only: bool) -> bool {
    let mut pending = requirements
        .iter()
        .map(|requirement| (requirement, 0_u32))
        .collect::<Vec<_>>();

    while let Some((requirement, binders)) = pending.pop() {
        match requirement {
            DependencyRequirement::Variable { depth, .. }
                if *depth == binders || !local_only && *depth > binders =>
            {
                return true;
            }
            DependencyRequirement::FixedPoint {
                definitions,
                result,
            } => pending.extend(
                definitions
                    .iter()
                    .flat_map(|definition| definition.iter())
                    .chain(result.iter())
                    .map(|requirement| (requirement, binders.saturating_add(1))),
            ),
            DependencyRequirement::ResultCall { inputs, .. } => pending.extend(
                inputs
                    .iter()
                    .flat_map(|input| input.values().iter().chain(input.storage()))
                    .map(|requirement| (requirement, binders)),
            ),
            DependencyRequirement::Guarded(guarded) => pending.extend(
                guarded
                    .requirements()
                    .iter()
                    .map(|requirement| (requirement, binders)),
            ),
            DependencyRequirement::Direct { .. } | DependencyRequirement::Variable { .. } => {}
        }
    }

    false
}
