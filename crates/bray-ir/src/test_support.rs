use bray_symbols::{SemanticValueStore, TypeData, TypeId};

use crate::MirTargetFacts;

pub(crate) fn test_type() -> TypeId {
    let Ok(store) = SemanticValueStore::try_new() else {
        panic!("test semantic value store must be available");
    };

    match store.intern_type(TypeData::Error) {
        Ok(ty) => ty,
        Err(error) => panic!("test type must be valid: {error:?}"),
    }
}

pub(crate) fn test_target() -> MirTargetFacts {
    let Some(identity) = bray_target::TargetIdentity::try_new("x86_64-linux") else {
        panic!("test target identity must be valid");
    };

    MirTargetFacts::new(identity, bray_target::test_support::test_target_machine())
}
