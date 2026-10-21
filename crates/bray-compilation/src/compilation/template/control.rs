use bray_binder::BindingQueryContext;

use super::build::SourceTemplateBuilder;
use crate::fact::FactQueryError;
use bray_bound_tree::{BoundBlockId, BoundExpression, BoundExpressionId, BoundStructuredExpression, CheckedTemplateNode, CheckedTemplateNodeId, CheckedTemplateOperation, CheckedTemplateShortCircuitKind};
use bray_symbols::TypeId;

/// Normal execution remaining after the returns encountered in source order.
#[derive(Clone, Copy)]
pub(super) enum TemplatePath {
    Always,
    When(CheckedTemplateNodeId),
    Unreachable,
}

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(super) fn push_node(&mut self, operation: CheckedTemplateOperation, ty: TypeId) -> CheckedTemplateNodeId {
        let id = CheckedTemplateNodeId::new(super::build::index(self.nodes.len()));
        self.nodes.push(CheckedTemplateNode::new(operation, ty));
        id
    }

    pub(super) fn unreachable_node(&mut self, ty: TypeId) -> CheckedTemplateNodeId {
        self.push_node(CheckedTemplateOperation::Unreachable, ty)
    }

    pub(super) fn callable_body(&mut self, block: BoundBlockId, result: TypeId) -> Result<CheckedTemplateNodeId, FactQueryError> {
        self.return_type = Some(result);
        self.block_value(block, result)?;
        let mut value = if self.is_unit(result) { self.unit_node(result) } else { self.unreachable_node(result) };
        // Each source return contributes one guarded result. Continuations are never rebuilt.
        for (path, returned) in std::mem::take(&mut self.returns).into_iter().rev() {
            value = match path {
                TemplatePath::Always => returned,
                TemplatePath::When(condition) => self.select_value(condition, returned, value, result),
                TemplatePath::Unreachable => value,
            };
        }
        Ok(value)
    }

    pub(super) fn block_value(&mut self, block: BoundBlockId, ty: TypeId) -> Result<CheckedTemplateNodeId, FactQueryError> {
        if self.execution.is_some_and(|execution| !execution.is_reachable(block.into())) {
            self.path = TemplatePath::Unreachable;
            return Ok(self.unreachable_node(ty));
        }
        let block = self.unit.view().block(block).expect("checked template block exists");
        for item in block.items() {
            if matches!(self.path, TemplatePath::Unreachable) { break; }
            match item {
                bray_bound_tree::BoundBlockItem::LocalConstant(constant) => {
                    if self.is_callable_value(constant.initializer()) { continue; }
                    let value = self.expression(constant.initializer())?;
                    let symbol = constant.symbol().expect("checked local constant has a symbol");
                    self.local_values.insert(bray_bound_tree::BoundReferenceTarget::Local(symbol.into()), value);
                }
                bray_bound_tree::BoundBlockItem::LocalBinding(binding) => {
                    if self.is_callable_value(binding.initializer()) { continue; }
                    let value = self.expression(binding.initializer())?;
                    self.bind_pattern_value(binding.pattern(), value)?;
                }
                bray_bound_tree::BoundBlockItem::Expression(id) => {
                    let expression = self.unit.view().expression(*id).expect("checked block item exists");
                    if let BoundExpression::ControlTransfer(transfer) = expression {
                        match transfer.kind() {
                            bray_bound_tree::BoundControlTransferKind::Yield => {
                                return transfer.operand().map(|operand| self.expression(operand))
                                    .unwrap_or_else(|| Ok(self.unit_node(ty)));
                            }
                            bray_bound_tree::BoundControlTransferKind::Return => {
                                let result = self.return_type.expect("checked return belongs to its callable body");
                                let value = transfer.operand().map(|operand| self.expression(operand))
                                    .transpose()?.unwrap_or_else(|| self.unit_node(result));
                                let value = if self.nodes[usize::try_from(value.raw()).expect("checked return value fits host")].ty() == result {
                                    value
                                } else {
                                    self.push_node(CheckedTemplateOperation::Convert { value, target: result }, result)
                                };
                                self.returns.push((self.path, value));
                                self.path = TemplatePath::Unreachable;
                                break;
                            }
                            _ => panic!("admitted constant body contains an unsupported control transfer"),
                        }
                    }
                    self.expression(*id)?;
                }
            }
        }
        Ok(if matches!(self.path, TemplatePath::Unreachable) || !self.is_unit(ty) {
            self.unreachable_node(ty)
        } else { self.unit_node(ty) })
    }

    pub(super) fn conditional(&mut self, expression: &BoundStructuredExpression, ty: TypeId) -> Result<CheckedTemplateOperation, FactQueryError> {
        let mut remaining = self.path;
        let mut normal = TemplatePath::Unreachable;
        let mut branches = Vec::with_capacity(expression.operands().len());
        for (condition, block) in expression.operands().iter().zip(expression.blocks()) {
            self.path = remaining;
            let condition = self.expression(*condition)?;
            remaining = self.path;
            self.path = self.refine_path(remaining, condition, true)?;
            let value = self.block_value(*block, ty)?;
            normal = self.merge_paths(normal, self.path)?;
            remaining = self.refine_path(remaining, condition, false)?;
            branches.push((condition, value));
        }
        self.path = remaining;
        let mut value = match expression.blocks().get(expression.operands().len()) {
            Some(block) => self.block_value(*block, ty)?,
            None => self.unit_node(ty),
        };
        self.path = self.merge_paths(normal, self.path)?;
        for (condition, when_true) in branches.into_iter().rev() {
            value = self.select_value(condition, when_true, value, ty);
        }
        Ok(CheckedTemplateOperation::Convert { value, target: ty })
    }

    pub(super) fn match_value(&mut self, id: BoundExpressionId, expression: &bray_bound_tree::BoundMatchExpression, ty: TypeId) -> Result<CheckedTemplateOperation, FactQueryError> {
        let subject = self.expression(expression.subject())?;
        let mut remaining = self.path;
        let mut normal = TemplatePath::Unreachable;
        let mut branches = Vec::with_capacity(expression.arms().len());
        for arm in expression.arms() {
            self.path = remaining;
            self.bind_pattern_value(arm.pattern(), subject)?;
            let mut condition = self.pattern_condition(arm.pattern(), subject)?;
            self.path = self.refine_path(remaining, condition, true)?;
            if let Some(guard) = arm.guard() {
                let guard = self.expression(guard)?;
                condition = self.boolean_operation(CheckedTemplateShortCircuitKind::And, condition, guard)?;
                self.path = self.refine_path(remaining, condition, true)?;
            }
            let value = self.block_value(arm.body(), ty)?;
            normal = self.merge_paths(normal, self.path)?;
            remaining = self.refine_path(remaining, condition, false)?;
            branches.push((condition, value));
        }
        let exhaustive = self.patterns.match_coverage(id).is_some_and(|coverage| coverage.is_exhaustive() && !coverage.is_recovered());
        let mut value = if exhaustive { self.unreachable_node(ty) } else { self.unit_node(ty) };
        self.path = self.merge_paths(normal, if exhaustive { TemplatePath::Unreachable } else { remaining })?;
        for (condition, when_true) in branches.into_iter().rev() {
            value = self.select_value(condition, when_true, value, ty);
        }
        Ok(CheckedTemplateOperation::Convert { value, target: ty })
    }

    fn is_callable_value(&self, expression: BoundExpressionId) -> bool {
        self.types.expression(expression).is_some_and(|ty| matches!(self.context.semantic_values().type_data(ty.ty()).as_ref(), bray_symbols::TypeData::Callable(_)))
    }

    fn is_unit(&self, ty: TypeId) -> bool {
        bray_checker::type_representation_for_values(self.context.semantic_values(),
            self.context.compilation().available_compiler_known_symbols(), ty) == Some(bray_compiler_known::RepresentationRole::Unit)
    }

    fn boolean_operation(&mut self, kind: CheckedTemplateShortCircuitKind, left: CheckedTemplateNodeId, right: CheckedTemplateNodeId) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let ty = self.context.compilation().target_property_type(bray_target::TargetPropertyKind::ScalarBool)?;
        Ok(self.push_node(CheckedTemplateOperation::ShortCircuit { kind, left, right }, ty))
    }

    pub(super) fn refine_path(&mut self, path: TemplatePath, condition: CheckedTemplateNodeId, holds: bool) -> Result<TemplatePath, FactQueryError> {
        if matches!(path, TemplatePath::Unreachable) { return Ok(path); }
        let condition = if holds { condition } else {
            let ty = self.context.compilation().target_property_type(bray_target::TargetPropertyKind::ScalarBool)?;
            self.push_node(CheckedTemplateOperation::Unary { operation: bray_symbols::ConstantUnaryOperation::LogicalNot, operand: condition }, ty)
        };
        Ok(match path {
            TemplatePath::Always => TemplatePath::When(condition),
            TemplatePath::When(previous) => TemplatePath::When(self.boolean_operation(CheckedTemplateShortCircuitKind::And, previous, condition)?),
            TemplatePath::Unreachable => unreachable!("unreachable paths were excluded before refining"),
        })
    }

    pub(super) fn merge_paths(&mut self, left: TemplatePath, right: TemplatePath) -> Result<TemplatePath, FactQueryError> {
        Ok(match (left, right) {
            (TemplatePath::Always, _) | (_, TemplatePath::Always) => TemplatePath::Always,
            (TemplatePath::Unreachable, other) | (other, TemplatePath::Unreachable) => other,
            (TemplatePath::When(left), TemplatePath::When(right)) => TemplatePath::When(self.boolean_operation(CheckedTemplateShortCircuitKind::Or, left, right)?),
        })
    }

    fn select_value(&mut self, condition: CheckedTemplateNodeId, when_true: CheckedTemplateNodeId, when_false: CheckedTemplateNodeId, ty: TypeId) -> CheckedTemplateNodeId {
        if when_true == when_false || [when_true, when_false].iter().all(|node|
            self.nodes[usize::try_from(node.raw()).expect("checked template node fits host")].operation() == &CheckedTemplateOperation::Unreachable) {
            return when_true;
        }
        self.push_node(CheckedTemplateOperation::Conditional { condition, when_true, when_false }, ty)
    }

    fn unit_node(&mut self, ty: TypeId) -> CheckedTemplateNodeId {
        assert!(self.is_unit(ty), "checked implicit unit completion has unit type");
        let values = self.context.semantic_values();
        let value = values.intern_constant_value(bray_symbols::ConstantValueData::new(ty, bray_symbols::ConstantValueKind::Unit))
            .expect("unit completion satisfies its checked representation");
        let term = values.intern_constant_term(bray_symbols::ConstantTermData::Value(value)).expect("unit completion belongs to this store");
        self.push_node(CheckedTemplateOperation::Constant { term, usage: Default::default() }, ty)
    }
}
