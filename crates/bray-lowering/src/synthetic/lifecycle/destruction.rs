use std::collections::BTreeMap;

use bray_bound_tree::LifecycleCallable;
use bray_ir::{
    MirBlockId, MirCleanupState, MirEdge, MirImmediateValue, MirOperand, MirOperationKind,
    MirPlace, MirSourceAnchor, MirStorageKind, MirStoreKind, MirSwitchCase, MirTerminatorKind,
    MirUnitBuilder, MirValueId,
};
use bray_symbols::{BorrowKind, TypeData};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};
use crate::DestructionBody;

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(super) fn push_destruction_body(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        callable: LifecycleCallable,
    ) -> Result<(MirBlockId, MirValueId), C::Error> {
        let body = self.context.destruction_body(callable.callable)?;

        let pointer = self
            .context
            .semantic_values()
            .intern_type(TypeData::Borrow {
                kind: BorrowKind::Mutable,
                target: place.ty(),
            })
            .map_err(SyntheticLoweringError::SemanticValue)?;

        let parameter = self.prepare_receiver_record(builder, block, source, &body)?;

        let result = crate::lifecycle_call::lower_lifecycle_call(
            bray_ir::MirCallableReference::new(callable.callable, callable.abi),
            pointer,
            place.clone(),
            Some(BorrowKind::Mutable),
            callable.result,
            true,
            parameter
                .as_ref()
                .map(|place| MirOperand::Copy(place.clone())),
            |operation, ty| builder.push_operation(block, source.clone(), operation, ty),
        )
        .map_err(|cause| self.capacity_error(cause))?;

        let (block, result) =
            self.check_lifecycle_value(builder, block, source, result, callable.result.ty())?;

        let Some(parameter) = parameter else {
            return Ok((block, result));
        };

        let finish = self.push_receiver_tail(builder, block, source, place, &body, parameter)?;

        Ok((finish, result))
    }

    fn prepare_receiver_record(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        body: &DestructionBody,
    ) -> Result<Option<MirPlace>, C::Error> {
        let Some(tail) = body.unit.cleanup_continuation() else {
            return Ok(None);
        };

        let storage = body
            .unit
            .storage(tail.context())
            .expect("checked body has its record parameter");

        let pointer = body.types[&storage.ty()];

        let TypeData::Borrow { target, .. } = *self.context.semantic_values().type_data(pointer)
        else {
            panic!("a checked destructor record parameter is a mutable borrow");
        };

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, target)
            .map_err(|cause| self.capacity_error(cause))?;

        let record = MirPlace::new(storage, [], target);

        let borrowed = builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Borrow {
                    kind: BorrowKind::Mutable,
                    place: record,
                },
                Some(pointer),
            )
            .map_err(|cause| self.capacity_error(cause))?;

        let storage = builder
            .push_storage(source.clone(), MirStorageKind::Temporary, pointer)
            .map_err(|cause| self.capacity_error(cause))?;

        let parameter = MirPlace::new(storage, [], pointer);

        builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Store {
                    kind: MirStoreKind::Initialize,
                    destination: parameter.clone(),
                    value: MirOperand::Value(
                        borrowed
                            .result()
                            .expect("record borrow produces its pointer"),
                    ),
                },
                None,
            )
            .map_err(|cause| self.capacity_error(cause))?;

        let selector = receiver_selector(body, &parameter);

        let zero =
            crate::operand::integer_constant(self.context.semantic_values(), selector.ty(), 0)
                .map_err(SyntheticLoweringError::SemanticValue)?;

        builder
            .push_operation(
                block,
                source.clone(),
                MirOperationKind::Store {
                    kind: MirStoreKind::Initialize,
                    destination: selector,
                    value: zero,
                },
                None,
            )
            .map_err(|cause| self.capacity_error(cause))?;

        Ok(Some(parameter))
    }

    fn push_receiver_tail(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
        place: MirPlace,
        body: &DestructionBody,
        parameter: MirPlace,
    ) -> Result<MirBlockId, C::Error> {
        let tail = body
            .unit
            .cleanup_continuation()
            .expect("record belongs to a checked receiver tail");

        let outcome = self.cleanup_outcome(builder, block, source)?;

        let finish = builder
            .push_block(source.clone(), builder.block_kind(block))
            .map_err(|cause| self.capacity_error(cause))?;

        let panicked = self.tail_failure(
            builder,
            source,
            block,
            finish,
            &outcome,
            MirCleanupState::Panicked,
        )?;

        let cancelled = self.tail_failure(
            builder,
            source,
            block,
            finish,
            &outcome,
            MirCleanupState::Cancelled,
        )?;

        let receiver = body
            .unit
            .storages_with_ids()
            .find_map(|(id, storage)| {
                matches!(storage.kind(), MirStorageKind::BorrowedParameter(0)).then_some(id)
            })
            .expect("a source destructor borrows its consumed receiver storage");

        let places = BTreeMap::from([(receiver, place), (tail.context(), parameter.clone())]);
        let block = self.begin_receiver_resolution(builder, block, source)?;

        let disposition = self
            .abandoned
            .clone()
            .expect("destruction has its selected disposition");

        let no_tail = if builder.block_kind(block) == builder.block_kind(finish) {
            finish
        } else {
            let no_tail = builder
                .push_block(source.clone(), builder.block_kind(block))
                .map_err(|cause| self.capacity_error(cause))?;

            builder.set_terminator(
                no_tail,
                source.clone(),
                MirTerminatorKind::Goto(MirEdge::new(finish, [])),
            );

            no_tail
        };

        let variants = match disposition {
            MirOperand::Immediate {
                value: MirImmediateValue::Boolean(abandoned),
                ..
            } => vec![(block, abandoned)],
            disposition => {
                let ordinary = builder
                    .push_block(source.clone(), builder.block_kind(block))
                    .map_err(|cause| self.capacity_error(cause))?;

                let abandoned = builder
                    .push_block(source.clone(), builder.block_kind(block))
                    .map_err(|cause| self.capacity_error(cause))?;

                builder.set_terminator(
                    block,
                    source.clone(),
                    MirTerminatorKind::Branch {
                        condition: disposition,
                        then_edge: MirEdge::new(abandoned, []),
                        else_edge: MirEdge::new(ordinary, []),
                    },
                );

                vec![(ordinary, false), (abandoned, true)]
            }
        };

        for (entry, abandoned) in variants {
            // Each disposition owns its selected graph. The source machine body is shared.
            let entries = bray_ir::append_cleanup_continuation(
                builder,
                &body.unit,
                &body.types,
                &body.callables,
                places.clone(),
                abandoned,
                finish,
                panicked,
                cancelled,
                outcome.collection().retention(),
            )
            .map_err(|cause| self.capacity_error(cause))?;

            let selector = receiver_selector(body, &parameter);

            let cases = entries
                .into_iter()
                .chain([(0, no_tail)])
                .map(|(index, block)| {
                    let MirOperand::Constant { value, .. } = crate::operand::integer_constant(
                        self.context.semantic_values(),
                        selector.ty(),
                        u64::from(index),
                    )
                    .map_err(SyntheticLoweringError::SemanticValue)?
                    else {
                        unreachable!("entry selector is an integer constant");
                    };

                    Ok(MirSwitchCase::new(value, MirEdge::new(block, [])))
                })
                .collect::<Result<Vec<_>, C::Error>>()?;

            let invalid = builder
                .push_block(source.clone(), builder.block_kind(entry))
                .map_err(|cause| self.capacity_error(cause))?;

            builder.set_terminator(invalid, source.clone(), MirTerminatorKind::Unreachable);

            builder.set_terminator(
                entry,
                source.clone(),
                MirTerminatorKind::Switch {
                    discriminant: MirOperand::Copy(selector),
                    cases: cases.into(),
                    otherwise: MirEdge::new(invalid, []),
                },
            );
        }

        let finish = self.finish_cleanup_outcome(builder, finish, source, &outcome, &mut None)?;

        Ok(finish)
    }

    fn begin_receiver_resolution(
        &self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
        source: &MirSourceAnchor,
    ) -> Result<MirBlockId, C::Error> {
        Ok(match builder.block_kind(block) {
            bray_ir::MirBlockKind::LifecycleResolution => block,
            bray_ir::MirBlockKind::Ordinary => {
                let broadcast = builder
                    .push_block(source.clone(), bray_ir::MirBlockKind::CleanupBroadcast)
                    .map_err(|cause| self.capacity_error(cause))?;

                let resolution = builder
                    .push_block(source.clone(), bray_ir::MirBlockKind::LifecycleResolution)
                    .map_err(|cause| self.capacity_error(cause))?;

                builder.set_terminator(
                    block,
                    source.clone(),
                    MirTerminatorKind::BeginCleanup(bray_ir::MirCleanupEdge::new(
                        bray_ir::MirCleanupPhase::TaskCancellation,
                        MirEdge::new(broadcast, []),
                    )),
                );

                builder.set_terminator(
                    broadcast,
                    source.clone(),
                    MirTerminatorKind::ContinueCleanup(bray_ir::MirCleanupEdge::new(
                        bray_ir::MirCleanupPhase::LifecycleResolution,
                        MirEdge::new(resolution, []),
                    )),
                );

                resolution
            }
            bray_ir::MirBlockKind::CleanupBroadcast => {
                unreachable!("receiver destruction belongs to lifecycle resolution")
            }
        })
    }

    fn tail_failure(
        &self,
        builder: &mut MirUnitBuilder,
        source: &MirSourceAnchor,
        block: MirBlockId,
        finish: MirBlockId,
        outcome: &crate::cleanup_outcome::CleanupOutcome,
        state: MirCleanupState,
    ) -> Result<MirBlockId, C::Error> {
        let entry = builder
            .push_block(source.clone(), builder.block_kind(block))
            .map_err(|cause| self.capacity_error(cause))?;

        let report_type = builder.operand_type(&outcome.report());

        let report = builder
            .push_block_parameter(entry, source.clone(), report_type)
            .map_err(|cause| self.capacity_error(cause))?;

        outcome
            .retain_report(
                builder,
                entry,
                source,
                finish,
                state,
                MirOperand::Value(report),
            )
            .map_err(|cause| self.capacity_error(cause))?;

        Ok(entry)
    }
}

fn receiver_selector(body: &DestructionBody, parameter: &MirPlace) -> MirPlace {
    let selector = body
        .unit
        .cleanup_continuation()
        .expect("record belongs to a checked receiver tail")
        .selector();

    selector
        .projections()
        .iter()
        .fold(parameter.clone(), |place, projection| {
            place.project(
                projection.kind().clone(),
                body.types[&projection.result_type()],
            )
        })
}
