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

                let effects = child.effect_outcomes().try_map(|effect| {
                    Ok::<_, std::convert::Infallible>(project_effect(effect, ordinal, shape.as_ref()))
                }).expect("conversion projection preserves effect outcome guards");

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

                    outcome
                }).collect::<Vec<_>>();

                let child = child.with_effect_outcomes(effects).with_captured_values(outcomes);

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
        .with_effects(first.effects().iter().chain(next.effects()).cloned())
        .with_synchronous_cleanup(
            first
                .synchronous_cleanup()
                .iter()
                .chain(next.synchronous_cleanup())
                .cloned(),
        )
        .with_normal_completion(normal)
        .with_normal_effects(first.normal_effects().iter().chain(next.normal_effects()).cloned());

    if normal {
        result = result.with_stored_values(first.stored_values().iter().chain(next.stored_values().iter()).cloned());
    }

    let reports = [first.completion_reports(), next.completion_reports()];

    if normal && reports.iter().any(Option::is_some) {
        result = result.with_completion_reports(reports.iter().flatten().flat_map(|report| report.iter().cloned()));
    }

    if first.abnormal_reports().is_some() || next.abnormal_reports().is_some() {
        result = result.with_abnormal_reports(
            first
                .abnormal_reports()
                .into_iter()
                .flatten()
                .chain(next.abnormal_reports().into_iter().flatten())
                .chain(
                    reports[0].as_deref()
                        .filter(|_| next.abnormal_reports().is_some())
                        .into_iter()
                        .flatten(),
                )
                .cloned(),
        );
    }

    result
}
