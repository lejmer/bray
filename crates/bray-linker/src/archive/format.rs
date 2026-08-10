use bray_target::{ObjectFormat, TargetArchitecture};

use crate::LinkTarget;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum ArchiveFormat {
    Coff,
    Darwin,
    Gnu,
    BigArchive,
}

impl ArchiveFormat {
    pub(crate) const TARGETS: [(TargetArchitecture, ObjectFormat, Self); 16] = [
        (TargetArchitecture::X86, ObjectFormat::Coff, Self::Coff),
        (TargetArchitecture::X86_64, ObjectFormat::Coff, Self::Coff),
        (TargetArchitecture::Arm, ObjectFormat::Coff, Self::Coff),
        (TargetArchitecture::Aarch64, ObjectFormat::Coff, Self::Coff),
        (TargetArchitecture::X86, ObjectFormat::Elf, Self::Gnu),
        (TargetArchitecture::X86_64, ObjectFormat::Elf, Self::Gnu),
        (TargetArchitecture::Arm, ObjectFormat::Elf, Self::Gnu),
        (TargetArchitecture::Aarch64, ObjectFormat::Elf, Self::Gnu),
        (TargetArchitecture::Riscv32, ObjectFormat::Elf, Self::Gnu),
        (TargetArchitecture::Riscv64, ObjectFormat::Elf, Self::Gnu),
        (TargetArchitecture::PowerPc64, ObjectFormat::Elf, Self::Gnu),
        (
            TargetArchitecture::Wasm32,
            ObjectFormat::WebAssembly,
            Self::Gnu,
        ),
        (
            TargetArchitecture::Wasm64,
            ObjectFormat::WebAssembly,
            Self::Gnu,
        ),
        (
            TargetArchitecture::X86_64,
            ObjectFormat::MachO,
            Self::Darwin,
        ),
        (
            TargetArchitecture::Aarch64,
            ObjectFormat::MachO,
            Self::Darwin,
        ),
        (
            TargetArchitecture::PowerPc64,
            ObjectFormat::Xcoff,
            Self::BigArchive,
        ),
    ];

    pub(super) fn for_target(target: &LinkTarget) -> Option<Self> {
        Self::TARGETS.iter().find_map(|candidate| {
            (candidate.0 == target.architecture() && candidate.1 == target.object_format())
                .then_some(candidate.2)
        })
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
