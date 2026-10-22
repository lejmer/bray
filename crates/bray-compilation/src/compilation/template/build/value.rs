use crate::fact::FactQueryError;
use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    BoundExpressionId, BoundOperator, BoundReferenceTarget, CheckedTemplateInput,
    CheckedTemplateInputId, CheckedTemplateInputKind, CheckedTemplateNodeId,
    CheckedTemplateOperation, CheckedTemplateShortCircuitKind, ConstructionTarget,
    SelectedConstruction, SelectedConstructionInput, SelectedOperation, SemanticSelection,
};
use bray_symbols::{
    AnySymbolId, ConstantBinaryOperation, ConstantField, ConstantTermData, ConstantTermId,
    ConstantUnaryOperation, ConstantValueData, ConstantValueId, ConstantValueKind,
    GenericSubstitutionId, ImplementationInstanceId, SymbolKey, TypeId,
};

use super::context::{SourceTemplateBuilder, incomplete, index};

impl<'context, 'compilation, 'unit> SourceTemplateBuilder<'context, 'compilation, 'unit> {
    pub(super) fn literal(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let value = self
            .literals
            .expression(expression)
            .ok_or_else(|| incomplete("missing_literal_value"))?;

        Ok(CheckedTemplateOperation::Constant {
            term: self.constant_value_term(value)?,
            usage: Default::default(),
        })
    }

    pub(super) fn payloadless_variant(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let Some(SemanticSelection::Operation(SelectedOperation::Construction(construction))) =
            self.selections.expression(expression)
        else {
            return Err(incomplete("invalid_payloadless_variant_selection"));
        };

        self.construction(construction)
    }

    pub(in crate::compilation::template) fn construction(
        &mut self,
        construction: &SelectedConstruction,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let substitution = match construction.target() {
            ConstructionTarget::TypeForm { callable, .. } => callable.substitution(),
            ConstructionTarget::Struct(_) | ConstructionTarget::UnionVariant(_) => match self
                .context
                .semantic_values()
                .type_data(construction.result_type())
                .as_ref()
            {
                bray_symbols::TypeData::Named { substitution, .. } => *substitution,
                _ => panic!("checked nominal construction retains its named application"),
            },
        };

        let nominal = !matches!(construction.target(), ConstructionTarget::TypeForm { .. });
        let count = construction.inputs().len();
        let mut fields = Vec::with_capacity(if nominal { count } else { 0 });
        let mut arguments = Vec::with_capacity(if nominal { 0 } else { count });
        let mut ordinals = Vec::with_capacity(if nominal { 0 } else { count });

        for selected in construction.inputs() {
            let (input, value, ordinal) = match selected {
                SelectedConstructionInput::Explicit {
                    expression,
                    input,
                    conversion,
                    ordinal,
                } => (
                    *input,
                    self.converted_expression(*expression, conversion)?,
                    *ordinal,
                ),
                SelectedConstructionInput::Default {
                    input,
                    provider,
                    ordinal,
                    ..
                } => {
                    let checked = self
                        .context
                        .compilation()
                        .checked_default_template(*provider, self.context.cancellation())?;

                    let template = checked
                        .value()
                        .as_ref()
                        .expect("checked default construction retains its template");

                    (
                        *input,
                        self.apply_template(template, &[], Some(substitution))?,
                        *ordinal,
                    )
                }
            };

            if nominal {
                fields.push(ConstantField::new(self.symbol_key(input.symbol())?, value));
            } else {
                arguments.push(value);
                ordinals.push(ordinal);
            }
        }

        match construction.target() {
            ConstructionTarget::Struct(_) => Ok(CheckedTemplateOperation::construct(None, fields)),
            ConstructionTarget::UnionVariant(variant) => Ok(CheckedTemplateOperation::construct(
                Some(self.symbol_key(variant.into())?),
                fields,
            )),
            ConstructionTarget::TypeForm {
                callable, witness, ..
            } => {
                let (callable, substitution) = self.declaration_reference(
                    callable.definition().symbol(),
                    callable.substitution(),
                )?;

                let (witness, witness_substitution) = self.implementation_reference(witness)?;

                Ok(CheckedTemplateOperation::ordered_call(
                    callable,
                    substitution,
                    arguments,
                    ordinals,
                    Some(bray_bound_tree::CheckedTemplateDispatch::Implementation(
                        witness,
                        witness_substitution,
                    )),
                ))
            }
        }
    }

    pub(super) fn reference(
        &mut self,
        expression: BoundExpressionId,
        target: BoundReferenceTarget,
        ty: TypeId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        if let Some(input) = self.input_ids.get(&target) {
            return Ok(CheckedTemplateOperation::Input(*input));
        }

        let BoundReferenceTarget::Surface(symbol) = target else {
            let BoundReferenceTarget::Local(bray_symbols::AnyLocalSymbolId::PostconditionResult(_)) =
                target
            else {
                return Err(incomplete("invalid_reference_selection"));
            };

            let input = CheckedTemplateInputId::new(index(self.inputs.len()));

            self.inputs.push(CheckedTemplateInput::new(
                CheckedTemplateInputKind::PostconditionResult,
                ty,
            ));

            self.input_ids.insert(target, input);

            return Ok(CheckedTemplateOperation::Input(input));
        };

        let substitution = match self.selections.expression(expression) {
            Some(SemanticSelection::StaticReference(instance)) => Some(instance.substitution()),
            _ => None,
        };

        Ok(CheckedTemplateOperation::Declaration {
            declaration: self.symbol_key(symbol)?,
            substitution,
        })
    }

    pub(in crate::compilation::template) fn unary(
        &mut self,
        operator: BoundOperator,
        operands: &[BoundExpressionId],
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let [operand] = operands else {
            return Err(incomplete("invalid_unary_operand_count"));
        };

        let operation =
            unary_operator(operator).ok_or_else(|| incomplete("unsupported_unary_operator"))?;

        let operand = self.expression(*operand)?;

        Ok(if self.unit.key().kind().uses_machine_arithmetic() {
            CheckedTemplateOperation::MachineUnary { operation, operand }
        } else {
            CheckedTemplateOperation::Unary { operation, operand }
        })
    }

    pub(in crate::compilation::template) fn binary(
        &mut self,
        operator: BoundOperator,
        operands: &[BoundExpressionId],
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let [left, right] = operands else {
            return Err(incomplete("invalid_binary_operand_count"));
        };

        let left = self.expression(*left)?;
        let path = self.path;

        if matches!(
            operator,
            BoundOperator::LogicalAnd | BoundOperator::LogicalOr
        ) {
            self.path = self.refine_path(path, left, operator == BoundOperator::LogicalAnd)?;
        }

        let checkpoint = self.state_changes.len();
        let evaluation = self.evaluation;
        let right = self.expression(*right)?;

        if matches!(
            operator,
            BoundOperator::LogicalAnd | BoundOperator::LogicalOr
        ) {
            self.evaluation = evaluation;

            let updates = self.take_state_updates(checkpoint).outer;
            let skipped = self.refine_path(path, left, operator != BoundOperator::LogicalAnd)?;

            self.path = self.merge_paths(skipped, self.path)?;

            let constant = self.boolean_constant(operator == BoundOperator::LogicalOr)?;
            let ty = self.nodes[usize::try_from(left.raw()).expect("Boolean input fits host")].ty();

            let (when_true, when_false, true_updates, false_updates) =
                if operator == BoundOperator::LogicalAnd {
                    (right, constant, updates, Default::default())
                } else {
                    (constant, right, Default::default(), updates)
                };

            let value =
                self.commit_choice(left, when_true, when_false, ty, true_updates, false_updates);

            return Ok(CheckedTemplateOperation::Sequence {
                before: value,
                value,
            });
        }

        match operator {
            BoundOperator::LogicalAnd => Ok(CheckedTemplateOperation::ShortCircuit {
                kind: CheckedTemplateShortCircuitKind::And,
                left,
                right,
            }),
            BoundOperator::LogicalOr => Ok(CheckedTemplateOperation::ShortCircuit {
                kind: CheckedTemplateShortCircuitKind::Or,
                left,
                right,
            }),
            _ => {
                let operation = binary_operator(operator)
                    .ok_or_else(|| incomplete("unsupported_binary_operator"))?;

                Ok(if self.unit.key().kind().uses_machine_arithmetic() {
                    CheckedTemplateOperation::MachineBinary {
                        operation,
                        left,
                        right,
                    }
                } else {
                    CheckedTemplateOperation::Binary {
                        operation,
                        left,
                        right,
                    }
                })
            }
        }
    }

    pub(super) fn expressions(
        &mut self,
        expressions: &[BoundExpressionId],
    ) -> Result<Vec<CheckedTemplateNodeId>, FactQueryError> {
        expressions
            .iter()
            .copied()
            .map(|expression| self.expression(expression))
            .collect()
    }
}

fn unary_operator(operator: BoundOperator) -> Option<ConstantUnaryOperation> {
    match operator {
        BoundOperator::Add => Some(ConstantUnaryOperation::Identity),
        BoundOperator::Subtract => Some(ConstantUnaryOperation::Negate),
        BoundOperator::LogicalNot => Some(ConstantUnaryOperation::LogicalNot),
        BoundOperator::BitwiseNot => Some(ConstantUnaryOperation::BitwiseNot),
        _ => None,
    }
}

pub(in crate::compilation::template) fn binary_operator(
    operator: BoundOperator,
) -> Option<ConstantBinaryOperation> {
    match operator {
        BoundOperator::LogicalOr => Some(ConstantBinaryOperation::LogicalOr),
        BoundOperator::LogicalAnd => Some(ConstantBinaryOperation::LogicalAnd),
        BoundOperator::Equal => Some(ConstantBinaryOperation::Equal),
        BoundOperator::NotEqual => Some(ConstantBinaryOperation::NotEqual),
        BoundOperator::Less => Some(ConstantBinaryOperation::Less),
        BoundOperator::LessEqual => Some(ConstantBinaryOperation::LessOrEqual),
        BoundOperator::Greater => Some(ConstantBinaryOperation::Greater),
        BoundOperator::GreaterEqual => Some(ConstantBinaryOperation::GreaterOrEqual),
        BoundOperator::BitwiseOr => Some(ConstantBinaryOperation::BitwiseOr),
        BoundOperator::BitwiseXor => Some(ConstantBinaryOperation::BitwiseXor),
        BoundOperator::BitwiseAnd => Some(ConstantBinaryOperation::BitwiseAnd),
        BoundOperator::ShiftLeft => Some(ConstantBinaryOperation::ShiftLeft),
        BoundOperator::ShiftRight => Some(ConstantBinaryOperation::ShiftRight),
        BoundOperator::Add => Some(ConstantBinaryOperation::Add),
        BoundOperator::Subtract => Some(ConstantBinaryOperation::Subtract),
        BoundOperator::Multiply => Some(ConstantBinaryOperation::Multiply),
        BoundOperator::Divide => Some(ConstantBinaryOperation::Divide),
        BoundOperator::Remainder => Some(ConstantBinaryOperation::Remainder),
        BoundOperator::Exponentiate => Some(ConstantBinaryOperation::Exponentiate),
        _ => None,
    }
}

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(in crate::compilation::template) fn symbol_key(
        &self,
        symbol: AnySymbolId,
    ) -> Result<SymbolKey, FactQueryError> {
        self.context
            .symbol_key(symbol)
            .map_err(crate::compilation::binder::binding_query_error)?
            .cloned()
            .ok_or_else(|| incomplete("checked template declaration has no stable identity"))
    }

    pub(in crate::compilation::template) fn declaration_reference(
        &self,
        symbol: AnySymbolId,
        substitution: GenericSubstitutionId,
    ) -> Result<(SymbolKey, GenericSubstitutionId), FactQueryError> {
        Ok((self.symbol_key(symbol)?, substitution))
    }

    pub(in crate::compilation::template) fn implementation_reference(
        &self,
        instance: ImplementationInstanceId,
    ) -> Result<(SymbolKey, GenericSubstitutionId), FactQueryError> {
        let data = self
            .context
            .semantic_values()
            .implementation_instance_data(instance);

        Ok((
            self.symbol_key(data.definition().into_any())?,
            data.substitution(),
        ))
    }

    pub(in crate::compilation::template) fn trait_default_requirement(
        &self,
        requirement: bray_symbols::ImplementationRequirementKey,
    ) -> Result<(TypeId, GenericSubstitutionId), FactQueryError> {
        let application = self
            .context
            .semantic_values()
            .trait_application_data(requirement.trait_application());

        Ok((requirement.subject(), application.substitution()))
    }

    pub(super) fn constant_value_term(
        &self,
        value: ConstantValueId,
    ) -> Result<ConstantTermId, FactQueryError> {
        self.context
            .semantic_values()
            .intern_constant_term(ConstantTermData::Value(value))
            .map_err(FactQueryError::from)
    }

    pub(in crate::compilation::template) fn nullable_absence_term(
        &self,
        ty: TypeId,
    ) -> Result<ConstantTermId, FactQueryError> {
        let value =
            self.context
                .semantic_values()
                .intern_constant_value(ConstantValueData::new(
                    ty,
                    ConstantValueKind::NullableAbsent,
                ))?;

        self.constant_value_term(value)
    }
}
