use bray_bound_tree::{
    ConversionTarget, ScalarConversionKind, SelectedConversion, SelectedOperation,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{GenericArgument, TypeData, TypeId};

use crate::representation::type_representation;
use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

pub(super) fn validate_conversion<C>(
    request: UnitCheckRequest<'_, C>,
    conversion: &SelectedConversion,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    for conversion in conversion.walk() {
        let source = conversion.source_type();
        let target = conversion.target_type();

        let is_valid = match conversion.target() {
            ConversionTarget::Identity => source == target,
            ConversionTarget::BuiltInScalar(kind) => {
                scalar_conversion_kind(request, source, target)?.as_ref() == Some(kind)
            }
            ConversionTarget::Tuple(children) => {
                tuple_conversion_is_valid(request, source, target, children)?
            }
            ConversionTarget::Array(child) => {
                array_conversion_is_valid(request, source, target, child)?
            }
            ConversionTarget::Nullable(child) => {
                nullable_conversion_is_valid(request, source, target, child)?
            }
            ConversionTarget::TupleToComplex { real, imaginary } => {
                tuple_to_complex_is_valid(request, source, target, real, imaginary)?
            }
            ConversionTarget::Trait { requirement, .. } => {
                source != target
                    && requirement.subject() == source
                    && trait_application_targets(request, requirement.trait_application(), target)?
            }
        };

        if !is_valid {
            return Ok(false);
        }
    }

    Ok(true)
}

fn scalar_conversion_kind<C>(
    request: UnitCheckRequest<'_, C>,
    source: TypeId,
    target: TypeId,
) -> Result<Option<ScalarConversionKind>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(source) = type_representation(request, source)? else {
        return Ok(None);
    };

    let Some(target) = type_representation(request, target)? else {
        return Ok(None);
    };

    let kind = match (scalar_shape(source), scalar_shape(target)) {
        (Some(ScalarShape::Signed(source)), Some(ScalarShape::Signed(target)))
            if source < target =>
        {
            Some(ScalarConversionKind::SignedIntegerWidening)
        }
        (Some(ScalarShape::Unsigned(source)), Some(ScalarShape::Unsigned(target)))
            if source < target =>
        {
            Some(ScalarConversionKind::UnsignedIntegerWidening)
        }
        (Some(ScalarShape::Unsigned(source)), Some(ScalarShape::Signed(target)))
            if source < target =>
        {
            Some(ScalarConversionKind::UnsignedToSigned)
        }
        (Some(ScalarShape::Signed(source)), Some(ScalarShape::Real(mantissa)))
            if source - 1 <= mantissa =>
        {
            Some(ScalarConversionKind::IntegerToReal)
        }
        (Some(ScalarShape::Unsigned(source)), Some(ScalarShape::Real(mantissa)))
            if source <= mantissa =>
        {
            Some(ScalarConversionKind::IntegerToReal)
        }
        (Some(ScalarShape::Real(source)), Some(ScalarShape::Real(target))) if source < target => {
            Some(ScalarConversionKind::RealWidening)
        }
        (Some(ScalarShape::Complex(source)), Some(ScalarShape::Complex(target)))
            if source < target =>
        {
            Some(ScalarConversionKind::ComplexWidening)
        }
        _ => None,
    };

    Ok(kind)
}

fn tuple_conversion_is_valid<C>(
    request: UnitCheckRequest<'_, C>,
    source: TypeId,
    target: TypeId,
    children: &[SelectedConversion],
) -> Result<bool, CheckerInfrastructureError>
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

    let (source, target) = match (source_data.as_ref(), target_data.as_ref()) {
        (TypeData::Tuple(source), TypeData::Tuple(target)) if source.len() == target.len() => {
            (source.as_ref(), target.as_ref())
        }
        _ => return Ok(false),
    };

    if source.len() != children.len() {
        return Ok(false);
    }

    for ((source, target), child) in source.iter().zip(target.iter()).zip(children) {
        if child.source_type() != *source || child.target_type() != *target {
            return Ok(false);
        }
    }

    Ok(true)
}

fn array_conversion_is_valid<C>(
    request: UnitCheckRequest<'_, C>,
    source: TypeId,
    target: TypeId,
    child: &SelectedConversion,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let source = type_data(request, source)?;
    let target = type_data(request, target)?;

    let (
        TypeData::Array {
            element: source_element,
            length: source_length,
        },
        TypeData::Array {
            element: target_element,
            length: target_length,
        },
    ) = (source.as_ref(), target.as_ref())
    else {
        return Ok(false);
    };

    Ok(source_length == target_length
        && child.source_type() == *source_element
        && child.target_type() == *target_element)
}

fn nullable_conversion_is_valid<C>(
    request: UnitCheckRequest<'_, C>,
    source: TypeId,
    target: TypeId,
    child: &SelectedConversion,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let source = type_data(request, source)?;
    let target = type_data(request, target)?;

    let (TypeData::Nullable(source), TypeData::Nullable(target)) =
        (source.as_ref(), target.as_ref())
    else {
        return Ok(false);
    };

    Ok(child.source_type() == *source && child.target_type() == *target)
}

fn tuple_to_complex_is_valid<C>(
    request: UnitCheckRequest<'_, C>,
    source: TypeId,
    target: TypeId,
    real: &SelectedConversion,
    imaginary: &SelectedConversion,
) -> Result<bool, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let source_data = type_data(request, source)?;
    let TypeData::Tuple(elements) = source_data.as_ref() else {
        return Ok(false);
    };

    let [source_real, source_imaginary] = elements.as_ref() else {
        return Ok(false);
    };

    let Some(component) = complex_component_type(request, target)? else {
        return Ok(false);
    };

    Ok(real.source_type() == *source_real
        && real.target_type() == component
        && imaginary.source_type() == *source_imaginary
        && imaginary.target_type() == component)
}

fn type_data<C>(
    request: UnitCheckRequest<'_, C>,
    ty: TypeId,
) -> Result<std::sync::Arc<TypeData>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    request
        .semantic_values()
        .type_data(ty)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}

fn complex_component_type<C>(
    request: UnitCheckRequest<'_, C>,
    target: TypeId,
) -> Result<Option<TypeId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(target_role) = type_representation(request, target)? else {
        return Ok(None);
    };

    let component = match target_role {
        RepresentationRole::ScalarC32 => RepresentationRole::ScalarR16,
        RepresentationRole::ScalarC64 => RepresentationRole::ScalarR32,
        RepresentationRole::ScalarC128 => RepresentationRole::ScalarR64,
        RepresentationRole::ScalarC256 => RepresentationRole::ScalarR128,
        _ => return Ok(None),
    };

    crate::representation::representation_type(request, component).map(Some)
}

fn trait_application_targets<C>(
    request: UnitCheckRequest<'_, C>,
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
    TargetSigned,
    TargetUnsigned,
}

const fn scalar_shape(role: RepresentationRole) -> Option<ScalarShape> {
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
        RepresentationRole::ScalarIsize => Some(ScalarShape::TargetSigned),
        RepresentationRole::ScalarUsize => Some(ScalarShape::TargetUnsigned),
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
            | ConversionTarget::BuiltInScalar(_)
            | ConversionTarget::Tuple(_)
            | ConversionTarget::Array(_)
            | ConversionTarget::Nullable(_)
            | ConversionTarget::TupleToComplex { .. }
    )
}

#[cfg(test)]
mod tests {
    use bray_bound_tree::{BoundUnitId, ConversionTarget, SelectedConversion};

    use crate::UnitCheckRequest;
    use crate::test_support::{
        TestCheckerContext, callable_entry, expression_unit, integer_literal_expression,
        push_expression, tuple_type,
    };

    use super::validate_conversion;

    #[test]
    fn composite_conversions_validate_and_retain_nested_conversion_plans() {
        let element = tuple_type([]);
        let source = tuple_type([element]);
        let target = tuple_type([element]);

        let child = SelectedConversion::new(element, element, ConversionTarget::Identity);

        let conversion = SelectedConversion::new(
            source,
            target,
            ConversionTarget::Tuple([child.clone()].into()),
        );

        let unit = test_unit(BoundUnitId::new(85), source);
        let context = TestCheckerContext::new(false);

        assert_eq!(validate(&unit, &context, &conversion), Ok(true));

        assert_eq!(
            conversion.target(),
            &ConversionTarget::Tuple([child].into())
        );
    }

    #[test]
    fn composite_conversions_reject_incomplete_nested_plans() {
        let element = tuple_type([]);
        let source = tuple_type([element]);
        let target = tuple_type([element]);

        let conversion =
            SelectedConversion::new(source, target, ConversionTarget::Tuple([].into()));

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

        match UnitCheckRequest::new(unit, &entry, context) {
            Ok(request) => validate_conversion(request, conversion),
            Err(error) => panic!("conversion test request must validate: {error:?}"),
        }
    }
}
