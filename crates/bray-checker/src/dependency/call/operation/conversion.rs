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
                    .try_map_requirements(|requirements| {
                        crate::dependency::witness::map_requirements(
                            requirements,
                            &mut |subject, kind| {
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

                let effects = child
                    .effects()
                    .iter()
                    .map(|effect| project_effect(effect, ordinal, shape.as_ref()))
                    .collect::<Vec<_>>();

                let child = child.with_effects(effects);

                let child = if matches!(shape.as_ref(), TypeData::Nullable(_)) {
                    child
                        .try_map_requirements(|requirements| {
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
            )]).with_effects([]);
            if matches!(conversion.target(), ConversionTarget::Identity
                | ConversionTarget::CallableContract | ConversionTarget::NullablePresent)
            {
                let values = request.semantic_values();
                let input = values.intern_constant_term(bray_symbols::ConstantTermData::CallableArgument(SymbolOrdinal::new(0)))
                    .expect("canonical conversion input belongs to the checked semantic store");
                let input = if matches!(conversion.target(), ConversionTarget::NullablePresent) {
                    values.intern_constant_term(bray_symbols::ConstantTermData::NullablePresent(input))
                        .expect("canonical nullable conversion belongs to the checked semantic store")
                } else {
                    input
                };
                result = result.with_returned_input(Some(input));
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
        DependencyStorageEffect::Direct { subject, kind } => {
            project_subject(subject, ordinal, shape)
                .map(|subject| DependencyStorageEffect::Direct {
                    subject,
                    kind: *kind,
                })
                .unwrap_or_else(|| DependencyStorageEffect::unresolved(*kind))
        }
        DependencyStorageEffect::Indirect {
            reference,
            projections,
            kind,
        } => project_subject(reference, ordinal, shape)
            .map(|reference| DependencyStorageEffect::Indirect {
                reference,
                projections: std::sync::Arc::clone(projections),
                kind: *kind,
            })
            // A uniform array summary cannot select one runtime element. Use the
            // existing unresolved-alias meaning, never a path in the element type.
            .unwrap_or_else(|| DependencyStorageEffect::unresolved(*kind)),
        DependencyStorageEffect::Atomic
        | DependencyStorageEffect::Escape
        | DependencyStorageEffect::Unknown
        | DependencyStorageEffect::Cleanup(_) => effect.clone(),
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

    let mut result = DependencyContractTemplateData::new(requirements)
        .with_effects(first.effects().iter().chain(next.effects()).cloned())
        .with_normal_completion(first.normal_completion() && next.normal_completion())
        .with_normal_storage_preservation(
            first.preserves_storage_on_normal_return() && next.preserves_storage_on_normal_return(),
        );

    if first.completion_reports().is_some() || next.completion_reports().is_some() {
        result = result.with_completion_reports(
            first
                .completion_reports()
                .into_iter()
                .flatten()
                .chain(next.completion_reports().into_iter().flatten())
                .cloned(),
        );
    }

    if first.abnormal_reports().is_some() || next.abnormal_reports().is_some() {
        result = result.with_abnormal_reports(
            first
                .abnormal_reports()
                .into_iter()
                .flatten()
                .chain(next.abnormal_reports().into_iter().flatten())
                .chain(
                    first
                        .completion_reports()
                        .into_iter()
                        .flatten()
                        .filter(|_| next.abnormal_reports().is_some()),
                )
                .cloned(),
        );
    }

    result
}
