use bray_bound_tree::{BoundCallResult, StorageProtocolCall};
use bray_ir::{
    MirBlockId, MirCallableReference, MirOperand, MirOperationKind, MirPlace, MirProjection,
    MirProjectionKind, MirSourceAnchor, MirStorageKind, MirStoreKind,
};
use bray_symbols::{BorrowKind, CallableAbi, TypeData, TypeId};

use super::LoweringError;
use super::lowerer::Lowerer;

impl Lowerer<'_> {
    /// Retains the owning path when a protocol returns a pointer into separately allocated storage.
    pub(super) fn ownership_place(&self, place: &MirPlace) -> MirPlace {
        if place
            .projections()
            .first()
            .is_none_or(|projection| projection.kind() != &MirProjectionKind::Dereference)
        {
            return Self::retained_place(place);
        }

        let Some(owner) = self.owned_targets.get(&place.storage()) else {
            return Self::retained_place(place);
        };

        MirPlace::new(
            owner.storage(),
            owner
                .projections()
                .iter()
                .cloned()
                .chain(place.projections().iter().skip(1).cloned()),
            place.ty(),
        )
    }

    pub(super) fn owned_target_call(
        &self,
        owner: TypeId,
        kind: BorrowKind,
    ) -> Option<StorageProtocolCall> {
        self.input.storage_plan().owned_borrow(owner, kind)
    }

    pub(super) fn project_owned_target(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        owner: &MirPlace,
        call: StorageProtocolCall,
        target: TypeId,
    ) -> Result<MirPlace, LoweringError> {
        let value = self.push_storage_protocol_call(block, source, owner, call, false)?;

        let storage = self.builder.push_storage(
            Self::retained_source(source),
            MirStorageKind::Temporary,
            call.result(),
        )?;

        let pointer = MirPlace::new(storage, [], call.result());

        self.push_operation(
            block,
            Self::retained_source(source),
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: pointer,
                value,
            },
            None,
        )?;

        let logical = self.ownership_place(owner);

        let logical = MirPlace::new(
            logical.storage(),
            logical
                .projections()
                .iter()
                .cloned()
                .chain([MirProjection::new(
                    MirProjectionKind::Dereference,
                    owner.ty(),
                    target,
                )]),
            target,
        );

        self.owned_targets.insert(storage, logical);

        Ok(MirPlace::new(
            storage,
            [MirProjection::new(
                MirProjectionKind::Dereference,
                call.result(),
                target,
            )],
            target,
        ))
    }

    pub(super) fn push_storage_protocol_call(
        &mut self,
        block: MirBlockId,
        source: &MirSourceAnchor,
        owner: &MirPlace,
        call: StorageProtocolCall,
        cleanup: bool,
    ) -> Result<MirOperand, LoweringError> {
        let data = self.input.semantic_values().type_data(owner.ty())?;

        let TypeData::OwnedIndirection { storage, .. } = data.as_ref() else {
            return Err(LoweringError::SemanticValueUnavailable);
        };

        let policy = MirPlace::new(
            owner.storage(),
            owner
                .projections()
                .iter()
                .cloned()
                .chain([MirProjection::new(
                    MirProjectionKind::OwnedStorage,
                    owner.ty(),
                    *storage,
                )]),
            *storage,
        );

        let parameter = self.input.semantic_values().type_data(call.parameter())?;

        let borrow = match parameter.as_ref() {
            TypeData::Borrow { kind, .. } => Some(*kind),
            _ => None,
        };

        let value = crate::lifecycle_call::lower_lifecycle_call(
            MirCallableReference::new(call.callable(), CallableAbi::Bray),
            call.parameter(),
            policy,
            borrow,
            BoundCallResult::Immediate(call.result()),
            cleanup,
            |operation, result| {
                self.push_operation(block, Self::retained_source(source), operation, result)
            },
        )?;

        Ok(MirOperand::Value(value))
    }
}
