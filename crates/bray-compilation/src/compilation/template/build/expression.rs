use crate::fact::FactQueryError;
use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    BoundExpression, BoundExpressionId, BoundReferenceTarget, BoundStructuredExpressionKind,
    CheckedTemplateInput, CheckedTemplateNode, CheckedTemplateNodeId, CheckedTemplateOperation,
    SelectedOperation, SemanticSelection,
};
use bray_symbols::{ConstantBinaryOperation, TypeId};

use super::context::{SourceTemplateBuilder, incomplete, index};

impl<'context, 'compilation, 'unit> SourceTemplateBuilder<'context, 'compilation, 'unit> {
    pub(super) fn pattern_test(
        &mut self,
        expression: &bray_bound_tree::BoundStructuredExpression,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let [subject] = expression.operands() else {
            panic!("checked pattern test has one subject");
        };

        let [pattern] = expression.patterns() else {
            panic!("checked pattern test has one pattern");
        };

        let subject = self.expression(*subject)?;

        self.bind_pattern_value(*pattern, subject)?;

        let value = self.pattern_condition(*pattern, subject)?;

        let target = self.nodes
            [usize::try_from(value.raw()).expect("checked pattern condition fits host")]
        .ty();

        Ok(CheckedTemplateOperation::Convert { value, target })
    }

    pub(in crate::compilation::template) fn nullable_test(
        &mut self,
        subject: CheckedTemplateNodeId,
        subject_type: TypeId,
        present: bool,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let subject_type = self.context.semantic_values().unborrowed_type(subject_type);
        let term = self.nullable_absence_term(subject_type)?;

        let right = self.push_node(
            CheckedTemplateOperation::Constant {
                term,
                usage: Default::default(),
            },
            subject_type,
        );

        Ok(CheckedTemplateOperation::Binary {
            operation: if present {
                ConstantBinaryOperation::NotEqual
            } else {
                ConstantBinaryOperation::Equal
            },
            left: subject,
            right,
        })
    }

    pub(in crate::compilation) fn expression(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        if let Some((_, Some(value))) = self.expression_nodes.get(&expression) {
            return Ok(*value);
        }

        let place = self.place_expression(expression)?;
        let value = self.snapshot(place);

        self.expression_nodes
            .insert(expression, (place, Some(value)));

        Ok(value)
    }

    pub(in crate::compilation::template) fn snapshot(
        &mut self,
        value: CheckedTemplateNodeId,
    ) -> CheckedTemplateNodeId {
        let node = &self.nodes[usize::try_from(value.raw()).expect("checked value fits host")];

        if matches!(
            node.operation(),
            CheckedTemplateOperation::Input(_)
                | CheckedTemplateOperation::Project { .. }
                | CheckedTemplateOperation::TupleElement { .. }
                | CheckedTemplateOperation::NullableValue { .. }
                | CheckedTemplateOperation::Index { .. }
                | CheckedTemplateOperation::Convert { .. }
                | CheckedTemplateOperation::Declaration { .. }
                | CheckedTemplateOperation::Temporary(_)
                | CheckedTemplateOperation::Update { .. }
                | CheckedTemplateOperation::StateAfter { .. }
        ) {
            self.push_node(CheckedTemplateOperation::Snapshot { value }, node.ty())
        } else {
            value
        }
    }

    pub(in crate::compilation::template) fn place_expression(
        &mut self,
        expression_id: BoundExpressionId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        if self
            .execution
            .is_some_and(|execution| !execution.is_reachable(expression_id.into()))
        {
            let ty = self
                .types
                .expression(expression_id)
                .expect("excluded checked expression retains its type")
                .ty();

            return Ok(self.unreachable_node(ty));
        }

        // Consume the owning alias analysis for value reads. An alias binding's initializer
        // is not a new snapshot of the storage reached by subsequent accesses.
        if self.types.expression(expression_id).is_some_and(|ty| {
            !matches!(
                self.context.semantic_values().type_data(ty.ty()).as_ref(),
                bray_symbols::TypeData::Borrow { .. }
            )
        }) {
            let path = self
                .storage
                .node_plans(expression_id.into())
                .filter(|plan| plan.purpose() == bray_bound_tree::StorageAccessPurpose::Read)
                .find_map(|plan| {
                    self.flow
                        .access_target(plan)
                        .and_then(|target| target.inline_state_path())
                });

            if let Some(path) = path
                && let Some(root) = self.current_state(path.root)
            {
                let value = self.project_path(root, &path.projections)?;

                return self.bounded_integer_node(expression_id, value);
            }
        }

        let target = match self.unit.view().expression(expression_id) {
            Some(BoundExpression::Name(name)) => Some(name.target()),
            Some(BoundExpression::PatternReference(reference)) => {
                Some(BoundReferenceTarget::Local(reference.binding().into()))
            }
            _ => None,
        };

        if let Some(node) = target.and_then(|target| self.reference_value(target)) {
            return self.bounded_integer_node(expression_id, node);
        }

        if let Some((node, _)) = self.expression_nodes.get(&expression_id) {
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

        let node_ty = match &operation {
            CheckedTemplateOperation::Input(input) => self
                .inputs
                .get(
                    usize::try_from(input.raw())
                        .map_err(|_| incomplete("template input index does not fit the host"))?,
                )
                .map(CheckedTemplateInput::ty)
                .ok_or_else(|| incomplete("missing_template_input"))?,
            CheckedTemplateOperation::Sequence { value, .. } => self.nodes
                [usize::try_from(value.raw()).expect("checked sequence value fits host")]
            .ty(),
            _ => ty,
        };

        let node = self.push_node(operation, node_ty);

        let node = self.bounded_integer_node(expression_id, node)?;

        self.expression_nodes.insert(expression_id, (node, None));

        Ok(node)
    }

    pub(in crate::compilation::template) fn converted_expression(
        &mut self,
        expression: BoundExpressionId,
        conversion: &bray_bound_tree::SelectedConversion,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let value = self.expression(expression)?;

        let target = conversion.target_type();
        let operation = self.conversion_operation(value, conversion)?;
        let node = CheckedTemplateNodeId::new(index(self.nodes.len()));

        self.nodes.push(CheckedTemplateNode::new(operation, target));

        Ok(node)
    }

    pub(super) fn bounded_integer_node(
        &mut self,
        expression: BoundExpressionId,
        node: CheckedTemplateNodeId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let Some(maximum) = self
            .execution
            .and_then(|execution| execution.integer_maxima().get(&expression))
        else {
            return Ok(node);
        };

        let bits = u16::try_from((u64::BITS - maximum.leading_zeros()).max(1))
            .expect("u64 upper bound has at most 64 bits");

        let bits = std::num::NonZeroU8::new(u8::try_from(bits).expect("u64 width fits u8"))
            .expect("integer bound has a nonzero width");

        let ty = self.nodes[usize::try_from(node.raw()).expect("checked node fits host")].ty();
        let bounded = CheckedTemplateNodeId::new(index(self.nodes.len()));

        self.nodes.push(
            CheckedTemplateNode::new(
                CheckedTemplateOperation::Convert {
                    value: node,
                    target: ty,
                },
                ty,
            )
            .with_unsigned_integer_bits(Some(bits)),
        );

        Ok(bounded)
    }

    pub(in crate::compilation::template) fn operation(
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
            BoundExpression::StructConstruction(_) => {
                let Some(SemanticSelection::Operation(SelectedOperation::Construction(
                    construction,
                ))) = self.selections.expression(expression_id)
                else {
                    panic!("checked aggregate construction retains its selection");
                };

                self.construction(construction)
            }
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
            BoundExpression::Block(block) => {
                let value = self.block_value(block.block(), ty)?;

                Ok(CheckedTemplateOperation::Convert { value, target: ty })
            }
            BoundExpression::For(iteration) => self.for_loop(expression_id, iteration, ty),
            BoundExpression::Match(matched) => self.match_value(expression_id, matched, ty),
            BoundExpression::Assignment(assignment) => {
                self.assignment(expression_id, assignment, ty)
            }
            BoundExpression::Call(_) => self.call(expression_id),
            BoundExpression::Conversion(conversion) => {
                let value = self.expression(conversion.operand())?;

                let Some(SemanticSelection::Operation(SelectedOperation::Conversion(selected))) =
                    self.selections.expression(expression_id)
                else {
                    return Err(incomplete("missing_conversion_selection"));
                };

                self.conversion_operation(value, selected)
            }
            BoundExpression::Structured(expression) => match expression.kind() {
                BoundStructuredExpressionKind::BooleanAllFold
                | BoundStructuredExpressionKind::BooleanAnyFold => {
                    self.boolean_fold(expression_id, expression, ty)
                }
                BoundStructuredExpressionKind::While => self.while_loop(expression, ty),
                BoundStructuredExpressionKind::Conditional => self.conditional(expression, ty),
                BoundStructuredExpressionKind::NullablePropagation
                | BoundStructuredExpressionKind::ResultPropagation => {
                    self.propagation(expression_id, expression)
                }
                BoundStructuredExpressionKind::Unit => {
                    let value = self.unit_node(ty);

                    Ok(CheckedTemplateOperation::Convert { value, target: ty })
                }
                BoundStructuredExpressionKind::Condition => {
                    let [condition] = expression.operands() else {
                        panic!("checked condition has one Boolean operand");
                    };

                    let node = self.expression(*condition)?;

                    Ok(CheckedTemplateOperation::Sequence {
                        before: node,
                        value: node,
                    })
                }
                BoundStructuredExpressionKind::Absence => Ok(CheckedTemplateOperation::Constant {
                    term: self.nullable_absence_term(ty)?,
                    usage: Default::default(),
                }),
                BoundStructuredExpressionKind::PatternTest
                | BoundStructuredExpressionKind::PatternBinding => self.pattern_test(expression),
                BoundStructuredExpressionKind::Tuple | BoundStructuredExpressionKind::Range => Ok(
                    CheckedTemplateOperation::tuple(self.expressions(expression.operands())?),
                ),
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
                        operand: self.place_expression(*operand)?,
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
                        subject: self.place_expression(member.receiver())?,
                        index: bray_symbols::SymbolOrdinal::new(*index),
                    });
                }

                match self.selections.expression(expression_id) {
                    Some(SemanticSelection::Operation(SelectedOperation::Member(target))) => {
                        Ok(CheckedTemplateOperation::Project {
                            subject: self.place_expression(member.receiver())?,
                            member: self.symbol_key(target.member())?,
                        })
                    }
                    Some(SemanticSelection::Operation(SelectedOperation::Construction(
                        construction,
                    ))) => self.construction(construction),
                    _ => Err(incomplete("invalid_member_access_selection")),
                }
            }
            _ => Err(incomplete("unsupported_template_expression")),
        }
    }
}
