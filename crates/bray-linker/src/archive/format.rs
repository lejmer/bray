use bray_target::{ObjectFormat, TargetArchitecture};

use crate::LinkTarget;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum ArchiveFormat {
    Coff,
    Darwin,
    Gnu,
    BigArchive,
}

impl ArchiveFormat {
    pub(super) const fn for_target(target: &LinkTarget) -> Option<Self> {
        match (target.object_format(), target.architecture()) {
            (
                ObjectFormat::Coff,
                TargetArchitecture::X86
                | TargetArchitecture::X86_64
                | TargetArchitecture::Arm
                | TargetArchitecture::Aarch64,
            ) => Some(Self::Coff),
            (
                ObjectFormat::Elf,
                TargetArchitecture::X86
                | TargetArchitecture::X86_64
                | TargetArchitecture::Arm
                | TargetArchitecture::Aarch64
                | TargetArchitecture::Riscv32
                | TargetArchitecture::Riscv64
                | TargetArchitecture::PowerPc64,
            )
            | (
                ObjectFormat::WebAssembly,
                TargetArchitecture::Wasm32 | TargetArchitecture::Wasm64,
            ) => Some(Self::Gnu),
            (ObjectFormat::MachO, TargetArchitecture::X86_64 | TargetArchitecture::Aarch64) => {
                Some(Self::Darwin)
            }
            (ObjectFormat::Xcoff, TargetArchitecture::PowerPc64) => Some(Self::BigArchive),
            (
                ObjectFormat::Coff
                | ObjectFormat::Elf
                | ObjectFormat::MachO
                | ObjectFormat::WebAssembly
                | ObjectFormat::Xcoff,
                _,
            ) => None,
        }
    }

    pub(super) const fn llvm_name(self) -> &'static str {
        match self {
            Self::Coff => "coff",
            Self::Darwin => "darwin",
            Self::Gnu => "gnu",
            Self::BigArchive => "bigarchive",
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_target::{ObjectFormat, TargetArchitecture};

    use super::ArchiveFormat;
    use crate::test_support::archive_target as target;

    #[test]
    fn formats_accept_only_supported_target_pairs() {
        let cases = [
            (
                target(TargetArchitecture::X86_64, ObjectFormat::Coff),
                Some(ArchiveFormat::Coff),
            ),
            (
                target(TargetArchitecture::Aarch64, ObjectFormat::MachO),
                Some(ArchiveFormat::Darwin),
            ),
            (
                target(TargetArchitecture::Riscv64, ObjectFormat::Elf),
                Some(ArchiveFormat::Gnu),
            ),
            (
                target(TargetArchitecture::Wasm32, ObjectFormat::WebAssembly),
                Some(ArchiveFormat::Gnu),
            ),
            (
                target(TargetArchitecture::PowerPc64, ObjectFormat::Xcoff),
                Some(ArchiveFormat::BigArchive),
            ),
            (target(TargetArchitecture::Arm, ObjectFormat::MachO), None),
            (
                target(TargetArchitecture::X86_64, ObjectFormat::Xcoff),
                None,
            ),
        ];

        for (target, expected) in cases {
            assert_eq!(ArchiveFormat::for_target(&target), expected);
        }
    }
}
