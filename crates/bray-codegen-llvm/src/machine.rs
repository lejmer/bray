use bray_codegen::{CodegenFailure, CodegenTarget};
use bray_target::{CodeModel as BrayCodeModel, RelocationModel};
use inkwell::OptimizationLevel;
use inkwell::module::Module;
use inkwell::targets::{
    CodeModel, RelocMode, Target, TargetMachine, TargetTriple,
};

use crate::initialization;

pub(crate) struct LlvmTargetMachine {
    machine: TargetMachine,
    triple: TargetTriple,
}

impl LlvmTargetMachine {
    pub(crate) fn create(target: &CodegenTarget) -> Result<Self, CodegenFailure> {
        initialization::initialize();

        let triple = TargetTriple::create(target.triple());

        let llvm_target =
            Target::from_triple(&triple).map_err(|_| CodegenFailure::UnsupportedTarget)?;

        let features = target
            .features()
            .map(|feature| format!("+{feature}"))
            .collect::<Vec<_>>()
            .join(",");

        let relocation = relocation_model(target.relocation_model());
        let code_model = code_model(target.code_model())?;

        let Some(machine) = llvm_target.create_target_machine(
            &triple,
            target.cpu(),
            &features,
            OptimizationLevel::None,
            relocation,
            code_model,
        ) else {
            return Err(CodegenFailure::UnsupportedTarget);
        };

        Ok(Self { machine, triple })
    }

    pub(crate) fn configure_module(&self, module: &Module<'_>) {
        module.set_triple(&self.triple);
        module.set_data_layout(&self.machine.get_target_data().get_data_layout());
    }
}

const fn relocation_model(model: RelocationModel) -> RelocMode {
    match model {
        RelocationModel::Default => RelocMode::Default,
        RelocationModel::Static => RelocMode::Static,
        RelocationModel::PositionIndependent => RelocMode::PIC,
        RelocationModel::DynamicNoPic => RelocMode::DynamicNoPic,
    }
}

const fn code_model(model: BrayCodeModel) -> Result<CodeModel, CodegenFailure> {
    match model {
        BrayCodeModel::Default => Ok(CodeModel::Default),
        BrayCodeModel::Small => Ok(CodeModel::Small),
        BrayCodeModel::Medium => Ok(CodeModel::Medium),
        BrayCodeModel::Large => Ok(CodeModel::Large),
        BrayCodeModel::Kernel => Ok(CodeModel::Kernel),
        BrayCodeModel::Tiny => Err(CodegenFailure::UnsupportedTarget),
    }
}

#[cfg(test)]
mod tests {
    use bray_codegen::test_support::codegen_target;

    use super::LlvmTargetMachine;

    #[test]
    fn target_machines_configure_task_local_modules() {
        let target = codegen_target();

        let Ok(machine) = LlvmTargetMachine::create(&target) else {
            panic!("test target must be supported by LLVM");
        };

        let context = inkwell::context::Context::create();
        let module = context.create_module("bray.codegen.test");

        machine.configure_module(&module);

        assert_eq!(
            module.get_triple().as_str(),
            inkwell::targets::TargetTriple::create(target.triple()).as_str()
        );

        assert!(!module.get_data_layout().as_str().is_empty());
    }
}
