use bray_target::{ObjectFormat, TargetArchitecture};

use crate::{LinkTarget, LinkedProductKind};

/// LLD command-language flavor selected by a native target.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum LldFlavor {
    /// GNU-compatible ELF linker command language.
    Elf,
    /// Microsoft-compatible COFF linker command language.
    Coff,
    /// Apple-compatible Mach-O linker command language.
    MachO,
}

impl LldFlavor {
    pub(crate) const TARGETS: [(TargetArchitecture, ObjectFormat, Self); 13] = [
        (TargetArchitecture::X86, ObjectFormat::Elf, Self::Elf),
        (TargetArchitecture::X86_64, ObjectFormat::Elf, Self::Elf),
        (TargetArchitecture::Arm, ObjectFormat::Elf, Self::Elf),
        (TargetArchitecture::Aarch64, ObjectFormat::Elf, Self::Elf),
        (TargetArchitecture::Riscv32, ObjectFormat::Elf, Self::Elf),
        (TargetArchitecture::Riscv64, ObjectFormat::Elf, Self::Elf),
        (TargetArchitecture::PowerPc64, ObjectFormat::Elf, Self::Elf),
        (TargetArchitecture::X86, ObjectFormat::Coff, Self::Coff),
        (TargetArchitecture::X86_64, ObjectFormat::Coff, Self::Coff),
        (TargetArchitecture::Arm, ObjectFormat::Coff, Self::Coff),
        (TargetArchitecture::Aarch64, ObjectFormat::Coff, Self::Coff),
        (TargetArchitecture::X86_64, ObjectFormat::MachO, Self::MachO),
        (
            TargetArchitecture::Aarch64,
            ObjectFormat::MachO,
            Self::MachO,
        ),
    ];

    pub(crate) const fn object_format(self) -> ObjectFormat {
        match self {
            Self::Elf => ObjectFormat::Elf,
            Self::Coff => ObjectFormat::Coff,
            Self::MachO => ObjectFormat::MachO,
        }
    }

    pub(crate) fn for_target(target: &LinkTarget, product: LinkedProductKind) -> Option<Self> {
        if product == LinkedProductKind::StaticLibrary {
            return None;
        }

        Self::TARGETS.iter().find_map(|candidate| {
            (candidate.0 == target.architecture() && candidate.1 == target.object_format())
                .then_some(candidate.2)
        })
    }

    pub(super) const fn external_selector(self) -> &'static str {
        match self {
            Self::Elf => "gnu",
            Self::Coff => "link",
            Self::MachO => "darwin",
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_target::{ObjectFormat, TargetArchitecture};

    use super::LldFlavor;
    use crate::LinkedProductKind;
    use crate::test_support::target;

    #[test]
    fn target_object_formats_select_compatible_lld_flavors() {
        let cases = [
            (
                TargetArchitecture::X86_64,
                ObjectFormat::Elf,
                Some(LldFlavor::Elf),
            ),
            (
                TargetArchitecture::Aarch64,
                ObjectFormat::Coff,
                Some(LldFlavor::Coff),
            ),
            (
                TargetArchitecture::Aarch64,
                ObjectFormat::MachO,
                Some(LldFlavor::MachO),
            ),
            (TargetArchitecture::PowerPc64, ObjectFormat::Xcoff, None),
            (TargetArchitecture::Wasm32, ObjectFormat::WebAssembly, None),
        ];

        for (architecture, object_format, expected) in cases {
            let target = target(architecture, object_format);

            assert_eq!(
                LldFlavor::for_target(&target, LinkedProductKind::Executable),
                expected
            );
        }
    }

    #[test]
    fn lld_does_not_claim_static_archive_construction() {
        let target = target(TargetArchitecture::X86_64, ObjectFormat::Elf);

        assert_eq!(
            LldFlavor::for_target(&target, LinkedProductKind::StaticLibrary),
            None
        );
    }
}
