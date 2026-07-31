use bray_codegen::{CodegenFailure, CodegenOptions, OptimizationLevel, SizePreference};
use inkwell::module::Module;

use crate::machine::LlvmTargetMachine;

pub(crate) fn optimize_module(
    module: &Module<'_>,
    machine: &LlvmTargetMachine,
    options: CodegenOptions,
) -> Result<(), CodegenFailure> {
    let Some(pipeline) = optimization_pipeline(options) else {
        return Ok(());
    };

    machine.run_passes(module, pipeline)
}

const fn optimization_pipeline(options: CodegenOptions) -> Option<&'static str> {
    match (options.optimization(), options.size_preference()) {
        (_, SizePreference::MinimumSize) => Some("default<Oz>"),
        (_, SizePreference::Size) => Some("default<Os>"),
        (OptimizationLevel::None, SizePreference::None) => None,
        (OptimizationLevel::Basic, SizePreference::None) => Some("default<O1>"),
        (OptimizationLevel::Full, SizePreference::None) => Some("default<O2>"),
    }
}

#[cfg(test)]
mod tests {
    use bray_codegen::{CodegenOptions, DebugInformationMode, OptimizationLevel, SizePreference};

    use super::optimization_pipeline;

    #[test]
    fn optimization_intent_selects_canonical_llvm_pipelines() {
        assert_eq!(
            optimization_pipeline(options(OptimizationLevel::None, SizePreference::None)),
            None
        );

        assert_eq!(
            optimization_pipeline(options(OptimizationLevel::Basic, SizePreference::None)),
            Some("default<O1>")
        );

        assert_eq!(
            optimization_pipeline(options(OptimizationLevel::Full, SizePreference::None)),
            Some("default<O2>")
        );

        assert_eq!(
            optimization_pipeline(options(OptimizationLevel::Full, SizePreference::Size)),
            Some("default<Os>")
        );

        assert_eq!(
            optimization_pipeline(options(
                OptimizationLevel::Basic,
                SizePreference::MinimumSize
            )),
            Some("default<Oz>")
        );
    }

    const fn options(
        optimization: OptimizationLevel,
        size_preference: SizePreference,
    ) -> CodegenOptions {
        CodegenOptions::new(optimization, size_preference, DebugInformationMode::None)
    }
}
