use std::collections::BTreeMap;

use bray_ir::{MirBlockId, MirEdge, MirOperand, MirOperationKind, MirTerminatorKind, MirValueId};
use bray_symbols::TypeId;

use super::core::{PredicateLowerer, PredicateLoweringContext, capacity};

impl<C: PredicateLoweringContext + ?Sized> PredicateLowerer<'_, C> {
    pub(super) fn parameter(
        &mut self,
        block: MirBlockId,
        ty: TypeId,
    ) -> Result<MirValueId, C::Error> {
        let value = self
            .builder
            .push_block_parameter(block, self.source.clone(), ty)
            .map_err(capacity)?;

        self.value_blocks.insert(value, block);

        Ok(value)
    }

    fn local_value(
        &mut self,
        block: MirBlockId,
        value: MirValueId,
    ) -> Result<MirValueId, C::Error> {
        if *self
            .value_blocks
            .get(&value)
            .expect("predicate SSA value retains its defining block")
            == block
        {
            return Ok(value);
        }

        if let Some(local) = self
            .forwarded
            .get(&block)
            .and_then(|values| values.get(&value))
        {
            return Ok(*local);
        }

        let ty = self.builder.operand_type(&MirOperand::Value(value));
        let local = self.parameter(block, ty)?;

        self.forwarded
            .entry(block)
            .or_default()
            .insert(value, local);

        self.imports.entry(block).or_default().push(value);

        // Install the parameter before following predecessors so loop backedges terminate.
        // Retain predecessor IDs across recursive updates of the construction graph.
        let predecessors = self.predecessors.get(&block).cloned().unwrap_or_default();

        for predecessor in predecessors {
            self.local_value(predecessor, value)?;
        }

        Ok(local)
    }

    fn replacements(
        &mut self,
        block: MirBlockId,
        values: Vec<MirValueId>,
    ) -> Result<BTreeMap<MirValueId, MirOperand>, C::Error> {
        let mut replacements = BTreeMap::new();

        for value in values {
            let local = self.local_value(block, value)?;

            if local != value {
                replacements.insert(value, MirOperand::Value(local));
            }
        }

        Ok(replacements)
    }

    pub(super) fn local_operation(
        &mut self,
        block: MirBlockId,
        mut operation: MirOperationKind,
    ) -> Result<MirOperationKind, C::Error> {
        let mut values = Vec::new();

        operation.for_each_operand(|operand| {
            if let MirOperand::Value(value) = operand
                && self.value_blocks[value] != block {
                    values.push(*value);
                }
        });

        operation.substitute_values(&self.replacements(block, values)?);

        Ok(operation)
    }

    pub(super) fn predecessor(
        &mut self,
        source: MirBlockId,
        target: MirBlockId,
    ) -> Result<(), C::Error> {
        let predecessors = self.predecessors.entry(target).or_default();

        if predecessors.contains(&source) {
            return Ok(());
        }

        predecessors.push(source);

        // Retain the demanded IDs while recursive forwarding may add imports on other blocks.
        let imports = self.imports.get(&target).cloned().unwrap_or_default();

        for value in imports {
            self.local_value(source, value)?;
        }

        Ok(())
    }

    pub(super) fn terminate(
        &mut self,
        block: MirBlockId,
        mut kind: MirTerminatorKind,
    ) -> Result<(), C::Error> {
        let mut values = Vec::new();

        kind.for_each_input(|operand| {
            if let MirOperand::Value(value) = operand
                && self.value_blocks[value] != block {
                    values.push(*value);
                }
        });

        kind.try_for_each_edge_mut::<C::Error>(|edge| {
            for argument in edge.arguments() {
                argument.for_each_operand(|operand| {
                    if let MirOperand::Value(value) = operand
                        && self.value_blocks[value] != block {
                            values.push(*value);
                        }
                });
            }

            self.predecessor(block, edge.target())
        })?;

        kind.substitute_values(&self.replacements(block, values)?);

        self.builder
            .set_terminator(block, self.source.clone(), kind);

        Ok(())
    }

    pub(super) fn finish_edges(&mut self) {
        let imports = &self.imports;
        let forwarded = &self.forwarded;
        let owners = &self.value_blocks;

        // Each edge is extended once, avoiding repeated copies of its immutable argument list.
        self.builder
            .try_for_each_edge_mut::<()>(|source, edge| {
                if let Some(values) = imports.get(&edge.target()) {
                    let arguments =
                        edge.arguments()
                            .iter()
                            .cloned()
                            .chain(values.iter().map(|value| {
                                MirOperand::Value(if owners[value] == source {
                                    *value
                                } else {
                                    forwarded[&source][value]
                                })
                            }));

                    *edge = MirEdge::new(edge.target(), arguments);
                }

                Ok(())
            })
            .expect("completing established predicate edges is infallible");
    }
}
