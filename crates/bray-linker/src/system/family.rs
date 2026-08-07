use crate::external_tool::ResponseFileEncoding;
use crate::{LinkTarget, LinkedProductKind, LldFlavor};

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

    pub(super) fn supports(self, target: &LinkTarget, product: LinkedProductKind) -> bool {
        LldFlavor::for_target(target, product) == Some(self.flavor())
    }

    pub(super) const fn response_file_encoding(self) -> Option<ResponseFileEncoding> {
        match self {
            Self::Gnu | Self::GnuCompiler | Self::AppleCompiler => Some(ResponseFileEncoding::Utf8),
            Self::WslGnuCompiler => None,
            Self::Microsoft => Some(ResponseFileEncoding::Utf16LittleEndian),
            Self::MicrosoftCompiler => Some(ResponseFileEncoding::Utf8),
            Self::Apple => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_target::{ObjectFormat, TargetArchitecture};

    use super::SystemLinkerFamily;
    use crate::LinkedProductKind;
    use crate::test_support::target;

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
            assert_eq!(
                family.supports(&target, LinkedProductKind::Executable),
                expected
            );
        }
    }

    #[test]
    fn system_linkers_do_not_claim_static_archive_construction() {
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Elf);

        assert!(!SystemLinkerFamily::Gnu.supports(&target, LinkedProductKind::StaticLibrary));
    }
}
