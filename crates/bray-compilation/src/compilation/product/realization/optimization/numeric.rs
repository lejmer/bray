use std::collections::BTreeMap;

use bray_bound_tree::BoundOperator;
use bray_checker::{fold_machine_integer_binary, fold_machine_integer_truncate, fold_machine_integer_unary};
use bray_ir::{
    MirAggregateKind, MirBinaryOperator, MirNullableQueryKind, MirStorageId,
    MirOperationKind, MirUnaryOperator, MirValueId,
};
use bray_symbols::{ConstantValueData, ConstantValueKind, TypeData, TypeId};

use super::analysis::{Scalar, ScalarAnalysis, unresolved};
use super::super::super::super::{CodegenPreparationError, Compilation};
use super::super::super::specialization::ConcreteCodegenInstance;
use crate::fact::CancellationToken;

impl ScalarAnalysis<'_> {
    pub(super) fn operation(
        &self,
        compilation: &Compilation,
        realization: &ConcreteCodegenInstance,
        result: MirValueId,
        kind: &MirOperationKind,
        local: &BTreeMap<MirStorageId, Scalar>,
        cancellation: &CancellationToken,
    ) -> Result<Scalar, CodegenPreparationError> {
        match kind {
            MirOperationKind::Unary { operator: MirUnaryOperator::Not, operand } => {
                let state = self.operand(operand, local);

                Ok(self.boolean(state).map_or_else(|| unresolved(state), |value| Scalar::Boolean(!value)))
            }
            MirOperationKind::Unary { operator, operand } => {
                let state = self.operand(operand, local);

                let Scalar::Constant(value) = state else {
                    return Ok(unresolved(state));
                };

                let data = self.values.constant_value_data(value);

                let ConstantValueKind::Integer(integer) = data.kind() else {
                    return Ok(Scalar::Overdefined);
                };

                let Some(role) = self.integer_role(compilation, data.ty()) else {
                    return Ok(Scalar::Overdefined);
                };

                let operator = match operator {
                    MirUnaryOperator::Negate => BoundOperator::Subtract,
                    MirUnaryOperator::BitwiseNot => BoundOperator::BitwiseNot,
                    MirUnaryOperator::Not => unreachable!("boolean negation handled above"),
                };

                let folded = fold_machine_integer_unary(
                    operator,
                    integer,
                    role,
                    compilation.selected_target().target().profile().machine().pointer_width_bits(),
                );

                self.intern_folded(compilation, realization, result, Some(folded), cancellation)
            }
            MirOperationKind::Binary { operator, left, right } => {
                let left = self.operand(left, local);
                let right = self.operand(right, local);

                if matches!(operator, MirBinaryOperator::Equal | MirBinaryOperator::NotEqual)
                    && let (Some(left), Some(right)) = (self.boolean(left), self.boolean(right))
                {
                    return Ok(Scalar::Boolean((left == right) == (*operator == MirBinaryOperator::Equal)));
                }

                let (Scalar::Constant(left_id), Scalar::Constant(right_id)) = (left, right) else {
                    return Ok(unresolved_pair(left, right));
                };

                let left_data = self.values.constant_value_data(left_id);
                let right_data = self.values.constant_value_data(right_id);

                let (ConstantValueKind::Integer(left), ConstantValueKind::Integer(right)) =
                    (left_data.kind(), right_data.kind()) else {
                    return Ok(Scalar::Overdefined);
                };

                let Some(role) = self.integer_role(compilation, left_data.ty()) else {
                    return Ok(Scalar::Overdefined);
                };

                if self.integer_role(compilation, right_data.ty()) != Some(role) {
                    return Ok(Scalar::Overdefined);
                }

                let folded = fold_machine_integer_binary(
                    bound_binary_operator(*operator),
                    left,
                    right,
                    role.integer_representation().expect("integer role has a representation"),
                    compilation.selected_target().target().profile().machine().pointer_width_bits(),
                );

                self.intern_folded(compilation, realization, result, folded, cancellation)
            }
            MirOperationKind::NumericConversion { operand, .. } => {
                let state = self.operand(operand, local);

                let Scalar::Constant(value) = state else {
                    return Ok(unresolved(state));
                };

                let data = self.values.constant_value_data(value);

                let ConstantValueKind::Integer(integer) = data.kind() else {
                    return Ok(Scalar::Overdefined);
                };

                let target_type = compilation.substitute_codegen_type(
                    self.unit.value(result).expect("valid MIR result").ty(),
                    realization.substitution(),
                    cancellation,
                )?;

                let (Some(source), Some(target)) = (
                    self.integer_role(compilation, data.ty()),
                    self.integer_role(compilation, target_type),
                ) else {
                    return Ok(Scalar::Overdefined);
                };

                let folded = fold_machine_integer_truncate(
                    integer,
                    source.integer_representation().expect("integer role has a representation"),
                    target.integer_representation().expect("integer role has a representation"),
                    compilation.selected_target().target().profile().machine().pointer_width_bits(),
                );

                self.intern_folded(compilation, realization, result, Some(folded), cancellation)
            }
            MirOperationKind::NullableQuery(query) => {
                let state = self.nullable_operand(query.operand(), local);

                Ok(match self.nullable_present(state) {
                    Some(present) => Scalar::Boolean(
                        present == matches!(query.kind(), MirNullableQueryKind::IsPresent),
                    ),
                    None => unresolved(state),
                })
            }
            MirOperationKind::Aggregate(aggregate)
                if aggregate.kind() == MirAggregateKind::NullablePresent =>
            {
                Ok(Scalar::NullablePresent)
            }
            _ => Ok(Scalar::Overdefined),
        }
    }

    fn integer_role(&self, compilation: &Compilation, ty: TypeId) -> Option<bray_compiler_known::RepresentationRole> {
        let data = self.values.type_data(ty);

        let TypeData::Named { definition, .. } = data.as_ref() else {
            return None;
        };

        crate::compilation::foreign::compiler_known_representation(compilation, *definition)
            .filter(|role| role.integer_representation().is_some())
    }

    fn intern_folded(
        &self,
        compilation: &Compilation,
        realization: &ConcreteCodegenInstance,
        result: MirValueId,
        folded: Option<ConstantValueKind>,
        cancellation: &CancellationToken,
    ) -> Result<Scalar, CodegenPreparationError> {
        let Some(folded) = folded else {
            return Ok(Scalar::Overdefined);
        };

        if let ConstantValueKind::Boolean(value) = folded {
            return Ok(Scalar::Boolean(value));
        }

        let ty = compilation.substitute_codegen_type(
            self.unit.value(result).expect("valid MIR result").ty(),
            realization.substitution(),
            cancellation,
        )?;

        let value = self.values.intern_constant_value(ConstantValueData::new(ty, folded))
            .map_err(|error| crate::fact::FactQueryError::SemanticValueStore(error))?;

        Ok(Scalar::Constant(value))
    }

}

fn unresolved_pair(left: Scalar, right: Scalar) -> Scalar {
    if matches!(left, Scalar::Unknown) || matches!(right, Scalar::Unknown) {
        Scalar::Unknown
    } else {
        Scalar::Overdefined
    }
}
fn bound_binary_operator(operator: MirBinaryOperator) -> BoundOperator {
    match operator {
        MirBinaryOperator::Add => BoundOperator::Add,
        MirBinaryOperator::Subtract => BoundOperator::Subtract,
        MirBinaryOperator::Multiply => BoundOperator::Multiply,
        MirBinaryOperator::Divide => BoundOperator::Divide,
        MirBinaryOperator::Remainder => BoundOperator::Remainder,
        MirBinaryOperator::Equal => BoundOperator::Equal,
        MirBinaryOperator::NotEqual => BoundOperator::NotEqual,
        MirBinaryOperator::LessThan => BoundOperator::Less,
        MirBinaryOperator::LessThanOrEqual => BoundOperator::LessEqual,
        MirBinaryOperator::GreaterThan => BoundOperator::Greater,
        MirBinaryOperator::GreaterThanOrEqual => BoundOperator::GreaterEqual,
        MirBinaryOperator::BitwiseAnd => BoundOperator::BitwiseAnd,
        MirBinaryOperator::BitwiseOr => BoundOperator::BitwiseOr,
        MirBinaryOperator::BitwiseXor => BoundOperator::BitwiseXor,
        MirBinaryOperator::ShiftLeft => BoundOperator::ShiftLeft,
        MirBinaryOperator::ShiftRight => BoundOperator::ShiftRight,
    }
}
