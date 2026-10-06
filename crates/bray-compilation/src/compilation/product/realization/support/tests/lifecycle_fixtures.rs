use bray_codegen::CodegenTarget;
use bray_ir::{MirHelperReference, MirOperationKind, MirUnit, MirUnitId};

use crate::{CancellationToken, Compilation};

pub(super) fn generated_lifecycle(
    compilation: &Compilation,
    target: &CodegenTarget,
    reference: MirHelperReference,
    unit: u32,
) -> MirUnit {
    let instance = compilation
        .concrete_codegen_lifecycle(reference, target)
        .expect("lifecycle instance must realize");

    let reference = instance
        .generated_lifecycle_reference()
        .expect("generated lifecycle payload must be retained");

    compilation
        .codegen_generated_lifecycle_mir(
            instance.key(),
            reference,
            MirUnitId::new(unit),
            &CancellationToken::new(),
        )
        .expect("generated lifecycle MIR must realize")
}

pub(super) fn reachable_cleanup_operations(
    mir: &MirUnit,
    entry: bray_ir::MirBlockId,
) -> Vec<&MirOperationKind> {
    let mut pending = vec![entry];
    let mut visited = std::collections::BTreeSet::new();
    let mut operations = std::collections::BTreeMap::new();

    while let Some(id) = pending.pop() {
        if !visited.insert(id) {
            continue;
        }

        let block = mir.block(id).expect("cleanup block must exist");

        for id in block.operations() {
            let operation = mir
                .operation(*id)
                .expect("cleanup operation must exist")
                .kind();

            if matches!(
                operation,
                MirOperationKind::Cleanup { .. }
                    | MirOperationKind::Finalize(_)
                    | MirOperationKind::Destroy(_)
            ) {
                operations.insert(*id, operation);
            }
        }

        block
            .terminator()
            .kind()
            .for_each_successor(|target| pending.push(target));
    }

    operations.into_values().collect()
}
