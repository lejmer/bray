use bray_codegen::CodegenTarget;

use crate::{Compilation, SelectedTarget};

pub(super) fn codegen_target(compilation: &Compilation) -> CodegenTarget {
    compilation
        .selected_target()
        .target()
        .codegen_target()
        .expect("test codegen target must validate")
}

pub(super) fn baseline_codegen_target() -> CodegenTarget {
    SelectedTarget::baseline()
        .codegen_target()
        .unwrap_or_else(|error| panic!("baseline codegen target must be valid: {error:?}"))
}
