use bray_bound_tree::{CheckedTemplateNodeId, CheckedTemplateShortCircuitKind};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCall, MirEdge, MirImmediateValue, MirOperand, MirOperationKind,
    MirPlace, MirRuntimeReference, MirStorageKind, MirTerminatorKind,
};
use bray_runtime_interface::RuntimeAbiRole;
use bray_symbols::TypeId;

use super::core::{PredicateLowerer, PredicateLoweringContext, capacity};

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    pub(super) fn conditional(
        &mut self,
        block: MirBlockId,
        owner: CheckedTemplateNodeId,
        condition: CheckedTemplateNodeId,
        when_true: CheckedTemplateNodeId,
        when_false: CheckedTemplateNodeId,
        states: &[(Option<CheckedTemplateNodeId>, Option<CheckedTemplateNodeId>)],
        ty: TypeId,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        let condition_node = condition;

        let (block, condition) = self.node(block, condition)?;

        if let MirOperand::Immediate {
            value: MirImmediateValue::Boolean(selected),
            ..
        } = &condition
        {
            let mut outputs = Vec::with_capacity(states.len());

            let result = self.conditional_value(
                block,
                if *selected { when_true } else { when_false },
                condition_node,
                *selected,
                states,
                &mut outputs,
            )?;

            self.outputs.insert(owner, outputs);

            return Ok(result);
        }

        let excluded = |node: CheckedTemplateNodeId| {
            matches!(
                self.template.nodes()[usize::try_from(node.raw()).expect("checked node fits host")]
                    .operation(),
                bray_bound_tree::CheckedTemplateOperation::Unreachable
            )
        };

        let (true_excluded, false_excluded) = (excluded(when_true), excluded(when_false));

        if true_excluded || false_excluded {
            assert!(
                !(true_excluded && false_excluded),
                "admitted predicate conditional retains a normal result path"
            );

            let (success, failure) = self.branch_blocks(block, condition)?;

            let (excluded, continued, value) = if true_excluded {
                (success, failure, when_false)
            } else {
                (failure, success, when_true)
            };

            self.terminate(excluded, MirTerminatorKind::Unreachable)?;

            let mut outputs = Vec::with_capacity(states.len());

            let result = self.conditional_value(
                continued,
                value,
                condition_node,
                !true_excluded,
                states,
                &mut outputs,
            )?;

            self.outputs.insert(owner, outputs);

            return Ok(result);
        }

        let mut true_states = Vec::with_capacity(states.len());
        let mut false_states = Vec::with_capacity(states.len());

        let result = self.branch_values(
            block,
            condition,
            ty,
            |lowerer, block| {
                lowerer.conditional_value(
                    block,
                    when_true,
                    condition_node,
                    true,
                    states,
                    &mut true_states,
                )
            },
            |lowerer, block| {
                lowerer.conditional_value(
                    block,
                    when_false,
                    condition_node,
                    false,
                    states,
                    &mut false_states,
                )
            },
        )?;

        let outputs = true_states
            .into_iter()
            .zip(false_states)
            .map(|(left, right)| match (left, right) {
                (Some(left), Some(right)) => {
                    assert_eq!(
                        left, right,
                        "conditional owner retains one private storage home"
                    );

                    Some(left)
                }
                (Some(value), None) | (None, Some(value)) => Some(value),
                (None, None) => {
                    unreachable!("checked state exists on at least one conditional path")
                }
            })
            .collect();

        self.outputs.insert(owner, outputs);

        Ok(result)
    }

    fn conditional_value(
        &mut self,
        block: MirBlockId,
        value: CheckedTemplateNodeId,
        condition: CheckedTemplateNodeId,
        selected: bool,
        states: &[(Option<CheckedTemplateNodeId>, Option<CheckedTemplateNodeId>)],
        outputs: &mut Vec<Option<MirOperand>>,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        let known = self.boolean_value(selected);

        let previous = self
            .values
            .insert(condition, known)
            .expect("conditional evaluates its condition before entering an alternative");

        let (mut block, value) = self.node(block, value)?;

        for (left, right) in states {
            let state = if selected { left } else { right };

            outputs.push(if let Some(state) = state {
                let (continued, state) = self.node(block, *state)?;

                block = continued;

                let MirOperand::Copy(place) = &state else {
                    panic!("changed owner retains its private storage: {state:?}");
                };

                assert!(
                    self.private_homes.contains(&place.storage()),
                    "owner state uses invocation-private storage"
                );

                Some(state)
            } else {
                None
            });
        }

        self.values.insert(condition, previous);

        Ok((block, value))
    }

    pub(super) fn short_circuit(
        &mut self,
        block: MirBlockId,
        kind: CheckedTemplateShortCircuitKind,
        left: CheckedTemplateNodeId,
        right: CheckedTemplateNodeId,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        let (block, left) = self.node(block, left)?;

        let evaluate_when_true = kind == CheckedTemplateShortCircuitKind::And;

        self.branch_values(
            block,
            left,
            self.boolean,
            |lowerer, block| {
                if evaluate_when_true {
                    lowerer.node(block, right)
                } else {
                    Ok((block, lowerer.boolean_value(true)))
                }
            },
            |lowerer, block| {
                if evaluate_when_true {
                    Ok((block, lowerer.boolean_value(false)))
                } else {
                    lowerer.node(block, right)
                }
            },
        )
    }

    pub(super) fn branch_values(
        &mut self,
        block: MirBlockId,
        condition: MirOperand,
        ty: TypeId,
        when_true: impl FnOnce(&mut Self, MirBlockId) -> Result<(MirBlockId, MirOperand), C::Error>,
        when_false: impl FnOnce(&mut Self, MirBlockId) -> Result<(MirBlockId, MirOperand), C::Error>,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        if let MirOperand::Immediate {
            value: MirImmediateValue::Boolean(selected),
            ..
        } = &condition
        {
            return if *selected {
                when_true(self, block)
            } else {
                when_false(self, block)
            };
        }

        let (success, failure) = self.branch_blocks(block, condition)?;

        let join = self
            .builder
            .push_block(self.source.clone(), MirBlockKind::Ordinary)
            .map_err(capacity)?;

        let retained = self.inserted.len();

        let (success, when_true) = when_true(self, success)?;

        self.rewind_values(retained);

        let (failure, when_false) = when_false(self, failure)?;

        self.rewind_values(retained);

        let common = self.common_type(&when_true, &when_false, ty)?;
        let when_true = self.convert(success, when_true, common)?;
        let when_false = self.convert(failure, when_false, common)?;
        let value = self.parameter(join, common)?;

        self.terminate(
            success,
            MirTerminatorKind::Goto(MirEdge::new(join, [when_true])),
        )?;

        self.terminate(
            failure,
            MirTerminatorKind::Goto(MirEdge::new(join, [when_false])),
        )?;

        Ok((join, MirOperand::Value(value)))
    }

    pub(super) fn branch_blocks(
        &mut self,
        block: MirBlockId,
        condition: MirOperand,
    ) -> Result<(MirBlockId, MirBlockId), C::Error> {
        let success = self
            .builder
            .push_block(self.source.clone(), MirBlockKind::Ordinary)
            .map_err(capacity)?;

        let failure = self
            .builder
            .push_block(self.source.clone(), MirBlockKind::Ordinary)
            .map_err(capacity)?;

        self.terminate(
            block,
            MirTerminatorKind::Branch {
                condition,
                then_edge: MirEdge::new(success, []),
                else_edge: MirEdge::new(failure, []),
            },
        )?;

        Ok((success, failure))
    }

    pub(super) fn rewind_values(&mut self, retained: usize) {
        while self.inserted.len() > retained {
            let id = self
                .inserted
                .pop()
                .expect("branch cache contains its newly inserted value");

            self.outputs.remove(&id);

            self.values
                .remove(&id)
                .expect("branch-local value is retained until its branch ends");
        }
    }

    pub(super) fn boolean_value(&self, value: bool) -> MirOperand {
        MirOperand::Immediate {
            value: MirImmediateValue::Boolean(value),
            ty: self.boolean,
        }
    }
}

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    pub(super) fn variant_test(
        &mut self,
        block: MirBlockId,
        subject: MirOperand,
        variant: bray_symbols::UnionVariantSymbolId,
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        let join = self
            .builder
            .push_block(self.source.clone(), MirBlockKind::Ordinary)
            .map_err(capacity)?;

        let result = self.parameter(join, self.boolean)?;

        self.terminate(
            block,
            MirTerminatorKind::PatternBranch {
                subject,
                predicate: bray_ir::MirPatternPredicate::ActiveUnionVariant(variant),
                matched: MirEdge::new(join, [self.boolean_value(true)]),
                unmatched: MirEdge::new(join, [self.boolean_value(false)]),
            },
        )?;

        Ok((join, MirOperand::Value(result)))
    }
}

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    pub(super) fn call(
        &mut self,
        block: MirBlockId,
        (call, parameters): (MirCall, Vec<TypeId>),
    ) -> Result<(MirBlockId, MirOperand), C::Error> {
        let ty = call.result().ty();

        // Compiler operations retain checked operand types. Catalog receiver markers are not ABI input types.
        if let Some(value) = self.builtin_call(block, &call, ty)? {
            return Ok((block, value));
        }

        assert_eq!(
            call.arguments().len(),
            parameters.len(),
            "selected predicate call retains its complete parameter mapping"
        );

        let mut parameters = parameters.into_iter();

        let call = call.try_map_inputs(|value| {
            self.convert(
                block,
                value.clone(),
                parameters
                    .next()
                    .expect("checked argument has its formal parameter"),
            )
        })?;

        let fallible = call.may_propagate_panic();
        let value = self.value(block, MirOperationKind::Call(call), ty)?;

        if !fallible {
            return Ok((block, value));
        }

        let report_type = self
            .context
            .representation_type(RepresentationRole::PanicReport)?;

        let storage = self
            .builder
            .push_storage(self.source.clone(), MirStorageKind::Temporary, report_type)
            .map_err(capacity)?;

        let report = MirPlace::new(storage, [], report_type);

        let panicked = self
            .builder
            .push_block(self.source.clone(), MirBlockKind::Ordinary)
            .map_err(capacity)?;

        let cancelled = self
            .builder
            .push_block(self.source.clone(), MirBlockKind::Ordinary)
            .map_err(capacity)?;

        let runtime = MirRuntimeReference::new(
            RuntimeAbiRole::PanicPropagation,
            self.builder.target().runtime_abi(),
        );

        self.terminate(
            panicked,
            MirTerminatorKind::PropagatePanic {
                report: MirOperand::Move(report.clone()),
                runtime,
            },
        )?;

        self.terminate(
            cancelled,
            MirTerminatorKind::PropagateCancellation {
                report: MirOperand::Move(report.clone()),
                runtime,
            },
        )?;

        let (continued, value) = crate::continuation::check_call_outcome(
            &mut self.builder,
            block,
            &self.source,
            panicked,
            cancelled,
            report,
            None,
            Some(value),
        )
        .map_err(capacity)?;

        let value = value.expect("predicate call retains its checked result");

        let MirOperand::Value(result) = &value else {
            unreachable!("call continuation retains its block parameter");
        };

        self.value_blocks.insert(*result, continued);

        for target in [continued, panicked, cancelled] {
            self.predecessor(block, target)?;
        }

        Ok((continued, value))
    }
}
