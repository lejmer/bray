use bray_bound_tree::{CheckedTemplateLoopBody, CheckedTemplateNodeId, CheckedTemplateOperation};
use bray_ir::{
    MirBlockId, MirBlockKind, MirEdge, MirOperand, MirPatternPredicate, MirStoreKind,
    MirTerminatorKind,
};
use bray_symbols::TypeId;

use super::super::core::{PredicateLowerer, PredicateLoweringContext, capacity};

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    pub(in crate::predicate) fn loop_value(
        &mut self,
        mut block: MirBlockId,
        owner: CheckedTemplateNodeId,
        operation: &CheckedTemplateOperation,
        ty: TypeId,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        let CheckedTemplateOperation::Loop {
            bindings,
            condition,
            body,
            next,
            completed,
            outputs,
            ..
        } = operation
        else {
            unreachable!("loop lowering receives its checked kernel");
        };

        let mut homes = Vec::with_capacity(bindings.len());

        for (ordinal, binding) in bindings.iter().enumerate() {
            let binding_data = &self.template.nodes()
                [usize::try_from(binding.raw()).expect("checked binding fits host")];

            let CheckedTemplateOperation::LoopValue { initial } = binding_data.operation() else {
                panic!("checked loop state retains its initial value");
            };

            let binding_type = self.context.predicate_type(binding_data.ty())?;

            let (continued, value) = self.node(block, *initial)?;

            block = continued;

            let value = self.convert(block, value, binding_type)?;
            let changed = next[ordinal] != *binding || outputs[ordinal] != *binding;

            let value = if changed {
                MirOperand::Copy(self.private_owner(block, value)?)
            } else {
                value
            };

            homes.push(value);
        }

        let retained = self.inserted.len();

        let header = self
            .builder
            .push_block(self.source.clone(), MirBlockKind::Ordinary)
            .map_err(capacity)?;

        let entered = self
            .builder
            .push_block(self.source.clone(), MirBlockKind::Ordinary)
            .map_err(capacity)?;

        let exhausted = self
            .builder
            .push_block(self.source.clone(), MirBlockKind::Ordinary)
            .map_err(capacity)?;

        let finished = self
            .builder
            .push_block(self.source.clone(), MirBlockKind::Ordinary)
            .map_err(capacity)?;

        let result = self.parameter(finished, ty)?;

        self.terminate(block, MirTerminatorKind::Goto(MirEdge::new(header, [])))?;

        for (binding, home) in bindings.iter().zip(&homes) {
            assert!(
                self.values.insert(*binding, home.clone()).is_none(),
                "kernel owns its value binding"
            );

            self.inserted.push(*binding);
        }

        let previous_condition = self.values.get(condition).cloned();

        let (header_end, test) = self.node(header, *condition)?;

        let known = match &test {
            MirOperand::Immediate {
                value: bray_ir::MirImmediateValue::Boolean(value),
                ..
            } => Some(*value),
            _ => None,
        };

        match known {
            Some(value) => self.terminate(
                header_end,
                MirTerminatorKind::Goto(MirEdge::new(if value { entered } else { exhausted }, [])),
            )?,
            None => self.terminate(
                header_end,
                MirTerminatorKind::Branch {
                    condition: test,
                    then_edge: MirEdge::new(entered, []),
                    else_edge: MirEdge::new(exhausted, []),
                },
            )?,
        }

        let condition_retained = self.inserted.len();

        self.values.insert(*condition, self.boolean_value(true));

        if known == Some(false) {
            self.terminate(entered, MirTerminatorKind::Unreachable)?;
        } else if matches!(body, CheckedTemplateLoopBody::Return(_)) {
            let mut finish = |lowerer: &mut Self, block, value, present| {
                lowerer.loop_body_outcome(block, value, present, &homes, next, header, finished)
            };

            self.loop_control(entered, body.node(), &mut Vec::new(), &mut finish)?;
        } else {
            let (block, _) = self.node(entered, body.node())?;

            let block = self.store_loop_state(block, &homes, next)?;

            self.terminate(block, MirTerminatorKind::Goto(MirEdge::new(header, [])))?;
        }

        self.rewind_values(condition_retained);
        self.values.insert(*condition, self.boolean_value(false));

        if known == Some(true) {
            self.terminate(exhausted, MirTerminatorKind::Unreachable)?;
        } else {
            let (block, value) = self.node(exhausted, *completed)?;

            let block = self.store_loop_state(block, &homes, outputs)?;
            let value = self.convert(block, value, ty)?;

            self.terminate(
                block,
                MirTerminatorKind::Goto(MirEdge::new(finished, [value])),
            )?;
        }

        self.rewind_values(retained);

        if let Some(value) = previous_condition {
            self.values.insert(*condition, value);
        }

        self.outputs
            .insert(owner, homes.into_iter().map(Some).collect());

        Ok((finished, MirOperand::Value(result)))
    }

    fn store_loop_state(
        &mut self,
        mut block: MirBlockId,
        homes: &[MirOperand],
        states: &[CheckedTemplateNodeId],
    ) -> Result<MirBlockId, C::Error> {
        for (home, state) in homes.iter().zip(states) {
            let (continued, value) = self.node(block, *state)?;

            block = continued;

            if *home == value {
                continue;
            }

            let MirOperand::Copy(destination) = home else {
                panic!("changing loop state retains private mutable storage");
            };

            assert!(
                self.private_homes.contains(&destination.storage()),
                "loop writes remain invocation-private"
            );

            let value = self.convert(block, value, destination.ty())?;

            self.store(block, MirStoreKind::Assign, destination.clone(), value)?;
        }

        Ok(block)
    }

    fn loop_body_outcome(
        &mut self,
        block: MirBlockId,
        value: MirOperand,
        present: Option<bool>,
        homes: &[MirOperand],
        next: &[CheckedTemplateNodeId],
        header: MirBlockId,
        finished: MirBlockId,
    ) -> Result<(), C::Error> {
        let block = self.store_loop_state(block, homes, next)?;

        match present {
            Some(true) => self.terminate(
                block,
                MirTerminatorKind::Goto(MirEdge::new(finished, [value])),
            ),
            Some(false) => self.terminate(block, MirTerminatorKind::Goto(MirEdge::new(header, []))),
            None => self.terminate(
                block,
                MirTerminatorKind::PatternBranch {
                    subject: value.clone(),
                    predicate: MirPatternPredicate::NullablePresent,
                    matched: MirEdge::new(finished, [value]),
                    unmatched: MirEdge::new(header, []),
                },
            ),
        }
    }
}
