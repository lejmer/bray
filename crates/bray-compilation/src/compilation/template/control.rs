use bray_binder::BindingQueryContext;

use super::build::SourceTemplateBuilder;
use crate::fact::FactQueryError;
use bray_bound_tree::{
    BoundBlockId, BoundExpression, BoundExpressionId, BoundStructuredExpression,
    CheckedTemplateNode, CheckedTemplateNodeId, CheckedTemplateOperation,
    CheckedTemplateShortCircuitKind,
};
use bray_symbols::TypeId;

/// Normal execution remaining after the returns encountered in source order.
#[derive(Clone, Copy)]
pub(super) enum TemplatePath {
    Always,
    When(CheckedTemplateNodeId),
    Unreachable,
}

pub(super) struct TemplateBoundary {
    checkpoint: usize,
    pub(super) exits: Vec<TemplateExit>,
}

impl TemplateBoundary {
    pub(super) fn new(checkpoint: usize) -> Self {
        Self {
            checkpoint,
            exits: Vec::new(),
        }
    }
}

pub(super) struct TemplateExit {
    path: TemplatePath,
    pub(super) value: CheckedTemplateNodeId,
    updates: std::collections::BTreeMap<bray_bound_tree::StorageIdentityId, CheckedTemplateNodeId>,
}

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(super) fn push_node(
        &mut self,
        operation: CheckedTemplateOperation,
        ty: TypeId,
    ) -> CheckedTemplateNodeId {
        if let CheckedTemplateOperation::Convert { value, target } = &operation
            && self.nodes[usize::try_from(value.raw()).expect("checked conversion input fits host")]
                .ty()
                == *target
        {
            return *value;
        }

        let id = CheckedTemplateNodeId::new(super::build::index(self.nodes.len()));

        self.nodes.push(CheckedTemplateNode::new(operation, ty));

        id
    }

    fn sequence(&mut self, value: CheckedTemplateNodeId) -> CheckedTemplateNodeId {
        let Some(before) = self.evaluation else {
            return value;
        };

        let node =
            &self.nodes[usize::try_from(value.raw()).expect("checked sequence value fits host")];

        if before == value || matches!(node.operation(), CheckedTemplateOperation::Unreachable) {
            return value;
        }

        let ty = node.ty();

        self.push_node(CheckedTemplateOperation::Sequence { before, value }, ty)
    }

    pub(super) fn record_evaluation(
        &mut self,
        value: CheckedTemplateNodeId,
    ) -> CheckedTemplateNodeId {
        let value = self.sequence(value);

        self.evaluation = Some(value);

        value
    }

    pub(super) fn unreachable_node(&mut self, ty: TypeId) -> CheckedTemplateNodeId {
        self.push_node(CheckedTemplateOperation::Unreachable, ty)
    }

    pub(super) fn record_exit(
        &mut self,
        boundary: bray_bound_tree::SelectedPropagationBoundary,
        value: CheckedTemplateNodeId,
        ty: TypeId,
    ) {
        let value =
            if self.nodes[usize::try_from(value.raw()).expect("checked exit value fits host")].ty()
                == ty
            {
                value
            } else {
                self.push_node(CheckedTemplateOperation::Convert { value, target: ty }, ty)
            };

        let value = self.sequence(value);

        let checkpoint = self
            .exits
            .get(&boundary)
            .expect("source exit has its owning boundary")
            .checkpoint;

        let updates = self.retained_updates(checkpoint);

        self.exits
            .get_mut(&boundary)
            .expect("source exit has its owning boundary")
            .exits
            .push(TemplateExit {
                path: self.path,
                value,
                updates,
            });

        self.path = TemplatePath::Unreachable;
    }

    pub(super) fn finish_exit(
        &mut self,
        boundary: bray_bound_tree::SelectedPropagationBoundary,
        mut value: CheckedTemplateNodeId,
        ty: TypeId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let boundary = self
            .exits
            .remove(&boundary)
            .expect("completed region has its owning boundary");

        for TemplateExit {
            path,
            value: exited,
            updates,
        } in boundary.exits.into_iter().rev()
        {
            match path {
                TemplatePath::Always => {
                    value = exited;

                    for (root, value) in updates {
                        self.assign_state(root, value);
                    }
                }
                TemplatePath::When(condition) => {
                    value = self.commit_choice(
                        condition,
                        exited,
                        value,
                        ty,
                        updates,
                        Default::default(),
                    );
                }
                TemplatePath::Unreachable => {}
            }

            self.path = self.merge_paths(self.path, path)?;
        }

        Ok(value)
    }

    pub(super) fn propagation(
        &mut self,
        id: BoundExpressionId,
        expression: &BoundStructuredExpression,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let Some(bray_bound_tree::SemanticSelection::Propagation(selection)) =
            self.selections.expression(id)
        else {
            panic!("checked propagation retains its selected result boundary");
        };

        let [operand] = expression.operands() else {
            panic!("checked propagation has one operand");
        };

        let subject = self.expression(*operand)?;

        let boolean = self
            .context
            .compilation()
            .target_property_type(bray_target::TargetPropertyKind::ScalarBool)?;

        let (condition, exit, continuation) = match selection {
            bray_bound_tree::SelectedPropagation::Nullable { result_type, .. } => {
                let subject_type = self
                    .types
                    .expression(*operand)
                    .expect("checked propagation operand retains its type")
                    .ty();

                let operation = self.nullable_test(subject, subject_type, true)?;
                let condition = self.push_node(operation, boolean);
                let term = self.nullable_absence_term(*result_type)?;

                let exit = self.push_node(
                    CheckedTemplateOperation::Constant {
                        term,
                        usage: Default::default(),
                    },
                    *result_type,
                );

                (
                    condition,
                    exit,
                    CheckedTemplateOperation::NullableValue { subject },
                )
            }
            bray_bound_tree::SelectedPropagation::Result {
                result_type,
                error_conversion,
                error,
                success,
                ..
            } => {
                let condition = self.push_node(
                    CheckedTemplateOperation::VariantTest {
                        subject,
                        variant: self.symbol_key(success.0.into())?,
                    },
                    boolean,
                );

                let error_value = self.push_node(
                    CheckedTemplateOperation::Project {
                        subject,
                        member: self.symbol_key(error.1.into())?,
                    },
                    error_conversion.source_type(),
                );

                let converted = self.conversion_operation(error_value, error_conversion)?;
                let error_value = self.push_node(converted, error_conversion.target_type());

                let exit = self.push_node(
                    CheckedTemplateOperation::construct(
                        Some(self.symbol_key(error.0.into())?),
                        [bray_symbols::ConstantField::new(
                            self.symbol_key(error.1.into())?,
                            error_value,
                        )],
                    ),
                    *result_type,
                );

                (
                    condition,
                    exit,
                    CheckedTemplateOperation::Project {
                        subject,
                        member: self.symbol_key(success.1.into())?,
                    },
                )
            }
            bray_bound_tree::SelectedPropagation::CurrentRun { .. } => {
                panic!("run propagation is excluded from constant templates")
            }
        };

        let path = self.path;

        self.path = self.refine_path(path, condition, false)?;

        self.record_exit(
            selection
                .boundary()
                .expect("constant propagation has a value boundary"),
            exit,
            selection
                .result_type()
                .expect("constant propagation boundary retains its result type"),
        );

        self.path = self.refine_path(path, condition, true)?;

        Ok(continuation)
    }

    pub(super) fn callable_body(
        &mut self,
        block: BoundBlockId,
        result: TypeId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        self.return_type = Some(result);

        let value = self.block_value(block, result)?;

        self.finish_exit(
            bray_bound_tree::SelectedPropagationBoundary::Callable,
            value,
            result,
        )
    }

    pub(super) fn block_value(
        &mut self,
        block: BoundBlockId,
        ty: TypeId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        if self
            .execution
            .is_some_and(|execution| !execution.is_reachable(block.into()))
        {
            self.path = TemplatePath::Unreachable;

            return Ok(self.unreachable_node(ty));
        }

        let block = self
            .unit
            .view()
            .block(block)
            .expect("checked template block exists");

        let boundary = bray_bound_tree::SelectedPropagationBoundary::YieldRegion(
            block.origin().source_anchor().syntax(),
        );

        assert!(
            self.exits
                .insert(boundary, TemplateBoundary::new(self.state_changes.len()))
                .is_none(),
            "source block has one active exit boundary"
        );

        for item in block.items() {
            if matches!(self.path, TemplatePath::Unreachable) {
                break;
            }

            match item {
                bray_bound_tree::BoundBlockItem::LocalConstant(constant) => {
                    if self.is_callable_value(constant.initializer()) {
                        continue;
                    }

                    let value = self.expression(constant.initializer())?;
                    let value = self.record_evaluation(value);

                    let symbol = constant
                        .symbol()
                        .expect("checked local constant has a symbol");

                    self.bindings.insert(
                        bray_bound_tree::BoundReferenceTarget::Local(symbol.into()),
                        value,
                    );
                }
                bray_bound_tree::BoundBlockItem::LocalBinding(binding) => {
                    if self.is_callable_value(binding.initializer()) {
                        continue;
                    }

                    if let Some(producer) =
                        self.generator_observations.producer(binding.initializer())
                    {
                        self.generator_summary(producer)?;
                        continue;
                    }

                    let value = self.expression(binding.initializer())?;
                    let value = self.record_evaluation(value);

                    self.bind_pattern_value(binding.pattern(), value)?;
                }
                bray_bound_tree::BoundBlockItem::Expression(id) => {
                    if let Some(producer) = self.generator_observations.producer(*id) {
                        self.generator_summary(producer)?;
                        continue;
                    }

                    let expression = self
                        .unit
                        .view()
                        .expression(*id)
                        .expect("checked block item exists");

                    if let BoundExpression::ControlTransfer(transfer) = expression {
                        if transfer.kind() == bray_bound_tree::BoundControlTransferKind::Yield
                            && let Some(root) = transfer
                                .target()
                                .and_then(|target| self.generator_targets.get(&target))
                                .copied()
                        {
                            self.generator_yield(
                                root,
                                transfer
                                    .operand()
                                    .expect("Boolean generator yield retains its value"),
                            )?;

                            continue;
                        }

                        match transfer.kind() {
                            bray_bound_tree::BoundControlTransferKind::Yield
                            | bray_bound_tree::BoundControlTransferKind::Return => {
                                let (boundary, result) = if transfer.kind()
                                    == bray_bound_tree::BoundControlTransferKind::Return
                                {
                                    (
                                        bray_bound_tree::SelectedPropagationBoundary::Callable,
                                        self.return_type
                                            .expect("checked return belongs to its callable body"),
                                    )
                                } else {
                                    (
                                        bray_bound_tree::SelectedPropagationBoundary::YieldRegion(
                                            transfer
                                                .target()
                                                .expect("checked yield has a selected region"),
                                        ),
                                        ty,
                                    )
                                };

                                let value = transfer
                                    .operand()
                                    .map(|operand| self.expression(operand))
                                    .transpose()?
                                    .unwrap_or_else(|| self.unit_node(result));

                                self.record_exit(boundary, value, result);
                                break;
                            }
                            _ => panic!(
                                "admitted constant body contains an unsupported control transfer"
                            ),
                        }
                    }

                    let value = self.expression(*id)?;

                    self.record_evaluation(value);
                }
            }
        }

        let value = if matches!(self.path, TemplatePath::Unreachable) || !self.is_unit(ty) {
            self.unreachable_node(ty)
        } else {
            self.unit_node(ty)
        };

        let value = self.sequence(value);

        self.finish_exit(boundary, value, ty)
    }

    pub(super) fn conditional(
        &mut self,
        expression: &BoundStructuredExpression,
        ty: TypeId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let evaluation = self.evaluation;
        let value = self.conditional_case(expression, 0, ty)?;

        self.evaluation = evaluation;

        Ok(CheckedTemplateOperation::Convert { value, target: ty })
    }

    fn conditional_case(
        &mut self,
        expression: &BoundStructuredExpression,
        index: usize,
        ty: TypeId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let Some(condition) = expression.operands().get(index) else {
            return match expression.blocks().get(index) {
                Some(block) => self.block_value(*block, ty),
                None => Ok(self.unit_node(ty)),
            };
        };

        let condition = self.expression(*condition)?;
        let condition = self.record_evaluation(condition);
        let remaining = self.path;
        let evaluation = self.evaluation;
        let checkpoint = self.state_changes.len();

        self.path = self.refine_path(remaining, condition, true)?;

        let when_true = self.block_value(expression.blocks()[index], ty)?;
        let true_path = self.path;
        let true_bindings = self.take_state_updates(checkpoint);

        self.path = self.refine_path(remaining, condition, false)?;
        self.evaluation = evaluation;

        let when_false = self.conditional_case(expression, index + 1, ty)?;
        let false_bindings = self.take_state_updates(checkpoint);

        self.path = self.merge_paths(true_path, self.path)?;
        self.evaluation = evaluation;

        Ok(self.commit_choice(
            condition,
            when_true,
            when_false,
            ty,
            true_bindings.outer,
            false_bindings.outer,
        ))
    }

    pub(super) fn match_value(
        &mut self,
        id: BoundExpressionId,
        expression: &bray_bound_tree::BoundMatchExpression,
        ty: TypeId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let evaluation = self.evaluation;
        let subject = self.expression(expression.subject())?;
        let subject = self.record_evaluation(subject);

        let exhaustive = self
            .patterns
            .match_coverage(id)
            .is_some_and(|coverage| coverage.is_exhaustive() && !coverage.is_recovered());

        let value = self.match_case(expression.arms(), subject, exhaustive, ty)?;

        self.evaluation = evaluation;

        Ok(CheckedTemplateOperation::Convert { value, target: ty })
    }

    fn match_case(
        &mut self,
        arms: &[bray_bound_tree::BoundMatchArm],
        subject: CheckedTemplateNodeId,
        exhaustive: bool,
        ty: TypeId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let Some((arm, remaining_arms)) = arms.split_first() else {
            if exhaustive {
                self.path = TemplatePath::Unreachable;

                return Ok(self.unreachable_node(ty));
            }

            return Ok(self.unit_node(ty));
        };

        let remaining = self.path;
        let arm_checkpoint = self.state_changes.len();
        let pattern = self.pattern_condition(arm.pattern(), subject)?;

        self.path = self.refine_path(remaining, pattern, true)?;
        self.bind_pattern_value(arm.pattern(), subject)?;

        let mut condition = pattern;

        if let Some(guard) = arm.guard() {
            let evaluation = self.evaluation;
            let guard = self.expression(guard)?;

            self.evaluation = evaluation;

            let updates = self.take_state_updates(arm_checkpoint);
            let mut active = updates.outer;

            active.extend(updates.local);

            let boolean = self.nodes
                [usize::try_from(pattern.raw()).expect("checked condition fits host")]
            .ty();

            let absent = self.boolean_constant(false)?;

            condition =
                self.commit_choice(pattern, guard, absent, boolean, active, Default::default());
        }

        let condition = self.record_evaluation(condition);
        let evaluation = self.evaluation;
        let checkpoint = arm_checkpoint;

        self.path = self.refine_path(remaining, condition, true)?;

        let when_true = self.block_value(arm.body(), ty)?;
        let true_path = self.path;
        let true_bindings = self.take_state_updates(checkpoint);

        self.path = self.refine_path(remaining, condition, false)?;
        self.evaluation = evaluation;

        let when_false = self.match_case(remaining_arms, subject, exhaustive, ty)?;
        let false_bindings = self.take_state_updates(checkpoint);

        self.path = self.merge_paths(true_path, self.path)?;
        self.evaluation = evaluation;

        Ok(self.commit_choice(
            condition,
            when_true,
            when_false,
            ty,
            true_bindings.outer,
            false_bindings.outer,
        ))
    }

    fn is_callable_value(&self, expression: BoundExpressionId) -> bool {
        self.types.expression(expression).is_some_and(|ty| {
            matches!(
                self.context.semantic_values().type_data(ty.ty()).as_ref(),
                bray_symbols::TypeData::Callable(_)
            )
        })
    }

    fn is_unit(&self, ty: TypeId) -> bool {
        self.context
            .compilation()
            .available_compiler_known_symbols()
            .type_representation(self.context.semantic_values(), ty)
            == Some(bray_compiler_known::RepresentationRole::Unit)
    }

    fn boolean_operation(
        &mut self,
        kind: CheckedTemplateShortCircuitKind,
        left: CheckedTemplateNodeId,
        right: CheckedTemplateNodeId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let ty = self
            .context
            .compilation()
            .target_property_type(bray_target::TargetPropertyKind::ScalarBool)?;

        Ok(self.push_node(
            CheckedTemplateOperation::ShortCircuit { kind, left, right },
            ty,
        ))
    }

    pub(super) fn refine_path(
        &mut self,
        path: TemplatePath,
        condition: CheckedTemplateNodeId,
        holds: bool,
    ) -> Result<TemplatePath, FactQueryError> {
        if matches!(path, TemplatePath::Unreachable) {
            return Ok(path);
        }

        let condition = if holds {
            condition
        } else {
            let ty = self
                .context
                .compilation()
                .target_property_type(bray_target::TargetPropertyKind::ScalarBool)?;

            self.push_node(
                CheckedTemplateOperation::Unary {
                    operation: bray_symbols::ConstantUnaryOperation::LogicalNot,
                    operand: condition,
                },
                ty,
            )
        };

        Ok(match path {
            TemplatePath::Always => TemplatePath::When(condition),
            TemplatePath::When(previous) => TemplatePath::When(self.boolean_operation(
                CheckedTemplateShortCircuitKind::And,
                previous,
                condition,
            )?),
            TemplatePath::Unreachable => {
                unreachable!("unreachable paths were excluded before refining")
            }
        })
    }

    pub(super) fn merge_paths(
        &mut self,
        left: TemplatePath,
        right: TemplatePath,
    ) -> Result<TemplatePath, FactQueryError> {
        Ok(match (left, right) {
            (TemplatePath::Always, _) | (_, TemplatePath::Always) => TemplatePath::Always,
            (TemplatePath::Unreachable, other) | (other, TemplatePath::Unreachable) => other,
            (TemplatePath::When(left), TemplatePath::When(right)) => TemplatePath::When(
                self.boolean_operation(CheckedTemplateShortCircuitKind::Or, left, right)?,
            ),
        })
    }

    pub(super) fn select_value(
        &mut self,
        condition: CheckedTemplateNodeId,
        when_true: CheckedTemplateNodeId,
        when_false: CheckedTemplateNodeId,
        ty: TypeId,
    ) -> CheckedTemplateNodeId {
        if when_true == when_false
            || [when_true, when_false].iter().all(|node| {
                self.nodes[usize::try_from(node.raw()).expect("checked template node fits host")]
                    .operation()
                    == &CheckedTemplateOperation::Unreachable
            })
        {
            return self.push_node(
                CheckedTemplateOperation::Sequence {
                    before: condition,
                    value: when_true,
                },
                ty,
            );
        }

        self.push_node(
            CheckedTemplateOperation::Conditional {
                condition,
                when_true,
                when_false,
                state_outputs: Default::default(),
            },
            ty,
        )
    }

    pub(super) fn boolean_constant(
        &mut self,
        value: bool,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let ty = self
            .context
            .compilation()
            .target_property_type(bray_target::TargetPropertyKind::ScalarBool)?;

        let value = self.context.semantic_values().intern_constant_value(
            bray_symbols::ConstantValueData::new(
                ty,
                bray_symbols::ConstantValueKind::Boolean(value),
            ),
        )?;

        let term = self
            .context
            .semantic_values()
            .intern_constant_term(bray_symbols::ConstantTermData::Value(value))?;

        Ok(self.push_node(
            CheckedTemplateOperation::Constant {
                term,
                usage: Default::default(),
            },
            ty,
        ))
    }

    pub(super) fn unit_node(&mut self, ty: TypeId) -> CheckedTemplateNodeId {
        assert!(
            self.is_unit(ty),
            "checked implicit unit completion has unit type"
        );

        let values = self.context.semantic_values();

        let value = values
            .intern_constant_value(bray_symbols::ConstantValueData::new(
                ty,
                bray_symbols::ConstantValueKind::Unit,
            ))
            .expect("unit completion satisfies its checked representation");

        let term = values
            .intern_constant_term(bray_symbols::ConstantTermData::Value(value))
            .expect("unit completion belongs to this store");

        self.push_node(
            CheckedTemplateOperation::Constant {
                term,
                usage: Default::default(),
            },
            ty,
        )
    }
}
