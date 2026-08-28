use std::collections::{BTreeMap, BTreeSet};

use bray_ir::MirStorageKind;

use crate::{
    CodegenConstantMapping, CodegenInstance, CodegenInstanceKey, CodegenNativeStaticMapping,
    CodegenStaticStorageMapping, CodegenSymbolMapping, CodegenUnit,
};

use super::core::CodegenMappingsBuildError;

pub(super) fn validate_static_storage_mappings(
    unit: &CodegenUnit,
    instances: &BTreeSet<&CodegenInstanceKey>,
    symbols: &[CodegenSymbolMapping],
    constants: &[CodegenConstantMapping],
    mappings: &[CodegenStaticStorageMapping],
    native_mappings: &[CodegenNativeStaticMapping],
) -> Result<(), CodegenMappingsBuildError> {
    let mut expected: BTreeSet<_> = unit
        .instances()
        .iter()
        .flat_map(|instance| {
            instance
                .mir()
                .storages_with_ids()
                .filter_map(|(storage, model)| {
                    matches!(model.kind(), MirStorageKind::Static(_))
                        .then_some((instance.key(), storage))
                })
        })
        .collect();

    expected.extend(
        native_mappings
            .iter()
            .filter(|mapping| mapping.direction() == bray_symbols::ForeignCallableDirection::Export)
            .map(|mapping| (mapping.owner(), mapping.storage())),
    );

    let actual: BTreeSet<_> = mappings
        .iter()
        .map(|mapping| (mapping.owner(), mapping.storage()))
        .collect();

    if actual != expected {
        return Err(CodegenMappingsBuildError::StaticStorageCoverageMismatch);
    }

    let mut realizations = BTreeMap::new();

    for mapping in mappings {
        let (owner, storage) = mapped_storage(unit, mapping.owner(), mapping.storage())
            .ok_or(CodegenMappingsBuildError::InvalidStaticStorage)?;

        let storage_type_matches = match storage.kind() {
            MirStorageKind::Static(_) => storage.ty() == mapping.ty(),
            MirStorageKind::NativeStatic(_) => native_mappings.iter().any(|native| {
                native.owner() == mapping.owner()
                    && native.storage() == mapping.storage()
                    && native.pointee_type() == mapping.ty()
            }),
            _ => false,
        };

        if !storage_type_matches
            || mapping.instance().target() != owner.key().target()
            || !constants
                .iter()
                .any(|constant| constant.value() == mapping.initial_value())
            || mapping.finalization().is_some_and(|finalization| {
                !instances.contains(finalization.instance())
                    || finalization
                        .incident_cleanup()
                        .is_some_and(|cleanup| !instances.contains(cleanup))
                    || finalization.incident_memory().is_some_and(|memory| {
                        !instances.contains(memory.allocation())
                            || !instances.contains(memory.deallocation())
                    })
                    || match finalization.result() {
                        bray_runtime_interface::ExecutableEntryResult::Fallible { .. } => {
                            finalization.error_type_identity().is_none()
                                || finalization.incident_cleanup().is_none()
                                || finalization.incident_memory().is_none()
                        }
                        bray_runtime_interface::ExecutableEntryResult::Unit => {
                            finalization.error_type_identity().is_some()
                                || finalization.incident_cleanup().is_some()
                                || finalization.incident_memory().is_some()
                        }
                        bray_runtime_interface::ExecutableEntryResult::I32 => true,
                    }
            })
            || mapping
                .destroy()
                .is_some_and(|destroy| !instances.contains(destroy))
            || mapping
                .relocations()
                .windows(2)
                .any(|pair| pair[0].value() >= pair[1].value())
            || mapping.relocations().iter().any(|relocation| {
                relocation.instance().target() != owner.key().target()
                    || !constants
                        .iter()
                        .any(|constant| constant.value() == relocation.value())
            })
        {
            return Err(CodegenMappingsBuildError::InvalidStaticStorage);
        }

        if realizations
            .insert(mapping.instance(), mapping)
            .is_some_and(|previous| {
                previous.symbol() != mapping.symbol()
                    || previous.native_binding() != mapping.native_binding()
                    || previous.defines_storage() != mapping.defines_storage()
                    || previous.initial_value() != mapping.initial_value()
                    || previous.relocations() != mapping.relocations()
                    || previous.finalization() != mapping.finalization()
                    || previous.destroy() != mapping.destroy()
                    || previous.ty() != mapping.ty()
            })
        {
            return Err(CodegenMappingsBuildError::InvalidStaticStorage);
        }
    }

    let mut names: BTreeSet<String> = symbols
        .iter()
        .map(|mapping| mapping.name().as_str().to_owned())
        .collect();

    for mapping in realizations.values() {
        for name in [
            mapping.symbol().as_str().to_owned(),
            mapping.accessor_name(),
            mapping.host_name(),
            mapping.attachment_name(),
            mapping.prepare_name(),
            mapping.finalize_name(),
            mapping.destroy_name(),
            mapping.detach_name(),
        ] {
            if !names.insert(name) {
                return Err(CodegenMappingsBuildError::DuplicateBinarySymbolName);
            }
        }
    }

    Ok(())
}

pub(super) fn validate_native_static_mappings(
    unit: &CodegenUnit,
    mappings: &[CodegenNativeStaticMapping],
) -> Result<(), CodegenMappingsBuildError> {
    let expected: BTreeSet<_> = unit
        .instances()
        .iter()
        .flat_map(|instance| {
            instance
                .mir()
                .storages_with_ids()
                .filter_map(|(storage, model)| {
                    matches!(model.kind(), MirStorageKind::NativeStatic(_))
                        .then_some((instance.key(), storage))
                })
        })
        .collect();

    let actual: BTreeSet<_> = mappings
        .iter()
        .map(|mapping| (mapping.owner(), mapping.storage()))
        .collect();

    if actual != expected {
        return Err(CodegenMappingsBuildError::NativeStaticStorageCoverageMismatch);
    }

    for mapping in mappings {
        let (_, storage) = mapped_storage(unit, mapping.owner(), mapping.storage())
            .ok_or(CodegenMappingsBuildError::InvalidNativeStaticStorage)?;

        if !matches!(storage.kind(), MirStorageKind::NativeStatic(_))
            || storage.ty() != mapping.pointer_type()
            || (mapping.direction() == bray_symbols::ForeignCallableDirection::Export
                && mapping.presence() == bray_symbols::NativeSymbolPresence::Optional)
        {
            return Err(CodegenMappingsBuildError::InvalidNativeStaticStorage);
        }
    }

    Ok(())
}

fn mapped_storage<'a>(
    unit: &'a CodegenUnit,
    owner: &CodegenInstanceKey,
    storage: bray_ir::MirStorageId,
) -> Option<(&'a CodegenInstance, &'a bray_ir::MirStorage)> {
    let owner = unit
        .instances()
        .iter()
        .find(|instance| instance.key() == owner)?;

    Some((owner, owner.mir().storage(storage)?))
}
