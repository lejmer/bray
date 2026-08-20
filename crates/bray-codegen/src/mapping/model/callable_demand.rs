use std::collections::BTreeSet;
use std::sync::Arc;

use bray_ir::{
    MirAsyncOperation, MirCall, MirCallTarget, MirCallableReference, MirFrameInitializer,
    MirOperationKind, MirTerminatorKind,
};
use bray_symbols::ImplementationInstanceId;

use crate::{CodegenCallSite, CodegenInstanceKey, CodegenUnit};

/// One direct callable reference and the exact implementation witnesses selected for it.
#[derive(Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct DemandedCallableInstance {
    site: CodegenCallSite,
    reference: MirCallableReference,
    trait_dispatch: Option<bray_symbols::TraitConstraintDispatch>,
    intrinsic: Option<bray_ir::MirCallIntrinsic>,
    witnesses: Arc<[ImplementationInstanceId]>,
}

impl DemandedCallableInstance {
    /// Returns the exact MIR occurrence containing the call.
    pub const fn site(&self) -> CodegenCallSite {
        self.site
    }

    /// Returns the selected callable reference.
    pub const fn reference(&self) -> MirCallableReference {
        self.reference
    }

    /// Returns the generic constraint supplying callable dispatch.
    pub const fn trait_dispatch(&self) -> Option<bray_symbols::TraitConstraintDispatch> {
        self.trait_dispatch
    }

    /// Returns the compiler-defined realization available after concrete specialization.
    pub const fn intrinsic(&self) -> Option<bray_ir::MirCallIntrinsic> {
        self.intrinsic
    }

    /// Returns selected implementation witnesses in canonical semantic order.
    pub fn witnesses(&self) -> &[ImplementationInstanceId] {
        &self.witnesses
    }
}

/// Returns direct callable references retained by one code generation unit.
pub fn demanded_callable_references(
    unit: &CodegenUnit,
) -> BTreeSet<(CodegenInstanceKey, CodegenCallSite, MirCallableReference)> {
    demanded_callable_instances(unit)
        .into_iter()
        .map(|(owner, demand)| (owner, demand.site(), demand.reference()))
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
) -> BTreeSet<(CodegenCallSite, MirCallableReference)> {
    demanded_callable_instances_for_mir(unit)
        .into_iter()
        .map(|demand| (demand.site(), demand.reference()))
        .collect()
}

/// Returns direct callable instances and witness payloads retained by one MIR definition.
pub fn demanded_callable_instances_for_mir(
    unit: &bray_ir::MirUnit,
) -> BTreeSet<DemandedCallableInstance> {
    unit.operations_with_ids()
        .flat_map(|(operation, data)| operation_callable_instances(operation, data.kind()))
        .chain(unit.blocks_with_ids().flat_map(|(block, data)| {
            terminator_callable_instances(block, data.terminator().kind())
        }))
        .collect()
}

fn operation_callable_instances(
    operation_id: bray_ir::MirOperationId,
    operation: &MirOperationKind,
) -> Vec<DemandedCallableInstance> {
    if let MirOperationKind::Memory(memory) = operation {
        return memory
            .inline_assembly_symbols()
            .iter()
            .copied()
            .enumerate()
            .map(|(symbol, reference)| DemandedCallableInstance {
                site: CodegenCallSite::InlineAssemblyOperation {
                    operation: operation_id,
                    symbol,
                },
                reference,
                trait_dispatch: None,
                intrinsic: None,
                witnesses: Arc::from([]),
            })
            .collect();
    }

    let call = match operation {
        MirOperationKind::Call(call)
        | MirOperationKind::Async(MirAsyncOperation::CreateFrame {
            initializer: MirFrameInitializer::Callable(call),
            ..
        }) => call,
        MirOperationKind::AnonymousCallable(_)
        | MirOperationKind::DeclaredCallable(_)
        | MirOperationKind::Store { .. }
        | MirOperationKind::Borrow { .. }
        | MirOperationKind::Unary { .. }
        | MirOperationKind::Binary { .. }
        | MirOperationKind::Aggregate(_)
        | MirOperationKind::Construct(_)
        | MirOperationKind::Convert { .. }
        | MirOperationKind::NumericConversion { .. }
        | MirOperationKind::PatternProjection { .. }
        | MirOperationKind::Generator(_)
        | MirOperationKind::Memory(_)
        | MirOperationKind::Text(_)
        | MirOperationKind::PanicReport(_)
        | MirOperationKind::Finalize(_)
        | MirOperationKind::Destroy(_)
        | MirOperationKind::Cleanup { .. }
        | MirOperationKind::Async(_)
        | MirOperationKind::Host(_) => return Vec::new(),
    };

    demanded_callable_instance_for_call(CodegenCallSite::Operation(operation_id), call)
        .into_iter()
        .collect()
}

/// Returns the direct callable demand represented by one call occurrence.
pub fn demanded_callable_instance_for_call(
    site: CodegenCallSite,
    call: &MirCall,
) -> Option<DemandedCallableInstance> {
    let MirCallTarget::Direct(reference) = call.target() else {
        return None;
    };

    let mut witnesses = call.dispatch_witnesses().to_vec();

    witnesses.extend(call.witnesses().iter().map(|selection| selection.witness()));
    witnesses.sort_unstable();
    witnesses.dedup();

    Some(DemandedCallableInstance {
        site,
        reference: *reference,
        trait_dispatch: call.trait_dispatch(),
        intrinsic: call.intrinsic(),
        witnesses: witnesses.into(),
    })
}

fn terminator_callable_instances(
    block: bray_ir::MirBlockId,
    terminator: &MirTerminatorKind,
) -> Vec<DemandedCallableInstance> {
    match terminator {
        MirTerminatorKind::Iterate { next, witness, .. } => vec![DemandedCallableInstance {
            site: CodegenCallSite::Terminator(block),
            reference: *next,
            trait_dispatch: None,
            intrinsic: None,
            witnesses: Arc::from([*witness]),
        }],
        MirTerminatorKind::InlineAssembly(assembly) => assembly
            .symbols()
            .iter()
            .copied()
            .enumerate()
            .map(|(symbol, reference)| DemandedCallableInstance {
                site: CodegenCallSite::InlineAssemblyTerminator { block, symbol },
                reference,
                trait_dispatch: None,
                intrinsic: None,
                witnesses: Arc::from([]),
            })
            .collect(),
        _ => Vec::new(),
    }
}
