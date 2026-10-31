use super::super::mapping::LocalIds;
use crate::unit::local_id_remap::MirLocalIdMapping;
use crate::{
    MirBlockId, MirCapacityError, MirOperand, MirOperationId, MirPlace, MirStorageId,
    MirStorageKind, MirTerminatorKind, MirUnit, MirUnitBuilder, MirValueId,
};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct InlineValues {
    pub(super) parameter_order: Vec<MirStorageId>,
    pub(super) parameters: BTreeMap<MirBlockId, BTreeMap<MirStorageId, MirValueId>>,
    pub(super) retained: BTreeMap<MirBlockId, Vec<MirValueId>>,
    pub(super) continuation: BTreeMap<MirValueId, MirValueId>,
    pub(super) spills: BTreeMap<MirValueId, MirPlace>,
}

impl InlineValues {
    pub(super) fn prepare(
        caller: &MirUnit,
        callee: &MirUnit,
        site: (MirOperationId, &crate::MirBlock),
        continuation: (MirBlockId, &mut MirTerminatorKind),
        parameters: &BTreeMap<u32, MirStorageId>,
        ids: &LocalIds<'_>,
        builder: &mut MirUnitBuilder,
    ) -> Result<Self, MirCapacityError> {
        let source = caller.operation(site.0).expect("call operation").source();
        let retained = continuation_values(caller, site.1, site.0, continuation.1);

        let mut values = Self {
            parameter_order: parameters.values().copied().collect(),
            parameters: BTreeMap::new(),
            retained: BTreeMap::new(),
            continuation: BTreeMap::new(),
            spills: BTreeMap::new(),
        };

        for (block, _) in callee.blocks_with_ids() {
            let mut inputs = BTreeMap::new();

            for parameter in &values.parameter_order {
                let ty = ids.ty(callee.storage(*parameter).expect("callee parameter").ty());

                inputs.insert(
                    *parameter,
                    builder.push_block_parameter(ids.block(block), source.clone(), ty)?,
                );
            }

            values.parameters.insert(block, inputs);

            values.retained.insert(
                block,
                retained
                    .iter()
                    .map(|value| {
                        builder.push_block_parameter(
                            ids.block(block),
                            source.clone(),
                            caller.value(*value).expect("caller value").ty(),
                        )
                    })
                    .collect::<Result<Vec<_>, _>>()?,
            );
        }

        let has_failure = callee.blocks().iter().any(|block| {
            matches!(
                block.terminator().kind(),
                MirTerminatorKind::CheckCallOutcome { .. }
            )
        });

        for value in retained {
            let ty = caller.value(value).expect("caller value").ty();

            values.continuation.insert(
                value,
                builder.push_block_parameter(continuation.0, source.clone(), ty)?,
            );

            if has_failure {
                let storage = builder.push_storage(source.clone(), MirStorageKind::Local, ty)?;

                values.spills.insert(value, MirPlace::new(storage, [], ty));
            }
        }

        Ok(values)
    }

    pub(super) fn map_caller(
        &self,
        ids: &mut LocalIds<'_>,
        values: impl IntoIterator<Item = MirValueId>,
    ) {
        for (old, value) in self.continuation.keys().zip(values) {
            ids.values[old.to_index().expect("caller value slot")] = Some(value);
        }
    }
}

fn continuation_values(
    unit: &MirUnit,
    block: &crate::MirBlock,
    site: MirOperationId,
    continuation: &mut MirTerminatorKind,
) -> BTreeSet<MirValueId> {
    // Expanded effects retain arena identities while inserting work before an old
    // suffix. Only execution order establishes which values exist at this call.
    let available = block
        .parameters()
        .iter()
        .copied()
        .chain(
            block
                .operations()
                .iter()
                .take_while(|operation| **operation != site)
                .filter_map(|operation| {
                    unit.operation(*operation)
                        .expect("valid prefix operation")
                        .result()
                }),
        )
        .collect::<BTreeSet<_>>();

    let mut values = BTreeSet::new();

    let mut visit = |operand: &MirOperand| {
        if let MirOperand::Value(value) = operand
            && available.contains(value)
        {
            values.insert(*value);
        }
    };

    for operation in block
        .operations()
        .iter()
        .skip_while(|operation| **operation != site)
        .skip(1)
    {
        unit.operation(*operation)
            .expect("valid suffix operation")
            .kind()
            .for_each_operand(&mut visit);
    }

    continuation.for_each_input(&mut visit);

    continuation
        .try_for_each_edge_mut::<()>(|edge| {
            for argument in edge.arguments() {
                argument.for_each_operand(&mut visit);
            }

            Ok(())
        })
        .expect("reading continuation edges cannot fail");

    values
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::Arc;

    use bray_testing::test_bound_unit;

    use super::continuation_values;
    use crate::{
        MirBlock, MirBlockKind, MirImmediateValue, MirOperand, MirOperationKind, MirSourceAnchor,
        MirTerminatorKind, MirUnaryOperator, MirUnitBuilder, MirUnitKind,
    };

    #[test]
    fn scalar_continuation_tracks_execution_order_after_effect_expansion() {
        let bound = test_bound_unit(801);
        let source = MirSourceAnchor::from(bound.key().source());
        let ty = crate::test_support::test_type();

        let mut builder = MirUnitBuilder::for_bound(
            bound.identity(),
            MirUnitKind::Synchronous,
            crate::test_support::test_target(),
        );

        let entry = builder
            .push_block(source.clone(), MirBlockKind::Ordinary)
            .expect("fixture block must fit its identity space");

        let mut unary = |operand| {
            builder
                .push_operation(
                    entry,
                    source.clone(),
                    MirOperationKind::Unary {
                        operator: MirUnaryOperator::Not,
                        operand,
                    },
                    Some(ty),
                )
                .expect("fixture operation must fit its identity space")
        };

        let prefix = unary(MirOperand::Immediate {
            ty,
            value: MirImmediateValue::Boolean(false),
        });

        let suffix = unary(MirOperand::Value(
            prefix.result().expect("fixture `result` call must succeed"),
        ));

        let use_suffix = unary(MirOperand::Value(
            suffix.result().expect("fixture `result` call must succeed"),
        ));

        let site = unary(MirOperand::Immediate {
            ty,
            value: MirImmediateValue::Boolean(true),
        });

        builder.set_terminator(
            entry,
            source,
            MirTerminatorKind::Return {
                value: Some(MirOperand::Value(
                    site.result().expect("fixture `result` call must succeed"),
                )),
                report: None,
            },
        );

        let mut unit = builder.finish(entry);
        let block = unit.blocks()[0].clone();

        // Effect expansion inserts newly allocated operations ahead of an existing suffix.
        Arc::make_mut(&mut unit.blocks)[0] = MirBlock::new(
            block.source().clone(),
            block.kind(),
            block.parameters().to_vec(),
            vec![
                prefix.operation(),
                site.operation(),
                suffix.operation(),
                use_suffix.operation(),
            ],
            block.terminator().clone(),
        );

        assert!(unit.is_valid());

        let block = unit.block(entry).expect("fixture MIR block must exist");
        let mut continuation = block.terminator().kind().clone();

        assert_eq!(
            continuation_values(&unit, block, site.operation(), &mut continuation),
            BTreeSet::from([prefix.result().expect("fixture `result` call must succeed")]),
        );
    }
}
