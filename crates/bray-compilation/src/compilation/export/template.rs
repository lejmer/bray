use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundCallableTarget, BoundExpression, BoundExpressionId, BoundOperator, BoundReferenceTarget,
    BoundStructuredExpressionKind, BoundUnit, BoundUnitKey, CheckedExpressionTypes,
    CheckedLiteralValues, CheckedSemanticSelections, CheckedTemplateInputId, CheckedTemplateKind,
    CheckedTemplateNodeId, CheckedTemplateShortCircuitKind, SelectedArgument, SelectedOperation,
    SemanticSelection,
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
    inputs: Vec<SourceTemplateInput>,
    dependency: bray_symbols::DependencyContractTemplateId,
) -> Result<InterfaceCheckedTemplate, PackageInterfaceExportError> {
    let cancellation = &compilation.state.cancellation;

    let bound = compilation
        .bound_unit_with_cancellation(key.clone(), cancellation)
        .map_err(|_| incomplete())?;

    let semantics = compilation
        .expression_semantics_with_cancellation(key.clone(), cancellation)
        .map_err(|_| incomplete())?;

    let body = compilation
        .body_behavior_with_cancellation(key, cancellation)
        .map_err(|_| incomplete())?;

    if bound.result().diagnostics().has_errors()
        || semantics.result().diagnostics().has_errors()
        || body.result().diagnostics().has_errors()
        || body.result().value().is_recovered()
    {
        return Err(incomplete());
    }

    let behavior = body.result().value();

    let execution_requirements = behavior
        .execution_requirements()
        .iter()
        .map(|requirement| export.symbol_reference(requirement.declaration()))
        .collect::<Result<Vec<_>, _>>()?;

    let interface_behavior = InterfaceCheckedTemplateBehavior::new(
        behavior
            .effects()
            .iter()
            .map(|requirement| export.symbol_reference(requirement.declaration()))
            .collect::<Result<Vec<_>, _>>()?,
        behavior
            .capabilities()
            .iter()
            .map(|requirement| export.symbol_reference(requirement.declaration()))
            .collect::<Result<Vec<_>, _>>()?,
        behavior
            .trusted_capabilities()
            .iter()
            .copied()
            .map(Into::into)
            .map(|requirement| export.symbol_reference(requirement))
            .collect::<Result<Vec<_>, _>>()?,
        bray_package_interface::InterfaceCheckedTemplateExecution::new(
            execution_requirements,
            behavior.current_run_cancellation(),
        ),
        behavior.lifecycle_obligations().iter().copied(),
        export.dependency_contract_id(dependency)?,
        [],
    );

    let (types, selections, literals) = semantics.result().value();

    export_source_template(
        export,
        SourceTemplateRequest {
            kind,
            unit: bound.result().value(),
            types,
            selections,
            literals,
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

pub(super) fn export_source_template<'values, 'unit>(
    export: &mut SemanticExporter<'values>,
    request: SourceTemplateRequest<'unit>,
) -> Result<InterfaceCheckedTemplate, PackageInterfaceExportError> {
    let root = request
        .unit
        .tree()
        .expressions()
        .find_map(|(id, expression)| {
            let source = expression.origin().source_anchor().syntax();

            (source.source_id() == request.expression.source_id()
                && source.full_range() == request.expression.full_range())
            .then_some(id)
        })
        .ok_or_else(incomplete)?;

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
                    return Err(incomplete());
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
            .ok_or_else(incomplete)?;

        if expression.is_recovered() {
            return Err(incomplete());
        }

        let ty = self
            .types
            .expression(expression_id)
            .filter(|result| !result.is_recovered())
            .map(|result| result.ty())
            .ok_or_else(incomplete)?;

        let operation = self.operation(expression_id, expression, ty)?;
        let node = CheckedTemplateNodeId::new(index(self.nodes.len())?);

        self.nodes.push(InterfaceCheckedTemplateNode::new(
            operation,
            self.export.type_id(ty)?,
        ));

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
            BoundExpression::Name(reference) => self.reference(reference.target(), ty),
            BoundExpression::PatternReference(_) => {
                let Some(SemanticSelection::Reference(target)) =
                    self.selections.expression(expression_id)
                else {
                    return Err(incomplete());
                };

                self.reference(*target, ty)
            }
            BoundExpression::Unary(unary) => self.unary(unary.operator(), unary.operands()),
            BoundExpression::Binary(binary) => self.binary(binary.operator(), binary.operands()),
            BoundExpression::Call(_) => self.call(expression_id),
            BoundExpression::Conversion(conversion) => {
                let value = self.expression(conversion.operand())?;

                Ok(InterfaceCheckedTemplateOperation::Convert {
                    value,
                    target: self
                        .export
                        .type_id(conversion.target_type().ok_or_else(incomplete)?)?,
                })
            }
            BoundExpression::Structured(expression) => match expression.kind() {
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
                BoundStructuredExpressionKind::TrustBoundary => {
                    let [operand] = expression.operands() else {
                        return Err(incomplete());
                    };

                    let node = self.expression(*operand)?;
                    let index = usize::try_from(node.raw()).map_err(|_| incomplete())?;

                    let operation = self
                        .nodes
                        .get(index)
                        .map(InterfaceCheckedTemplateNode::operation)
                        .cloned()
                        .ok_or_else(incomplete)?;

                    Ok(operation)
                }
                _ => Err(incomplete()),
            },
            BoundExpression::MemberAccess(member) => {
                let Some(SemanticSelection::Operation(SelectedOperation::Member(target))) =
                    self.selections.expression(expression_id)
                else {
                    return Err(incomplete());
                };

                Ok(InterfaceCheckedTemplateOperation::Project {
                    subject: self.expression(member.receiver())?,
                    member: InterfaceTemplateReference::Symbol(
                        self.export.symbol_reference(target.member())?,
                    ),
                })
            }
            _ => Err(incomplete()),
        }
    }

    fn literal(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
        let value = self
            .literals
            .expression(expression)
            .ok_or_else(incomplete)?;

        Ok(InterfaceCheckedTemplateOperation::Constant {
            term: self.export.constant_value_term_id(value)?,
            usage: Default::default(),
        })
    }

    fn reference(
        &mut self,
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
                return Err(incomplete());
            };

            let input = CheckedTemplateInputId::new(index(self.inputs.len())?);

            self.inputs.push(InterfaceCheckedTemplateInput::new(
                InterfaceCheckedTemplateInputKind::PostconditionResult,
                self.export.type_id(ty)?,
            ));

            self.input_ids.insert(target, input);

            return Ok(InterfaceCheckedTemplateOperation::Input(input));
        };

        Ok(InterfaceCheckedTemplateOperation::Declaration(
            InterfaceTemplateReference::Symbol(self.export.symbol_reference(symbol)?),
        ))
    }

    fn unary(
        &mut self,
        operator: BoundOperator,
        operands: &[BoundExpressionId],
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
        let [operand] = operands else {
            return Err(incomplete());
        };

        Ok(InterfaceCheckedTemplateOperation::Unary {
            operation: unary_operator(operator).ok_or_else(incomplete)?,
            operand: self.expression(*operand)?,
        })
    }

    fn binary(
        &mut self,
        operator: BoundOperator,
        operands: &[BoundExpressionId],
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
        let [left, right] = operands else {
            return Err(incomplete());
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
                operation: binary_operator(operator).ok_or_else(incomplete)?,
                left,
                right,
            }),
        }
    }

    fn call(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<InterfaceCheckedTemplateOperation, PackageInterfaceExportError> {
        let Some(SemanticSelection::Call(call)) = self.selections.expression(expression) else {
            return Err(incomplete());
        };

        let (declaration, substitution) = match call.target() {
            BoundCallableTarget::Declaration(callable) => {
                (callable.definition().symbol(), callable.substitution())
            }
            BoundCallableTarget::Predicate(predicate) => {
                (predicate.definition().into_any(), predicate.substitution())
            }
            BoundCallableTarget::Anonymous(_) | BoundCallableTarget::Indirect(_) => {
                return Err(incomplete());
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
                return Err(incomplete());
            };

            arguments.push(self.expression(*expression)?);
        }

        let implementation = match call.witnesses() {
            [] => None,
            [witness] => Some(
                self.export
                    .implementation_template_reference(witness.witness())?,
            ),
            _ => return Err(incomplete()),
        };

        Ok(InterfaceCheckedTemplateOperation::call(
            callable,
            substitution,
            arguments,
            implementation,
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
    u32::try_from(length).map_err(|_| incomplete())
}

const fn incomplete() -> PackageInterfaceExportError {
    PackageInterfaceExportError::InvalidCompilation
}
