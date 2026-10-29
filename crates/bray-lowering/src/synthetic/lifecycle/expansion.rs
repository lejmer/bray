use std::collections::{BTreeMap, BTreeSet};

use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirCleanupCollection, MirFrameDescriptor, MirGeneratedLifecycleRole, MirOperand,
    MirOperationKind, MirPlace, MirSourceAnchor, MirStorageKind, MirStoreKind, MirTerminatorKind,
    MirUnit, MirUnitBuilder, MirValueId,
};

use super::super::{SyntheticLowerer, SyntheticLoweringContext, SyntheticLoweringError};
use crate::cleanup_outcome::CleanupOutcome;

/// Expands selected structural cleanup into its checked enclosing frame before layout.
/// Synchronous units retain their ordinary calling convention. No new activation is
/// introduced for structural traversal, and all generated actions share the original collection.
pub fn expand_lifecycle_continuations<C: SyntheticLoweringContext + ?Sized>(
    context: &C,
    unit: &MirUnit,
) -> Result<Option<MirUnit>, C::Error> {
    let Some(frame) = unit.frame_descriptor() else {
        return Ok(None);
    };

    if !unit
        .operations()
        .iter()
        .any(|operation| selected_cleanup(operation.kind()).is_some())
    {
        return Ok(None);
    }

    let mut lowerer = SyntheticLowerer {
        context,
        abandoned: None,
        collection: None,
        pending_reports: None,
        frame: Some(frame.frame()),
        // Expansion extends the enclosing descriptor once, not once per cleanup site.
        frame_states: frame.states().to_vec(),
        continuation: None,
        owner_retirement: None,
    };

    let mut builder = MirUnitBuilder::for_expansion(unit);
    let mut index = 0;

    while index < builder.block_count() {
        let block = MirBlockId::from_slot(
            unit.unit(),
            u32::try_from(index).expect("allocated MIR block identity fits u32"),
        );

        while builder
            .last_operation(block)
            .is_some_and(|operation| selected_cleanup(operation.kind()).is_some())
        {
            if !lowerer.expand_cleanup_tail(&mut builder, block)? {
                break;
            }
        }

        index += 1;
    }

    let descriptor = MirFrameDescriptor::new(
        frame.frame(),
        frame.abi_version(),
        frame.frame_abi(),
        frame.result_type(),
        frame.completion_state(),
        lowerer.frame_states,
    )
    .map_err(SyntheticLoweringError::Capacity)?;

    builder.set_frame_descriptor(descriptor.with_capture_cleanup(frame.capture_cleanup()));

    Ok(Some(builder.finish(unit.entry())))
}

fn selected_cleanup(
    operation: &MirOperationKind,
) -> Option<(MirGeneratedLifecycleRole, &MirPlace, &MirCleanupCollection)> {
    let (role, place, collection) = match operation {
        // Cancellation broadcast cannot suspend. Keep its shared synchronous helper
        // instead of copying the structural traversal into every retained frame state.
        MirOperationKind::Cleanup {
            phase: bray_ir::MirCleanupPhase::TaskCancellation,
            ..
        } => return None,
        MirOperationKind::Cleanup {
            phase,
            place,
            collection,
        } => (
            MirGeneratedLifecycleRole::Cleanup(*phase),
            place,
            collection,
        ),
        MirOperationKind::Quiesce { place, collection } => {
            (MirGeneratedLifecycleRole::Quiesce, place, collection)
        }
        MirOperationKind::ReleaseBuffer { place, collection } => {
            (MirGeneratedLifecycleRole::ReleaseBuffer, place, collection)
        }
        MirOperationKind::Destroy {
            place,
            collection,
            abandoned,
        } => (
            MirGeneratedLifecycleRole::Destroy(bray_ir::MirDestructionDisposition::from_argument(
                abandoned.as_ref(),
            )),
            place,
            collection,
        ),
        _ => return None,
    };

    collection
        .as_deref()
        .filter(|collection| collection.retention().is_some())
        .map(|collection| (role, place, collection))
}

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    fn expand_cleanup_tail(
        &mut self,
        builder: &mut MirUnitBuilder,
        block: MirBlockId,
    ) -> Result<bool, C::Error> {
        let (role, place, _) = selected_cleanup(
            builder
                .last_operation(block)
                .expect("selected cleanup has a final operation")
                .kind(),
        )
        .expect("selected cleanup retains its operation contract");

        let action = self.context.lifecycle_action(place.ty(), role.phase())?;

        let indirect = matches!(
            action,
            bray_bound_tree::LifecycleAction::RawBuffer(_)
                | bray_bound_tree::LifecycleAction::Generator(_)
                | bray_bound_tree::LifecycleAction::Owned { .. }
        );

        if indirect && !self.context.lifecycle_requires_frame(place.ty(), role)? {
            return Ok(false);
        }

        let buffer = matches!(
            action,
            bray_bound_tree::LifecycleAction::RawBuffer(_)
                | bray_bound_tree::LifecycleAction::Generator(_)
        );

        let discharge =
            matches!(role, MirGeneratedLifecycleRole::Destroy(_)) && (!indirect || buffer);

        let (operation, terminal) = builder.take_last_effect(block);

        let source = operation.source();

        // The input nodes keep shared provenance while expansion substitutes their boundary inputs.
        let mut kind = operation.kind().clone();
        let mut terminal = terminal.kind().clone();

        let MirTerminatorKind::Goto(continuation) = &terminal else {
            panic!("structural cleanup must end at its established continuation");
        };

        assert!(
            continuation.arguments().is_empty(),
            "cleanup carries pending owners in established storage"
        );

        let continuation = continuation.target();

        retain_boundary_values(builder, block, source, &mut kind, &mut terminal)
            .map_err(|cause| self.capacity_error(cause))?;

        let (role, place, collection) =
            selected_cleanup(&kind).expect("selected structural cleanup retains its collection");

        let finish = if discharge {
            builder
                .push_block(source.clone(), builder.block_kind(block))
                .map_err(|cause| self.capacity_error(cause))?
        } else {
            continuation
        };

        self.continuation = Some(finish);

        let state = self
            .context
            .representation_type(RepresentationRole::ScalarU32)?;

        let states = CleanupOutcome::state_constants(self.context.semantic_values(), state)
            .map_err(SyntheticLoweringError::SemanticValue)?;

        self.collection = Some(CleanupOutcome::from_collection(
            collection.clone(),
            states.1,
            self.context
                .representation_type(RepresentationRole::ScalarBool)?,
            self.context.representation_type(RepresentationRole::Unit)?,
            builder.target().runtime_abi(),
        ));

        self.abandoned = match &kind {
            MirOperationKind::ReleaseBuffer { .. } => Some(MirOperand::Immediate {
                ty: self
                    .context
                    .representation_type(RepresentationRole::ScalarBool)?,
                value: bray_ir::MirImmediateValue::Boolean(false),
            }),
            MirOperationKind::Destroy { abandoned, .. } => Some(
                abandoned.clone().unwrap_or(MirOperand::Immediate {
                    ty: self
                        .context
                        .representation_type(RepresentationRole::ScalarBool)?,
                    value: bray_ir::MirImmediateValue::Boolean(false),
                }),
            ),
            _ => None,
        };

        let abi = builder.target().runtime_abi();

        let end = if indirect {
            self.push_lifecycle_frame(
                builder,
                block,
                source,
                if buffer && matches!(role, MirGeneratedLifecycleRole::Destroy(_)) {
                    MirGeneratedLifecycleRole::ReleaseBuffer
                } else {
                    role
                },
                place.clone(),
            )?
        } else {
            self.expand_lifecycle_action(
                builder,
                block,
                source,
                role,
                place.clone(),
                abi,
                action,
                None,
            )?
        };

        if discharge {
            builder.set_terminator(
                end,
                source.clone(),
                MirTerminatorKind::Goto(bray_ir::MirEdge::new(finish, [])),
            );

            builder
                .push_operation(
                    finish,
                    source.clone(),
                    MirOperationKind::DischargeOutgoing {
                        owner: bray_ir::MirOutgoingOwner::Value(place.ty()),
                        runtime: bray_ir::MirRuntimeReference::new(
                            bray_runtime_interface::RuntimeAbiRole::OutgoingDischarge,
                            abi,
                        ),
                    },
                    None,
                )
                .map_err(|cause| self.capacity_error(cause))?;
        }

        let completed = if discharge { finish } else { end };

        builder.set_terminator(completed, source.clone(), terminal);

        Ok(true)
    }
}

fn retain_boundary_values(
    builder: &mut MirUnitBuilder,
    block: MirBlockId,
    source: &MirSourceAnchor,
    operation: &mut MirOperationKind,
    terminal: &mut MirTerminatorKind,
) -> Result<(), bray_ir::MirCapacityError> {
    let mut values = BTreeSet::<MirValueId>::new();

    let mut collect = |operand: &MirOperand| {
        if let MirOperand::Value(value) = operand {
            values.insert(*value);
        }
    };

    operation.for_each_operand(&mut collect);
    terminal.for_each_input(&mut collect);

    terminal
        .try_for_each_edge_mut::<()>(|edge| {
            for argument in edge.arguments() {
                argument.for_each_operand(&mut collect);
            }

            Ok(())
        })
        .expect("observing continuation arguments cannot fail");

    let mut replacements = BTreeMap::new();

    for value in values {
        let operand = MirOperand::Value(value);
        let ty = builder.operand_type(&operand);
        let storage = builder.push_storage(source.clone(), MirStorageKind::Temporary, ty)?;
        let place = MirPlace::new(storage, [], ty);

        builder.push_operation(
            block,
            source.clone(),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: place.clone(),
                value: operand,
            },
            None,
        )?;

        replacements.insert(value, MirOperand::Copy(place));
    }

    operation.substitute_values(&replacements);
    terminal.substitute_values(&replacements);

    Ok(())
}
