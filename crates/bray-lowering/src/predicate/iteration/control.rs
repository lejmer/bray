use bray_bound_tree::{CheckedTemplateNodeId, CheckedTemplateOperation};
use bray_ir::{MirBlockId, MirImmediateValue, MirOperand, MirTerminatorKind};
use bray_symbols::TypeData;

use super::super::core::{PredicateLowerer, PredicateLoweringContext};

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    /// Emits the existing return control directly, without constructing and retesting nullable joins.
    pub(super) fn loop_control(
        &mut self,
        block: MirBlockId,
        id: CheckedTemplateNodeId,
        aliases: &mut Vec<(CheckedTemplateNodeId, bool)>,
        finish: &mut impl FnMut(&mut Self, MirBlockId, MirOperand, Option<bool>) -> Result<(), C::Error>,
    ) -> Result<(), C::Error> {
        // Retain the immutable operation while recursive lowering updates this invocation's cache.
        let operation = self.template.nodes()
            [usize::try_from(id.raw()).expect("checked node fits host")]
        .operation()
        .clone();

        if !self.values.contains_key(&id) {
            match operation {
                CheckedTemplateOperation::Unreachable => {
                    return self.terminate(block, MirTerminatorKind::Unreachable);
                }
                CheckedTemplateOperation::Sequence { before, value } => {
                    let (block, _) = self.node(block, before)?;

                    return self.loop_control(block, value, aliases, finish);
                }
                CheckedTemplateOperation::Conditional {
                    condition,
                    when_true,
                    when_false,
                    ..
                } => {
                    let (block, test) = self.node(block, condition)?;

                    let previous = self.values[&condition].clone();
                    let retained = self.inserted.len();

                    if let MirOperand::Immediate {
                        value: MirImmediateValue::Boolean(selected),
                        ..
                    } = test
                    {
                        aliases.push((id, selected));

                        let result = self.loop_control(
                            block,
                            if selected { when_true } else { when_false },
                            aliases,
                            finish,
                        );

                        aliases.pop();

                        return result;
                    }

                    let (yes, no) = self.branch_blocks(block, test)?;

                    for (block, value, selected) in
                        [(yes, when_true, true), (no, when_false, false)]
                    {
                        self.values.insert(condition, self.boolean_value(selected));
                        aliases.push((id, selected));
                        self.loop_control(block, value, aliases, finish)?;
                        aliases.pop();
                        self.rewind_values(retained);
                    }

                    self.values.insert(condition, previous);

                    return Ok(());
                }
                _ => {}
            }
        }

        let (block, value) = self.node(block, id)?;

        let block = self.commit_loop_aliases(block, aliases, &value)?;

        let present =
            match crate::operand::nullable_presence(self.context.semantic_values(), &value) {
                Some(present) => Some(present),
                None => self.injected_presence(id)?,
            };

        finish(self, block, value, present)
    }

    fn injected_presence(&self, node: CheckedTemplateNodeId) -> Result<Option<bool>, C::Error> {
        let CheckedTemplateOperation::Convert { value, target } = self.template.nodes()
            [usize::try_from(node.raw()).expect("checked node fits host")]
        .operation() else {
            return Ok(None);
        };

        let source = self.context.predicate_type(
            self.template.nodes()[usize::try_from(value.raw()).expect("checked value fits host")]
                .ty(),
        )?;

        let target = self.context.predicate_type(*target)?;
        let source = self.context.semantic_values().unborrowed_type(source);

        Ok(
            matches!(self.context.semantic_values().type_data(target).as_ref(),
            TypeData::Nullable(element) if *element == source)
            .then_some(true),
        )
    }

    fn commit_loop_aliases(
        &mut self,
        mut block: MirBlockId,
        aliases: &[(CheckedTemplateNodeId, bool)],
        value: &MirOperand,
    ) -> Result<MirBlockId, C::Error> {
        for (owner, selected) in aliases.iter().rev() {
            if self.values.insert(*owner, value.clone()).is_none() {
                self.inserted.push(*owner);
            }

            let CheckedTemplateOperation::Conditional { state_outputs, .. } = self.template.nodes()
                [usize::try_from(owner.raw()).expect("checked owner fits host")]
            .operation() else {
                unreachable!("control alias retains its conditional owner");
            };

            // Branch emission owns its output list independently of the immutable checked graph.
            let states = state_outputs.clone();
            let mut outputs = Vec::with_capacity(states.len());

            for (left, right) in states.iter() {
                outputs.push(if let Some(state) = if *selected { left } else { right } {
                    let (continued, state) = self.node(block, *state)?;

                    block = continued;

                    Some(state)
                } else {
                    None
                });
            }

            self.outputs.insert(*owner, outputs);
        }

        Ok(block)
    }
}
