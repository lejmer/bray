use crate::{NativeTarget, TargetArchitecture, TargetProfile};

/// Validated option bits controlling one trusted inline-assembly operation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct InlineAssemblyOptions {
    bits: u64,
}

impl InlineAssemblyOptions {
    const PURE: u64 = 1;
    const ALIGNED_STACK: u64 = 1 << 1;
    const INTEL_DIALECT: u64 = 1 << 2;
    const MAY_UNWIND: u64 = 1 << 3;
    const SUPPORTED: u64 =
        Self::PURE | Self::ALIGNED_STACK | Self::INTEL_DIALECT | Self::MAY_UNWIND;

    /// Validates and retains a complete inline-assembly option bit set.
    pub const fn try_new(bits: u64) -> Option<Self> {
        if bits & !Self::SUPPORTED == 0 {
            Some(Self { bits })
        } else {
            None
        }
    }

    /// Returns whether the assembly has no effects outside its typed output.
    pub const fn pure(self) -> bool {
        self.bits & Self::PURE != 0
    }

    /// Returns whether the assembly requires stack realignment.
    pub const fn aligned_stack(self) -> bool {
        self.bits & Self::ALIGNED_STACK != 0
    }

    /// Returns whether the assembly uses Intel syntax.
    pub const fn intel_dialect(self) -> bool {
        self.bits & Self::INTEL_DIALECT != 0
    }

    /// Returns whether the assembly may unwind through its caller.
    pub const fn may_unwind(self) -> bool {
        self.bits & Self::MAY_UNWIND != 0
    }
}

/// Target instruction, register, feature, and inline-assembly properties.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct TargetControlSupport {
    architecture: TargetArchitecture,
    native_target: Option<NativeTarget>,
}

impl TargetControlSupport {
    /// Returns the complete target-control properties for one machine architecture.
    pub const fn for_architecture(architecture: TargetArchitecture) -> Self {
        Self {
            architecture,
            native_target: None,
        }
    }

    /// Returns the complete target-control properties for one exact target profile.
    pub fn for_profile(profile: &TargetProfile) -> Self {
        Self {
            architecture: profile.machine().architecture(),
            native_target: NativeTarget::for_profile(profile),
        }
    }

    /// Returns the instruction-set spelling accepted by target-gated code.
    pub const fn instruction_set(self) -> &'static str {
        self.architecture.as_str()
    }

    /// Returns whether trusted inline assembly is available.
    pub const fn inline_assembly(self) -> bool {
        !matches!(self.architecture, TargetArchitecture::Wasm32 | TargetArchitecture::Wasm64)
    }

    /// Returns whether LLVM's Intel assembly dialect is valid for this target.
    pub const fn intel_assembly_dialect(self) -> bool {
        matches!(self.architecture, TargetArchitecture::X86 | TargetArchitecture::X86_64)
    }

    /// Returns whether a target instruction feature is guaranteed by the profile.
    pub fn supports_feature(self, feature: &str) -> bool {
        required_features(self.architecture).contains(&feature)
    }

    /// Returns whether the named register or register class is available.
    pub fn supports_register(self, register: &str) -> bool {
        register_classes(self.architecture).contains(&register)
            || self.supports_physical_register(register)
    }

    /// Returns whether the name identifies one exact physical register on this architecture.
    pub fn supports_physical_register(self, register: &str) -> bool {
        clobber_registers(self.architecture).contains(&register)
    }

    /// Returns whether the named portable clobber has meaning on this target.
    pub fn supports_clobber(self, clobber: &str) -> bool {
        match clobber {
            "memory" => true,
            "cc" => self.inline_assembly(),
            "flags" | "dirflag" => self.intel_assembly_dialect(),
            "fpsr" => matches!(
                self.architecture,
                TargetArchitecture::X86
                    | TargetArchitecture::X86_64
                    | TargetArchitecture::Arm
                    | TargetArchitecture::Aarch64
                    | TargetArchitecture::PowerPc64
            ),
            _ => false,
        }
    }

    /// Returns the LLVM constraint spelling for one target register class.
    pub fn register_constraint(self, register_class: &str) -> Option<&'static str> {
        let constraint = match self.architecture {
            TargetArchitecture::X86 | TargetArchitecture::X86_64 => match register_class {
                "reg" => "r",
                "reg_byte" => "q",
                "xmm" | "ymm" | "zmm" => "x",
                _ => return None,
            },
            TargetArchitecture::Arm => match register_class {
                "reg" => "r",
                "sreg" | "dreg" | "qreg" => "w",
                _ => return None,
            },
            TargetArchitecture::Aarch64 => match register_class {
                "reg" => "r",
                "vreg" => "w",
                _ => return None,
            },
            TargetArchitecture::Riscv32 | TargetArchitecture::Riscv64 => match register_class {
                "reg" => "r",
                "freg" => "f",
                _ => return None,
            },
            TargetArchitecture::PowerPc64 => match register_class {
                "reg" => "r",
                "freg" => "f",
                "vreg" => "v",
                _ => return None,
            },
            TargetArchitecture::Wasm32 | TargetArchitecture::Wasm64 => return None,
        };

        Some(constraint)
    }

    /// Returns whether the target accepts a named assembly ABI clobber set.
    pub fn supports_clobber_abi(self, abi: &str) -> bool {
        self.native_target.is_some() && (abi == "C" || abi == "system")
    }

    /// Returns the exact caller-saved register set for a supported assembly ABI.
    pub fn abi_clobbers(self, abi: &str) -> Option<&'static [&'static str]> {
        if !self.supports_clobber_abi(abi) {
            return None;
        }

        Some(match self.native_target? {
            NativeTarget::X86_64LinuxGnu | NativeTarget::X86_64MacOs => &[
                "rax", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11", "xmm0",
                "xmm1", "xmm2", "xmm3", "xmm4", "xmm5", "xmm6", "xmm7", "xmm8",
                "xmm9", "xmm10", "xmm11", "xmm12", "xmm13", "xmm14", "xmm15",
            ],
            NativeTarget::X86_64WindowsMsvc => &[
                "rax", "rcx", "rdx", "r8", "r9", "r10", "r11", "xmm0", "xmm1", "xmm2",
                "xmm3", "xmm4", "xmm5",
            ],
            NativeTarget::Aarch64LinuxGnu
            | NativeTarget::Aarch64WindowsMsvc
            | NativeTarget::Aarch64MacOs => &[
                "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10",
                "x11", "x12", "x13", "x14", "x15", "x16", "x17", "x30", "v0", "v1",
                "v2", "v3", "v4", "v5", "v6", "v7", "v16", "v17", "v18", "v19",
                "v20", "v21", "v22", "v23", "v24", "v25", "v26", "v27", "v28", "v29",
                "v30", "v31",
            ],
        })
    }
}

const fn required_features(architecture: TargetArchitecture) -> &'static [&'static str] {
    match architecture {
        TargetArchitecture::X86 => &["x87"],
        TargetArchitecture::X86_64 => &["x87", "sse2"],
        TargetArchitecture::Arm => &[],
        TargetArchitecture::Aarch64 => &["neon"],
        TargetArchitecture::Riscv32 | TargetArchitecture::Riscv64 => &["i"],
        TargetArchitecture::PowerPc64 => &[],
        TargetArchitecture::Wasm32 | TargetArchitecture::Wasm64 => &[],
    }
}

const fn register_classes(architecture: TargetArchitecture) -> &'static [&'static str] {
    match architecture {
        TargetArchitecture::X86 => &["reg", "reg_byte"],
        TargetArchitecture::X86_64 => &["reg", "reg_byte", "xmm"],
        TargetArchitecture::Arm => &["reg"],
        TargetArchitecture::Aarch64 => &["reg", "vreg"],
        TargetArchitecture::Riscv32 | TargetArchitecture::Riscv64 => &["reg"],
        TargetArchitecture::PowerPc64 => &["reg"],
        TargetArchitecture::Wasm32 | TargetArchitecture::Wasm64 => &[],
    }
}

const fn clobber_registers(architecture: TargetArchitecture) -> &'static [&'static str] {
    match architecture {
        TargetArchitecture::X86 => &["eax", "ebx", "ecx", "edx", "esi", "edi"],
        TargetArchitecture::X86_64 => &[
            "rax", "rbx", "rcx", "rdx", "rsi", "rdi", "r8", "r9", "r10", "r11", "r12",
            "r13", "r14", "r15", "xmm0", "xmm1", "xmm2", "xmm3", "xmm4", "xmm5",
            "xmm6", "xmm7", "xmm8", "xmm9", "xmm10", "xmm11", "xmm12", "xmm13",
            "xmm14", "xmm15",
        ],
        TargetArchitecture::Arm => &[
            "r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10", "r11",
            "r12", "lr",
        ],
        TargetArchitecture::Aarch64 => &[
            "x0", "x1", "x2", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11",
            "x12", "x13", "x14", "x15", "x16", "x17", "x19", "x20", "x21",
            "x22", "x23", "x24", "x25", "x26", "x27", "x28", "x29", "x30", "v0", "v1",
            "v2", "v3", "v4", "v5", "v6", "v7", "v8", "v9", "v10", "v11", "v12",
            "v13", "v14", "v15", "v16", "v17", "v18", "v19", "v20", "v21", "v22",
            "v23", "v24", "v25", "v26", "v27", "v28", "v29", "v30", "v31",
        ],
        TargetArchitecture::Riscv32 | TargetArchitecture::Riscv64 => &[
            "x1", "x3", "x4", "x5", "x6", "x7", "x8", "x9", "x10", "x11", "x12", "x13",
            "x14", "x15", "x16", "x17", "x18", "x19", "x20", "x21", "x22", "x23",
            "x24", "x25", "x26", "x27", "x28", "x29", "x30", "x31",
        ],
        TargetArchitecture::PowerPc64 => &[
            "r0", "r1", "r2", "r3", "r4", "r5", "r6", "r7", "r8", "r9", "r10",
            "r11", "r12", "r13", "r14", "r15", "r16", "r17", "r18", "r19", "r20",
            "r21", "r22", "r23", "r24", "r25", "r26", "r27", "r28", "r29", "r30",
            "r31",
        ],
        TargetArchitecture::Wasm32 | TargetArchitecture::Wasm64 => &[],
    }
}

#[cfg(test)]
mod tests {
    use super::{InlineAssemblyOptions, TargetControlSupport};
    use crate::{NativeTarget, TargetArchitecture};

    #[test]
    fn profiles_publish_exact_architecture_control_properties() {
        let x86_profile = NativeTarget::X86_64WindowsMsvc.profile();
        let x86 = TargetControlSupport::for_profile(&x86_profile);
        let wasm = TargetControlSupport::for_architecture(TargetArchitecture::Wasm32);

        assert!(x86.inline_assembly());
        assert!(x86.intel_assembly_dialect());
        assert!(x86.supports_feature("sse2"));
        assert!(x86.supports_register("reg"));
        assert!(x86.supports_register("rax"));
        assert!(!x86.supports_physical_register("reg"));
        assert!(x86.supports_physical_register("rax"));
        assert!(x86.supports_clobber("dirflag"));
        assert_eq!(x86.register_constraint("reg"), Some("r"));
        assert!(x86.supports_clobber_abi("system"));

        assert_eq!(
            x86.abi_clobbers("C"),
            Some(
                [
                    "rax", "rcx", "rdx", "r8", "r9", "r10", "r11", "xmm0", "xmm1",
                    "xmm2", "xmm3", "xmm4", "xmm5",
                ]
                .as_slice()
            )
        );

        let system_v_profile = NativeTarget::X86_64LinuxGnu.profile();
        let system_v = TargetControlSupport::for_profile(&system_v_profile);

        assert!(system_v
            .abi_clobbers("C")
            .is_some_and(|registers| registers.contains(&"rsi") && registers.contains(&"xmm15")));

        assert!(!TargetControlSupport::for_architecture(TargetArchitecture::X86_64)
            .supports_clobber_abi("C"));

        assert!(!wasm.inline_assembly());
        assert!(!wasm.intel_assembly_dialect());
        assert!(!wasm.supports_register("reg"));
    }

    #[test]
    fn inline_assembly_options_reject_unknown_bits() {
        let Some(options) = InlineAssemblyOptions::try_new(0b1111) else {
            panic!("all declared inline assembly options must be valid");
        };

        assert!(options.pure());
        assert!(options.aligned_stack());
        assert!(options.intel_dialect());
        assert!(options.may_unwind());
        assert!(InlineAssemblyOptions::try_new(1 << 4).is_none());
    }
}
