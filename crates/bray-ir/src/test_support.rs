use bray_symbols::{
    ConstantValueData, ConstantValueId, ConstantValueKind, SemanticValueStore, TypeData, TypeId,
};

use crate::MirTargetContract;

pub(crate) fn test_type() -> TypeId {
    let Ok(store) = SemanticValueStore::try_new() else {
        panic!("test semantic value store must be available");
    };

    match store.intern_type(TypeData::Error) {
        Ok(ty) => ty,
        Err(error) => panic!("test type must be valid: {error:?}"),
    }
}

pub(crate) fn test_other_type() -> TypeId {
    let Ok(store) = SemanticValueStore::try_new() else {
        panic!("test semantic value store must be available");
    };

    match store.intern_type(TypeData::tuple([])) {
        Ok(ty) => ty,
        Err(error) => panic!("test type must be valid: {error:?}"),
    }
}

pub(crate) fn test_constant_value() -> ConstantValueId {
    let Ok(store) = SemanticValueStore::try_new() else {
        panic!("test semantic value store must be available");
    };

    let ty = store
        .intern_type(TypeData::Error)
        .unwrap_or_else(|error| panic!("test constant type must be valid: {error:?}"));

    store
        .intern_constant_value(ConstantValueData::new(ty, ConstantValueKind::Boolean(false)))
        .unwrap_or_else(|error| panic!("test constant must be valid: {error:?}"))
}

pub(crate) fn test_target() -> MirTargetContract {
    MirTargetContract::new(
        bray_target::test_support::test_target_profile(),
        bray_runtime_interface::RuntimeAbiVersion::new(1, 0),
    )
}
