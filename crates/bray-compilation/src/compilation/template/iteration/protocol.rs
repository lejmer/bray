use bray_binder::BindingQueryContext;
use bray_bound_tree::{
    BoundExecutionSite, BoundExpressionId, BoundForExpression, BoundStructuredExpression,
    BoundStructuredExpressionKind, CheckedTemplateDispatch, CheckedTemplateLoopBody,
    CheckedTemplateNodeId, CheckedTemplateOperation, IterationSourceMode, SemanticSelection,
    StorageIdentityId,
};
use bray_symbols::{BorrowKind, TypeData, TypeId};

use super::super::build::{SourceTemplateBuilder, index};
use super::super::control::TemplatePath;
use super::state::LoopScope;
use crate::fact::FactQueryError;

impl SourceTemplateBuilder<'_, '_, '_> {
    fn iteration_selection(
        &self,
        id: BoundExpressionId,
    ) -> &bray_bound_tree::SelectedIterationSource {
        let Some(SemanticSelection::Iteration(selected)) = self.selections.expression(id) else {
            panic!("admitted iteration retains its selected protocol: {id:?}");
        };

        selected
    }

    fn protocol_call(
        &mut self,
        callable: bray_symbols::CallableInstanceData,
        witness: bray_symbols::ImplementationInstanceId,
        argument: CheckedTemplateNodeId,
        result: TypeId,
        mutable: bool,
    ) -> Result<CheckedTemplateNodeId, FactQueryError> {
        let (declaration, substitution) =
            self.declaration_reference(callable.definition().symbol(), callable.substitution())?;

        let (key, implementation) = self.implementation_reference(witness)?;

        let operation = CheckedTemplateOperation::ordered_call(
            declaration,
            substitution,
            [argument],
            [0],
            Some(CheckedTemplateDispatch::Implementation(key, implementation)),
        )
        .with_state_inputs(if mutable { vec![0] } else { Vec::new() });

        Ok(self.push_node(operation, result))
    }

    fn iteration_cursor(
        &mut self,
        id: BoundExpressionId,
    ) -> Result<StorageIdentityId, FactQueryError> {
        let selected = self.iteration_selection(id);

        let (source, mode, iterate, witness, ty) = (
            selected.source(),
            selected.mode(),
            selected.iterate(),
            selected.iterable_witness(),
            selected.cursor_type(),
        );

        let mut argument = self.expression(source)?;

        let primitive = matches!(
            self.context
                .compilation()
                .available_compiler_known_symbols()
                .symbol_implementation(iterate.definition().symbol()),
            Some(
                bray_compiler_known::ImplementationHook::RangeSharedIterate
                    | bray_compiler_known::ImplementationHook::RangeMoveIterate
            )
        );

        if !primitive && mode != IterationSourceMode::Move {
            let kind = if mode == IterationSourceMode::Shared {
                BorrowKind::Shared
            } else {
                BorrowKind::Mutable
            };

            let target =
                self.nodes[usize::try_from(argument.raw()).expect("checked source fits host")].ty();

            let ty = self
                .context
                .semantic_values()
                .intern_type(TypeData::Borrow { kind, target })?;

            argument = self.push_node(
                CheckedTemplateOperation::Borrow {
                    kind,
                    operand: argument,
                },
                ty,
            );
        }

        let cursor = if primitive {
            self.push_node(
                CheckedTemplateOperation::Convert {
                    value: argument,
                    target: ty,
                },
                ty,
            )
        } else {
            self.protocol_call(iterate, witness, argument, ty, false)?
        };

        let cursor = self.materialize(cursor);
        let cursor = self.record_evaluation(cursor);

        let access = self
            .storage
            .invocation_result(BoundExecutionSite::Iterate(id))
            .expect("checked iteration owns its cursor");

        let root = self
            .storage
            .root_identity(access)
            .expect("iteration cursor has its own storage identity");

        self.introduce_state(root, cursor);

        Ok(root)
    }

    fn iteration_step(
        &mut self,
        id: BoundExpressionId,
        root: StorageIdentityId,
    ) -> Result<(CheckedTemplateNodeId, CheckedTemplateNodeId), FactQueryError> {
        let selected = self.iteration_selection(id);

        let (next, witness, cursor_type, element) = (
            selected.next(),
            selected.iterator_witness(),
            selected.cursor_type(),
            selected.element_type(),
        );

        let result_type = self
            .context
            .semantic_values()
            .intern_type(TypeData::Nullable(element))?;

        let cursor = self
            .current_state(root)
            .expect("iteration cursor is initialized before its step");

        let (next, state) = if self
            .context
            .compilation()
            .available_compiler_known_symbols()
            .symbol_implementation(next.definition().symbol())
            == Some(bray_compiler_known::ImplementationHook::RangeNext)
        {
            self.range_step(cursor, cursor_type, result_type)?
        } else {
            let next = self.protocol_call(next, witness, cursor, result_type, true)?;

            let state = self.push_node(
                CheckedTemplateOperation::StateAfter {
                    operation: next,
                    ordinal: 0,
                },
                cursor_type,
            );

            (next, state)
        };

        self.assign_state(root, state);

        let boolean = self
            .context
            .compilation()
            .target_property_type(bray_target::TargetPropertyKind::ScalarBool)?;

        let condition = self.nullable_test(next, result_type, true)?;
        let condition = self.push_node(condition, boolean);

        let value = self.push_node(
            CheckedTemplateOperation::NullableValue { subject: next },
            element,
        );

        Ok((condition, value))
    }

    pub(in crate::compilation::template) fn for_loop(
        &mut self,
        id: BoundExpressionId,
        source: &BoundForExpression,
        ty: TypeId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let operation = self.iteration_loop(id, source.pattern(), source.body(), ty)?;

        let Some(block) = source.else_body() else {
            return Ok(operation);
        };

        let before = self.push_node(operation, ty);

        self.record_evaluation(before);

        let value = self.block_value(block, ty)?;

        Ok(CheckedTemplateOperation::Sequence { before, value })
    }

    pub(in crate::compilation::template) fn iteration_loop(
        &mut self,
        id: BoundExpressionId,
        pattern: bray_bound_tree::BoundPatternId,
        body_block: bray_bound_tree::BoundBlockId,
        ty: TypeId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let root = self.iteration_cursor(id)?;
        let path = self.path;
        let evaluation = self.evaluation.take();
        let checkpoint = self.state_changes.len();

        self.loops
            .push(LoopScope::new(CheckedTemplateNodeId::new(index(
                self.nodes.len(),
            ))));

        self.path = TemplatePath::Always;

        let (condition, element) = self.iteration_step(id, root)?;

        let condition_updates = self.retained_updates(checkpoint);

        self.bind_pattern_value(pattern, element)?;

        let (body, completed, result_type, has_returns) =
            self.loop_body(body_block, condition, ty, checkpoint)?;

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

    pub(in crate::compilation::template) fn boolean_fold(
        &mut self,
        id: BoundExpressionId,
        fold: &BoundStructuredExpression,
        ty: TypeId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        if let Some(producer) = self.generator_observations.fold(id) {
            return self.generator_fold(id, producer, ty);
        }

        let root = self.iteration_cursor(id)?;
        let path = self.path;
        let evaluation = self.evaluation.take();
        let checkpoint = self.state_changes.len();

        self.loops
            .push(LoopScope::new(CheckedTemplateNodeId::new(index(
                self.nodes.len(),
            ))));

        self.path = TemplatePath::Always;

        let (condition, element) = self.iteration_step(id, root)?;

        let condition_updates = self.retained_updates(checkpoint);

        self.path = self.refine_path(TemplatePath::Always, condition, true)?;
        self.evaluation = None;

        let value = element;
        let all = fold.kind() == BoundStructuredExpressionKind::BooleanAllFold;

        let result_type = self
            .context
            .semantic_values()
            .intern_type(TypeData::Nullable(ty))?;

        let stopped = self.boolean_constant(!all)?;

        let stopped = self.push_node(
            CheckedTemplateOperation::Convert {
                value: stopped,
                target: result_type,
            },
            result_type,
        );

        let absent = self.nullable_absence_node(result_type)?;

        let (when_true, when_false) = if all {
            (absent, stopped)
        } else {
            (stopped, absent)
        };

        let body = self.push_node(
            CheckedTemplateOperation::Conditional {
                condition: value,
                when_true,
                when_false,
                state_outputs: Default::default(),
            },
            result_type,
        );

        let completed = self.boolean_constant(all)?;

        let completed = self.push_node(
            CheckedTemplateOperation::Convert {
                value: completed,
                target: result_type,
            },
            result_type,
        );

        let operation = self.retire_loop(
            checkpoint,
            condition,
            CheckedTemplateLoopBody::Return(body),
            completed,
            result_type,
            &condition_updates,
        );

        self.path = path;
        self.evaluation = evaluation;

        let value = self.push_node(
            CheckedTemplateOperation::NullableValue { subject: operation },
            ty,
        );

        Ok(CheckedTemplateOperation::Sequence {
            before: operation,
            value,
        })
    }
    pub(in crate::compilation::template) fn advance_range(
        &mut self,
        expression: BoundExpressionId,
    ) -> Result<CheckedTemplateOperation, FactQueryError> {
        let target = self
            .flow
            .invocation_input_target(expression.into(), bray_symbols::SymbolOrdinal::new(0))
            .expect("selected range next retains its mutable receiver");

        let path = target
            .inline_state_path()
            .expect("admitted range next has its confined cursor");

        let (base, component) = self.prepare_state_path(&path)?;

        let cursor_type =
            self.nodes[usize::try_from(component.raw()).expect("checked cursor fits host")].ty();

        let result_type = self
            .types
            .expression(expression)
            .expect("range next retains its nullable result")
            .ty();

        let (result, value) = self.range_step(component, cursor_type, result_type)?;

        let subject = self.state_values[&path.root];

        let ty =
            self.nodes[usize::try_from(subject.raw()).expect("checked range owner fits host")].ty();

        let state = self.push_node(
            CheckedTemplateOperation::Update {
                subject,
                base,
                component,
                value,
            },
            ty,
        );

        self.assign_state(path.root, state);

        Ok(CheckedTemplateOperation::Sequence {
            before: result,
            value: result,
        })
    }
}
