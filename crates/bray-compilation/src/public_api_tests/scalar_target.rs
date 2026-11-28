use crate::{CompilationOptions, SelectedTarget, WorkerBudget};
use bray_symbols::ProductKind;
use bray_target::{TargetProfile, TargetProperties, TargetScalarSupport};

pub(crate) fn extended_options() -> CompilationOptions {
    let baseline = SelectedTarget::baseline();
    let profile = baseline.profile();
    let properties = profile.properties();

    let extended = TargetProperties::new(
        properties.identity().clone(),
        TargetScalarSupport::new(true, true, true, true),
        properties.atomics(),
        properties.abis(),
        properties.c_abi(),
        properties.address_spaces(),
        properties.alignments(),
        properties.operations(),
    );

    let profile = TargetProfile::try_new(
        profile.identity().clone(),
        profile.machine().clone(),
        extended,
    )
    .expect("the optional scalar target must support all advertised representations");

    CompilationOptions::new(
        WorkerBudget::serial(),
        ProductKind::Library,
        SelectedTarget::new(profile, baseline.runtime_abi()),
    )
}
