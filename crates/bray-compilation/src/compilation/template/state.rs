use std::collections::BTreeMap;

use bray_bound_tree::{
    BoundAssignmentExpression, BoundAssignmentOperator, BoundExpressionId, BoundReferenceTarget,
    CheckedTemplateNodeId, CheckedTemplateOperation, OperatorTarget, SelectedOperation,
    SemanticSelection, StorageBinding, StorageBindingTarget,
};
use bray_symbols::TypeId;

use super::build::SourceTemplateBuilder;
use crate::fact::FactQueryError;

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(super) fn assign_binding(
        &mut self,
        target: BoundReferenceTarget,
        value: CheckedTemplateNodeId,
    ) {
        let previous = self.local_values.insert(target, value)
            .expect("a mutable template binding has its entry value");
        self.binding_changes.push((target, previous));
    }

    /// Restore only changed bindings. Node IDs preserve each branch's value snapshots.
    pub(super) fn take_binding_updates(
        &mut self,
        checkpoint: usize,
    ) -> BTreeMap<BoundReferenceTarget, CheckedTemplateNodeId> {
        let mut updates = BTreeMap::new();
        while self.binding_changes.len() > checkpoint {
            let (target, previous) = self.binding_changes.pop()
                .expect("binding checkpoint lies within its change log");
            let value = self.local_values.insert(target, previous)
                .expect("changed binding retains its current value");
            updates.entry(target).or_insert(value);
        }
        updates
    }

    pub(super) fn merge_bindings(
        &mut self,
        condition: CheckedTemplateNodeId,
        mut when_true: BTreeMap<BoundReferenceTarget, CheckedTemplateNodeId>,
        when_false: BTreeMap<BoundReferenceTarget, CheckedTemplateNodeId>,
    ) {
        for (target, value) in when_false {
            let baseline = self.local_values[&target];
            let when_true_value = when_true.remove(&target).unwrap_or(baseline);
            self.merge_binding(target, condition, when_true_value, value);
        }
        for (target, value) in when_true {
            let baseline = self.local_values[&target];
            self.merge_binding(target, condition, value, baseline);
        }
    }

    fn merge_binding(
        &mut self,
        target: BoundReferenceTarget,
        condition: CheckedTemplateNodeId,
        when_true: CheckedTemplateNodeId,
        when_false: CheckedTemplateNodeId,
    ) {
        let value = if when_true == when_false {
            when_true
        } else {
            let ty = self.nodes[usize::try_from(when_true.raw()).expect("checked branch node fits host")].ty();
            self.select_value(condition, when_true, when_false, ty)
        };
        self.assign_binding(target, value);
    }

    pub(super) fn assignment(
        &mut self,
        expression: BoundExpressionId,
        assignment: &BoundAssignmentExpression,
        ty: TypeId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let [destination, source] = assignment.operands() else {
            panic!("checked assignment has its destination and source");
        };
        let path = self.flow.confined_assignment(self.storage, expression)
            .expect("admitted template assignment has its exact confined slot");
        let target = self.storage.binding_targets(StorageBinding::Identity(path.root))
            .find_map(StorageBindingTarget::reference_target).expect("admitted mutable slot retains its source binding");

        let previous = self.expression(*destination)?;
        self.local_values.entry(target).or_insert(previous);
        let source = self.expression(*source)?;
        let value = if assignment.operator() == BoundAssignmentOperator::Assign {
            source
        } else {
            let Some(SemanticSelection::Operation(SelectedOperation::CompoundAssignment(selected))) =
                self.selections.expression(expression) else {
                panic!("checked compound assignment retains its exact operator");
            };
            let OperatorTarget::BuiltIn(operator) = selected.target() else {
                panic!("admitted compound assignment retains its scalar operator");
            };
            self.push_node(CheckedTemplateOperation::MachineBinary {
                operation: super::build::binary_operator(operator)
                    .expect("admitted compound assignment has a binary operation"),
                left: previous, right: source,
            }, selected.value_type())
        };
        let target_type = self.nodes[usize::try_from(previous.raw()).expect("checked assignment node fits host")].ty();
        let value = self.push_node(CheckedTemplateOperation::Convert { value, target: target_type }, target_type);
        self.assign_binding(target, value);
        let unit = self.unit_node(ty);
        Ok(CheckedTemplateOperation::Sequence { before: value, value: unit })
    }
}
