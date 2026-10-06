use bray_bound_tree::{BoundUnitKey, BoundUnitKind};
use bray_diagnostics::DiagnosticResult;
use bray_ir::MirUnit;
use bray_lowering::LoweredUnit;
use bray_symbols::{PackageIdentity, ProductKind};

use crate::test_support::source_input;
use crate::{Compilation, CompilationOptions, CompilationRequest, SelectedTarget, WorkerBudget};

pub(super) fn standard_text_compilation(additional_sources: &[&str]) -> Compilation {
    let package = PackageIdentity::try_new("std")
        .unwrap_or_else(|| panic!("standard library identity must be valid"));

    let sources = [
        include_str!("../../../../../../standard-library/std/src/std.bray"),
        include_str!("../../../../../../standard-library/std/src/string.bray"),
        include_str!("../../../../../../standard-library/std/src/character.bray"),
    ]
    .into_iter()
    .chain(additional_sources.iter().copied())
    .enumerate()
    .map(|(index, source)| {
        let version = u32::try_from(index)
            .unwrap_or_else(|_| panic!("standard text source index must fit in u32"));

        source_input(source, version)
    })
    .collect();

    let request = CompilationRequest::with_options(
        package,
        sources,
        CompilationOptions::new(
            WorkerBudget::serial(),
            ProductKind::Library,
            SelectedTarget::baseline(),
        ),
    )
    .with_standard_library_source_authority();

    Compilation::load(request)
        .unwrap_or_else(|error| panic!("standard text compilation must load: {error:?}"))
}

pub(super) fn lowered_mir(result: &DiagnosticResult<Option<LoweredUnit>>) -> &MirUnit {
    result
        .value()
        .as_ref()
        .and_then(LoweredUnit::mir)
        .unwrap_or_else(|| panic!("checked executable unit must produce MIR: {result:#?}"))
}

pub(super) fn declared_unit_key(compilation: &Compilation, kind: BoundUnitKind) -> BoundUnitKey {
    compilation
        .declared_unit_keys_for_test()
        .unwrap_or_else(|error| panic!("declared units must be available: {error:?}"))
        .into_iter()
        .find(|key| key.kind() == kind)
        .unwrap_or_else(|| panic!("test source must contain a {kind:?} unit"))
}
