use std::collections::BTreeMap;
use std::sync::Arc;

use bray_bound_tree::{StorageAccessId, StorageCleanupSchedule, StorageExitPoint};
use bray_compiler_known::RepresentationRole;
use bray_ir::{MirBlockId, MirCleanupPhase, MirEdge, MirOperand, MirSourceAnchor, MirTerminatorKind};

use crate::lowering::LoweringError;
use crate::lowering::arrays::CleanupPartCursor;
use crate::lowering::lowerer::Lowerer;

use super::steps::CleanupStep;

impl Lowerer<'_> {
    pub(in crate::lowering) fn push_conditional_lifecycle(
        &mut self,
        entry: MirBlockId,
        source: &MirSourceAnchor,
        exit: StorageExitPoint,
        schedule: &StorageCleanupSchedule,
        selected: &[StorageAccessId],
    ) -> Result<MirBlockId, LoweringError> {
        let StorageCleanupSchedule::Conditional { priority, consumers } = schedule else {
            unreachable!("conditional cleanup retains its checked priority and dependencies");
        };

        if selected.is_empty() {
            return Ok(entry);
        }

        let kind = self.builder.block_kind(entry);
        let header = self.builder.push_block(Self::retained_source(source), kind)?;
        let complete = self.builder.push_block(Self::retained_source(source), kind)?;
        let boolean = self.representation_type(RepresentationRole::ScalarBool)?;
        let mut guards = BTreeMap::new();

        for &access in selected {
            let place = self.place_for_access(access, false)?;

            let initialization = self.initialization_guards.get(&place.storage())
                .expect("conditional lifecycle uses the owner's initialization state");

            let root = Self::retained_place(&initialization.guard);
            let parts = Arc::clone(&initialization.parts);

            let present = if parts.is_empty() {
                Self::retained_place(&root)
            } else {
                self.new_initialization_guard(entry, source, boolean, &[], false)?
            };

            guards.insert(access, (place, root, parts, present));
        }

        self.set_terminator(entry, Self::retained_source(source), MirTerminatorKind::Goto(MirEdge::new(header, [])))?;

        let mut current = header;

        for (&access, (place, root, parts, present)) in &guards {
            if parts.is_empty() {
                continue;
            }

            self.set_boolean_guard(current, source, present, false)?;

            current = self.guarded_cleanup_region(current, source, Self::retained_place(place), Some(Self::retained_place(root)), None,
                |lowerer, reached, place, _| lowerer.push_part_cleanup(
                    reached, source, MirCleanupPhase::LifecycleResolution,
                    CleanupPartCursor::new(access, place, Arc::clone(parts), 0), None, Some(present),
                ))?.0;
        }

        for (&access, consumers) in priority.iter().zip(consumers.iter()) {
            let Some((place, _, _, present)) = guards.get(&access) else { continue; };

            let next = self.builder.push_block(Self::retained_source(source), kind)?;
            let mut ready = self.builder.push_block(Self::retained_source(source), kind)?;

            self.set_terminator(current, Self::retained_source(source), MirTerminatorKind::Branch {
                condition: MirOperand::Copy(Self::retained_place(present)),
                then_edge: MirEdge::new(ready, []),
                else_edge: MirEdge::new(next, []),
            })?;

            for consumer in consumers.iter().filter_map(|access| guards.get(access).map(|(_, _, _, guard)| guard)) {
                let unblocked = self.builder.push_block(Self::retained_source(source), kind)?;

                self.set_terminator(ready, Self::retained_source(source), MirTerminatorKind::Branch {
                    condition: MirOperand::Copy(Self::retained_place(consumer)),
                    then_edge: MirEdge::new(next, []),
                    else_edge: MirEdge::new(unblocked, []),
                })?;

                ready = unblocked;
            }

            let discharged = self.input.finalizer_is_complete(exit.exit(), access);
            let occurrence = self.cleanup_occurrence;

            let (resolved, _) = self.with_cleanup_remainder(
                || {
                    let remaining = selected.iter().copied().filter(|other| *other != access).collect();

                    CleanupStep::conditional(occurrence, exit, schedule, remaining)
                },
                |lowerer| lowerer.push_cleanup_place(
                    ready, source, MirCleanupPhase::LifecycleResolution, access,
                    Self::retained_place(place), discharged, None,
                ),
            )?;

            self.set_storage_initialized(resolved, source, place, false)?;
            self.set_terminator(resolved, Self::retained_source(source), MirTerminatorKind::Goto(MirEdge::new(header, [])))?;

            current = next;
        }

        // Every reached state admits an order. A pending owner here would have no ready predecessor.
        let invalid = self.builder.push_block(Self::retained_source(source), kind)?;

        self.set_terminator(invalid, Self::retained_source(source), MirTerminatorKind::Unreachable)?;

        for (_, _, _, guard) in guards.values() {
            let next = self.builder.push_block(Self::retained_source(source), kind)?;

            self.set_terminator(current, Self::retained_source(source), MirTerminatorKind::Branch {
                condition: MirOperand::Copy(Self::retained_place(guard)),
                then_edge: MirEdge::new(invalid, []),
                else_edge: MirEdge::new(next, []),
            })?;

            current = next;
        }

        self.set_terminator(current, Self::retained_source(source), MirTerminatorKind::Goto(MirEdge::new(complete, [])))?;

        Ok(complete)
    }
}
