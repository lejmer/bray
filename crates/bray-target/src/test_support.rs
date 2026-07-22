use std::num::{NonZeroU16, NonZeroU32};

use crate::{
    Endianness, ObjectFormat, TargetArchitecture, TargetIdentity, TargetMachineProperties,
    TargetProfile,
};

/// Returns the canonical target profile used by compiler tests.
pub fn test_target_profile() -> TargetProfile {
    let Some(identity) = TargetIdentity::try_new("x86_64-unknown-linux-gnu") else {
        panic!("test target identity must be valid");
    };

    match TargetProfile::try_new(identity, test_target_machine(), test_target_facts()) {
        Ok(profile) => profile,
        Err(error) => panic!("test target profile must be valid: {error:?}"),
    }
}

/// Returns the canonical language-defined target facts used by compiler tests.
pub fn test_target_facts() -> crate::TargetFacts {
    let Some(facts) = crate::TargetFacts::try_portable("unknown", "linux", "gnu", "gnu") else {
        panic!("test target facts must be valid");
    };

    facts
}

/// Returns canonical x86-64 target-machine properties for compiler tests.
pub fn test_target_machine() -> TargetMachineProperties {
    let pointer_width = NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN);
    let pointer_alignment = NonZeroU32::new(8).unwrap_or(NonZeroU32::MIN);
    let stack_alignment = NonZeroU32::new(16).unwrap_or(NonZeroU32::MIN);

    let Some(machine) = TargetMachineProperties::try_new(
        TargetArchitecture::X86_64,
        ObjectFormat::Elf,
        Endianness::Little,
        pointer_width,
        pointer_alignment,
        stack_alignment,
    ) else {
        panic!("test target machine properties must be valid");
    };

    machine
}
