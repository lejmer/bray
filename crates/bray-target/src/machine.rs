use std::num::{NonZeroU16, NonZeroU32};

/// Processor architecture understood across backend-neutral native-output phases.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum TargetArchitecture {
    /// 32-bit x86.
    X86,
    /// 64-bit x86.
    X86_64,
    /// 32-bit Arm.
    Arm,
    /// 64-bit Arm.
    Aarch64,
    /// 32-bit RISC-V.
    Riscv32,
    /// 64-bit RISC-V.
    Riscv64,
    /// 64-bit PowerPC.
    PowerPc64,
    /// 32-bit WebAssembly.
    Wasm32,
    /// 64-bit WebAssembly.
    Wasm64,
}

impl TargetArchitecture {
    /// Returns the language-defined architecture spelling.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::X86 => "x86",
            Self::X86_64 => "x86_64",
            Self::Arm => "arm",
            Self::Aarch64 => "aarch64",
            Self::Riscv32 => "riscv32",
            Self::Riscv64 => "riscv64",
            Self::PowerPc64 => "powerpc64",
            Self::Wasm32 => "wasm32",
            Self::Wasm64 => "wasm64",
        }
    }
}

/// Object format produced for one target platform.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ObjectFormat {
    /// Common Object File Format used by Windows targets.
    Coff,
    /// Executable and Linkable Format used by many Unix targets.
    Elf,
    /// Mach object format used by Apple targets.
    MachO,
    /// WebAssembly binary module format.
    WebAssembly,
    /// Extended Common Object File Format.
    Xcoff,
}

/// Byte order used by a target machine.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum Endianness {
    /// Least-significant byte first.
    Little,
    /// Most-significant byte first.
    Big,
}

/// Relocation policy used when producing or linking machine code.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RelocationModel {
    /// Toolchain-selected target default.
    Default,
    /// Statically addressed code.
    Static,
    /// Position-independent code.
    PositionIndependent,
    /// Dynamic code without position-independent data.
    DynamicNoPic,
}

/// Addressing range policy used when producing or linking machine code.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodeModel {
    /// Toolchain-selected target default.
    Default,
    /// Smallest addressing range supported by the target.
    Tiny,
    /// Small addressing range.
    Small,
    /// Medium addressing range.
    Medium,
    /// Large addressing range.
    Large,
    /// Kernel-oriented addressing range.
    Kernel,
}

/// Validated machine representation facts shared by backend-neutral compiler phases.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetMachineProperties {
    architecture: TargetArchitecture,
    object_format: ObjectFormat,
    endianness: Endianness,
    pointer_width_bits: NonZeroU16,
    pointer_alignment_bytes: NonZeroU32,
    stack_alignment_bytes: NonZeroU32,
}

impl TargetMachineProperties {
    /// Creates machine facts when pointer and alignment values are valid.
    pub const fn try_new(
        architecture: TargetArchitecture,
        object_format: ObjectFormat,
        endianness: Endianness,
        pointer_width_bits: NonZeroU16,
        pointer_alignment_bytes: NonZeroU32,
        stack_alignment_bytes: NonZeroU32,
    ) -> Option<Self> {
        if !pointer_width_bits.get().is_multiple_of(8)
            || !pointer_alignment_bytes.get().is_power_of_two()
            || !stack_alignment_bytes.get().is_power_of_two()
        {
            return None;
        }

        Some(Self {
            architecture,
            object_format,
            endianness,
            pointer_width_bits,
            pointer_alignment_bytes,
            stack_alignment_bytes,
        })
    }

    /// Returns the target processor architecture.
    pub const fn architecture(&self) -> TargetArchitecture {
        self.architecture
    }

    /// Returns the target object format.
    pub const fn object_format(&self) -> ObjectFormat {
        self.object_format
    }

    /// Returns the target byte order.
    pub const fn endianness(&self) -> Endianness {
        self.endianness
    }

    /// Returns the target pointer width in bits.
    pub const fn pointer_width_bits(&self) -> NonZeroU16 {
        self.pointer_width_bits
    }

    /// Returns the target pointer alignment in bytes.
    pub const fn pointer_alignment_bytes(&self) -> NonZeroU32 {
        self.pointer_alignment_bytes
    }

    /// Returns the minimum stack alignment in bytes.
    pub const fn stack_alignment_bytes(&self) -> NonZeroU32 {
        self.stack_alignment_bytes
    }
}

#[cfg(test)]
mod tests {
    use std::num::{NonZeroU16, NonZeroU32};

    use super::{Endianness, ObjectFormat, TargetArchitecture, TargetMachineProperties};

    #[test]
    fn machine_properties_reject_invalid_pointer_and_alignment_facts() {
        let non_byte_addressable_width = NonZeroU16::new(12).unwrap_or(NonZeroU16::MIN);
        let alignment = NonZeroU32::new(16).unwrap_or(NonZeroU32::MIN);

        assert_eq!(
            TargetMachineProperties::try_new(
                TargetArchitecture::X86_64,
                ObjectFormat::Elf,
                Endianness::Little,
                non_byte_addressable_width,
                alignment,
                alignment,
            ),
            None
        );

        let pointer_width = NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN);
        let invalid_alignment = NonZeroU32::new(3).unwrap_or(NonZeroU32::MIN);

        assert_eq!(
            TargetMachineProperties::try_new(
                TargetArchitecture::X86_64,
                ObjectFormat::Elf,
                Endianness::Little,
                pointer_width,
                invalid_alignment,
                alignment,
            ),
            None
        );
    }
}
