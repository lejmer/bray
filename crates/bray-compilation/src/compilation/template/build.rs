use std::collections::BTreeMap;

use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundOperator, BoundReferenceTarget,
    BoundStructuredExpressionKind, BoundUnit, CheckedExpressionTypes, CheckedLiteralValues,
    CheckedSemanticSelections, CheckedTemplate, CheckedTemplateBehavior, CheckedTemplateInput,
    CheckedTemplateInputId, CheckedTemplateInputKind, CheckedTemplateKind, CheckedTemplateNode,
    CheckedTemplateNodeId, CheckedTemplateOperation, CheckedTemplateShortCircuitKind,
    ConstructionInputId, ConstructionTarget, SelectedConstruction, SelectedConstructionInput,
    SelectedOperation, SemanticSelection,
};
use bray_declarations::SyntaxAnchor;
use bray_symbols::{
    AnySymbolId, ConstantBinaryOperation, ConstantField, ConstantTermData, ConstantTermId,
    ConstantUnaryOperation, ConstantValueData, ConstantValueId, ConstantValueKind,
    GenericSubstitutionId, ImplementationInstanceId, SymbolKey, TypeId,
};

use crate::compilation::binder::CompilationBindingContext;
use crate::fact::FactQueryError;

pub(in crate::compilation) struct SourceTemplateInput {
    kind: CheckedTemplateInputKind,
    pub(in crate::compilation) target: Option<BoundReferenceTarget>,
    pub(in crate::compilation) ty: TypeId,
}

impl SourceTemplateInput {
    pub(in crate::compilation) const fn new(
        kind: CheckedTemplateInputKind,
        target: Option<BoundReferenceTarget>,
        ty: TypeId,
    ) -> Self {
        Self { kind, target, ty }
    }
}

pub(in crate::compilation) struct SourceTemplateRequest<'a> {
    pub(in crate::compilation) kind: CheckedTemplateKind,
    pub(in crate::compilation) unit: &'a BoundUnit,
    pub(in crate::compilation) patterns: &'a bray_bound_tree::CheckedPatterns,
    pub(in crate::compilation) types: &'a CheckedExpressionTypes,
    pub(in crate::compilation) selections: &'a CheckedSemanticSelections,
    pub(in crate::compilation) literals: &'a CheckedLiteralValues,
    pub(in crate::compilation) expression: SyntaxAnchor,
    pub(in crate::compilation) inputs: Vec<SourceTemplateInput>,
    pub(in crate::compilation) behavior: CheckedTemplateBehavior,
}

pub(in crate::compilation) fn build_source_template(
    context: &CompilationBindingContext<'_>,
    request: SourceTemplateRequest,
) -> Result<CheckedTemplate, FactQueryError> {
    let root = source_expression_root(request.unit, request.expression).unwrap_or_else(|| {
        panic!(
            "checked source template {:?} must contain expression {:?} in {:?}",
            request.kind,
            request.expression,
            request.unit.key(),
        )
    });

    let mut builder = SourceTemplateBuilder::new(context, &request)?;

    let result = builder.expression(root)?;

    Ok(CheckedTemplate::from_checked_parts(
        request.kind,
        builder.inputs,
        builder.nodes,
        [],
        result,
        request.behavior,
    ))
}

fn source_expression_root(unit: &BoundUnit, syntax: SyntaxAnchor) -> Option<BoundExpressionId> {
    let block = match unit.root() {
        bray_bound_tree::BoundUnitRoot::Expression(root) => return Some(root),
        bray_bound_tree::BoundUnitRoot::ExpressionSequence(block) => block,
        root => panic!("checked declaration template requires an expression root, found {root:?}"),
    };

    // Binding already selected each complete clause expression. Parentheses and
    // postfix syntax may give its root a narrower anchor than the source clause.
    unit.view()
        .block(block)
        .expect("bound expression sequence retains its root block")
        .items()
        .iter()
        .filter_map(|item| item.expression())
        .find(|id| {
            let source = unit
                .view()
                .expression(*id)
                .expect("bound expression sequence retains each expression")
                .origin()
                .source_anchor()
                .syntax();

            source.source_id() == syntax.source_id()
                && syntax.full_range().contains_range(source.full_range())
        })
}

pub(in crate::compilation) struct SourceTemplateBuilder<'context, 'compilation, 'unit> {
    pub(super) context: &'context CompilationBindingContext<'compilation>,
    pub(super) unit: &'unit BoundUnit,
    patterns: &'unit bray_bound_tree::CheckedPatterns,
    types: &'unit CheckedExpressionTypes,
    pub(in crate::compilation) selections: &'unit CheckedSemanticSelections,
    literals: &'unit CheckedLiteralValues,
    inputs: Vec<CheckedTemplateInput>,
    input_ids: BTreeMap<BoundReferenceTarget, CheckedTemplateInputId>,
    pub(super) nodes: Vec<CheckedTemplateNode>,
    expression_nodes: BTreeMap<BoundExpressionId, CheckedTemplateNodeId>,
}

impl<'context, 'compilation, 'unit> SourceTemplateBuilder<'context, 'compilation, 'unit> {
    fn new(
        context: &'context CompilationBindingContext<'compilation>,
        request: &SourceTemplateRequest<'unit>,
    ) -> Result<Self, FactQueryError> {
        let mut inputs = Vec::with_capacity(request.inputs.len());
        let mut input_ids = BTreeMap::new();

        for input in &request.inputs {
            let id = CheckedTemplateInputId::new(index(inputs.len()));

            if let Some(target) = input.target {
                if input_ids.insert(target, id).is_some() {
                    return Err(incomplete("duplicate_template_input_target"));
                }
            }

            inputs.push(CheckedTemplateInput::new(input.kind.clone(), input.ty));
        }

        Ok(Self {
            context,
            unit: request.unit,
            patterns: request.patterns,
            types: request.types,
            selections: request.selections,
            literals: request.literals,
            inputs,
            input_ids,
            nodes: Vec::new(),
            expression_nodes: BTreeMap::new(),
        })
    }

    fn pattern_test(
        &mut self,
        expression: &bray_bound_tree::BoundStructuredExpression,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let [subject] = expression.operands() else {
            panic!("checked pattern test has one subject");
        };

        let pattern = expression
            .patterns()
            .first()
            .and_then(|pattern| self.patterns.complete_test(self.unit, *pattern))
            .ok_or_else(|| incomplete("unsupported_pattern_test"))?;

        let left = self.expression(*subject)?;

        let operation = match pattern {
            bray_bound_tree::PatternPredicate::ActiveUnionVariant(variant) => {
                return Ok(CheckedTemplateOperation::VariantTest {
                    subject: left,
                    variant: self.symbol_key(variant.into())?,
                });
            }
            bray_bound_tree::PatternPredicate::NullableAbsent => ConstantBinaryOperation::Equal,
            bray_bound_tree::PatternPredicate::NullablePresent => ConstantBinaryOperation::NotEqual,
            _ => return Err(incomplete("unsupported_pattern_test")),
        };

        let subject_type = self
            .types
            .expression(*subject)
            .expect("checked pattern subject retains its type")
            .ty();

        let subject_type = self.context.semantic_values().unborrowed_type(subject_type);
        let right = CheckedTemplateNodeId::new(index(self.nodes.len()));

        self.nodes.push(CheckedTemplateNode::new(
            CheckedTemplateOperation::Constant {
                term: self.nullable_absence_term(subject_type)?,
                usage: Default::default(),
            },
            subject_type,
        ));

        Ok(CheckedTemplateOperation::Binary {
            operation,
            left,
            right,
        })
    }

    pub(in crate::compilation) fn expression(
        &mut self,
        expression_id: BoundExpressionId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        if let Some(node) = self.expression_nodes.get(&expression_id) {
            return Ok(*node);
        }

        let expression = self
            .unit
            .view()
            .expression(expression_id)
            .ok_or_else(|| incomplete("missing_bound_expression"))?;

        if expression.is_recovered() {
            return Err(incomplete("recovered_bound_expression"));
        }

        let ty = self
            .types
            .expression(expression_id)
            .filter(|result| !result.is_recovered())
            .map(|result| result.ty())
            .ok_or_else(|| incomplete("missing_expression_type"))?;

        let operation = self.operation(expression_id, expression, ty)?;
        let node = CheckedTemplateNodeId::new(index(self.nodes.len()));

        let node_ty = match &operation {
            CheckedTemplateOperation::Input(input) => self
                .inputs
                .get(
                    usize::try_from(input.raw())
                        .map_err(|_| incomplete("template input index does not fit the host"))?,
                )
                .map(CheckedTemplateInput::ty)
                .ok_or_else(|| incomplete("missing_template_input"))?,
            _ => ty,
        };

        self.nodes
            .push(CheckedTemplateNode::new(operation, node_ty));

        self.expression_nodes.insert(expression_id, node);

        Ok(node)
    }

    pub(super) fn converted_expression(
        &mut self,
        expression: BoundExpressionId,
        conversion: &bray_bound_tree::SelectedConversion,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let value = self.expression(expression)?;

        let target = conversion.target_type();
        let node = CheckedTemplateNodeId::new(index(self.nodes.len()));

        self.nodes.push(CheckedTemplateNode::new(
            CheckedTemplateOperation::Convert { value, target },
            target,
        ));

        Ok(node)
    }

    pub(super) fn operation(
        &mut self,
        expression_id: BoundExpressionId,
        expression: &BoundExpression,
        ty: TypeId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        match expression {
            BoundExpression::Literal(_) => self.literal(expression_id),
            BoundExpression::Name(reference) => {
                self.reference(expression_id, reference.target(), ty)
            }
            BoundExpression::LeadingDotVariant(_) | BoundExpression::UnqualifiedVariant(_) => {
                self.payloadless_variant(expression_id)
            }
            BoundExpression::StructConstruction(_) => self.product(expression_id),
            BoundExpression::PatternReference(_) => {
                let Some(SemanticSelection::Reference(target)) =
                    self.selections.expression(expression_id)
                else {
                    return Err(incomplete("missing_pattern_reference_selection"));
                };

                self.reference(expression_id, *target, ty)
            }
            BoundExpression::Unary(unary) => {
                self.operator(expression_id, unary.operator(), unary.operands())
            }
            BoundExpression::Binary(binary) => {
                self.operator(expression_id, binary.operator(), binary.operands())
            }
            BoundExpression::Call(_) => self.call(expression_id),
            BoundExpression::Conversion(conversion) => {
                let value = self.expression(conversion.operand())?;

                let Some(SemanticSelection::Operation(SelectedOperation::Conversion(selected))) =
                    self.selections.expression(expression_id)
                else {
                    return Err(incomplete("missing_conversion_selection"));
                };

                Ok(CheckedTemplateOperation::Convert {
                    value,
                    target: selected.target_type(),
                })
            }
            BoundExpression::Structured(expression) => match expression.kind() {
                BoundStructuredExpressionKind::Conditional => self.conditional(expression, ty),
                BoundStructuredExpressionKind::Condition => {
                    let [condition] = expression.operands() else {
                        panic!("checked condition has one Boolean operand");
                    };

                    let node = self.expression(*condition)?;

                    Ok(
                        self.nodes[usize::try_from(node.raw()).expect("checked node fits host")]
                            .operation()
                            .clone(),
                    )
                }
                BoundStructuredExpressionKind::Absence => Ok(CheckedTemplateOperation::Constant {
                    term: self.nullable_absence_term(ty)?,
                    usage: Default::default(),
                }),
                BoundStructuredExpressionKind::PatternTest => self.pattern_test(expression),
                BoundStructuredExpressionKind::Tuple => Ok(CheckedTemplateOperation::tuple(
                    self.expressions(expression.operands())?,
                )),
                BoundStructuredExpressionKind::Array => Ok(CheckedTemplateOperation::array(
                    self.expressions(expression.operands())?,
                )),
                BoundStructuredExpressionKind::Borrow => {
                    let [operand] = expression.operands() else {
                        return Err(incomplete("invalid_borrow_operand_count"));
                    };

                    Ok(CheckedTemplateOperation::Borrow {
                        kind: expression
                            .borrow_kind()
                            .ok_or_else(|| incomplete("missing_borrow_kind"))?,
                        operand: self.expression(*operand)?,
                    })
                }
                BoundStructuredExpressionKind::ElementIndex
                | BoundStructuredExpressionKind::SliceIndex => {
                    self.index(expression_id, expression)
                }
                BoundStructuredExpressionKind::TrustBoundary => {
                    let [operand] = expression.operands() else {
                        return Err(incomplete("invalid_trust_boundary_operand_count"));
                    };

                    let node = self.expression(*operand)?;

                    let index = usize::try_from(node.raw())
                        .map_err(|_| incomplete("template node index does not fit the host"))?;

                    let operation = self
                        .nodes
                        .get(index)
                        .map(CheckedTemplateNode::operation)
                        .cloned()
                        .ok_or_else(|| incomplete("missing_trust_boundary_operation"))?;

                    Ok(operation)
                }
                _ => Err(incomplete("unsupported_structured_expression")),
            },
            BoundExpression::MemberAccess(member) => {
                if let Some(bray_bound_tree::BoundMemberSelector::TupleElement(index)) =
                    member.selector()
                {
                    return Ok(CheckedTemplateOperation::TupleElement {
                        subject: self.expression(member.receiver())?,
                        index: bray_symbols::SymbolOrdinal::new(*index),
                    });
                }

                match self.selections.expression(expression_id) {
                    Some(SemanticSelection::Operation(SelectedOperation::Member(target))) => {
                        Ok(CheckedTemplateOperation::Project {
                            subject: self.expression(member.receiver())?,
                            member: self.symbol_key(target.member())?,
                        })
                    }
                    Some(SemanticSelection::Operation(SelectedOperation::Construction(
                        construction,
                    ))) => self.payloadless_variant_construction(construction),
                    _ => Err(incomplete("invalid_member_access_selection")),
                }
            }
            _ => Err(incomplete("unsupported_template_expression")),
        }
    }

    fn literal(
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

    fn payloadless_variant(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let Some(SemanticSelection::Operation(SelectedOperation::Construction(construction))) =
            self.selections.expression(expression)
        else {
            return Err(incomplete("invalid_payloadless_variant_selection"));
        };

        self.payloadless_variant_construction(construction)
    }

    fn product(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let Some(SemanticSelection::Operation(SelectedOperation::Construction(construction))) =
            self.selections.expression(expression)
        else {
            return Err(incomplete("invalid_product_construction_selection"));
        };

        if !matches!(construction.target(), ConstructionTarget::Struct(_)) {
            return Err(incomplete("invalid_product_construction_target"));
        }

        let mut fields = Vec::with_capacity(construction.inputs().len());

        for input in construction.inputs() {
            let SelectedConstructionInput::Explicit {
                expression,
                input,
                conversion,
                ..
            } = input
            else {
                return Err(incomplete("default_product_construction_input"));
            };

            let ConstructionInputId::StructField(field) = input else {
                return Err(incomplete("invalid_product_construction_input"));
            };

            let value = self.converted_expression(*expression, conversion)?;

            fields.push(ConstantField::new(self.symbol_key((*field).into())?, value));
        }

        Ok(CheckedTemplateOperation::product(fields))
    }

    fn payloadless_variant_construction(
        &mut self,
        construction: &SelectedConstruction,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let ConstructionTarget::UnionVariant(variant) = construction.target() else {
            return Err(incomplete("invalid_payloadless_variant_target"));
        };

        if !construction.inputs().is_empty() {
            return Err(incomplete("payloadless_variant_has_inputs"));
        }

        Ok(CheckedTemplateOperation::Declaration {
            declaration: self.symbol_key(variant.into())?,
            substitution: None,
        })
    }

    fn reference(
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

    pub(super) fn unary(
        &mut self,
        operator: BoundOperator,
        operands: &[BoundExpressionId],
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let [operand] = operands else {
            return Err(incomplete("invalid_unary_operand_count"));
        };

        Ok(CheckedTemplateOperation::Unary {
            operation: unary_operator(operator)
                .ok_or_else(|| incomplete("unsupported_unary_operator"))?,
            operand: self.expression(*operand)?,
        })
    }

    pub(super) fn binary(
        &mut self,
        operator: BoundOperator,
        operands: &[BoundExpressionId],
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let [left, right] = operands else {
            return Err(incomplete("invalid_binary_operand_count"));
        };

        let left = self.expression(*left)?;
        let right = self.expression(*right)?;

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
            _ => Ok(CheckedTemplateOperation::Binary {
                operation: binary_operator(operator)
                    .ok_or_else(|| incomplete("unsupported_binary_operator"))?,
                left,
                right,
            }),
        }
    }

    fn expressions(
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

fn binary_operator(operator: BoundOperator) -> Option<ConstantBinaryOperation> {
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
    pub(super) fn symbol_key(&self, symbol: AnySymbolId) -> Result<SymbolKey, FactQueryError> {
        self.context
            .symbol_key(symbol)
            .map_err(crate::compilation::binder::binding_query_error)?
            .cloned()
            .ok_or_else(|| incomplete("checked template declaration has no stable identity"))
    }

    pub(super) fn declaration_reference(
        &self,
        symbol: AnySymbolId,
        substitution: GenericSubstitutionId,
    ) -> Result<(SymbolKey, GenericSubstitutionId), FactQueryError> {
        Ok((self.symbol_key(symbol)?, substitution))
    }

    pub(super) fn implementation_reference(
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

    pub(super) fn trait_default_requirement(
        &self,
        requirement: bray_symbols::ImplementationRequirementKey,
    ) -> Result<(TypeId, GenericSubstitutionId), FactQueryError> {
        let application = self
            .context
            .semantic_values()
            .trait_application_data(requirement.trait_application());

        Ok((requirement.subject(), application.substitution()))
    }

    fn constant_value_term(
        &self,
        value: ConstantValueId,
    ) -> Result<ConstantTermId, FactQueryError> {
        self.context
            .semantic_values()
            .intern_constant_term(ConstantTermData::Value(value))
            .map_err(FactQueryError::from)
    }

    fn nullable_absence_term(&self, ty: TypeId) -> Result<ConstantTermId, FactQueryError> {
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

pub(super) fn index(length: usize) -> u32 {
    u32::try_from(length).expect("checked source template identity fits its bound unit")
}

pub(super) fn incomplete(reason: &'static str) -> FactQueryError {
    panic!("checked source template construction violated its phase contract: {reason}")
}
