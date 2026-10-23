use bray_ir::{
    MirCapacityError, MirOperand, MirPlace, MirSourceAnchor, MirStorageKind, MirUnitBuilder,
};
use bray_runtime_interface::RuntimeAbiVersion;
use bray_symbols::TypeId;
use std::cell::Cell;
use std::rc::Rc;

pub(crate) enum CleanupFailure {
    Panic(MirOperand),
    Cancellation(Option<MirOperand>),
}

/// Retains cleanup incidents until every selected obligation has been resolved.
#[derive(Clone)]
pub(crate) struct CleanupOutcome {
    pub(super) state: MirPlace,
    pub(super) retention: Option<std::sync::Arc<bray_ir::MirFrameRetention>>,
    pub(super) states: [bray_symbols::ConstantValueId; 4],
    pub(super) boolean: TypeId,
    pub(super) report: MirPlace,
    pub(super) incoming_report: Rc<Cell<Option<bray_ir::MirStorageId>>>,
    pub(super) incoming_kind: MirStorageKind,
    pub(super) unit: TypeId,
    pub(super) runtime_abi: RuntimeAbiVersion,
}

impl CleanupOutcome {
    #[expect(
        clippy::too_many_arguments,
        reason = "cleanup storage retains its semantic types, state constants, ABI and callback lifetime"
    )]
    pub(crate) fn allocate(
        builder: &mut MirUnitBuilder,
        source: &MirSourceAnchor,
        boolean: TypeId,
        report: TypeId,
        unit: TypeId,
        runtime_abi: RuntimeAbiVersion,
        states: (TypeId, [bray_symbols::ConstantValueId; 4]),
        kind: MirStorageKind,
    ) -> Result<Self, MirCapacityError> {
        // Every generated operation retains the same Arc-backed source provenance.
        let mut storage = |ty| {
            builder
                .push_storage(source.clone(), kind.clone(), ty)
                .map(|id| MirPlace::new(id, [], ty))
        };

        Ok(Self {
            state: storage(states.0)?,
            retention: None,
            states: states.1,
            boolean,
            report: storage(report)?,
            incoming_report: Rc::new(Cell::new(None)),
            incoming_kind: kind,
            unit,
            runtime_abi,
        })
    }

    pub(crate) fn from_collection(
        collection: bray_ir::MirCleanupCollection,
        states: [bray_symbols::ConstantValueId; 4],
        boolean: TypeId,
        unit: TypeId,
        runtime_abi: RuntimeAbiVersion,
    ) -> Self {
        Self {
            state: collection.state().clone(),
            retention: collection.retention().cloned(),
            report: collection.report().clone(),
            incoming_report: Rc::new(Cell::new(None)),
            incoming_kind: MirStorageKind::Temporary,
            states,
            boolean,
            unit,
            runtime_abi,
        }
    }

    pub(crate) fn incoming_report(
        &self,
        builder: &mut MirUnitBuilder,
        source: &MirSourceAnchor,
    ) -> Result<MirPlace, MirCapacityError> {
        let storage = if let Some(storage) = self.incoming_report.get() {
            storage
        } else {
            let storage = builder.push_storage(
                source.clone(),
                self.incoming_kind.clone(),
                self.report.ty(),
            )?;

            self.incoming_report.set(Some(storage));

            storage
        };

        Ok(MirPlace::new(storage, [], self.report.ty()))
    }

    pub(crate) fn collection(&self) -> bray_ir::MirCleanupCollection {
        bray_ir::MirCleanupCollection::new(self.state.clone(), self.report.clone())
            .with_retention(self.retention.clone())
    }

    pub(crate) fn with_retention(
        mut self,
        retention: Option<std::sync::Arc<bray_ir::MirFrameRetention>>,
    ) -> Self {
        // A nested synchronous step must not discard its enclosing frame.
        self.retention = retention.or(self.retention);

        self
    }

    pub(crate) fn report(&self) -> MirOperand {
        // The move operand owns its path while this builder retains the storage identity.
        MirOperand::Move(self.report.clone())
    }
}
