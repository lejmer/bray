use bray_bound_tree::{ConversionTarget, SelectedConversion};
use bray_symbols::{
    DependencyContractTemplateData, DependencyProjection, DependencyRequirement,
    DependencyRequirementKind, DependencyStorageEffect, DependencySubject, DependencySubjectRoot,
    SymbolOrdinal, TypeData,
};

use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

pub(in crate::dependency::call) fn conversion_dependencies<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    conversion: &SelectedConversion,
) -> Result<DependencyContractTemplateData, CheckerQueryError<C::UpstreamError>> {
    match conversion.target() {
        ConversionTarget::Trait { fulfillment, .. } => super::templates::invocation_dependencies(
            request,
            *fulfillment,
            None,
            [DependencySubjectRoot::Receiver],
        ),
        ConversionTarget::TraitConstraint {
            member, dispatch, ..
        } => super::templates::invocation_dependencies(
            request,
            *member,
            Some(*dispatch),
            [DependencySubjectRoot::Receiver],
        ),
        ConversionTarget::Composite(children) => {
            let shape = request
                .semantic_values()
                .type_data(conversion.source_type());

            let mut result = DependencyContractTemplateData::new([]).with_effects([]);

            let may_skip_child = match shape.as_ref() {
                TypeData::Nullable(_) => true,
                TypeData::Array { length, .. } => {
                    match request.semantic_values().constant_term_integer(*length) {
                        Some(length) if length.is_zero() => return Ok(result),
                        Some(_) => false,
                        None => true,
                    }
                }
                TypeData::Tuple(_) => false,
                _ => panic!("composite conversion retains its checked aggregate shape"),
            };

            for (ordinal, child) in children.iter().enumerate() {
                let child = conversion_dependencies(request, child)?;
                let normal_completion = may_skip_child || child.normal_completion();
                let child = child.with_normal_completion(normal_completion);

                let child = child
                    .try_map_requirements(|output, requirements| {
                        if output == bray_symbols::DependencyCallOutput::SynchronousCleanup {
                            return Ok(requirements.to_vec());
                        }

                        crate::dependency::witness::map_requirements::<std::convert::Infallible>(
                            requirements,
                            &mut |subject, kind, _| {
                                // Array elements form one dependency summary. Retain the
                                // aggregate instead of manufacturing an element identity.
                                let subject = project_subject(subject, ordinal, shape.as_ref())
                                    .unwrap_or_else(|| {
                                        DependencySubject::root(DependencySubjectRoot::Receiver)
                                    });

                                Ok(vec![DependencyRequirement::direct(subject, kind)])
                            },
                        )
                    })
                    .expect("conversion projection preserves checked dependency equation bindings");

                let project_effect = |effect: &DependencyStorageEffect| project_effect(effect, ordinal, shape.as_ref());
                let effects = child.effects().iter().map(project_effect).collect::<Vec<_>>();

                let outcomes = child.captured_values().iter().cloned().map(|mut outcome| {
                    let project = |path: &bray_symbols::DependencyStoredValuePath| {
                        let subject = path.subject();

                        if !matches!(subject.subject_root(), DependencySubjectRoot::Receiver | DependencySubjectRoot::Result) {
                            return path.clone();
                        }

                        match project_component(subject, ordinal, shape.as_ref()) {
                            Some(subject) => bray_symbols::DependencyStoredValuePath::new(
                                subject, path.dereferences(),
                            ),
                            None => bray_symbols::DependencyStoredValuePath::new(
                                DependencySubject::root(subject.subject_root()), 0,
                            ),
                        }
                    };

                    outcome.stored_values = bray_symbols::DependencyStoredValue::normalize(
                        outcome.stored_values.iter().map(|stored| bray_symbols::DependencyStoredValue::new(
                            project(stored.destination()),
                            stored.sources().iter().map(project),
                            stored.is_complete() && !matches!(shape.as_ref(), TypeData::Array { .. }),
                        )),
                    );

                    let effects = outcome.effects.iter().map(project_effect).collect::<Vec<_>>();

                    outcome.with_effects(effects)
                }).collect::<Vec<_>>();

                let failures = child.failure_outcomes().map(|rows| rows.iter().cloned().map(|mut row| {
                    // A child guard names the child's receiver. Composition loses its
                    // condition when the outer conversion cannot name that receiver.
                    row.guard = None;

                    row.effects = row.effects.as_deref().map(|effects|
                        bray_base::sorted_unique_shared_slice(effects.iter().map(project_effect)));

                    row
                }).collect::<Vec<_>>());

                let child = child.with_captured_values(outcomes).with_failure_outcomes(failures).with_possible_effects(effects);

                let child = if matches!(shape.as_ref(), TypeData::Nullable(_)) {
                    child
                        .try_map_requirements(|output, requirements| {
                            if output == bray_symbols::DependencyCallOutput::SynchronousCleanup {
                                return Ok(requirements.to_vec());
                            }

                            Ok::<_, std::convert::Infallible>(vec![DependencyRequirement::guarded(
                                bray_symbols::DependencyGuard::NullablePresent(
                                    DependencySubject::root(DependencySubjectRoot::Receiver),
                                ),
                                requirements.iter().cloned(),
                            )])
                        })
                        .expect("nullable conversion dependency guard is infallible")
                } else {
                    child
                };

                result = sequence_dependencies(result, child);
            }

            Ok(result)
        }
        ConversionTarget::Identity
        | ConversionTarget::CallableContract
        | ConversionTarget::NullablePresent
        | ConversionTarget::BuiltInScalar
        | ConversionTarget::CVariadicPromotion => {
            let mut result = DependencyContractTemplateData::new([DependencyRequirement::direct(
                DependencySubject::root(DependencySubjectRoot::Receiver),
                DependencyRequirementKind::ValueDependencies,
            )])
            .with_effects([]);

            if matches!(
                conversion.target(),
                ConversionTarget::Identity
                    | ConversionTarget::CallableContract
                    | ConversionTarget::NullablePresent
            ) {
                let values = request.semantic_values();

                let input = values
                    .intern_constant_term(bray_symbols::ConstantTermData::CallableArgument(
                        SymbolOrdinal::new(0),
                    ))
                    .expect("canonical conversion input belongs to the checked semantic store");

                let nullable = matches!(conversion.target(), ConversionTarget::NullablePresent);
                let value = bray_symbols::CapturedValueTemplate::Input(input);

                let value = if nullable {
                    bray_symbols::CapturedValueTemplate::Operation(
                        bray_symbols::CapturedValueOperation::Present, [value].into(),
                    )
                } else {
                    value
                };

                let transfer = bray_symbols::DependencyOwnedTransfer::new(
                    input, nullable.then_some(bray_symbols::ConstantProjectionKind::NullableValue),
                ).with_preservation(bray_symbols::DependencyValuePreservation::all());

                let outcome = bray_symbols::CapturedValueOutcome::new(None, [
                    (DependencySubject::root(DependencySubjectRoot::Result), value),
                ]).with_ownership(bray_symbols::DependencyOwnedResult::Transfer(transfer));

                result = result.with_captured_values([outcome]);
            }

            Ok(result)
        }
    }
}

fn project_subject(
    subject: &DependencySubject,
    ordinal: usize,
    shape: &TypeData,
) -> Option<DependencySubject> {
    if subject.subject_root() != DependencySubjectRoot::Receiver {
        return Some(subject.clone());
    }

    project_component(subject, ordinal, shape)
}

fn project_component(
    subject: &DependencySubject,
    ordinal: usize,
    shape: &TypeData,
) -> Option<DependencySubject> {
    let prefix = match shape {
        TypeData::Tuple(_) => DependencyProjection::TupleElement(SymbolOrdinal::new(
            u32::try_from(ordinal).expect("tuple conversion has a finite component count"),
        )),
        TypeData::Nullable(_) => DependencyProjection::NullableValue,
        TypeData::Array { .. } => return None,
        _ => panic!("composite conversion retains its checked aggregate shape"),
    };

    Some(DependencySubject::new(
        subject.subject_root(),
        std::iter::once(prefix).chain(subject.projections().iter().copied()),
    ))
}

fn project_effect(
    effect: &DependencyStorageEffect,
    ordinal: usize,
    shape: &TypeData,
) -> DependencyStorageEffect {
    // Projection retains the callee's immutable reference suffix and mutation kind.
    match effect {
        DependencyStorageEffect::Direct { addressed, subject, kind, access_type } => {
            project_subject(subject, ordinal, shape)
                .map(|subject| DependencyStorageEffect::Direct { addressed: *addressed,
                    subject,
                    kind: *kind,
                    access_type: *access_type,
                })
                .unwrap_or_else(|| DependencyStorageEffect::unresolved(*kind, *access_type))
        }
        DependencyStorageEffect::Indirect { addressed, reads,
            reference,
            projections,
            kind,
            access_type,
        } => project_subject(reference, ordinal, shape)
            .map(|reference| DependencyStorageEffect::Indirect { addressed: *addressed, reads: std::sync::Arc::clone(reads),
                reference,
                projections: std::sync::Arc::clone(projections),
                kind: *kind,
                access_type: *access_type,
            })
            // A uniform array summary cannot select one runtime element. Use the
            // existing unresolved-alias meaning, never a path in the element type.
            .unwrap_or_else(|| DependencyStorageEffect::unresolved(*kind, *access_type)),
        DependencyStorageEffect::UnboundedMutation { .. }
        | DependencyStorageEffect::Atomic
        | DependencyStorageEffect::Escape
        | DependencyStorageEffect::Ambient
        | DependencyStorageEffect::Unknown => effect.clone(),
    }
}

pub(super) fn sequence_dependencies(
    first: DependencyContractTemplateData,
    next: DependencyContractTemplateData,
) -> DependencyContractTemplateData {
    let requirements = first
        .requirements()
        .iter()
        .chain(next.requirements())
        .cloned();

    let normal = first.normal_completion() && next.normal_completion();

    let mut result = DependencyContractTemplateData::new(requirements)
        .with_possible_effects(first.effects().iter().chain(next.effects()).cloned())
        .with_synchronous_cleanup(
            first
                .synchronous_cleanup()
                .iter()
                .chain(next.synchronous_cleanup())
                .cloned(),
        )
        .with_normal_completion(normal);

    if normal {
        let effects = first.normal_effects().iter().chain(next.normal_effects()).cloned();

        result = result.with_captured_values([bray_symbols::CapturedValueOutcome::new(None, []).with_effects(effects)]);
        result = result.with_stored_values(first.stored_values().iter().chain(next.stored_values().iter()).cloned());
    }

    let reports = [first.completion_reports(), next.completion_reports()];

    if normal && reports.iter().any(Option::is_some) {
        result = result.with_completion_reports(reports.iter().flatten().flat_map(|report| report.iter().cloned()));
    }

    let failures = |contract: &DependencyContractTemplateData| {
        contract.failure_outcomes().map(<[_]>::to_vec).unwrap_or_else(|| vec![bray_symbols::CapturedFailureOutcome {
            kind: None,
            guard: None,
            report: None,
            effects: Some(contract.effects().into()),
        }])
    };

    let mut outcomes = failures(&first);

    if first.normal_completion() {
        outcomes.extend(failures(&next).into_iter().map(|mut row| {
            let effects = row.effects.as_deref().unwrap_or(next.effects());

            row.effects = Some(bray_base::sorted_unique_shared_slice(first.normal_effects().iter().chain(effects).cloned()));

            if let Some(completed) = &reports[0] {
                row.report = Some(bray_base::sorted_unique_shared_slice(completed.iter()
                    .chain(row.report.as_deref().into_iter().flatten()).cloned()));
            }

            row
        }));
    }

    result.with_failure_outcomes(Some(outcomes))
}
