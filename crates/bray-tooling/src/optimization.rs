use std::ffi::OsString;
use std::fs::File;
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

use bray_base::Cancellation;
use bray_codegen::{
    ArtifactContent, BackendBitcodeOptimizationOutcome, BackendBitcodeOptimizer, CodeGenerator,
    CodeGeneratorRegistry, CodegenConfiguration, CodegenFailure,
};
use bray_codegen_llvm::LlvmCodeGenerator;
use bray_compilation::{Compilation, CompilationRequest};
use bray_linker::{
    ExternalToolFailure, ExternalToolHost, ExternalToolInvocation, ExternalToolProcessBudget,
    NativeExternalToolHost,
};

use crate::product::LlvmCompilationLoadError;
use crate::toolchain::llvm_tool_path;

/// Loads a compilation configured with the LLVM code-generation backend.
pub fn load_llvm_compilation(
    request: CompilationRequest,
) -> Result<Compilation, LlvmCompilationLoadError> {
    let program = llvm_tool_path(bray_diagnostics::DiagnosticLlvmToolRole::Optimizer)
        .map_err(LlvmCompilationLoadError::Tool)?;

    let process_budget = request.options().worker_budget().nonzero();

    let host = Arc::new(NativeExternalToolHost::new(ExternalToolProcessBudget::new(
        process_budget,
    )));

    let optimizer = Arc::new(NativeBitcodeOptimizer::new(program, host));

    let generator = LlvmCodeGenerator::try_with_bitcode_optimizer(Some(optimizer))
        .map_err(LlvmCompilationLoadError::BackendInitialization)?;

    let identity = generator.identity().clone();

    let registry = CodeGeneratorRegistry::try_new([Arc::new(generator) as Arc<dyn CodeGenerator>])
        .map_err(LlvmCompilationLoadError::Registry)?;

    let codegen = CodegenConfiguration::try_new(registry, identity)
        .map_err(LlvmCompilationLoadError::BackendSelection)?;

    Compilation::load_with_codegen(request, codegen).map_err(LlvmCompilationLoadError::Compilation)
}

pub(super) struct NativeBitcodeOptimizer {
    program: PathBuf,
    host: Arc<NativeExternalToolHost>,
}

impl NativeBitcodeOptimizer {
    pub(super) const fn new(program: PathBuf, host: Arc<NativeExternalToolHost>) -> Self {
        Self { program, host }
    }
}

impl BackendBitcodeOptimizer for NativeBitcodeOptimizer {
    fn add_thin_lto_summary(
        &self,
        bitcode: &ArtifactContent,
        cancellation: &dyn Cancellation,
    ) -> Result<BackendBitcodeOptimizationOutcome, CodegenFailure> {
        if cancellation.is_cancelled() {
            return Ok(BackendBitcodeOptimizationOutcome::Cancelled);
        }

        let directory = tempfile::tempdir().map_err(CodegenFailure::backend_library)?;
        let input = directory.path().join("input.bc");
        let output = directory.path().join("summarized.bc");

        write_artifact(bitcode, &input).map_err(CodegenFailure::backend_library)?;

        let invocation = ExternalToolInvocation::try_new(
            self.program.clone(),
            [
                input.into_os_string(),
                OsString::from("-module-summary"),
                OsString::from("-o"),
                output.clone().into_os_string(),
            ],
            [],
            None,
            [],
        )
        .map_err(|_| CodegenFailure::InvalidConfiguration)?;

        match self.host.run(&invocation, cancellation) {
            Ok(output) if output.success() => {}
            Err(ExternalToolFailure::Cancelled) => {
                return Ok(BackendBitcodeOptimizationOutcome::Cancelled);
            }
            Ok(output) => {
                return Err(CodegenFailure::BackendToolExited {
                    program: self.program.clone(),
                    exit: bray_diagnostics::DiagnosticExternalToolExit::new(
                        output.exit_code(),
                        output.standard_output(),
                        output.standard_error(),
                    ),
                });
            }
            Err(error) => {
                return Err(CodegenFailure::backend_library(format!("{error:?}")));
            }
        }

        if cancellation.is_cancelled() {
            return Ok(BackendBitcodeOptimizationOutcome::Cancelled);
        }

        let bytes = std::fs::read(output).map_err(CodegenFailure::backend_library)?;

        let content = ArtifactContent::try_memory(bytes).map_err(|_| {
            CodegenFailure::ArtifactConstruction(bray_codegen::BackendArtifactKind::BackendBitcode)
        })?;

        Ok(BackendBitcodeOptimizationOutcome::Complete(content))
    }
}

fn write_artifact(content: &ArtifactContent, destination: &std::path::Path) -> io::Result<()> {
    let mut destination = File::create(destination)?;

    let mut source = content.open_reader().map_err(|error| {
        io::Error::new(
            error.source().map_or(io::ErrorKind::Other, io::Error::kind),
            "artifact content is unavailable",
        )
    })?;

    io::copy(&mut source, &mut destination).map(|_| ())
}
