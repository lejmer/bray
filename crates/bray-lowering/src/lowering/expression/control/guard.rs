use bray_bound_tree::{BoundNodeOrigin, StorageIdentityId};
use bray_ir::{
    MirBlockId, MirOperand, MirOperationKind, MirPlace, MirProjection, MirProjectionKind,
    MirStorageKind, MirStoreKind, MirUnitBuildError,
};
use bray_symbols::{BorrowKind, TypeData, TypeId};

use crate::lowering::{LoweringError, lowerer::Lowerer};

impl Lowerer<'_> {
    pub(super) fn store_guard_binding(
        &mut self,
        identity: StorageIdentityId,
        value: MirOperand,
        ty: TypeId,
        origin: BoundNodeOrigin,
        block: MirBlockId,
    ) -> Result<(), LoweringError> {
        let source = self.source(origin);

        let place = match value {
            MirOperand::Copy(place) | MirOperand::Move(place) => place,
            value => {
                let storage = self.builder.push_storage(
                    Self::retained_source(&source),
                    MirStorageKind::Temporary,
                    ty,
                )?;

                let place = MirPlace::new(storage, [], ty);

                self.push_operation(
                    block,
                    Self::retained_source(&source),
                    MirOperationKind::Store {
                        kind: MirStoreKind::Initialize,
                        destination: Self::retained_place(&place),
                        value,
                    },
                    None,
                )?;

                place
            }
        };

        let borrow_type = self.input.semantic_values().intern_type(TypeData::Borrow {
            kind: BorrowKind::Shared,
            target: ty,
        })?;

        let alias = match self
            .guard_bindings
            .last()
            .and_then(|bindings| bindings.get(&identity))
        {
            Some(alias) => Self::retained_place(alias),
            None => {
                let storage = self.builder.push_storage(
                    Self::retained_source(&source),
                    MirStorageKind::Temporary,
                    borrow_type,
                )?;

                MirPlace::new(
                    storage,
                    [MirProjection::new(
                        MirProjectionKind::Dereference,
                        borrow_type,
                        ty,
                    )],
                    ty,
                )
            }
        };

        let borrowed = self.push_operation(
            block,
            Self::retained_source(&source),
            MirOperationKind::Borrow {
                kind: BorrowKind::Shared,
                place,
            },
            Some(borrow_type),
        )?;

        let value = borrowed
            .result()
            .ok_or(MirUnitBuildError::MissingOperationResult(
                borrowed.operation(),
            ))?;

        self.push_operation(
            block,
            source,
            MirOperationKind::Store {
                kind: MirStoreKind::Initialize,
                destination: MirPlace::new(alias.storage(), [], borrow_type),
                value: MirOperand::Value(value),
            },
            None,
        )?;

        if let Some(bindings) = self.guard_bindings.last_mut() {
            bindings.insert(identity, alias);
        }

        Ok(())
    }
}
