use std::collections::{BTreeMap, BTreeSet, HashMap, VecDeque};

use bray_ir::{
    MirBinaryOperator, MirBlockId, MirOperand, MirOperationId, MirOperationKind, MirTerminatorKind,
    MirValueId, MirValueOrigin,
};

use super::super::super::super::{CodegenPreparationError, Compilation};
use super::super::super::specialization::ConcreteCodegenInstance;
use super::analysis::{ScalarAnalysis, slot};
use crate::fact::CancellationToken;

impl ScalarAnalysis<'_> {
    pub(super) fn local_value_aliases(
        &self,
        compilation: &Compilation,
        realization: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<BTreeMap<MirValueId, MirValueId>, CodegenPreparationError> {
        let mut aliases = BTreeMap::new();
        let mut concrete_types = BTreeMap::new();

        for (block_id, block) in self.unit.blocks_with_ids() {
            if !self.blocks[slot(block_id.slot())].executable {
                continue;
            }

            let mut seen = HashMap::new();

            for operation_id in block.operations() {
                let operation = self
                    .unit
                    .operation(*operation_id)
                    .expect("valid MIR operation");

                let Some(result) = operation.result() else {
                    continue;
                };

                if !numberable_scalar_expression(operation.kind()) {
                    continue;
                }

                let raw_type = self.unit.value(result).expect("valid MIR result").ty();

                let concrete_type = if let Some(concrete) = concrete_types.get(&raw_type) {
                    *concrete
                } else {
                    let concrete = compilation.substitute_codegen_type(
                        raw_type,
                        realization.substitution(),
                        cancellation,
                    )?;

                    concrete_types.insert(raw_type, concrete);

                    concrete
                };

                let key = (operation.kind().clone(), concrete_type);

                if let Some(previous) = seen.get(&key) {
                    aliases.insert(result, *previous);
                } else {
                    seen.insert(key, result);
                }
            }
        }

        Ok(aliases)
    }

    pub(super) fn dead_scalar_operations(
        &self,
        terminators: &BTreeMap<MirBlockId, MirTerminatorKind>,
        aliases: &BTreeMap<MirValueId, MirValueId>,
    ) -> BTreeSet<MirOperationId> {
        let mut uses = vec![0_usize; self.unit.values().len()];
        let mut pending = VecDeque::new();

        let mut omitted = aliases
            .keys()
            .map(|value| {
                let MirValueOrigin::Operation(operation) =
                    self.unit.value(*value).expect("valid MIR alias").origin()
                else {
                    panic!("numbered MIR value must be an operation result");
                };

                operation
            })
            .collect::<BTreeSet<_>>();

        for (block_id, block) in self.unit.blocks_with_ids() {
            if !self.blocks[slot(block_id.slot())].executable {
                continue;
            }

            for operation_id in block.operations() {
                if omitted.contains(operation_id) {
                    continue;
                }

                self.unit
                    .operation(*operation_id)
                    .expect("valid MIR operation")
                    .kind()
                    .for_each_operand(|operand| count_use(operand, &mut uses, aliases));
            }

            let mut terminator = terminators
                .get(&block_id)
                .unwrap_or_else(|| block.terminator().kind())
                .clone();

            terminator.for_each_input(|operand| count_use(operand, &mut uses, aliases));

            let _ = terminator.try_for_each_edge_mut::<()>(|edge| {
                for argument in edge.arguments() {
                    argument.for_each_operand(|operand| count_use(operand, &mut uses, aliases));
                }

                Ok(())
            });
        }

        for (block_id, block) in self.unit.blocks_with_ids() {
            if !self.blocks[slot(block_id.slot())].executable {
                continue;
            }

            for operation_id in block.operations() {
                if omitted.contains(operation_id) {
                    continue;
                }

                if let Some(result) = self
                    .unit
                    .operation(*operation_id)
                    .expect("valid MIR operation")
                    .result()
                    && uses[slot(result.slot())] == 0
                {
                    pending.push_back(*operation_id);
                }
            }
        }

        while let Some(operation_id) = pending.pop_front() {
            let operation = self
                .unit
                .operation(operation_id)
                .expect("valid MIR operation");

            let Some(result) = operation.result() else {
                continue;
            };

            if uses[slot(result.slot())] != 0
                || !dead_scalar_is_pure(operation.kind())
                || !omitted.insert(operation_id)
            {
                continue;
            }

            operation.kind().for_each_operand(|operand| {
                if let MirOperand::Value(value) = operand {
                    let value = aliases.get(value).copied().unwrap_or(*value);
                    let count = &mut uses[slot(value.slot())];

                    *count -= 1;

                    if *count == 0
                        && let MirValueOrigin::Operation(producer) =
                            self.unit.value(value).expect("valid MIR value").origin()
                    {
                        pending.push_back(producer);
                    }
                }
            });
        }

        omitted
    }
}

fn count_use(operand: &MirOperand, uses: &mut [usize], aliases: &BTreeMap<MirValueId, MirValueId>) {
    if let MirOperand::Value(value) = operand {
        let value = aliases.get(value).copied().unwrap_or(*value);

        uses[slot(value.slot())] += 1;
    }
}

fn numberable_scalar_expression(kind: &MirOperationKind) -> bool {
    let scalar_operand = |operand: &MirOperand| {
        matches!(
            operand,
            MirOperand::Value(_)
                | MirOperand::Constant { .. }
                | MirOperand::ConstantTerm { .. }
                | MirOperand::Immediate { .. }
        )
    };

    match kind {
        MirOperationKind::Unary { operand, .. }
        | MirOperationKind::NumericConversion { operand, .. } => scalar_operand(operand),
        MirOperationKind::Binary { left, right, .. } => {
            scalar_operand(left) && scalar_operand(right)
        }
        MirOperationKind::NullableQuery(query) => scalar_operand(query.operand()),
        _ => false,
    }
}

fn dead_scalar_is_pure(kind: &MirOperationKind) -> bool {
    numberable_scalar_expression(kind)
        && !matches!(
            kind,
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Divide
                    | MirBinaryOperator::Remainder
                    | MirBinaryOperator::ShiftLeft
                    | MirBinaryOperator::ShiftRight,
                ..
            }
        )
}
