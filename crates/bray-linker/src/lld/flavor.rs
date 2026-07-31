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
    pub(crate) fn for_target(target: &LinkTarget, product: LinkedProductKind) -> Option<Self> {
        if product == LinkedProductKind::StaticLibrary {
            return None;
        }

        match (target.object_format(), target.architecture()) {
            (
                ObjectFormat::Elf,
                TargetArchitecture::X86
                | TargetArchitecture::X86_64
                | TargetArchitecture::Arm
                | TargetArchitecture::Aarch64
                | TargetArchitecture::Riscv32
                | TargetArchitecture::Riscv64
                | TargetArchitecture::PowerPc64,
            ) => Some(Self::Elf),
            (
                ObjectFormat::Coff,
                TargetArchitecture::X86
                | TargetArchitecture::X86_64
                | TargetArchitecture::Arm
                | TargetArchitecture::Aarch64,
            ) => Some(Self::Coff),
            (ObjectFormat::MachO, TargetArchitecture::X86_64 | TargetArchitecture::Aarch64) => {
                Some(Self::MachO)
            }
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
