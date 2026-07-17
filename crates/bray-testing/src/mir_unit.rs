use bray_ir::{MirSourceOrigin, MirUnit, MirUnitBuilder, MirUnitExecution};

use crate::test_bound_unit;

/// Builds one valid single-block MIR unit with a deterministic semantic identity.
pub fn test_mir_unit(unit: u32) -> MirUnit {
    let bound = test_bound_unit(unit);
    let source = bound.key().source();

    let mut builder = MirUnitBuilder::for_bound(bound.identity(), MirUnitExecution::Synchronous);

    let Ok(entry) = builder.push_block(MirSourceOrigin::Source(source)) else {
        panic!("test MIR block must be valid");
    };

    let Ok(unit) = builder.finish(entry) else {
        panic!("test MIR unit must be valid");
    };

    unit
}
