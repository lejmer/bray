use bray_base::NonEmptySharedStr;
use bray_symbols::{NativeLinkKind, NativeLinkRequirement};
use bray_target::NativeTarget;

pub(super) fn common_support_requirements(
    target: NativeTarget,
    reported: &[NativeLinkRequirement],
) -> Vec<NativeLinkRequirement> {
    let mut requirements = reported.to_vec();

    if matches!(
        target,
        NativeTarget::X86_64WindowsMsvc | NativeTarget::Aarch64WindowsMsvc
    )
    {
        let name = NonEmptySharedStr::try_new("synchronization")
            .unwrap_or_else(|| unreachable!("the native library name is non-empty"));

        requirements.push(NativeLinkRequirement::new(name, NativeLinkKind::System));
    }

    requirements
}
