use std::collections::{BTreeMap, BTreeSet};

use bray_ir::MirStorageKind;

use crate::{
    CodegenInstanceKey, CodegenStaticStorageMapping, CodegenSymbolMapping, CodegenUnit,
};

use super::core::CodegenMappingsBuildError;

pub(super) fn validate_static_storage_mappings(
    unit: &CodegenUnit,
    instances: &BTreeSet<&CodegenInstanceKey>,
    symbols: &[CodegenSymbolMapping],
    mappings: &[CodegenStaticStorageMapping],
) -> Result<(), CodegenMappingsBuildError> {
    let expected: BTreeSet<_> = unit
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

    let actual: BTreeSet<_> = mappings
        .iter()
        .map(|mapping| (mapping.owner(), mapping.storage()))
        .collect();

    if actual != expected {
        return Err(CodegenMappingsBuildError::StaticStorageCoverageMismatch);
    }

    let mut realizations = BTreeMap::new();

    for mapping in mappings {
        let Some(owner) = unit
            .instances()
            .iter()
            .find(|instance| instance.key() == mapping.owner())
        else {
            return Err(CodegenMappingsBuildError::InvalidStaticStorage);
        };

        let Some(storage) = owner.mir().storage(mapping.storage()) else {
            return Err(CodegenMappingsBuildError::InvalidStaticStorage);
        };

        if !matches!(storage.kind(), MirStorageKind::Static(_))
            || storage.ty() != mapping.ty()
            || mapping.instance().target() != owner.key().target()
            || !instances.contains(mapping.initializer())
        {
            return Err(CodegenMappingsBuildError::InvalidStaticStorage);
        }

        if realizations
            .insert(mapping.instance(), mapping)
            .is_some_and(|previous| {
                previous.symbol() != mapping.symbol()
                    || previous.initializer() != mapping.initializer()
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
            mapping.state_name(),
        ] {
            if !names.insert(name) {
                return Err(CodegenMappingsBuildError::DuplicateBinarySymbolName);
            }
        }
    }

    Ok(())
}
