use bray_runtime_abi::{
    NativeCleanupExecution, NativeProductHostDescriptor, NativeProductHostStatus,
    NativeStaticHostEntry, NativeStaticIdentity, PRODUCT_HOST_ABI_VERSION,
};

use super::model::ProductStatic;

const MAXIMUM_STATIC_ENTRIES: usize = 1_000_000;

pub(super) fn read_statics(
    descriptor: &NativeProductHostDescriptor,
) -> Result<Vec<ProductStatic>, NativeProductHostStatus> {
    if descriptor.abi_version() != PRODUCT_HOST_ABI_VERSION
        || descriptor.static_count() > MAXIMUM_STATIC_ENTRIES
    {
        return Err(NativeProductHostStatus::INVALID_ARGUMENT);
    }

    let mut entries = Vec::new();

    crate::allocation::reserve_vec_entries(&mut entries, descriptor.static_count())
        .map_err(|_| NativeProductHostStatus::ALLOCATION_FAILURE)?;

    for index in 0..descriptor.static_count() {
        let entry = descriptor.static_entry()(index);
        let finalizer = entry.finalizer();
        let execution = finalizer.execution();

        let valid_result_layout = finalizer.result_alignment().is_power_of_two()
            && (execution != NativeCleanupExecution::NONE
                || (finalizer.result_size() == 0 && finalizer.result_alignment() == 1));

        if entry.abi_version() != PRODUCT_HOST_ABI_VERSION
            || !entry.duration().is_known()
            || !execution.is_known()
            || !valid_result_layout
            || entry.dependency_count() > MAXIMUM_STATIC_ENTRIES
        {
            return Err(NativeProductHostStatus::INVALID_ARGUMENT);
        }

        entries.push(entry);
    }

    entries.sort_unstable_by_key(|entry| entry.identity());

    if entries
        .windows(2)
        .any(|pair| pair[0].identity() == pair[1].identity())
    {
        return Err(NativeProductHostStatus::INVALID_ARGUMENT);
    }

    validate_dependencies(&entries)?;
    entries.sort_unstable_by_key(|entry| entry.order());

    if entries
        .windows(2)
        .any(|pair| pair[0].order() == pair[1].order())
    {
        return Err(NativeProductHostStatus::INVALID_ARGUMENT);
    }

    let mut statics = Vec::new();

    crate::allocation::reserve_vec_entries(&mut statics, entries.len())
        .map_err(|_| NativeProductHostStatus::ALLOCATION_FAILURE)?;

    for entry in entries {
        statics.push(ProductStatic {
            identity: entry.identity(),
            duration: entry.duration(),
            order: entry.order(),
            prepare: entry.prepare(),
            finalizer: entry.finalizer(),
            destroy: entry.destroy(),
            detach: entry.detach(),
        });
    }

    Ok(statics)
}

fn validate_dependencies(entries: &[NativeStaticHostEntry]) -> Result<(), NativeProductHostStatus> {
    let mut dependencies: Vec<NativeStaticIdentity> = Vec::new();

    for entry in entries {
        dependencies.clear();

        crate::allocation::reserve_vec_entries(&mut dependencies, entry.dependency_count())
            .map_err(|_| NativeProductHostStatus::ALLOCATION_FAILURE)?;

        dependencies.extend((0..entry.dependency_count()).map(|index| entry.dependency()(index)));
        dependencies.sort_unstable();

        if dependencies.windows(2).any(|pair| pair[0] == pair[1]) {
            return Err(NativeProductHostStatus::INVALID_ARGUMENT);
        }

        for dependency in &dependencies {
            let index = entries
                .binary_search_by_key(dependency, |entry| entry.identity())
                .map_err(|_| NativeProductHostStatus::INVALID_ARGUMENT)?;

            if entries[index].order() <= entry.order() {
                return Err(NativeProductHostStatus::INVALID_ARGUMENT);
            }
        }
    }

    Ok(())
}
