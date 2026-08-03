use std::collections::BTreeSet;
use std::sync::Arc;

use bray_ir::{
    MirAsyncOperation, MirCallTarget, MirCallableReference, MirFrameInitializer, MirOperationKind,
    MirTerminatorKind,
};
use bray_symbols::ImplementationInstanceId;

use crate::{CodegenInstanceKey, CodegenUnit};

/// One direct callable reference and the exact implementation witnesses selected for it.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DemandedCallableInstance {
    reference: MirCallableReference,
    trait_dispatch: Option<bray_symbols::TraitConstraintDispatch>,
    witnesses: Arc<[ImplementationInstanceId]>,
}

impl DemandedCallableInstance {
    /// Returns the selected callable reference.
    pub const fn reference(&self) -> MirCallableReference {
        self.reference
    }

    /// Returns the generic constraint supplying callable dispatch.
    pub const fn trait_dispatch(&self) -> Option<bray_symbols::TraitConstraintDispatch> {
        self.trait_dispatch
    }

    /// Returns selected implementation witnesses in canonical semantic order.
    pub fn witnesses(&self) -> &[ImplementationInstanceId] {
        &self.witnesses
    }
}

/// Returns direct callable references retained by one code generation unit.
pub fn demanded_callable_references(
    unit: &CodegenUnit,
) -> BTreeSet<(CodegenInstanceKey, MirCallableReference)> {
    demanded_callable_instances(unit)
        .into_iter()
        .map(|(owner, demand)| (owner, demand.reference()))
        .collect()
}

/// Returns direct callable instances and witness payloads retained by one code generation unit.
pub fn demanded_callable_instances(
    unit: &CodegenUnit,
) -> BTreeSet<(CodegenInstanceKey, DemandedCallableInstance)> {
    unit.instances()
        .iter()
        .flat_map(|instance| {
            demanded_callable_instances_for_mir(instance.mir())
                .into_iter()
                .map(|demand| (instance.key().clone(), demand))
        })
        .collect()
}

/// Returns direct callable references retained by one MIR definition.
pub fn demanded_callable_references_for_mir(
    unit: &bray_ir::MirUnit,
) -> BTreeSet<MirCallableReference> {
    demanded_callable_instances_for_mir(unit)
        .into_iter()
        .map(|demand| demand.reference())
        .collect()
}

/// Returns direct callable instances and witness payloads retained by one MIR definition.
pub fn demanded_callable_instances_for_mir(
    unit: &bray_ir::MirUnit,
) -> BTreeSet<DemandedCallableInstance> {
    unit.operations()
        .iter()
        .filter_map(|operation| operation_callable_instance(operation.kind()))
        .chain(
            unit.blocks()
                .iter()
                .filter_map(|block| terminator_callable_instance(block.terminator().kind())),
        )
        .collect()
}

fn operation_callable_instance(operation: &MirOperationKind) -> Option<DemandedCallableInstance> {
    let call = match operation {
        MirOperationKind::Call(call)
        | MirOperationKind::Async(MirAsyncOperation::CreateFrame {
            initializer: MirFrameInitializer::Callable(call),
            ..
        }) => call,
        MirOperationKind::AnonymousCallable(_)
        | MirOperationKind::Store { .. }
        | MirOperationKind::Borrow { .. }
        | MirOperationKind::Unary { .. }
        | MirOperationKind::Binary { .. }
        | MirOperationKind::Aggregate(_)
        | MirOperationKind::Construct(_)
        | MirOperationKind::Convert { .. }
        | MirOperationKind::PatternProjection { .. }
        | MirOperationKind::Generator(_)
        | MirOperationKind::Memory(_)
        | MirOperationKind::Text(_)
        | MirOperationKind::PanicReport(_)
        | MirOperationKind::Finalize(_)
        | MirOperationKind::Destroy(_)
        | MirOperationKind::Cleanup { .. }
        | MirOperationKind::Async(_)
        | MirOperationKind::Host(_) => return None,
    };

    let MirCallTarget::Direct(reference) = call.target() else {
        return None;
    };

    let mut witnesses = call.dispatch_witnesses().to_vec();

    witnesses.extend(call.witnesses().iter().map(|selection| selection.witness()));
    witnesses.sort_unstable();
    witnesses.dedup();

    Some(DemandedCallableInstance {
        reference: *reference,
        trait_dispatch: call.trait_dispatch(),
        witnesses: witnesses.into(),
    })
}

fn terminator_callable_instance(
    terminator: &MirTerminatorKind,
) -> Option<DemandedCallableInstance> {
    let MirTerminatorKind::Iterate { next, .. } = terminator else {
        return None;
    };

    Some(DemandedCallableInstance {
        reference: *next,
        trait_dispatch: None,
        witnesses: Arc::from([]),
    })
}
