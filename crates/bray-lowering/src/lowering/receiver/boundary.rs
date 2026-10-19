use bray_bound_tree::{StorageAccessId, StorageIdentity, StorageIdentityId};
use bray_compiler_known::RepresentationRole;
use bray_ir::{
    MirBlockId, MirBlockKind, MirCleanupPhase, MirEdge, MirOperationKind, MirPlace,
    MirSourceAnchor, MirStorageKind, MirStoreKind, MirTerminatorKind,
};

use crate::lowering::LoweringError;
use crate::lowering::lowerer::Lowerer;

pub(in crate::lowering) struct ReceiverCleanup {
    pub(in crate::lowering) identity: StorageIdentityId,
    pub(in crate::lowering) selector: MirPlace,
    pub(in crate::lowering) entries: Vec<(MirBlockId, MirBlockId, MirBlockId)>,
    pub(in crate::lowering) mode: Option<bool>,
}

impl ReceiverCleanup {
    pub(in crate::lowering) fn pending_successors(
        &self,
        block: MirBlockId,
    ) -> impl Iterator<Item = MirBlockId> + '_ {
        self.entries
            .iter()
            .filter(move |(handoff, _, _)| *handoff == block)
            .flat_map(|(_, ordinary, abandoned)| [*ordinary, *abandoned])
    }
}

impl Lowerer<'_> {
    pub(in crate::lowering) fn initialize_receiver_cleanup(
        &mut self,
        source: &MirSourceAnchor,
    ) -> Result<(), LoweringError> {
        if self.input.unit().key().kind() != bray_bound_tree::BoundUnitKind::CallableBody
            || self.input.unit().key().declared_owner().kind()
                != bray_symbols::SymbolKind::Destructor
        {
            return Ok(());
        }

        let identity = self
            .input
            .storage_plan()
            .identity_entries()
            .find_map(|(id, identity)| {
                matches!(identity, StorageIdentity::Receiver(_)).then_some(id)
            })
            .expect("a checked destructor retains its consuming receiver");

        let ty = self.representation_type(RepresentationRole::ScalarU32)?;

        let storage =
            self.builder
                .push_storage(Self::retained_source(source), MirStorageKind::Local, ty)?;

        let selector = MirPlace::new(storage, [], ty);

        self.receiver_cleanup = Some(ReceiverCleanup {
            identity,
            selector,
            entries: Vec::new(),
            mode: None,
        });

        Ok(())
    }

    pub(in crate::lowering) fn defer_receiver_cleanup(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        access: StorageAccessId,
        place: MirPlace,
        completed: bool,
    ) -> Result<MirBlockId, LoweringError> {
        let ordinary = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let abandoned = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let continuation = self.builder.push_block(
            Self::retained_source(source),
            MirBlockKind::LifecycleResolution,
        )?;

        let receiver = self
            .receiver_cleanup
            .as_mut()
            .expect("receiver boundary belongs to a destructor");

        receiver.entries.push((block, ordinary, abandoned));

        let entry = u64::try_from(receiver.entries.len())
            .expect("MIR block capacity bounds cleanup entries");

        let selector = Self::retained_place(&receiver.selector);

        let selected =
            crate::operand::integer_constant(self.input.semantic_values(), selector.ty(), entry)?;

        // The immutable view refers to the same admitted report slots across the handoff.
        let outcome = self.cleanup_outcome.clone();

        self.builder.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: MirStoreKind::Assign,
                destination: Self::retained_place(&selector),
                value: selected,
            },
            None,
        )?;

        if let Some(outcome) = &outcome {
            outcome.end_shield(&mut self.builder, block, source)?;
        }

        self.set_terminator(
            block,
            Self::retained_source(source),
            MirTerminatorKind::Return {
                value: None,
                report: None,
            },
        )?;

        for (entry, mode) in [(ordinary, false), (abandoned, true)] {
            let zero =
                crate::operand::integer_constant(self.input.semantic_values(), selector.ty(), 0)?;

            self.builder.push_operation(
                entry,
                Self::retained_source(source),
                MirOperationKind::Store {
                    kind: MirStoreKind::Assign,
                    destination: Self::retained_place(&selector),
                    value: zero,
                },
                None,
            )?;

            if outcome.is_some() {
                self.cleanup_shield(
                    entry,
                    source,
                    bray_runtime_interface::RuntimeAbiRole::CleanupShieldEnter,
                )?;
            }

            self.receiver_cleanup
                .as_mut()
                .expect("receiver boundary remains active")
                .mode = Some(mode);

            let (end, _) = self.push_cleanup_place(
                entry,
                source,
                MirCleanupPhase::LifecycleResolution,
                access,
                Self::retained_place(&place),
                completed,
                None,
            )?;

            self.set_terminator(
                end,
                Self::retained_source(source),
                MirTerminatorKind::Goto(MirEdge::new(continuation, [])),
            )?;
        }

        self.receiver_cleanup
            .as_mut()
            .expect("receiver boundary remains active")
            .mode = None;

        Ok(continuation)
    }
}
