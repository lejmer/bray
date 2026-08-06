use bray_bound_tree::{ConversionTarget, SelectedConversion, SelectedOperation};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{GenericArgument, TypeData, TypeId};

use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

/// Resolves the complete compiler-defined conversion plan when one applies.
pub fn built_in_conversion_plan<C>(
    request: CheckerUnitView<'_, C>,
    source: TypeId,
    target: TypeId,
) -> Result<Option<SelectedConversion>, CheckerInfrastructureError>
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
) -> Result<Option<SelectedConversion>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    if source == target {
        return Ok(Some(SelectedConversion::new(
            source,
            target,
            ConversionTarget::Identity,
        )));
    }

    if scalar_conversion_is_valid(request, source, target)? {
        return Ok(Some(SelectedConversion::new(
            source,
            target,
            ConversionTarget::BuiltInScalar,
        )));
    }

    let Some(children) = composite_conversion_children_for_context(request, source, target)? else {
        return Ok(None);
    };

    let mut plans = Vec::with_capacity(children.len());

    for (source, target) in children {
        let Some(plan) = built_in_conversion_plan_for_context(request, source, target)? else {
            return Ok(None);
        };

        plans.push(plan);
    }

    Ok(Some(SelectedConversion::new(
        source,
        target,
        ConversionTarget::Composite(plans.into()),
    )))
}

pub(super) fn validate_conversion<C>(
    request: CheckerUnitView<'_, C>,
    conversion: &SelectedConversion,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut pending = vec![conversion];

    while let Some(conversion) = pending.pop() {
        let source = conversion.source_type();
        let target = conversion.target_type();

        let is_valid = match conversion.target() {
            ConversionTarget::Identity => source == target,
            ConversionTarget::BuiltInScalar => {
                scalar_conversion_is_valid(request.context(), source, target)?
            }
            ConversionTarget::Composite(children) => {
                let is_valid =
                    composite_conversion_shape_is_valid(request, source, target, children)?;

                if is_valid {
                    pending.extend(children.iter());
                }

                is_valid
            }
            ConversionTarget::Trait { requirement, .. } => {
                source != target
                    && requirement.subject() == source
                    && trait_application_targets(request, requirement.trait_application(), target)?
            }
            ConversionTarget::TraitConstraint { .. } => source != target,
        };

        if !is_valid {
            return Ok(false);
        }
    }

    Ok(true)
}

fn scalar_conversion_is_valid<C>(
    request: &C,
    source: TypeId,
    target: TypeId,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(source) = crate::representation::type_representation_for_context(request, source)?
    else {
        return Ok(false);
    };

    let Some(target) = crate::representation::type_representation_for_context(request, target)?
    else {
        return Ok(false);
    };

    let target_width = request
        .selected_target()
        .machine()
        .pointer_width_bits()
        .get();

    Ok(scalar_shape(source, target_width).is_some_and(|source| {
        scalar_shape(target, target_width).is_some_and(|target| source.can_represent(target))
    }))
}

fn composite_conversion_shape_is_valid<C>(
    request: CheckerUnitView<'_, C>,
    source: TypeId,
    target: TypeId,
    children: &[SelectedConversion],
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(expected) =
        composite_conversion_children_for_context(request.context(), source, target)?
    else {
        return Ok(false);
    };

    if expected.len() != children.len() {
        return Ok(false);
    }

    for ((source, target), child) in expected.into_iter().zip(children) {
        if child.source_type() != source || child.target_type() != target {
            return Ok(false);
        }
    }

    Ok(true)
}

/// Returns the ordered child conversion pairs for a compiler-defined composite conversion.
pub fn composite_conversion_children<C>(
    request: CheckerUnitView<'_, C>,
    source: TypeId,
    target: TypeId,
) -> Result<Option<Vec<(TypeId, TypeId)>>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    composite_conversion_children_for_context(request.context(), source, target)
}

fn composite_conversion_children_for_context<C>(
    request: &C,
    source: TypeId,
    target: TypeId,
) -> Result<Option<Vec<(TypeId, TypeId)>>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let source_data = request
        .semantic_values()
        .type_data(source)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let target_data = request
        .semantic_values()
        .type_data(target)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

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
            let Some(component) = complex_component_type(request, target)? else {
                return Ok(None);
            };

            elements
                .iter()
                .copied()
                .map(|element| (element, component))
                .collect()
        }
        _ => return Ok(None),
    };

    Ok(Some(expected))
}

fn complex_component_type<C>(
    request: &C,
    target: TypeId,
) -> Result<Option<TypeId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(target_role) =
        crate::representation::type_representation_for_context(request, target)?
    else {
        return Ok(None);
    };

    let component = match target_role {
        RepresentationRole::ScalarC32 => RepresentationRole::ScalarR16,
        RepresentationRole::ScalarC64 => RepresentationRole::ScalarR32,
        RepresentationRole::ScalarC128 => RepresentationRole::ScalarR64,
        RepresentationRole::ScalarC256 => RepresentationRole::ScalarR128,
        _ => return Ok(None),
    };

    crate::representation::representation_type_for_context(request, component).map(Some)
}

fn trait_application_targets<C>(
    request: CheckerUnitView<'_, C>,
    application: bray_symbols::TraitApplicationId,
    target: TypeId,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let application = request
        .semantic_values()
        .trait_application_data(application)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let substitution = request
        .semantic_values()
        .generic_substitution_data(application.substitution())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    Ok(matches!(
        substitution.bindings(),
        [binding] if binding.argument() == GenericArgument::Type(target)
    ))
}

#[derive(Clone, Copy)]
enum ScalarShape {
    Signed(u16),
    Unsigned(u16),
    Real(u16),
    Complex(u16),
}

impl ScalarShape {
    const fn can_represent(self, target: Self) -> bool {
        match (self, target) {
            (Self::Signed(source), Self::Signed(target)) => source <= target,
            (Self::Unsigned(source), Self::Unsigned(target)) => source <= target,
            (Self::Unsigned(source), Self::Signed(target)) => source < target,
            (Self::Signed(source), Self::Real(mantissa)) => source - 1 <= mantissa,
            (Self::Unsigned(source), Self::Real(mantissa)) => source <= mantissa,
            (Self::Real(source), Self::Real(target)) => source <= target,
            (Self::Complex(source), Self::Complex(target)) => source <= target,
            _ => false,
        }
    }
}

const fn scalar_shape(role: RepresentationRole, target_width: u16) -> Option<ScalarShape> {
    match role {
        RepresentationRole::ScalarI8 => Some(ScalarShape::Signed(8)),
        RepresentationRole::ScalarI16 => Some(ScalarShape::Signed(16)),
        RepresentationRole::ScalarI32 => Some(ScalarShape::Signed(32)),
        RepresentationRole::ScalarI64 => Some(ScalarShape::Signed(64)),
        RepresentationRole::ScalarI128 => Some(ScalarShape::Signed(128)),
        RepresentationRole::ScalarU8 => Some(ScalarShape::Unsigned(8)),
        RepresentationRole::ScalarU16 => Some(ScalarShape::Unsigned(16)),
        RepresentationRole::ScalarU32 => Some(ScalarShape::Unsigned(32)),
        RepresentationRole::ScalarU64 => Some(ScalarShape::Unsigned(64)),
        RepresentationRole::ScalarU128 => Some(ScalarShape::Unsigned(128)),
        RepresentationRole::ScalarIsize => Some(ScalarShape::Signed(target_width)),
        RepresentationRole::ScalarUsize => Some(ScalarShape::Unsigned(target_width)),
        RepresentationRole::ScalarR16 => Some(ScalarShape::Real(11)),
        RepresentationRole::ScalarR32 => Some(ScalarShape::Real(24)),
        RepresentationRole::ScalarR64 => Some(ScalarShape::Real(53)),
        RepresentationRole::ScalarR128 => Some(ScalarShape::Real(113)),
        RepresentationRole::ScalarC32 => Some(ScalarShape::Complex(11)),
        RepresentationRole::ScalarC64 => Some(ScalarShape::Complex(24)),
        RepresentationRole::ScalarC128 => Some(ScalarShape::Complex(53)),
        RepresentationRole::ScalarC256 => Some(ScalarShape::Complex(113)),
        _ => None,
    }
}

pub(super) const fn is_builtin_conversion(operation: &SelectedOperation) -> bool {
    let SelectedOperation::Conversion(conversion) = operation else {
        return false;
    };

    matches!(
        conversion.target(),
        ConversionTarget::Identity
            | ConversionTarget::BuiltInScalar
            | ConversionTarget::Composite(_)
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundUnitId, ConversionTarget, SelectedConversion};
    use bray_compiler_known::RepresentationRole;

    use crate::CheckerUnitView;
    use crate::test_support::{
        TestCheckerContext, callable_entry, expression_unit, integer_literal_expression,
        push_expression, tuple_type,
    };

    use super::{ScalarShape, scalar_shape, validate_conversion};

    #[test]
    fn target_sized_integers_convert_to_their_fixed_128_bit_domains() {
        let Some(signed) = scalar_shape(RepresentationRole::ScalarIsize, 128) else {
            panic!("target-sized signed integers must have a scalar shape");
        };

        let Some(unsigned) = scalar_shape(RepresentationRole::ScalarUsize, 128) else {
            panic!("target-sized unsigned integers must have a scalar shape");
        };

        assert!(signed.can_represent(ScalarShape::Signed(128)));
        assert!(unsigned.can_represent(ScalarShape::Unsigned(128)));
        assert!(!signed.can_represent(ScalarShape::Signed(64)));
        assert!(!unsigned.can_represent(ScalarShape::Unsigned(64)));
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

        assert_eq!(validate(&unit, &context, &conversion), Ok(true));

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

        assert_eq!(validate(&unit, &context, &conversion), Ok(false));
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
    ) -> Result<bool, crate::CheckerInfrastructureError> {
        let entry = callable_entry(unit.key());

        match CheckerUnitView::new(unit, &entry, context) {
            Ok(request) => validate_conversion(request, conversion),
            Err(error) => panic!("conversion test request must validate: {error:?}"),
        }
    }
}
