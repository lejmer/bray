use bray_ir::{
    MirBlockKind, MirSourceAnchor, MirTargetFacts, MirTerminatorKind, MirUnit, MirUnitBuilder,
    MirUnitExecution,
};
use bray_symbols::{SemanticValueStore, TypeData, TypeId};
use bray_target::TargetIdentity;

use crate::test_bound_unit;

/// Builds one valid single-block MIR unit with a deterministic semantic identity.
pub fn test_mir_unit(unit: u32) -> MirUnit {
    let bound = test_bound_unit(unit);
    let source = MirSourceAnchor::from(bound.key().source());

    let mut builder = MirUnitBuilder::for_bound(
        bound.identity(),
        MirUnitExecution::Synchronous,
        test_mir_target(),
    );

    let Ok(entry) = builder.push_block(source.clone(), MirBlockKind::Ordinary) else {
        panic!("test MIR block must be valid");
    };

    let Ok(()) = builder.set_terminator(entry, source, MirTerminatorKind::Return(None)) else {
        panic!("test MIR terminator must be valid");
    };

    let Ok(unit) = builder.finish(entry) else {
        panic!("test MIR unit must be valid");
    };

    unit
}

/// Returns deterministic target facts for MIR tests.
pub fn test_mir_target() -> MirTargetFacts {
    let Some(identity) = TargetIdentity::try_new("x86_64-linux") else {
        panic!("test target identity must be valid");
    };

    MirTargetFacts::new(
        identity,
        bray_target::test_support::test_target_machine(),
        bray_runtime_interface::RuntimeAbiVersion::new(1, 0),
    )
}

/// Returns a canonical error type for MIR tests.
pub fn test_mir_type() -> TypeId {
    let Ok(store) = SemanticValueStore::try_new() else {
        panic!("test semantic value store must be available");
    };

    match store.intern_type(TypeData::Error) {
        Ok(ty) => ty,
        Err(error) => panic!("test MIR type must be valid: {error:?}"),
    }
}
