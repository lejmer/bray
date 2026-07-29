use crate::{LinkTarget, LinkedProductKind, LldFlavor};

/// Supported command family of one explicitly configured platform linker.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum SystemLinkerFamily {
    /// GNU `ld` command language for ELF targets.
    Gnu,
    /// Microsoft `LINK` command language for COFF targets.
    Microsoft,
    /// Apple `ld` command language for Mach-O targets.
    Apple,
}

impl SystemLinkerFamily {
    pub(super) const fn flavor(self) -> LldFlavor {
        match self {
            Self::Gnu => LldFlavor::Elf,
            Self::Microsoft => LldFlavor::Coff,
            Self::Apple => LldFlavor::MachO,
        }
    }

    pub(super) fn supports(self, target: &LinkTarget, product: LinkedProductKind) -> bool {
        LldFlavor::for_target(target, product) == Some(self.flavor())
    }

    pub(super) const fn response_file_encoding(self) -> Option<ResponseFileEncoding> {
        match self {
            Self::Gnu => Some(ResponseFileEncoding::Utf8),
            Self::Microsoft => Some(ResponseFileEncoding::Utf16LittleEndian),
            Self::Apple => None,
        }
    }
}

#[derive(Clone, Copy)]
pub(super) enum ResponseFileEncoding {
    Utf8,
    Utf16LittleEndian,
}

#[cfg(test)]
mod tests {
    use bray_target::{
        CodeModel, ObjectFormat, RelocationModel, TargetArchitecture, TargetIdentity,
    };

    use super::SystemLinkerFamily;
    use crate::{LinkModel, LinkTarget, LinkedProductKind};

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

    fn target(architecture: TargetArchitecture, object_format: ObjectFormat) -> LinkTarget {
        let Some(identity) = TargetIdentity::try_new("test-target") else {
            panic!("test target identity must be valid");
        };

        LinkTarget::try_new(
            identity,
            "test-target-triple",
            architecture,
            object_format,
            RelocationModel::PositionIndependent,
            CodeModel::Small,
            LinkModel::Dynamic,
        )
        .unwrap_or_else(|error| panic!("test link target must be valid: {error:?}"))
    }
}
