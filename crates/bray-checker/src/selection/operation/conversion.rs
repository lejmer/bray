use bray_bound_tree::{ConversionTarget, SelectedConversion, SelectedOperation};
use bray_compiler_known::{RepresentationRole, ScalarShape};
use bray_symbols::{GenericArgument, TypeData, TypeId};

use crate::{CheckerRequestContext, CheckerUnitView};

pub(in crate::selection) fn implicit_input_conversion<C: CheckerRequestContext + ?Sized>(
    request: CheckerUnitView<'_, C>,
    expression: bray_bound_tree::BoundExpressionId,
    source: TypeId,
    target: TypeId,
) -> Option<SelectedConversion> {
    let kind = if source == target {
        ConversionTarget::Identity
    } else if matches!(request.semantic_values().type_data(target).as_ref(),
        TypeData::Nullable(contained) if *contained == source)
    {
        ConversionTarget::NullablePresent
    } else if crate::type_check::numeric_literal_accepts_type(request, expression, target) {
        ConversionTarget::BuiltInScalar
    } else {
        return None;
    };

    Some(SelectedConversion::new(source, target, kind))
}

/// Resolves the complete compiler-defined conversion plan when one applies.
pub fn built_in_conversion_plan<C>(
    request: CheckerUnitView<'_, C>,
    source: TypeId,
    target: TypeId,
) -> Option<SelectedConversion>
where
    C: CheckerRequestContext + ?Sized,
{
    built_in_conversion_plan_for_context(request.context(), source, target)
}

/// Resolves a compiler-defined conversion without requiring a bound unit.
pub fn built_in_conversion_plan_for_context<C>(
    request: &C,
    source: TypeId,
    target: TypeId,
) -> Option<SelectedConversion>
where
    C: CheckerRequestContext + ?Sized,
{
    if source == target {
        return Some(SelectedConversion::new(
            source,
            target,
            ConversionTarget::Identity,
        ));
    }

    if callable_contract_conversion_is_valid(request.semantic_values(), source, target) {
        return Some(SelectedConversion::new(
            source,
            target,
            ConversionTarget::CallableContract,
        ));
    }

    if scalar_conversion_is_valid(request, source, target) {
        return Some(SelectedConversion::new(
            source,
            target,
            ConversionTarget::BuiltInScalar,
        ));
    }

    let children = composite_conversion_children_for_context(request, source, target)?;

    let mut plans = Vec::with_capacity(children.len());

    for (source, target) in children {
        let plan = built_in_conversion_plan_for_context(request, source, target)?;

        plans.push(plan);
    }

    Some(SelectedConversion::new(
        source,
        target,
        ConversionTarget::Composite(plans.into()),
    ))
}

pub(super) fn validate_conversion<C>(
    request: CheckerUnitView<'_, C>,
    conversion: &SelectedConversion,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    let mut pending = vec![conversion];

    while let Some(conversion) = pending.pop() {
        let source = conversion.source_type();
        let target = conversion.target_type();

        let is_valid = match conversion.target() {
            ConversionTarget::Identity => source == target,
            ConversionTarget::CallableContract => {
                callable_contract_conversion_is_valid(request.semantic_values(), source, target)
            }
            ConversionTarget::NullablePresent => {
                let target = request.semantic_values().type_data(target);

                matches!(target.as_ref(), TypeData::Nullable(contained) if *contained == source)
            }
            ConversionTarget::BuiltInScalar => {
                scalar_conversion_is_valid(request.context(), source, target)
            }
            ConversionTarget::CVariadicPromotion => {
                c_variadic_promotion_target(request, source) == Some(target)
            }
            ConversionTarget::Composite(children) => {
                let is_valid =
                    composite_conversion_shape_is_valid(request, source, target, children);

                if is_valid {
                    pending.extend(children.iter());
                }

                is_valid
            }
            ConversionTarget::Trait { requirement, .. } => {
                source != target
                    && requirement.subject() == source
                    && trait_application_targets(request, requirement.trait_application(), target)
            }
            ConversionTarget::TraitConstraint { .. } => source != target,
        };

        if !is_valid {
            return false;
        }
    }

    true
}

/// Checks whether a callable conversion preserves the signature and cannot invent a promise.
/// The supplied callable's implementation must independently pass execution certification.
pub fn callable_contract_conversion_is_valid(
    values: &bray_symbols::SemanticValueStore,
    source: TypeId,
    target: TypeId,
) -> bool {
    let source = values.type_data(source);

    let target = values.type_data(target);

    let (TypeData::Callable(source), TypeData::Callable(target)) =
        (source.as_ref(), target.as_ref())
    else {
        return false;
    };

    source.parameters() == target.parameters()
        && source.is_variadic() == target.is_variadic()
        && source.result() == target.result()
        && source.constness() == target.constness()
        && source.trust() == target.trust()
        && source.abi() == target.abi()
        && phase_contract_is_compatible(
            source.phase_behaviors().invocation(),
            target.phase_behaviors().invocation(),
        )
        && match (
            source.phase_behaviors().deferred_execution(),
            target.phase_behaviors().deferred_execution(),
        ) {
            (Some(source), Some(target)) => phase_contract_is_compatible(source, target),
            (None, None) => true,
            _ => false,
        }
}

fn phase_contract_is_compatible(
    source: &bray_symbols::CallablePhaseBehavior,
    target: &bray_symbols::CallablePhaseBehavior,
) -> bool {
    // Implementation authority is non-transitive. Callable conversion compares caller
    // obligations and promises independently of the authority used inside either body.
    source
        .clone()
        .with_may_unwind(true)
        .with_execution_properties([])
        .with_predicates([], [])
        .with_trusted_capabilities([])
        == target
            .clone()
            .with_may_unwind(true)
            .with_execution_properties([])
            .with_predicates([], [])
            .with_trusted_capabilities([])
        && (target.may_unwind() || !source.may_unwind())
        && source.predicate_requirements().iter().all(|condition| {
            target.predicate_requirements().iter().any(|candidate| {
                candidate.condition() == condition.condition()
                    && candidate.domain() == condition.domain()
            })
        })
        && target.predicate_guarantees().iter().all(|condition| {
            source.predicate_guarantees().iter().any(|candidate| {
                candidate.condition() == condition.condition()
                    && candidate.domain() == condition.domain()
            })
        })
        && target
            .execution_properties()
            .iter()
            .all(|property| source.execution_properties().contains(property))
}

fn scalar_conversion_is_valid<C>(request: &C, source: TypeId, target: TypeId) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(source) = crate::representation::type_representation_for_context(request, source)
    else {
        return false;
    };

    let Some(target) = crate::representation::type_representation_for_context(request, target)
    else {
        return false;
    };

    let target_width = request
        .selected_target()
        .machine()
        .pointer_width_bits()
        .get();

    ScalarShape::for_role(source, target_width).is_some_and(|source| {
        ScalarShape::for_role(target, target_width)
            .is_some_and(|target| source.converts_losslessly_to(target))
    })
}

pub(crate) fn c_variadic_promotion_target<C>(
    request: CheckerUnitView<'_, C>,
    source: TypeId,
) -> Option<TypeId>
where
    C: CheckerRequestContext + ?Sized,
{
    let target = match crate::representation::type_representation(request, source) {
        Some(
            RepresentationRole::ScalarBool
            | RepresentationRole::ScalarI8
            | RepresentationRole::ScalarI16
            | RepresentationRole::ScalarU8
            | RepresentationRole::ScalarU16,
        ) => RepresentationRole::ScalarI32,
        Some(RepresentationRole::ScalarR32) => RepresentationRole::ScalarR64,
        _ => return None,
    };

    Some(crate::representation::representation_type(request, target))
}

fn composite_conversion_shape_is_valid<C>(
    request: CheckerUnitView<'_, C>,
    source: TypeId,
    target: TypeId,
    children: &[SelectedConversion],
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expected) =
        composite_conversion_children_for_context(request.context(), source, target)
    else {
        return false;
    };

    if expected.len() != children.len() {
        return false;
    }

    for ((source, target), child) in expected.into_iter().zip(children) {
        if child.source_type() != source || child.target_type() != target {
            return false;
        }
    }

    true
}

/// Returns the ordered child conversion pairs for a compiler-defined composite conversion.
pub fn composite_conversion_children<C>(
    request: CheckerUnitView<'_, C>,
    source: TypeId,
    target: TypeId,
) -> Option<Vec<(TypeId, TypeId)>>
where
    C: CheckerRequestContext + ?Sized,
{
    composite_conversion_children_for_context(request.context(), source, target)
}

fn composite_conversion_children_for_context<C>(
    request: &C,
    source: TypeId,
    target: TypeId,
) -> Option<Vec<(TypeId, TypeId)>>
where
    C: CheckerRequestContext + ?Sized,
{
    let source_data = request.semantic_values().type_data(source);

    let target_data = request.semantic_values().type_data(target);

    let expected = match (source_data.as_ref(), target_data.as_ref()) {
        (TypeData::Tuple(source), TypeData::Tuple(target)) if source.len() == target.len() => {
            source
                .iter()
                .copied()
                .zip(target.iter().copied())
                .collect::<Vec<_>>()
        }
        (
            TypeData::Array {
                element: source_element,
                length: source_length,
            },
            TypeData::Array {
                element: target_element,
                length: target_length,
            },
        ) if source_length == target_length => vec![(*source_element, *target_element)],
        (TypeData::Nullable(source), TypeData::Nullable(target)) => vec![(*source, *target)],
        (TypeData::Tuple(elements), _) if elements.len() == 2 => {
            let component = complex_component_type(request, target)?;

            elements
                .iter()
                .copied()
                .map(|element| (element, component))
                .collect()
        }
        _ => return None,
    };

    Some(expected)
}

fn complex_component_type<C>(request: &C, target: TypeId) -> Option<TypeId>
where
    C: CheckerRequestContext + ?Sized,
{
    let target_role = crate::representation::type_representation_for_context(request, target)?;

    let component = match target_role {
        RepresentationRole::ScalarC32 => RepresentationRole::ScalarR16,
        RepresentationRole::ScalarC64 => RepresentationRole::ScalarR32,
        RepresentationRole::ScalarC128 => RepresentationRole::ScalarR64,
        RepresentationRole::ScalarC256 => RepresentationRole::ScalarR128,
        _ => return None,
    };

    Some(crate::representation::representation_type_for_context(
        request, component,
    ))
}

fn trait_application_targets<C>(
    request: CheckerUnitView<'_, C>,
    application: bray_symbols::TraitApplicationId,
    target: TypeId,
) -> bool
where
    C: CheckerRequestContext + ?Sized,
{
    let application = request
        .semantic_values()
        .trait_application_data(application);

    let substitution = request
        .semantic_values()
        .generic_substitution_data(application.substitution());

    matches!(
        substitution.bindings(),
        [binding] if binding.argument() == GenericArgument::Type(target)
    )
}

pub(super) const fn is_builtin_conversion(operation: &SelectedOperation) -> bool {
    let SelectedOperation::Conversion(conversion) = operation else {
        return false;
    };

    matches!(
        conversion.target(),
        ConversionTarget::Identity
            | ConversionTarget::CallableContract
            | ConversionTarget::NullablePresent
            | ConversionTarget::BuiltInScalar
            | ConversionTarget::CVariadicPromotion
            | ConversionTarget::Composite(_)
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundUnitId, ConversionTarget, SelectedConversion};
    use bray_compiler_known::{RepresentationRole, ScalarShape};

    use crate::CheckerUnitView;
    use crate::test_support::{
        TestCheckerContext, callable_entry, expression_unit, integer_literal_expression,
        push_expression, tuple_type,
    };

    use super::{c_variadic_promotion_target, validate_conversion};

    #[test]
    fn callable_contracts_preserve_all_three_predicate_domains() {
        use bray_symbols::{CallablePhaseBehavior, PredicateDomain, PredicateSemanticSummary};

        let values = crate::test_support::semantic_values();
        let dependency = values.empty_dependency_contract_template().expect("fixture `empty_dependency_contract_template` call must succeed");

        for source in [
            PredicateDomain::Value,
            PredicateDomain::Trusted,
            PredicateDomain::Execution,
        ] {
            for target in [
                PredicateDomain::Value,
                PredicateDomain::Trusted,
                PredicateDomain::Execution,
            ] {
                for guarantee in [false, true] {
                    let phase = |domain| {
                        let condition =
                            PredicateSemanticSummary::new(dependency).with_condition(None, domain);

                        let (requirements, guarantees) = if guarantee {
                            (Vec::new(), vec![condition])
                        } else {
                            (vec![condition], Vec::new())
                        };

                        CallablePhaseBehavior::empty(dependency)
                            .with_predicates(requirements, guarantees)
                    };

                    assert_eq!(
                        super::phase_contract_is_compatible(&phase(source), &phase(target)),
                        source == target
                    );
                }
            }
        }
    }

    #[test]
    fn c_variadic_arguments_use_the_default_scalar_promotions() {
        let placeholder = tuple_type([]);
        let unit = test_unit(BoundUnitId::new(87), placeholder);
        let context = TestCheckerContext::new(false);
        let entry = callable_entry(unit.key());

        let request = CheckerUnitView::new(&unit, &entry, &context);

        for (source, target) in [
            (
                RepresentationRole::ScalarBool,
                RepresentationRole::ScalarI32,
            ),
            (RepresentationRole::ScalarI8, RepresentationRole::ScalarI32),
            (RepresentationRole::ScalarI16, RepresentationRole::ScalarI32),
            (RepresentationRole::ScalarU8, RepresentationRole::ScalarI32),
            (RepresentationRole::ScalarU16, RepresentationRole::ScalarI32),
            (RepresentationRole::ScalarR32, RepresentationRole::ScalarR64),
        ] {
            let source = crate::representation::representation_type(request, source);

            let target = crate::representation::representation_type(request, target);

            assert_eq!(c_variadic_promotion_target(request, source), Some(target));
        }

        for role in [
            RepresentationRole::ScalarI32,
            RepresentationRole::ScalarU32,
            RepresentationRole::ScalarR64,
        ] {
            let ty = crate::representation::representation_type(request, role);

            assert_eq!(c_variadic_promotion_target(request, ty), None);
        }
    }

    #[test]
    fn target_sized_integers_convert_to_their_fixed_128_bit_domains() {
        let Some(signed) = ScalarShape::for_role(RepresentationRole::ScalarIsize, 128) else {
            panic!("target-sized signed integers must have a scalar shape");
        };

        let Some(unsigned) = ScalarShape::for_role(RepresentationRole::ScalarUsize, 128) else {
            panic!("target-sized unsigned integers must have a scalar shape");
        };

        assert!(signed.converts_losslessly_to(ScalarShape::Signed(128)));
        assert!(unsigned.converts_losslessly_to(ScalarShape::Unsigned(128)));
        assert!(!signed.converts_losslessly_to(ScalarShape::Signed(64)));
        assert!(!unsigned.converts_losslessly_to(ScalarShape::Unsigned(64)));
    }

    #[test]
    fn composite_conversions_validate_and_retain_nested_conversion_plans() {
        let element = tuple_type([]);
        let source = tuple_type([element]);
        let target = tuple_type([element]);

        let child = SelectedConversion::new(element, element, ConversionTarget::Identity);

        let conversion = SelectedConversion::new(
            source,
            target,
            ConversionTarget::Composite([child.clone()].into()),
        );

        let unit = test_unit(BoundUnitId::new(85), source);
        let context = TestCheckerContext::new(false);

        assert!(validate(&unit, &context, &conversion));

        assert_eq!(
            conversion.target(),
            &ConversionTarget::Composite([child].into())
        );
    }

    #[test]
    fn composite_conversions_reject_incomplete_nested_plans() {
        let element = tuple_type([]);
        let source = tuple_type([element]);
        let target = tuple_type([element]);

        let conversion =
            SelectedConversion::new(source, target, ConversionTarget::Composite([].into()));

        let unit = test_unit(BoundUnitId::new(86), source);
        let context = TestCheckerContext::new(false);

        assert!(!validate(&unit, &context, &conversion));
    }

    fn test_unit(unit: BoundUnitId, ty: bray_symbols::TypeId) -> bray_bound_tree::BoundUnit {
        expression_unit(unit, |tree, origin| {
            let expression = push_expression(tree, integer_literal_expression(origin, Some(ty)));

            vec![expression]
        })
        .0
    }

    fn validate(
        unit: &bray_bound_tree::BoundUnit,
        context: &TestCheckerContext,
        conversion: &SelectedConversion,
    ) -> bool {
        let entry = callable_entry(unit.key());

        let request = CheckerUnitView::new(unit, &entry, context);

        validate_conversion(request, conversion)
    }
}
