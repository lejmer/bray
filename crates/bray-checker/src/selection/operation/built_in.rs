use bray_bound_tree::BoundOperator;
use bray_compiler_known::{
    CompilerKnownOperationRole, NumericRepresentationKind, RepresentationRole,
};
use bray_symbols::{
    GenericArgument, ProofOutcome, TraitApplicationId, TraitTypeMemberSymbolId, TypeId,
};

use crate::representation::type_representation_for_context;
use crate::{CheckerInfrastructureError, CheckerRequestContext};

use super::conversion::built_in_conversion_plan_for_context;

/// Returns whether one trait application is satisfied by a compiler-defined operation.
pub fn built_in_trait_constraint_outcome<C>(
    request: &C,
    subject: TypeId,
    application: TraitApplicationId,
) -> Result<Option<ProofOutcome>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let application = request
        .semantic_values()
        .trait_application_data(application)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let Some(contract) = CompilerKnownOperationRole::ALL
        .iter()
        .filter_map(|role| {
            request
                .available_compiler_known_symbols()
                .operation_contract(*role)
        })
        .find(|contract| contract.trait_definition() == application.definition())
    else {
        return Ok(None);
    };

    let substitution = request
        .semantic_values()
        .generic_substitution_data(application.substitution())
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    if contract.role() == CompilerKnownOperationRole::PlainConversion {
        let [binding] = substitution.bindings() else {
            return Err(CheckerInfrastructureError::SemanticValueUnavailable);
        };

        let GenericArgument::Type(target) = binding.argument() else {
            return Err(CheckerInfrastructureError::SemanticValueUnavailable);
        };

        let conversion = built_in_conversion_plan_for_context(request, subject, target)?;

        return Ok(Some(if conversion.is_some() {
            ProofOutcome::Proven
        } else {
            ProofOutcome::Disproven
        }));
    }

    let role = type_representation_for_context(request, subject)?;

    let operands_match = match substitution.bindings() {
        [] => true,
        [binding] => {
            matches!(binding.argument(), GenericArgument::Type(operand) if operand == subject)
        }
        _ => false,
    };

    Ok(Some(
        if operands_match
            && role.is_some_and(|role| representation_supports_operation(role, contract.role()))
        {
            ProofOutcome::Proven
        } else {
            ProofOutcome::Disproven
        },
    ))
}

/// Resolves a compiler-defined type-valued operation result when one applies.
pub fn built_in_operation_result_type<C>(
    request: &C,
    subject: TypeId,
    application: TraitApplicationId,
    member: TraitTypeMemberSymbolId,
) -> Result<Option<TypeId>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let application_data = request
        .semantic_values()
        .trait_application_data(application)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let Some(contract) = CompilerKnownOperationRole::ALL
        .iter()
        .filter_map(|role| {
            request
                .available_compiler_known_symbols()
                .operation_contract(*role)
        })
        .find(|contract| contract.trait_definition() == application_data.definition())
    else {
        return Ok(None);
    };

    if contract.result_type_member() != Some(member)
        || built_in_trait_constraint_outcome(request, subject, application)?
            != Some(ProofOutcome::Proven)
    {
        return Ok(None);
    }

    Ok(Some(subject))
}

pub(crate) const fn representation_supports_operator(
    role: RepresentationRole,
    operator: BoundOperator,
) -> bool {
    match operator {
        BoundOperator::LogicalNot | BoundOperator::LogicalAnd | BoundOperator::LogicalOr => {
            matches!(role, RepresentationRole::ScalarBool)
        }
        BoundOperator::Assign | BoundOperator::MatrixMultiply => false,
        BoundOperator::Add => {
            representation_supports_operation(role, CompilerKnownOperationRole::BinaryAdd)
        }
        BoundOperator::Subtract => {
            representation_supports_operation(role, CompilerKnownOperationRole::BinarySubtract)
        }
        BoundOperator::Multiply => {
            representation_supports_operation(role, CompilerKnownOperationRole::BinaryMultiply)
        }
        BoundOperator::Divide => {
            representation_supports_operation(role, CompilerKnownOperationRole::BinaryDivide)
        }
        BoundOperator::Remainder => {
            representation_supports_operation(role, CompilerKnownOperationRole::BinaryRemainder)
        }
        BoundOperator::Exponentiate => {
            representation_supports_operation(role, CompilerKnownOperationRole::BinaryExponentiate)
        }
        BoundOperator::BitwiseNot => {
            representation_supports_operation(role, CompilerKnownOperationRole::UnaryBitNot)
        }
        BoundOperator::BitwiseAnd => {
            representation_supports_operation(role, CompilerKnownOperationRole::BinaryBitAnd)
        }
        BoundOperator::BitwiseOr => {
            representation_supports_operation(role, CompilerKnownOperationRole::BinaryBitOr)
        }
        BoundOperator::BitwiseXor => {
            representation_supports_operation(role, CompilerKnownOperationRole::BinaryBitXor)
        }
        BoundOperator::ShiftLeft => {
            representation_supports_operation(role, CompilerKnownOperationRole::BinaryShiftLeft)
        }
        BoundOperator::ShiftRight => {
            representation_supports_operation(role, CompilerKnownOperationRole::BinaryShiftRight)
        }
        BoundOperator::Equal | BoundOperator::NotEqual => {
            representation_supports_operation(role, CompilerKnownOperationRole::Equality)
        }
        BoundOperator::Less
        | BoundOperator::LessEqual
        | BoundOperator::Greater
        | BoundOperator::GreaterEqual => {
            representation_supports_operation(role, CompilerKnownOperationRole::Comparison)
        }
    }
}

const fn representation_supports_operation(
    role: RepresentationRole,
    operation: CompilerKnownOperationRole,
) -> bool {
    match operation {
        CompilerKnownOperationRole::UnaryNegate
        | CompilerKnownOperationRole::BinaryAdd
        | CompilerKnownOperationRole::BinarySubtract
        | CompilerKnownOperationRole::BinaryMultiply
        | CompilerKnownOperationRole::BinaryDivide
        | CompilerKnownOperationRole::BinaryExponentiate => role.numeric_kind().is_some(),
        CompilerKnownOperationRole::BinaryRemainder => matches!(
            role.numeric_kind(),
            Some(NumericRepresentationKind::Integer | NumericRepresentationKind::Real)
        ),
        CompilerKnownOperationRole::UnaryBitNot
        | CompilerKnownOperationRole::BinaryBitAnd
        | CompilerKnownOperationRole::BinaryBitOr
        | CompilerKnownOperationRole::BinaryBitXor
        | CompilerKnownOperationRole::BinaryShiftLeft
        | CompilerKnownOperationRole::BinaryShiftRight => {
            matches!(
                role.numeric_kind(),
                Some(NumericRepresentationKind::Integer)
            )
        }
        CompilerKnownOperationRole::Equality => {
            role.numeric_kind().is_some()
                || matches!(
                    role,
                    RepresentationRole::ScalarBool
                        | RepresentationRole::ScalarChar
                        | RepresentationRole::String
                )
        }
        CompilerKnownOperationRole::Comparison => {
            matches!(
                role.numeric_kind(),
                Some(NumericRepresentationKind::Integer | NumericRepresentationKind::Real)
            ) || matches!(
                role,
                RepresentationRole::ScalarChar | RepresentationRole::String
            )
        }
        CompilerKnownOperationRole::BinaryMatrixMultiply
        | CompilerKnownOperationRole::PlainConversion
        | CompilerKnownOperationRole::ElementIndex
        | CompilerKnownOperationRole::SliceIndex
        | CompilerKnownOperationRole::BoxConstruction => false,
    }
}
