use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundCallableTarget, BoundExpression, BoundExpressionId, BoundOperator, BoundReferenceTarget,
    BoundStructuredExpressionKind, BoundUnit, BoundUnitKey, CheckedExpressionTypes,
    CheckedLiteralValues, CheckedSemanticSelections, CheckedTemplateInputId, CheckedTemplateKind,
    CheckedTemplateNodeId, CheckedTemplateShortCircuitKind, ConstructionTarget, SelectedArgument,
    SelectedConstruction, SelectedOperation, SemanticSelection,
};
use bray_declarations::SyntaxAnchor;
use bray_package_interface::{
    InterfaceCheckedTemplate, InterfaceCheckedTemplateBehavior, InterfaceCheckedTemplateInput,
    InterfaceCheckedTemplateInputKind, InterfaceCheckedTemplateNode,
    InterfaceCheckedTemplateOperation, InterfaceTemplateReference,
};
use bray_symbols::{ConstantBinaryOperation, ConstantUnaryOperation, TypeId};

use super::PackageInterfaceExportError;
use super::semantic::SemanticExporter;
use crate::compilation::Compilation;

pub(super) fn export_checked_source_template(
    compilation: &Compilation,
    export: &mut SemanticExporter<'_>,
    key: BoundUnitKey,
    kind: CheckedTemplateKind,
    expression: SyntaxAnchor,
    mut inputs: Vec<SourceTemplateInput>,
    dependency: bray_symbols::DependencyContractTemplateId,
) -> Result<InterfaceCheckedTemplate, PackageInterfaceExportError> {
    let cancellation = &compilation.state.cancellation;

    let bound = compilation
        .bound_unit_with_cancellation(key.clone(), cancellation)
        .map_err(super::fact_query_export_error)?;

    let semantics = compilation
        .expression_semantics_with_cancellation(key.clone(), cancellation)
        .map_err(super::fact_query_export_error)?;

    let body = compilation
        .body_behavior_with_cancellation(key, cancellation)
        .map_err(super::fact_query_export_error)?;

    if bound.result().diagnostics().has_errors()
        || semantics.result().diagnostics().has_errors()
        || body.result().diagnostics().has_errors()
        || body.result().value().is_recovered()
    {
        return Err(incomplete("invalid_checked_body"));
    }

    let behavior = body.result().value();

    let execution_requirements = behavior
        .execution_requirements()
        .iter()
        .map(|requirement| export.symbol_reference(requirement.declaration()))
        .collect::<Result<Vec<_>, _>>()?;

    let effects = behavior
        .effects()
        .iter()
        .map(|requirement| export.symbol_reference(requirement.declaration()))
        .collect::<Result<Vec<_>, _>>()?;

    let capabilities = behavior
        .capabilities()
        .iter()
        .map(|requirement| export.symbol_reference(requirement.declaration()))
        .collect::<Result<Vec<_>, _>>()?;

    let trusted_capabilities = behavior
        .trusted_capabilities()
        .iter()
        .copied()
        .map(Into::into)
        .map(|requirement| export.symbol_reference(requirement))
        .collect::<Result<Vec<_>, _>>()?;

    let dependency = export.dependency_contract_id(dependency)?;

    let interface_behavior = InterfaceCheckedTemplateBehavior::new(
        effects,
        capabilities,
        trusted_capabilities,
        bray_package_interface::InterfaceCheckedTemplateExecution::new(
            execution_requirements,
            behavior.current_run_cancellation(),
        ),
        behavior.lifecycle_obligations().iter().copied(),
        dependency,
        [],
    );

    let semantics = semantics.result().value();

    if matches!(
        kind,
        CheckedTemplateKind::PredicateDefinition | CheckedTemplateKind::CallableContract
    ) {
        let unit = bound.result().value();

        let root = source_expression_root(unit, expression)
            .ok_or_else(|| incomplete("missing_source_expression_root"))?;

        let condition = compilation
            .symbolic_expression_terms(unit, semantics, &[root], cancellation)
            .map_err(super::fact_query_export_error)?
            .1
            .into_iter()
            .next()
            .flatten();

        if let Some(condition) = condition {
            for (id, expression) in unit.tree().expressions() {
                if let BoundExpression::Name(name) = expression
                    && let BoundReferenceTarget::Local(
                        bray_symbols::AnyLocalSymbolId::PostconditionResult(_),
                    ) = name.target()
                    && !inputs
                        .iter()
                        .any(|input| input.target == Some(name.target()))
                {
                    let ty = semantics
                        .types()
                        .expression(id)
                        .ok_or_else(|| incomplete("missing_postcondition_result_type"))?
                        .ty();

                    inputs.push(SourceTemplateInput::new(
                        InterfaceCheckedTemplateInputKind::PostconditionResult,
                        Some(name.target()),
                        ty,
                    ));
                }
            }

            let result_type = semantics
                .types()
                .expression(root)
                .ok_or_else(|| incomplete("missing_expression_type"))?
                .ty();

            let inputs = inputs
                .iter()
                .map(|input| {
                    // Input kinds hold only copyable roles or shared symbol references.
                    Ok(InterfaceCheckedTemplateInput::new(
                        input.kind.clone(),
                        export.type_id(input.ty)?,
                    ))
                })
                .collect::<Result<Vec<_>, PackageInterfaceExportError>>()?;

            return Ok(InterfaceCheckedTemplate::new(
                kind,
                inputs,
                [InterfaceCheckedTemplateNode::new(
                    InterfaceCheckedTemplateOperation::Constant {
                        term: export.constant_term_id(condition)?,
                        usage: Default::default(),
                    },
                    export.type_id(result_type)?,
                )],
                [],
                CheckedTemplateNodeId::new(0),
                interface_behavior,
            ));
        }
    }

    export_source_template(
        export,
        SourceTemplateRequest {
            kind,
            unit: bound.result().value(),
            types: semantics.types(),
            selections: semantics.selections(),
            literals: semantics.literals(),
            expression,
            inputs,
            behavior: interface_behavior,
        },
    )
}

pub(super) struct SourceTemplateInput {
    kind: InterfaceCheckedTemplateInputKind,
    target: Option<BoundReferenceTarget>,
    ty: TypeId,
}

impl SourceTemplateInput {
    pub(super) const fn new(
        kind: InterfaceCheckedTemplateInputKind,
        target: Option<BoundReferenceTarget>,
        ty: TypeId,
    ) -> Self {
        Self { kind, target, ty }
    }
}

pub(super) struct SourceTemplateRequest<'a> {
    pub(super) kind: CheckedTemplateKind,
    pub(super) unit: &'a BoundUnit,
    pub(super) types: &'a CheckedExpressionTypes,
    pub(super) selections: &'a CheckedSemanticSelections,
    pub(super) literals: &'a CheckedLiteralValues,
    pub(super) expression: SyntaxAnchor,
    pub(super) inputs: Vec<SourceTemplateInput>,
    pub(super) behavior: InterfaceCheckedTemplateBehavior,
}

pub(super) fn export_source_template(
    export: &mut SemanticExporter,
    request: SourceTemplateRequest,
) -> Result<InterfaceCheckedTemplate, PackageInterfaceExportError> {
    let root = source_expression_root(request.unit, request.expression)
        .ok_or_else(|| incomplete("missing_source_expression_root"))?;

    let mut builder = SourceTemplateBuilder::new(export, &request)?;

    let result = builder.expression(root)?;

    Ok(InterfaceCheckedTemplate::new(
        request.kind,
        builder.inputs,
        builder.nodes,
        [],
        result,
        request.behavior,
    ))
}

fn source_expression_root(unit: &BoundUnit, syntax: SyntaxAnchor) -> Option<BoundExpressionId> {
    let mut exact_root = None;
    let mut postfix_root = None;

    for (id, expression) in unit.tree().expressions() {
        let source = expression.origin().source_anchor().syntax();

        if source.source_id() != syntax.source_id() {
            continue;
        }

        if source.full_range() == syntax.full_range() {
            exact_root = Some(id);

            continue;
        }

        if syntax.full_range().contains_range(source.full_range())
            && source.full_range().end() == syntax.full_range().end()
            && postfix_root.is_none_or(|(_, start)| source.full_range().start() > start)
        {
            postfix_root = Some((id, source.full_range().start()));
        }
    }

    postfix_root.map(|(id, _)| id).or(exact_root)
}

struct SourceTemplateBuilder<'export, 'values, 'unit> {
    export: &'export mut SemanticExporter<'values>,
    unit: &'unit BoundUnit,
    types: &'unit CheckedExpressionTypes,
    selections: &'unit CheckedSemanticSelections,
    literals: &'unit CheckedLiteralValues,
    inputs: Vec<InterfaceCheckedTemplateInput>,
    input_ids: BTreeMap<BoundReferenceTarget, CheckedTemplateInputId>,
    nodes: Vec<InterfaceCheckedTemplateNode>,
    expression_nodes: BTreeMap<BoundExpressionId, CheckedTemplateNodeId>,
}

impl<'export, 'values, 'unit> SourceTemplateBuilder<'export, 'values, 'unit> {
    fn new(
        export: &'export mut SemanticExporter<'values>,
        request: &SourceTemplateRequest<'unit>,
    ) -> Result<Self, PackageInterfaceExportError> {
        let mut inputs = Vec::with_capacity(request.inputs.len());
        let mut input_ids = BTreeMap::new();

        for input in &request.inputs {
            let id = CheckedTemplateInputId::new(index(inputs.len())?);

            if let Some(target) = input.target {
                if input_ids.insert(target, id).is_some() {
                    return Err(incomplete("duplicate_template_input_target"));
                }
            }

            inputs.push(InterfaceCheckedTemplateInput::new(
                input.kind.clone(),
                export.type_id(input.ty)?,
            ));
        }

        Ok(Self {
            export,
            unit: request.unit,
            types: request.types,
            selections: request.selections,
            literals: request.literals,
            inputs,
            input_ids,
            nodes: Vec::new(),
            expression_nodes: BTreeMap::new(),
        })
    }

    fn expression(
        &mut self,
        expression_id: BoundExpressionId,
    ) -> Result<CheckedTemplateNodeId, PackageInterfaceExportError> {
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
        let node = CheckedTemplateNodeId::new(index(self.nodes.len())?);

        let interface_ty = match &operation {
            InterfaceCheckedTemplateOperation::Input(input) => self
                .inputs
                .get(usize::try_from(input.raw()).map_err(|_| {
                    super::capacity_export_error("template_input_reference", input.raw())
                })?)
                .map(InterfaceCheckedTemplateInput::ty)
                .ok_or_else(|| incomplete("missing_template_input"))?,
            _ => self.export.type_id(ty)?,
        };

        self.nodes
            .push(InterfaceCheckedTemplateNode::new(operation, interface_ty));

        self.expression_nodes.insert(expression_id, node);

        Ok(node)
    }

    fn operation(
        &mut self,
        expression_id: BoundExpressionId,
        expression: &BoundExpression,
        ty: TypeId,
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
        match expression {
            BoundExpression::Literal(_) => self.literal(expression_id),
            BoundExpression::Name(reference) => {
                self.reference(expression_id, reference.target(), ty)
            }
            BoundExpression::LeadingDotVariant(_) | BoundExpression::UnqualifiedVariant(_) => {
                self.payloadless_variant(expression_id)
            }
            BoundExpression::PatternReference(_) => {
                let Some(SemanticSelection::Reference(target)) =
                    self.selections.expression(expression_id)
                else {
                    return Err(incomplete("missing_pattern_reference_selection"));
                };

                self.reference(expression_id, *target, ty)
            }
            BoundExpression::Unary(unary) => self.unary(unary.operator(), unary.operands()),
            BoundExpression::Binary(binary) => self.binary(binary.operator(), binary.operands()),
            BoundExpression::Call(_) => self.call(expression_id),
            BoundExpression::Conversion(conversion) => {
                let value = self.expression(conversion.operand())?;

                Ok(InterfaceCheckedTemplateOperation::Convert {
                    value,
                    target: self.export.type_id(
                        conversion
                            .target_type()
                            .ok_or_else(|| incomplete("missing_conversion_target_type"))?,
                    )?,
                })
            }
            BoundExpression::Structured(expression) => match expression.kind() {
                BoundStructuredExpressionKind::Absence => {
                    Ok(InterfaceCheckedTemplateOperation::Constant {
                        term: self.export.nullable_absence_term_id(ty)?,
                        usage: Default::default(),
                    })
                }
                BoundStructuredExpressionKind::Tuple => {
                    Ok(InterfaceCheckedTemplateOperation::tuple(
                        self.expressions(expression.operands())?,
                    ))
                }
                BoundStructuredExpressionKind::Array => {
                    Ok(InterfaceCheckedTemplateOperation::array(
                        self.expressions(expression.operands())?,
                    ))
                }
                BoundStructuredExpressionKind::Borrow => {
                    let [operand] = expression.operands() else {
                        return Err(incomplete("invalid_borrow_operand_count"));
                    };

                    Ok(InterfaceCheckedTemplateOperation::Borrow {
                        kind: expression
                            .borrow_kind()
                            .ok_or_else(|| incomplete("missing_borrow_kind"))?,
                        operand: self.expression(*operand)?,
                    })
                }
                BoundStructuredExpressionKind::TrustBoundary => {
                    let [operand] = expression.operands() else {
                        return Err(incomplete("invalid_trust_boundary_operand_count"));
                    };

                    let node = self.expression(*operand)?;

                    let index = usize::try_from(node.raw()).map_err(|_| {
                        super::capacity_export_error("template_node_reference", node.raw())
                    })?;

                    let operation = self
                        .nodes
                        .get(index)
                        .map(InterfaceCheckedTemplateNode::operation)
                        .cloned()
                        .ok_or_else(|| incomplete("missing_trust_boundary_operation"))?;

                    Ok(operation)
                }
                _ => Err(incomplete("unsupported_structured_expression")),
            },
            BoundExpression::MemberAccess(member) => {
                match self.selections.expression(expression_id) {
                    Some(SemanticSelection::Operation(SelectedOperation::Member(target))) => {
                        Ok(InterfaceCheckedTemplateOperation::Project {
                            subject: self.expression(member.receiver())?,
                            member: InterfaceTemplateReference::Symbol(
                                self.export.symbol_reference(target.member())?,
                            ),
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
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
        let value = self
            .literals
            .expression(expression)
            .ok_or_else(|| incomplete("missing_literal_value"))?;

        Ok(InterfaceCheckedTemplateOperation::Constant {
            term: self.export.constant_value_term_id(value)?,
            usage: Default::default(),
        })
    }

    fn payloadless_variant(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
        let Some(SemanticSelection::Operation(SelectedOperation::Construction(construction))) =
            self.selections.expression(expression)
        else {
            return Err(incomplete("invalid_payloadless_variant_selection"));
        };

        self.payloadless_variant_construction(construction)
    }

    fn payloadless_variant_construction(
        &mut self,
        construction: &SelectedConstruction,
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
        let ConstructionTarget::UnionVariant(variant) = construction.target() else {
            return Err(incomplete("invalid_payloadless_variant_target"));
        };

        if !construction.inputs().is_empty() {
            return Err(incomplete("payloadless_variant_has_inputs"));
        }

        Ok(InterfaceCheckedTemplateOperation::Declaration {
            declaration: InterfaceTemplateReference::Symbol(
                self.export.symbol_reference(variant.into())?,
            ),
            substitution: None,
        })
    }

    fn reference(
        &mut self,
        expression: BoundExpressionId,
        target: BoundReferenceTarget,
        ty: TypeId,
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
        if let Some(input) = self.input_ids.get(&target) {
            return Ok(InterfaceCheckedTemplateOperation::Input(*input));
        }

        let BoundReferenceTarget::Surface(symbol) = target else {
            let BoundReferenceTarget::Local(bray_symbols::AnyLocalSymbolId::PostconditionResult(_)) =
                target
            else {
                return Err(incomplete("invalid_reference_selection"));
            };

            let input = CheckedTemplateInputId::new(index(self.inputs.len())?);

            self.inputs.push(InterfaceCheckedTemplateInput::new(
                InterfaceCheckedTemplateInputKind::PostconditionResult,
                self.export.type_id(ty)?,
            ));

            self.input_ids.insert(target, input);

            return Ok(InterfaceCheckedTemplateOperation::Input(input));
        };

        let substitution = match self.selections.expression(expression) {
            Some(SemanticSelection::StaticReference(instance)) => {
                Some(self.export.substitution_id(instance.substitution())?)
            }
            _ => None,
        };

        Ok(InterfaceCheckedTemplateOperation::Declaration {
            declaration: InterfaceTemplateReference::Symbol(self.export.symbol_reference(symbol)?),
            substitution,
        })
    }

    fn unary(
        &mut self,
        operator: BoundOperator,
        operands: &[BoundExpressionId],
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
        let [operand] = operands else {
            return Err(incomplete("invalid_unary_operand_count"));
        };

        Ok(InterfaceCheckedTemplateOperation::Unary {
            operation: unary_operator(operator)
                .ok_or_else(|| incomplete("unsupported_unary_operator"))?,
            operand: self.expression(*operand)?,
        })
    }

    fn binary(
        &mut self,
        operator: BoundOperator,
        operands: &[BoundExpressionId],
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
        let [left, right] = operands else {
            return Err(incomplete("invalid_binary_operand_count"));
        };

        let left = self.expression(*left)?;
        let right = self.expression(*right)?;

        match operator {
            BoundOperator::LogicalAnd => Ok(InterfaceCheckedTemplateOperation::ShortCircuit {
                kind: CheckedTemplateShortCircuitKind::And,
                left,
                right,
            }),
            BoundOperator::LogicalOr => Ok(InterfaceCheckedTemplateOperation::ShortCircuit {
                kind: CheckedTemplateShortCircuitKind::Or,
                left,
                right,
            }),
            _ => Ok(InterfaceCheckedTemplateOperation::Binary {
                operation: binary_operator(operator)
                    .ok_or_else(|| incomplete("unsupported_binary_operator"))?,
                left,
                right,
            }),
        }
    }

    fn call(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
        match self.selections.expression(expression) {
            Some(SemanticSelection::Call(call)) => self.callable_call(call),
            Some(SemanticSelection::Predicate(predicate)) => self.predicate_call(predicate),
            _ => Err(incomplete("invalid_call_selection")),
        }
    }

    fn callable_call(
        &mut self,
        call: &bray_bound_tree::SelectedCall,
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
        let (declaration, substitution) = match call.target() {
            BoundCallableTarget::Declaration(callable) => {
                (callable.definition().symbol(), callable.substitution())
            }
            BoundCallableTarget::Predicate(predicate) => {
                (predicate.definition().into_any(), predicate.substitution())
            }
            BoundCallableTarget::Anonymous(_) | BoundCallableTarget::Indirect(_) => {
                return Err(incomplete("unsupported_callable_target"));
            }
        };

        let (callable, substitution) = self
            .export
            .declaration_template_reference(declaration, substitution)?;

        let mut arguments = Vec::new();

        if let Some(receiver) = call.receiver() {
            arguments.push(self.expression(receiver.expression())?);
        }

        for argument in call.arguments() {
            let SelectedArgument::Explicit { expression, .. } = argument else {
                return Err(incomplete("non_explicit_template_argument"));
            };

            arguments.push(self.expression(*expression)?);
        }

        let implementation = match call.witnesses() {
            [] => None,
            [witness] => Some(
                self.export
                    .implementation_template_reference(witness.witness())?,
            ),
            _ => return Err(incomplete("invalid_predicate_dispatch")),
        };

        Ok(InterfaceCheckedTemplateOperation::call(
            callable,
            substitution,
            arguments,
            implementation,
        ))
    }

    fn predicate_call(
        &mut self,
        predicate: &bray_bound_tree::SelectedPredicateApplication,
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
        let callable = InterfaceTemplateReference::Symbol(
            self.export
                .symbol_reference(predicate.predicate().into_any())?,
        );

        let substitution = self.export.substitution_id(predicate.substitution())?;

        let arguments = predicate
            .arguments()
            .iter()
            .map(|argument| self.expression(argument.expression()))
            .collect::<Result<Vec<_>, _>>()?;

        Ok(InterfaceCheckedTemplateOperation::call(
            callable,
            substitution,
            arguments,
            None,
        ))
    }

    fn expressions(
        &mut self,
        expressions: &[BoundExpressionId],
    ) -> Result<Vec<CheckedTemplateNodeId>, PackageInterfaceExportError> {
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

fn index(length: usize) -> Result<u32, PackageInterfaceExportError> {
    u32::try_from(length)
        .map_err(|_| super::capacity_export_error("checked_template_node_count", length))
}

const fn incomplete(reason: &'static str) -> PackageInterfaceExportError {
    PackageInterfaceExportError::InvalidCompilationCause(
        super::PackageInterfaceInvalidCompilationCause::CheckedTemplate { reason },
    )
}
