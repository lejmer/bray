use bray_codegen::{
    CodegenInstanceBuildError, CodegenReachabilityBuildError, CodegenTargetBuildError,
    CodegenUnitBuildError,
};
use bray_emitter::EmissionBackendBuildError;
use bray_linker::LinkTargetBuildError;
use bray_runtime_interface::ExecutableHostContractBuildError;

use crate::fact::FactQueryError;

/// A failure to derive complete native product facts.
#[derive(Debug)]
pub enum NativeProductFactError {
    /// The compilation has no selected code generation backend.
    CodegenUnavailable,
    /// The selected product has no executable code root.
    MissingProductRoot,
    /// The checked executable entry result is inconsistent with product semantics.
    InvalidEntryResult,
    /// An asynchronous product has no selected runtime artifact.
    MissingRuntime,
    /// A generated binary symbol name is invalid.
    InvalidSymbolName,
    /// A configured native link input is invalid.
    InvalidNativeLinkInput,
    /// A lazy compilation fact could not be evaluated.
    Query(FactQueryError),
    /// The selected code generation target is invalid.
    InvalidCodegenTarget(CodegenTargetBuildError),
    /// Code generation reachability is inconsistent.
    InvalidReachability(CodegenReachabilityBuildError),
    /// One concrete code generation instance is invalid.
    InvalidCodegenInstance(CodegenInstanceBuildError),
    /// One code generation unit is invalid.
    InvalidCodegenUnit(CodegenUnitBuildError),
    /// The compiler-generated executable host MIR is invalid.
    InvalidHostMir(bray_ir::MirUnitBuildError),
    /// The compiler-generated executable host contract is invalid.
    InvalidExecutableHost(ExecutableHostContractBuildError),
    /// The selected emitter backend description is invalid.
    InvalidEmissionBackend(EmissionBackendBuildError),
    /// The selected linker target is invalid.
    InvalidLinkTarget(LinkTargetBuildError),
    /// No configured linker driver supports the selected product.
    Linker(bray_linker::LinkFailure),
    /// The configured standard library cannot supply a required native artifact.
    StandardLibrary(bray_standard_library::StandardLibraryLoadError),
    /// One code generation fact is unavailable.
    Codegen(super::super::super::CodegenFactError),
}

impl NativeProductFactError {
    /// Returns whether the selected target cannot realize a demanded native representation.
    pub const fn is_unsupported(&self) -> bool {
        matches!(
            self,
            Self::CodegenUnavailable
                | Self::MissingRuntime
                | Self::Codegen(super::super::super::CodegenFactError::UnsupportedType(_))
        )
    }

    /// Returns structured diagnostics produced while deriving native product facts.
    pub const fn diagnostics(&self) -> Option<&bray_diagnostics::DiagnosticBag> {
        match self {
            Self::Codegen(super::super::super::CodegenFactError::Diagnostics(diagnostics)) => {
                Some(diagnostics)
            }
            _ => None,
        }
    }
}

impl From<FactQueryError> for NativeProductFactError {
    fn from(error: FactQueryError) -> Self {
        Self::Query(error)
    }
}

impl From<super::super::super::CodegenFactError> for NativeProductFactError {
    fn from(error: super::super::super::CodegenFactError) -> Self {
        Self::Codegen(error)
    }
}
