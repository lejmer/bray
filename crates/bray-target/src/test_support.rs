use crate::{
    NativeTarget, TargetCDataModel, TargetIdentity, TargetMachineProperties, TargetProfile,
    TargetScalarKind,
};

/// Returns the canonical target profile used by compiler tests.
pub fn test_target_profile() -> TargetProfile {
    let identity = TargetIdentity::try_new("x86_64-unknown-linux-gnu")
        .unwrap_or_else(|| panic!("test target identity must be valid"));

    TargetProfile::try_new(identity, test_target_machine(), test_target_properties())
        .unwrap_or_else(|error| panic!("test target profile must be valid: {error:?}"))
}

/// Returns the canonical language-defined target properties used by compiler tests.
pub fn test_target_properties() -> crate::TargetProperties {
    let c_abi = TargetCDataModel::try_native(
        TargetScalarKind::I8,
        TargetScalarKind::I64,
        TargetScalarKind::U64,
        TargetScalarKind::I32,
        None,
    )
    .unwrap_or_else(|| panic!("test C ABI properties must be valid"));

    crate::TargetProperties::try_portable("unknown", "linux", "gnu", "gnu", c_abi)
        .unwrap_or_else(|| panic!("test target properties must be valid"))
}

/// Returns canonical x86-64 target-machine properties for compiler tests.
pub fn test_target_machine() -> TargetMachineProperties {
    NativeTarget::X86_64LinuxGnu.profile().machine().clone()
}
