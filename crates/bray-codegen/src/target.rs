use std::num::{NonZeroU16, NonZeroU32};
use std::sync::Arc;

use bray_base::{shared_str, sorted_unique_shared_slice};

/// Stable compiler-facing identity of one validated code generation target.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetIdentity(Arc<str>);

impl TargetIdentity {
    /// Creates a target identity unless its canonical representation is empty.
    pub fn try_new(value: impl Into<Arc<str>>) -> Option<Self> {
        let value = shared_str(value);

        if value.is_empty() {
            return None;
        }

        Some(Self(value))
    }

    /// Returns the canonical target identity.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for TargetIdentity {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

/// Processor architecture understood by backend-neutral code generation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

/// Object format produced for one target platform.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
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

/// Relocation policy used when producing machine code.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum RelocationModel {
    /// Backend-selected target default.
    Default,
    /// Statically addressed code.
    Static,
    /// Position-independent code.
    PositionIndependent,
    /// Dynamic code without position-independent data.
    DynamicNoPic,
}

/// Addressing range policy used when producing machine code.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum CodeModel {
    /// Backend-selected target default.
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

/// Validated machine representation facts shared by all backends.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetMachineProperties {
    architecture: TargetArchitecture,
    object_format: ObjectFormat,
    endianness: Endianness,
    pointer_width_bits: NonZeroU16,
    stack_alignment_bytes: NonZeroU32,
    default_address_space: u32,
}

impl TargetMachineProperties {
    /// Creates target machine facts when the pointer width is byte-addressable.
    pub fn try_new(
        architecture: TargetArchitecture,
        object_format: ObjectFormat,
        endianness: Endianness,
        pointer_width_bits: NonZeroU16,
        stack_alignment_bytes: NonZeroU32,
        default_address_space: u32,
    ) -> Option<Self> {
        if !pointer_width_bits.get().is_multiple_of(8) {
            return None;
        }

        Some(Self {
            architecture,
            object_format,
            endianness,
            pointer_width_bits,
            stack_alignment_bytes,
            default_address_space,
        })
    }

    /// Returns the target processor architecture.
    pub const fn architecture(&self) -> &TargetArchitecture {
        &self.architecture
    }

    /// Returns the target object format.
    pub const fn object_format(&self) -> &ObjectFormat {
        &self.object_format
    }

    /// Returns the target byte order.
    pub const fn endianness(&self) -> Endianness {
        self.endianness
    }

    /// Returns the target pointer width in bits.
    pub const fn pointer_width_bits(&self) -> NonZeroU16 {
        self.pointer_width_bits
    }

    /// Returns the minimum stack alignment in bytes.
    pub const fn stack_alignment_bytes(&self) -> NonZeroU32 {
        self.stack_alignment_bytes
    }

    /// Returns the target's ordinary address space.
    pub const fn default_address_space(&self) -> u32 {
        self.default_address_space
    }
}

/// Validated backend-neutral target configuration for code generation.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CodegenTarget {
    identity: TargetIdentity,
    triple: Arc<str>,
    machine: TargetMachineProperties,
    relocation_model: RelocationModel,
    code_model: CodeModel,
    cpu: Arc<str>,
    features: Arc<[Arc<str>]>,
}

impl CodegenTarget {
    /// Creates a target when its canonical triple, CPU, and feature names are non-empty.
    pub fn try_new<Features, Feature>(
        identity: TargetIdentity,
        triple: impl Into<Arc<str>>,
        machine: TargetMachineProperties,
        relocation_model: RelocationModel,
        code_model: CodeModel,
        cpu: impl Into<Arc<str>>,
        features: Features,
    ) -> Option<Self>
    where
        Features: IntoIterator<Item = Feature>,
        Feature: Into<Arc<str>>,
    {
        let triple = shared_str(triple);
        let cpu = shared_str(cpu);
        let features: Vec<_> = features.into_iter().map(Into::into).collect();

        if triple.is_empty() || cpu.is_empty() || features.iter().any(|feature| feature.is_empty())
        {
            return None;
        }

        Some(Self {
            identity,
            triple,
            machine,
            relocation_model,
            code_model,
            cpu,
            features: sorted_unique_shared_slice(features),
        })
    }

    /// Returns the stable target identity used by codegen facts and artifacts.
    pub const fn identity(&self) -> &TargetIdentity {
        &self.identity
    }

    /// Returns the canonical target triple.
    pub fn triple(&self) -> &str {
        &self.triple
    }

    /// Returns the validated machine representation facts.
    pub const fn machine(&self) -> &TargetMachineProperties {
        &self.machine
    }

    /// Returns the selected relocation policy.
    pub const fn relocation_model(&self) -> RelocationModel {
        self.relocation_model
    }

    /// Returns the selected code model.
    pub const fn code_model(&self) -> CodeModel {
        self.code_model
    }

    /// Returns the canonical target CPU name.
    pub fn cpu(&self) -> &str {
        &self.cpu
    }

    /// Returns enabled target features in canonical deterministic order.
    pub fn features(&self) -> impl ExactSizeIterator<Item = &str> {
        self.features.iter().map(AsRef::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CodeModel, CodegenTarget, Endianness, ObjectFormat, RelocationModel, TargetArchitecture,
        TargetIdentity, TargetMachineProperties,
    };
    use std::num::{NonZeroU16, NonZeroU32};

    #[test]
    fn targets_canonicalize_feature_order() {
        let target = target(["sse4.2", "avx", "sse4.2"]);

        assert_eq!(target.features().collect::<Vec<_>>(), ["avx", "sse4.2"]);
    }

    #[test]
    fn machine_properties_reject_non_byte_addressable_pointers() {
        let pointer_width = NonZeroU16::new(12).unwrap_or(NonZeroU16::MIN);
        let stack_alignment = NonZeroU32::new(16).unwrap_or(NonZeroU32::MIN);

        assert_eq!(
            TargetMachineProperties::try_new(
                TargetArchitecture::X86_64,
                ObjectFormat::Elf,
                Endianness::Little,
                pointer_width,
                stack_alignment,
                0,
            ),
            None
        );
    }

    fn target(features: impl IntoIterator<Item = &'static str>) -> CodegenTarget {
        let Some(identity) = TargetIdentity::try_new("x86_64-linux") else {
            panic!("test target identity must be valid");
        };

        let pointer_width = NonZeroU16::new(64).unwrap_or(NonZeroU16::MIN);
        let stack_alignment = NonZeroU32::new(16).unwrap_or(NonZeroU32::MIN);

        let Some(machine) = TargetMachineProperties::try_new(
            TargetArchitecture::X86_64,
            ObjectFormat::Elf,
            Endianness::Little,
            pointer_width,
            stack_alignment,
            0,
        ) else {
            panic!("test machine properties must be valid");
        };

        let Some(target) = CodegenTarget::try_new(
            identity,
            "x86_64-unknown-linux-gnu",
            machine,
            RelocationModel::PositionIndependent,
            CodeModel::Small,
            "x86-64-v3",
            features,
        ) else {
            panic!("test target must be valid");
        };

        target
    }
}
