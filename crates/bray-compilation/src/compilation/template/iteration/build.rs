use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    BoundStructuredExpression, CheckedTemplateLoopBody, CheckedTemplateNode, CheckedTemplateNodeId,
    CheckedTemplateOperation, SelectedPropagationBoundary,
};
use bray_symbols::{TypeData, TypeId};

use super::super::build::{SourceTemplateBuilder, index};
use super::super::control::{TemplateBoundary, TemplatePath};
use super::state::LoopScope;
use crate::fact::FactQueryError;

impl SourceTemplateBuilder<'_, '_, '_> {
    pub(in crate::compilation::template) fn while_loop(
        &mut self,
        expression: &BoundStructuredExpression,
        ty: TypeId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let ([condition], [body, ..]) = (expression.operands(), expression.blocks()) else {
            panic!("checked while retains its condition and body");
        };

        let path = self.path;
        let evaluation = self.evaluation.take();
        let checkpoint = self.state_changes.len();
        let start = CheckedTemplateNodeId::new(index(self.nodes.len()));

        self.loops.push(LoopScope::new(start));
        self.path = TemplatePath::Always;

        let condition = self.expression(*condition)?;
        let condition_updates = self.retained_updates(checkpoint);

        let (body, completed, result_type, has_returns) =
            self.loop_body(*body, condition, ty, checkpoint)?;

        let operation = self.retire_loop(
            checkpoint,
            condition,
            body,
            completed,
            result_type,
            &condition_updates,
        );

        self.loop_result(operation, has_returns, path, evaluation, ty)
    }

    pub(super) fn loop_body(
        &mut self,
        block: bray_bound_tree::BoundBlockId,
        condition: CheckedTemplateNodeId,
        ty: TypeId,
        checkpoint: usize,
    ) -> Result<(CheckedTemplateLoopBody, CheckedTemplateNodeId, TypeId, bool), FactQueryError>
    {
        let boundary = SelectedPropagationBoundary::Callable;

        let enclosing = self
            .exits
            .insert(boundary, TemplateBoundary::new(checkpoint))
            .expect("loop belongs to an active callable boundary");

        self.path = TemplatePath::Always;
        self.path = self.refine_path(TemplatePath::Always, condition, true)?;
        self.evaluation = None;

        let body = self.block_value(block, ty)?;

        let mut returns = self
            .exits
            .remove(&boundary)
            .expect("loop retains its return boundary");

        let has_returns = !returns.exits.is_empty();

        let (body, completed, result_type) = if has_returns {
            let result_type = self
                .context
                .semantic_values()
                .intern_type(TypeData::Nullable(
                    self.return_type
                        .expect("checked loop return retains its callable result"),
                ))?;

            for exit in &mut returns.exits {
                exit.value = self.push_node(
                    CheckedTemplateOperation::Convert {
                        value: exit.value,
                        target: result_type,
                    },
                    result_type,
                );
            }

            let absent = self.nullable_absence_node(result_type)?;

            let fallback = if matches!(self.path, TemplatePath::Unreachable) {
                self.unreachable_node(result_type)
            } else {
                self.push_node(
                    CheckedTemplateOperation::Sequence {
                        before: body,
                        value: absent,
                    },
                    result_type,
                )
            };

            self.exits.insert(boundary, returns);

            let body = self.finish_exit(boundary, fallback, result_type)?;

            (CheckedTemplateLoopBody::Return(body), absent, result_type)
        } else {
            (
                CheckedTemplateLoopBody::Repeat(body),
                self.unit_node(ty),
                ty,
            )
        };

        self.exits.insert(boundary, enclosing);

        Ok((body, completed, result_type, has_returns))
    }

    pub(super) fn loop_result(
        &mut self,
        operation: CheckedTemplateNodeId,
        has_returns: bool,
        path: TemplatePath,
        evaluation: Option<CheckedTemplateNodeId>,
        ty: TypeId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let boundary = SelectedPropagationBoundary::Callable;

        self.path = path;
        self.evaluation = evaluation;

        let before = if has_returns {
            let boolean = self
                .context
                .compilation()
                .target_property_type(bray_target::TargetPropertyKind::ScalarBool)?;

            let result_type =
                self.nodes[usize::try_from(operation.raw()).expect("checked loop fits host")].ty();

            let test = self.nullable_test(operation, result_type, true)?;
            let test = self.push_node(test, boolean);
            let test = self.record_evaluation(test);

            self.path = self.refine_path(path, test, true)?;

            let result_type = self
                .return_type
                .expect("loop return retains its enclosing result");

            let value = self.push_node(
                CheckedTemplateOperation::NullableValue { subject: operation },
                result_type,
            );

            self.record_exit(boundary, value, result_type);
            self.path = self.refine_path(path, test, false)?;

            test
        } else {
            operation
        };

        let value = self.unit_node(ty);

        Ok(CheckedTemplateOperation::Sequence { before, value })
    }

    pub(super) fn retire_loop(
        &mut self,
        checkpoint: usize,
        condition: CheckedTemplateNodeId,
        body: CheckedTemplateLoopBody,
        completed: CheckedTemplateNodeId,
        result_type: TypeId,
        condition_updates: &std::collections::BTreeMap<
            bray_bound_tree::StorageIdentityId,
            CheckedTemplateNodeId,
        >,
    ) -> CheckedTemplateNodeId {
        let scope = self
            .loops
            .pop()
            .expect("loop retains its lexical binder scope");

        let start = scope.start;
        let mut bindings = Vec::with_capacity(scope.bindings.len());
        let mut next = Vec::with_capacity(scope.bindings.len());
        let mut outputs = Vec::with_capacity(scope.bindings.len());
        let mut changed = Vec::new();

        for (root, binding) in &scope.bindings {
            let state = self.state_values[root];

            let output = condition_updates
                .get(root)
                .copied()
                .unwrap_or(binding.value);

            if state != binding.value || output != binding.value {
                changed.push((*root, bindings.len()));
            } else {
                // An immutable observation needs no private copy or mutable loop home.
                let ty = self.nodes
                    [usize::try_from(binding.value.raw()).expect("checked binder fits host")]
                .ty();

                self.nodes
                    [usize::try_from(binding.value.raw()).expect("checked binder fits host")] =
                    CheckedTemplateNode::new(
                        CheckedTemplateOperation::LoopValue {
                            initial: binding.original,
                        },
                        ty,
                    );
            }

            bindings.push(binding.value);
            next.push(state);
            outputs.push(output);
        }

        self.take_state_updates(checkpoint);

        for (root, binding) in &scope.bindings {
            let CheckedTemplateOperation::LoopValue { initial } = self.nodes
                [usize::try_from(binding.value.raw()).expect("checked binder fits host")]
            .operation() else {
                unreachable!("loop retirement retains its binder's established initial value");
            };

            self.state_values.insert(*root, *initial);
        }

        let operation = self.push_node(
            CheckedTemplateOperation::Loop {
                start,
                bindings: bindings.into(),
                condition,
                body,
                next: next.into(),
                completed,
                outputs: outputs.into(),
            },
            result_type,
        );

        for (root, ordinal) in changed {
            let ty = self.nodes
                [usize::try_from(self.state_values[&root].raw()).expect("checked state fits host")]
            .ty();

            let state = self.push_node(
                CheckedTemplateOperation::StateAfter {
                    operation,
                    ordinal: u32::try_from(ordinal).expect("checked state ordinal fits u32"),
                },
                ty,
            );

            self.assign_state(root, state);
        }

        operation
    }

    pub(super) fn nullable_absence_node(
        &mut self,
        ty: TypeId,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let term = self.nullable_absence_term(ty)?;

        Ok(self.push_node(
            CheckedTemplateOperation::Constant {
                term,
                usage: Default::default(),
            },
            ty,
        ))
    }
}
