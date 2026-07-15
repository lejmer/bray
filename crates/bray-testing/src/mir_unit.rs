use bray_ir::{MirUnit, MirUnitBuilder};

use crate::test_bound_unit;

/// Builds one valid single-block MIR unit with a deterministic semantic identity.
pub fn test_mir_unit(unit: u32) -> MirUnit {
    let bound = test_bound_unit(unit);
    let source = bound.key().source();

    let mut builder = MirUnitBuilder::new(bound.identity());

    let Ok(entry) = builder.push_block(source) else {
        panic!("test MIR block must be valid");
    };

    let Ok(unit) = builder.finish(entry) else {
        panic!("test MIR unit must be valid");
    };

    unit
}
