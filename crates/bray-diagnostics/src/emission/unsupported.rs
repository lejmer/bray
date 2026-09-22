use std::path::PathBuf;

use crate::DiagnosticIoErrorKind;

/// Compiler-owned LLVM tool role required by native product emission.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticLlvmToolRole {
    /// C-family compiler driver used to invoke a platform linker.
    CompilerDriver,
    /// LLVM linker used by native execution readiness checks.
    Linker,
    /// LLVM archive writer and reader.
    Archiver,
    /// LLVM optimizer used to form summary-bearing bitcode.
    Optimizer,
    /// LLVM symbol-table inspector.
    SymbolInspector,
    /// LLVM object-file inspector.
    ObjectInspector,
    /// LLVM bitcode structure inspector.
    BitcodeInspector,
}

impl DiagnosticLlvmToolRole {
    /// Returns the tool's stable machine key.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::CompilerDriver => "compiler_driver",
            Self::Linker => "linker",
            Self::Archiver => "archiver",
            Self::Optimizer => "optimizer",
            Self::SymbolInspector => "symbol_inspector",
            Self::ObjectInspector => "object_inspector",
            Self::BitcodeInspector => "bitcode_inspector",
        }
    }

    /// Returns the executable stem used by the LLVM distribution.
    pub const fn executable_name(self) -> &'static str {
        match self {
            Self::CompilerDriver => "clang",
            Self::Linker => "ld.lld",
            Self::Archiver => "llvm-ar",
            Self::Optimizer => "opt",
            Self::SymbolInspector => "llvm-nm",
            Self::ObjectInspector => "llvm-readobj",
            Self::BitcodeInspector => "llvm-dis",
        }
    }
}

/// Closed host environment value required by native toolchain selection.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticHostEnvironmentVariable {
    /// Windows installation root used to locate the WSL host bridge.
    WindowsSystemRoot,
}

impl DiagnosticHostEnvironmentVariable {
    /// Returns the variable's stable host key.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::WindowsSystemRoot => "SystemRoot",
        }
    }
}

/// Exact host-toolchain reason that prevents native product emission.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DiagnosticUnsupportedEmissionReason {
    /// Neither a configured installation nor the bundled toolchain contains the required tool.
    ToolUnavailable {
        /// Required LLVM tool role.
        tool: DiagnosticLlvmToolRole,
        /// Configured candidate path when an LLVM prefix selected one.
        configured: Option<PathBuf>,
    },
    /// The host could not inspect one concrete LLVM tool candidate.
    ToolInspectionFailed {
        /// Required LLVM tool role.
        tool: DiagnosticLlvmToolRole,
        /// Exact candidate path inspected by the compiler.
        path: PathBuf,
        /// Stable host I/O failure category.
        error: DiagnosticIoErrorKind,
    },
    /// A concrete LLVM tool candidate is not a regular executable file.
    InvalidToolFile {
        /// Required LLVM tool role.
        tool: DiagnosticLlvmToolRole,
        /// Exact candidate path that violated the tool contract.
        path: PathBuf,
    },
    /// A required closed host environment value is absent.
    MissingHostEnvironment(DiagnosticHostEnvironmentVariable),
}
