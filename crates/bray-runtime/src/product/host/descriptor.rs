use bray_runtime_abi::{
    NativeCleanupExecution, NativeProductHostDescriptor, NativeProductHostStatus,
    NativeStaticDuration, NativeStaticHostEntry, NativeStaticIdentity, PRODUCT_HOST_ABI_VERSION,
};

use crate::product::cleanup::StaticCleanup;

const MAXIMUM_STATIC_ENTRIES: usize = 1_000_000;

pub(super) fn read_statics(
    descriptor: &NativeProductHostDescriptor,
) -> Result<(Vec<StaticCleanup>, Vec<StaticCleanup>), NativeProductHostStatus> {
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

        let valid_metadata =
            finalizer.metadata().is_some() == (execution == NativeCleanupExecution::ASYNCHRONOUS);

        if entry.abi_version() != PRODUCT_HOST_ABI_VERSION
            || !entry.duration().is_known()
            || !execution.is_known()
            || !valid_metadata
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

    let product_count = entries
        .iter()
        .filter(|entry| entry.duration() == NativeStaticDuration::PRODUCT)
        .count();

    let mut statics = Vec::new();
    let mut thread_statics = Vec::new();

    crate::allocation::reserve_vec_entries(&mut statics, product_count)
        .map_err(|_| NativeProductHostStatus::ALLOCATION_FAILURE)?;

    crate::allocation::reserve_vec_entries(&mut thread_statics, entries.len() - product_count)
        .map_err(|_| NativeProductHostStatus::ALLOCATION_FAILURE)?;

    for entry in entries {
        let destination = if entry.duration() == NativeStaticDuration::PRODUCT {
            &mut statics
        } else {
            &mut thread_statics
        };

        destination.push(StaticCleanup {
            identity: entry.identity(),
            order: entry.order(),
            prepare: entry.prepare(),
            finalizer: entry.finalizer(),
            destroy: entry.destroy(),
            detach: entry.detach(),
        });
    }

    Ok((statics, thread_statics))
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
