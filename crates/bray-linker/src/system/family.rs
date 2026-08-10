use crate::LldFlavor;
use crate::external_tool::ResponseFileEncoding;

/// Supported command family of one explicitly configured platform linker.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SystemLinkerFamily {
    /// GNU `ld` command language for ELF targets.
    Gnu,
    /// GNU compiler-driver command language for ELF targets.
    GnuCompiler,
    /// GNU compiler-driver command language reached through Windows Subsystem for Linux.
    WslGnuCompiler,
    /// Microsoft `LINK` command language for COFF targets.
    Microsoft,
    /// Microsoft-compatible compiler-driver command language for COFF targets.
    MicrosoftCompiler,
    /// Apple `ld` command language for Mach-O targets.
    Apple,
    /// Apple compiler-driver command language for Mach-O targets.
    AppleCompiler,
}

impl SystemLinkerFamily {
    pub(crate) const fn flavor(self) -> LldFlavor {
        match self {
            Self::Gnu | Self::GnuCompiler | Self::WslGnuCompiler => LldFlavor::Elf,
            Self::Microsoft | Self::MicrosoftCompiler => LldFlavor::Coff,
            Self::Apple | Self::AppleCompiler => LldFlavor::MachO,
        }
    }

    pub(crate) const fn response_file_encoding(self) -> Option<ResponseFileEncoding> {
        match self {
            Self::Gnu | Self::GnuCompiler | Self::AppleCompiler => Some(ResponseFileEncoding::Utf8),
            Self::WslGnuCompiler => None,
            Self::Microsoft => Some(ResponseFileEncoding::Utf16LittleEndian),
            Self::MicrosoftCompiler => Some(ResponseFileEncoding::Utf8),
            Self::Apple => None,
        }
    }

    pub(crate) const fn supplies_platform_startup(self) -> bool {
        matches!(
            self,
            Self::GnuCompiler
                | Self::WslGnuCompiler
                | Self::MicrosoftCompiler
                | Self::AppleCompiler
        )
    }
}

#[cfg(test)]
mod tests {
    use bray_target::{ObjectFormat, TargetArchitecture};

    use super::SystemLinkerFamily;
    use crate::test_support::target;
    use crate::{
        LinkInputKind, LinkPlanCapability, LinkedProductKind, LinkerDriverCapabilities,
        LinkerDriverCapabilitiesBuildError, LinkerDriverIdentity, LinkerDriverKind,
        LinkerTargetIdentity,
    };

    #[test]
    fn families_accept_only_their_platform_command_contracts() {
        let cases = [
            (
                SystemLinkerFamily::Gnu,
                target(TargetArchitecture::X86_64, ObjectFormat::Elf),
                true,
            ),
            (
                SystemLinkerFamily::Microsoft,
                target(TargetArchitecture::Aarch64, ObjectFormat::Coff),
                true,
            ),
            (
                SystemLinkerFamily::Apple,
                target(TargetArchitecture::Aarch64, ObjectFormat::MachO),
                true,
            ),
            (
                SystemLinkerFamily::Microsoft,
                target(TargetArchitecture::X86_64, ObjectFormat::Elf),
                false,
            ),
            (
                SystemLinkerFamily::Apple,
                target(TargetArchitecture::Arm, ObjectFormat::MachO),
                false,
            ),
        ];

        for (family, target, expected) in cases {
            let identity = LinkerDriverIdentity::try_new(
                LinkerDriverKind::System,
                "test-system-linker",
                "1",
                "1",
            )
            .unwrap_or_else(|| panic!("test driver identity must be valid"));

            let selected = LinkerDriverCapabilities::try_for_system(
                identity,
                family,
                linker_target(&target),
            );

            if expected {
                let capabilities = selected.unwrap_or_else(|error| {
                    panic!("compatible test capabilities must be valid: {error:?}")
                });

                assert!(capabilities
                    .supports_target_product(&target, LinkedProductKind::Executable));
            } else {
                assert_eq!(
                    selected,
                    Err(LinkerDriverCapabilitiesBuildError::UnsupportedTarget)
                );
            }
        }
    }

    #[test]
    fn system_linkers_do_not_claim_static_archive_construction() {
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Elf);

        let identity =
            LinkerDriverIdentity::try_new(LinkerDriverKind::System, "test-system-linker", "1", "1")
                .unwrap_or_else(|| panic!("test driver identity must be valid"));

        let capabilities =
            LinkerDriverCapabilities::try_for_system(
                identity,
                SystemLinkerFamily::Gnu,
                linker_target(&target),
            )
            .unwrap_or_else(|error| panic!("test capabilities must be valid: {error:?}"));

        let exact = capabilities.targets()[0]
            .exact_target()
            .unwrap_or_else(|| panic!("system linker capabilities must retain exact targets"));

        assert_eq!(exact.identity(), target.identity());
        assert_eq!(exact.triple(), target.triple());

        assert!(!capabilities.supports_target_product(&target, LinkedProductKind::StaticLibrary));

        assert!(
            !capabilities.supports_plan_capability(
                &target,
                LinkPlanCapability::Input(LinkInputKind::Bitcode)
            )
        );
    }

    fn linker_target(target: &crate::LinkTarget) -> LinkerTargetIdentity {
        LinkerTargetIdentity::try_new(
            target.identity().clone(),
            target.triple(),
            target.architecture(),
            target.object_format(),
        )
        .unwrap_or_else(|| panic!("test linker target must be valid"))
    }
}
