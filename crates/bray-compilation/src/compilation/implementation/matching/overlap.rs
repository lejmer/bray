use std::collections::BTreeSet;

use bray_symbols::{
    ConstantTermData, ConstantTermId, GenericArgument, GenericParameterSymbolId,
    SemanticValueStore, SemanticValueStoreError, TraitApplicationId, TypeData, TypeId,
};

use super::super::index::ImplementationHeader;

pub(in crate::compilation::implementation) fn implementation_headers_overlap(
    left: &ImplementationHeader,
    right: &ImplementationHeader,
    values: &SemanticValueStore,
) -> Result<bool, SemanticValueStoreError> {
    let left_parameters = left.parameters().iter().copied().collect();
    let right_parameters = right.parameters().iter().copied().collect();
    let parameters = HeaderParameters {
        left: &left_parameters,
        right: &right_parameters,
    };

    if !types_may_overlap_with(left.subject(), right.subject(), parameters, values)? {
        return Ok(false);
    }

    trait_applications_may_overlap(
        left.trait_application(),
        right.trait_application(),
        parameters,
        values,
    )
}

#[derive(Clone, Copy)]
struct HeaderParameters<'parameters> {
    left: &'parameters BTreeSet<GenericParameterSymbolId>,
    right: &'parameters BTreeSet<GenericParameterSymbolId>,
}

fn types_may_overlap_with(
    left: TypeId,
    right: TypeId,
    parameters: HeaderParameters<'_>,
    values: &SemanticValueStore,
) -> Result<bool, SemanticValueStoreError> {
    if left == right {
        return Ok(true);
    }

    let left_data = values.type_data(left)?;
    let right_data = values.type_data(right)?;

    if is_header_type_parameter(left_data.as_ref(), parameters.left)
        || is_header_type_parameter(right_data.as_ref(), parameters.right)
    {
        return Ok(true);
    }

    match (left_data.as_ref(), right_data.as_ref()) {
        (TypeData::Error, _) | (_, TypeData::Error) => Ok(false),
        (
            TypeData::Named {
                definition: left_definition,
                substitution: left_substitution,
            },
            TypeData::Named {
                definition: right_definition,
                substitution: right_substitution,
            },
        ) => {
            if left_definition != right_definition {
                return Ok(false);
            }

            substitutions_may_overlap(*left_substitution, *right_substitution, parameters, values)
        }
        (
            TypeData::TypeValuedMemberProjection {
                subject: left_subject,
                application: left_application,
                member: left_member,
            },
            TypeData::TypeValuedMemberProjection {
                subject: right_subject,
                application: right_application,
                member: right_member,
            },
        ) => {
            if left_member != right_member
                || !types_may_overlap_with(*left_subject, *right_subject, parameters, values)?
            {
                return Ok(false);
            }

            trait_applications_may_overlap(
                *left_application,
                *right_application,
                parameters,
                values,
            )
        }
        (TypeData::Tuple(left), TypeData::Tuple(right)) => {
            type_lists_may_overlap(left, right, parameters, values)
        }
        (
            TypeData::Array {
                element: left_element,
                length: left_length,
            },
            TypeData::Array {
                element: right_element,
                length: right_length,
            },
        ) => {
            if !types_may_overlap_with(*left_element, *right_element, parameters, values)? {
                return Ok(false);
            }

            constants_may_overlap(*left_length, *right_length, parameters, values)
        }
        (TypeData::Slice(left), TypeData::Slice(right))
        | (TypeData::Generator(left), TypeData::Generator(right))
        | (TypeData::Nullable(left), TypeData::Nullable(right)) => {
            types_may_overlap_with(*left, *right, parameters, values)
        }
        (
            TypeData::Borrow {
                kind: left_kind,
                target: left_target,
            },
            TypeData::Borrow {
                kind: right_kind,
                target: right_target,
            },
        ) => {
            if left_kind != right_kind {
                return Ok(false);
            }

            types_may_overlap_with(*left_target, *right_target, parameters, values)
        }
        (TypeData::TraitView(left), TypeData::TraitView(right)) => {
            trait_applications_may_overlap(*left, *right, parameters, values)
        }
        (
            TypeData::OwnedIndirection {
                storage: left_storage,
                target: left_target,
            },
            TypeData::OwnedIndirection {
                storage: right_storage,
                target: right_target,
            },
        ) => {
            if !types_may_overlap_with(*left_storage, *right_storage, parameters, values)? {
                return Ok(false);
            }

            types_may_overlap_with(*left_target, *right_target, parameters, values)
        }
        (TypeData::Callable(left), TypeData::Callable(right)) => {
            if left.constness() != right.constness()
                || left.execution() != right.execution()
                || left.trust() != right.trust()
                || left.abi() != right.abi()
                || left.dependency_contracts() != right.dependency_contracts()
                || left.parameters().len() != right.parameters().len()
            {
                return Ok(false);
            }

            for (left, right) in left.parameters().iter().zip(right.parameters()) {
                if left.name() != right.name()
                    || left.position() != right.position()
                    || left.mode() != right.mode()
                    || !types_may_overlap_with(left.ty(), right.ty(), parameters, values)?
                {
                    return Ok(false);
                }
            }

            types_may_overlap_with(left.result(), right.result(), parameters, values)
        }
        _ => Ok(false),
    }
}

fn is_header_type_parameter(
    ty: &TypeData,
    parameters: &BTreeSet<GenericParameterSymbolId>,
) -> bool {
    matches!(
        ty,
        TypeData::TypeParameter(parameter)
            if parameters.contains(&GenericParameterSymbolId::Type(*parameter))
    )
}

fn trait_applications_may_overlap(
    left: TraitApplicationId,
    right: TraitApplicationId,
    parameters: HeaderParameters<'_>,
    values: &SemanticValueStore,
) -> Result<bool, SemanticValueStoreError> {
    let left = values.trait_application_data(left)?;
    let right = values.trait_application_data(right)?;

    if left.definition() != right.definition() {
        return Ok(false);
    }

    substitutions_may_overlap(
        left.substitution(),
        right.substitution(),
        parameters,
        values,
    )
}

fn substitutions_may_overlap(
    left: bray_symbols::GenericSubstitutionId,
    right: bray_symbols::GenericSubstitutionId,
    parameters: HeaderParameters<'_>,
    values: &SemanticValueStore,
) -> Result<bool, SemanticValueStoreError> {
    let left = values.generic_substitution_data(left)?;
    let right = values.generic_substitution_data(right)?;

    if left.owner() != right.owner() || left.bindings().len() != right.bindings().len() {
        return Ok(false);
    }

    for (left, right) in left.bindings().iter().zip(right.bindings()) {
        if left.parameter() != right.parameter()
            || !arguments_may_overlap(left.argument(), right.argument(), parameters, values)?
        {
            return Ok(false);
        }
    }

    Ok(true)
}

fn arguments_may_overlap(
    left: GenericArgument,
    right: GenericArgument,
    parameters: HeaderParameters<'_>,
    values: &SemanticValueStore,
) -> Result<bool, SemanticValueStoreError> {
    match (left, right) {
        (GenericArgument::Type(left), GenericArgument::Type(right)) => {
            types_may_overlap_with(left, right, parameters, values)
        }
        (GenericArgument::Constant(left), GenericArgument::Constant(right)) => {
            constants_may_overlap(left, right, parameters, values)
        }
        _ => Ok(false),
    }
}

fn type_lists_may_overlap(
    left: &[TypeId],
    right: &[TypeId],
    parameters: HeaderParameters<'_>,
    values: &SemanticValueStore,
) -> Result<bool, SemanticValueStoreError> {
    if left.len() != right.len() {
        return Ok(false);
    }

    for (left, right) in left.iter().zip(right) {
        if !types_may_overlap_with(*left, *right, parameters, values)? {
            return Ok(false);
        }
    }

    Ok(true)
}

fn constants_may_overlap(
    left: ConstantTermId,
    right: ConstantTermId,
    parameters: HeaderParameters<'_>,
    values: &SemanticValueStore,
) -> Result<bool, SemanticValueStoreError> {
    if left == right {
        return Ok(true);
    }

    let left = values.constant_term_data(left)?;
    let right = values.constant_term_data(right)?;

    if is_header_const_parameter(left.as_ref(), parameters.left)
        || is_header_const_parameter(right.as_ref(), parameters.right)
    {
        return Ok(true);
    }

    match (left.as_ref(), right.as_ref()) {
        (ConstantTermData::Value(left), ConstantTermData::Value(right)) => Ok(left == right),
        (
            ConstantTermData::IntegerLiteral {
                ty: left_type,
                value: left_value,
            },
            ConstantTermData::IntegerLiteral {
                ty: right_type,
                value: right_value,
            },
        ) => Ok(left_type == right_type && left_value == right_value),
        (ConstantTermData::Parameter(left), ConstantTermData::Parameter(right)) => {
            Ok(left == right)
        }
        _ => Ok(true),
    }
}

fn is_header_const_parameter(
    term: &ConstantTermData,
    parameters: &BTreeSet<GenericParameterSymbolId>,
) -> bool {
    matches!(
        term,
        ConstantTermData::Parameter(parameter)
            if parameters.contains(&GenericParameterSymbolId::Const(*parameter))
    )
}
