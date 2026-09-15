use bray_ir::{
    MirBinaryOperator, MirBlockId, MirEdge, MirOperand, MirOperationKind, MirPlace,
    MirSourceAnchor, MirStorageKind, MirStoreKind, MirTerminatorKind, MirUnitBuildError,
    MirUnitBuilder, MirValueId,
};
use bray_symbols::TypeId;

/// Visits an initialized prefix in reverse order while forwarding a pending scope-exit value.
pub(crate) struct ReverseCleanupLoop {
    condition: MirBlockId,
    pub(crate) body: MirBlockId,
    pub(crate) continuation: MirBlockId,
    pub(crate) counter: MirPlace,
    pub(crate) body_value: Option<MirValueId>,
    pub(crate) continuation_value: Option<MirValueId>,
}

// Instructions retain shared provenance and counter paths while the loop keeps their identities.
impl ReverseCleanupLoop {
    pub(crate) fn new(
        builder: &mut MirUnitBuilder,
        start: MirBlockId,
        source: &MirSourceAnchor,
        length: MirOperand,
        boolean: TypeId,
        constants: [MirOperand; 2],
        pending: Option<MirOperand>,
    ) -> Result<Self, MirUnitBuildError> {
        let integer = builder.operand_type(&length)?;
        let counter = builder.push_storage(source.clone(), MirStorageKind::Temporary, integer)?;
        let counter = MirPlace::new(counter, [], integer);

        builder.push_operation(
            start,
            source.clone(),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: counter.clone(),
                value: length,
            },
            None,
        )?;

        let kind = builder.block_kind(start)?;
        let condition = builder.push_block(source.clone(), kind)?;
        let body = builder.push_block(source.clone(), kind)?;
        let continuation = builder.push_block(source.clone(), kind)?;

        let mut parameter = |block| {
            pending
                .as_ref()
                .map(|value| {
                    let ty = builder.operand_type(value)?;

                    builder.push_block_parameter(block, source.clone(), ty)
                })
                .transpose()
        };

        let condition_value = parameter(condition)?;
        let body_value = parameter(body)?;
        let continuation_value = parameter(continuation)?;

        builder.set_terminator(
            start,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(condition, pending)),
        )?;

        let [zero, one] = constants;

        let nonempty = push_value(
            builder,
            condition,
            source,
            MirOperationKind::Binary {
                operator: MirBinaryOperator::GreaterThan,
                left: MirOperand::Copy(counter.clone()),
                right: zero,
            },
            boolean,
        )?;

        builder.set_terminator(
            condition,
            source.clone(),
            MirTerminatorKind::Branch {
                condition: nonempty,
                then_edge: MirEdge::new(body, condition_value.map(MirOperand::Value)),
                else_edge: MirEdge::new(continuation, condition_value.map(MirOperand::Value)),
            },
        )?;

        let index = push_value(
            builder,
            body,
            source,
            MirOperationKind::Binary {
                operator: MirBinaryOperator::Subtract,
                left: MirOperand::Copy(counter.clone()),
                right: one,
            },
            integer,
        )?;

        builder.push_operation(
            body,
            source.clone(),
            MirOperationKind::Store {
                kind: MirStoreKind::Assign,
                destination: counter.clone(),
                value: index,
            },
            None,
        )?;

        Ok(Self {
            condition,
            body,
            continuation,
            counter,
            body_value,
            continuation_value,
        })
    }

    pub(crate) fn close(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        pending: Option<MirOperand>,
    ) -> Result<(), MirUnitBuildError> {
        builder.set_terminator(
            block,
            source.clone(),
            MirTerminatorKind::Goto(MirEdge::new(self.condition, pending)),
        )
    }
}

fn push_value(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    kind: MirOperationKind,
    ty: TypeId,
) -> Result<MirOperand, MirUnitBuildError> {
    let operation = builder.push_operation(block, source.clone(), kind, Some(ty))?;

    Ok(MirOperand::Value(operation.result().ok_or(
        MirUnitBuildError::MissingOperationResult(operation.operation()),
    )?))
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::ReverseCleanupLoop;
    use bray_ir::{
        MirBinaryOperator, MirBlockKind, MirOperand, MirOperationKind, MirSourceAnchor,
        MirTerminatorKind, MirUnit, MirUnitBuilder, MirUnitKind, MirValueId,
    };
    use bray_symbols::{ConstantValueKind, SemanticValueStore, TypeData};

    #[test]
    fn reverse_cleanup_handles_empty_prefixes_and_forwards_pending_values() {
        for length in [0, 1, 4, 257] {
            for pending in [false, true] {
                let bound = bray_testing::test_bound_unit(1801);
                let source = MirSourceAnchor::from(bound.key().source());
                let values = SemanticValueStore::try_new().unwrap();
                let ty = values.intern_type(TypeData::tuple([])).unwrap();

                let constant =
                    |value| crate::operand::integer_constant(&values, ty, value).unwrap();

                let mut builder = MirUnitBuilder::for_bound(
                    bound.identity(),
                    MirUnitKind::Synchronous,
                    bray_testing::test_mir_target(),
                );

                let entry = builder
                    .push_block(source.clone(), MirBlockKind::Ordinary)
                    .unwrap();

                let cleanup = ReverseCleanupLoop::new(
                    &mut builder,
                    entry,
                    &source,
                    constant(length),
                    ty,
                    [constant(0), constant(1)],
                    pending.then(|| constant(73)),
                )
                .unwrap();

                cleanup
                    .close(
                        &mut builder,
                        cleanup.body,
                        &source,
                        cleanup.body_value.map(MirOperand::Value),
                    )
                    .unwrap();

                builder
                    .set_terminator(
                        cleanup.continuation,
                        source,
                        MirTerminatorKind::Return(
                            cleanup.continuation_value.map(MirOperand::Value),
                        ),
                    )
                    .unwrap();

                let unit = builder.finish(entry).unwrap();

                let (visited, returned) = evaluate_loop(&unit, &values, &cleanup);

                assert_eq!(visited, (0..length).rev().collect::<Vec<_>>());
                assert_eq!(returned, pending.then_some(73));
            }
        }
    }

    fn evaluate_loop(
        unit: &MirUnit,
        values: &SemanticValueStore,
        cleanup: &ReverseCleanupLoop,
    ) -> (Vec<u64>, Option<u64>) {
        let mut block = unit.entry();
        let mut temporaries = BTreeMap::new();
        let mut stored = BTreeMap::new();
        let mut visited = Vec::new();

        let operand = |operand: &MirOperand,
                       temporaries: &BTreeMap<MirValueId, u64>,
                       stored: &BTreeMap<_, u64>| match operand {
            MirOperand::Value(value) => temporaries[value],
            MirOperand::Copy(place) => stored[&place.storage()],
            MirOperand::Constant { value, .. } => match values.constant_value_data(*value).kind() {
                ConstantValueKind::Integer(value) => value.to_u64().unwrap(),
                other => panic!("unexpected loop constant: {other:?}"),
            },
            other => panic!("unexpected loop operand: {other:?}"),
        };

        loop {
            let current = unit.block(block).unwrap();

            for operation in current
                .operations()
                .iter()
                .map(|id| unit.operation(*id).unwrap())
            {
                match operation.kind() {
                    MirOperationKind::Store {
                        destination, value, ..
                    } => {
                        let value = operand(value, &temporaries, &stored);

                        stored.insert(destination.storage(), value);
                    }
                    MirOperationKind::Binary {
                        operator,
                        left,
                        right,
                    } => {
                        let left = operand(left, &temporaries, &stored);
                        let right = operand(right, &temporaries, &stored);

                        let value = match operator {
                            MirBinaryOperator::GreaterThan => u64::from(left > right),
                            MirBinaryOperator::Subtract => left.checked_sub(right).unwrap(),
                            other => panic!("unexpected loop operator: {other:?}"),
                        };

                        temporaries.insert(operation.result().unwrap(), value);
                    }
                    other => panic!("unexpected loop operation: {other:?}"),
                }
            }

            if block == cleanup.body {
                visited.push(stored[&cleanup.counter.storage()]);
            }

            let edge = match current.terminator().kind() {
                MirTerminatorKind::Goto(edge) => edge,
                MirTerminatorKind::Branch {
                    condition,
                    then_edge,
                    else_edge,
                } => {
                    if operand(condition, &temporaries, &stored) != 0 {
                        then_edge
                    } else {
                        else_edge
                    }
                }
                MirTerminatorKind::Return(value) => {
                    return (
                        visited,
                        value
                            .as_ref()
                            .map(|value| operand(value, &temporaries, &stored)),
                    );
                }
                other => panic!("unexpected loop terminator: {other:?}"),
            };

            let arguments = edge
                .arguments()
                .iter()
                .map(|value| operand(value, &temporaries, &stored))
                .collect::<Vec<_>>();

            block = edge.target();

            for (parameter, value) in unit
                .block(block)
                .unwrap()
                .parameters()
                .iter()
                .zip(arguments)
            {
                temporaries.insert(*parameter, value);
            }
        }
    }
}
