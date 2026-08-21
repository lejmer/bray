use std::num::NonZeroU32;
use std::sync::Arc;

use bray_codegen::{
    CodegenFailure, CodegenTarget, OptimizationLevel as BrayOptimizationLevel, TargetScalarKind,
    TargetScalarLayout,
};
use bray_target::{
    CodeModel as BrayCodeModel, Endianness as BrayEndianness, ObjectFormat, RelocationModel,
    TargetArchitecture,
};
use inkwell::OptimizationLevel;
use inkwell::context::Context;
use inkwell::module::Module;
use inkwell::targets::{
    ByteOrdering, CodeModel, FileType, RelocMode, Target, TargetMachine, TargetTriple,
};
use inkwell::types::BasicTypeEnum;
use target_lexicon::{Architecture, BinaryFormat, Endianness, Triple};

use crate::initialization;
use crate::session::LlvmBackendSession;

pub(crate) struct LlvmTargetMachine {
    machine: TargetMachine,
    triple: TargetTriple,
}

impl LlvmTargetMachine {
    #[cfg(test)]
    pub(crate) fn create(target: &CodegenTarget) -> Result<Self, CodegenFailure> {
        let session = LlvmBackendSession::try_new(target, BrayOptimizationLevel::None)?;

        Self::create_for_session(&session)
    }

    pub(crate) fn create_for_session(session: &LlvmBackendSession) -> Result<Self, CodegenFailure> {
        initialization::initialize();

        let target = session.target();

        let triple = TargetTriple::create(target.triple());

        let llvm_target =
            Target::from_triple(&triple).map_err(|_| CodegenFailure::UnsupportedTarget)?;

        let relocation = relocation_model(target.relocation_model());
        let code_model = code_model(target.code_model())?;

        let Some(machine) = llvm_target.create_target_machine(
            &triple,
            target.cpu(),
            session.features(),
            llvm_optimization_level(session.optimization()),
            relocation,
            code_model,
        ) else {
            return Err(CodegenFailure::UnsupportedTarget);
        };

        Ok(Self { machine, triple })
    }

    pub(crate) fn validate_contract(
        &self,
        target: &CodegenTarget,
        context: &Context,
    ) -> Result<(), CodegenFailure> {
        let data = self.machine.get_target_data();
        let machine = target.machine();

        if data.get_pointer_byte_size(None) * 8 != u32::from(machine.pointer_width_bits().get())
            || byte_ordering(data.get_byte_ordering()) != machine.endianness()
        {
            return Err(CodegenFailure::UnsupportedTarget);
        }

        if target
            .data_layout()
            .scalars()
            .iter()
            .any(|layout| !scalar_layout_matches(&data, context, *layout))
        {
            return Err(CodegenFailure::UnsupportedTarget);
        }

        Ok(())
    }

    pub(crate) fn configure_module(&self, module: &Module<'_>) {
        module.set_triple(&self.triple);
        module.set_data_layout(&self.machine.get_target_data().get_data_layout());
    }

    pub(crate) fn target_data(&self) -> inkwell::targets::TargetData {
        self.machine.get_target_data()
    }

    pub(crate) fn data_layout(&self) -> Result<Arc<str>, CodegenFailure> {
        self.machine
            .get_target_data()
            .get_data_layout()
            .as_str()
            .to_str()
            .map(Arc::from)
            .map_err(|_| CodegenFailure::BackendLibrary)
    }

    pub(crate) fn run_passes(
        &self,
        module: &Module<'_>,
        pipeline: &str,
    ) -> Result<(), CodegenFailure> {
        let options = inkwell::passes::PassBuilderOptions::create();

        module
            .run_passes(pipeline, &self.machine, options)
            .map_err(|_| CodegenFailure::BackendLibrary)
    }

    pub(crate) fn serialize(
        &self,
        module: &Module<'_>,
        file_type: FileType,
    ) -> Result<Vec<u8>, CodegenFailure> {
        self.machine
            .write_to_memory_buffer(module, file_type)
            .map(|buffer| buffer.as_slice().to_vec())
            .map_err(|_| CodegenFailure::BackendLibrary)
    }
}

const fn llvm_optimization_level(level: BrayOptimizationLevel) -> OptimizationLevel {
    match level {
        BrayOptimizationLevel::None => OptimizationLevel::None,
        BrayOptimizationLevel::Basic => OptimizationLevel::Less,
        BrayOptimizationLevel::Full => OptimizationLevel::Default,
    }
}

pub(crate) fn validate_target_configuration(target: &CodegenTarget) -> Result<(), CodegenFailure> {
    let triple: Triple = target
        .triple()
        .parse()
        .map_err(|_| CodegenFailure::UnsupportedTarget)?;

    let machine = target.machine();

    if architecture(triple.architecture) != Some(machine.architecture())
        || object_format(triple.binary_format) != Some(machine.object_format())
        || triple.pointer_width().map(|width| u16::from(width.bits()))
            != Ok(machine.pointer_width_bits().get())
        || triple.endianness().map(endianness) != Ok(machine.endianness())
    {
        return Err(CodegenFailure::UnsupportedTarget);
    }

    Ok(())
}

const fn architecture(architecture: Architecture) -> Option<TargetArchitecture> {
    match architecture {
        Architecture::X86_32(_) => Some(TargetArchitecture::X86),
        Architecture::X86_64 | Architecture::X86_64h => Some(TargetArchitecture::X86_64),
        Architecture::Arm(_) => Some(TargetArchitecture::Arm),
        Architecture::Aarch64(_) => Some(TargetArchitecture::Aarch64),
        Architecture::Riscv32(_) => Some(TargetArchitecture::Riscv32),
        Architecture::Riscv64(_) => Some(TargetArchitecture::Riscv64),
        Architecture::Powerpc64 | Architecture::Powerpc64le => Some(TargetArchitecture::PowerPc64),
        Architecture::Wasm32 => Some(TargetArchitecture::Wasm32),
        Architecture::Wasm64 => Some(TargetArchitecture::Wasm64),
        _ => None,
    }
}

const fn object_format(format: BinaryFormat) -> Option<ObjectFormat> {
    match format {
        BinaryFormat::Coff => Some(ObjectFormat::Coff),
        BinaryFormat::Elf => Some(ObjectFormat::Elf),
        BinaryFormat::Macho => Some(ObjectFormat::MachO),
        BinaryFormat::Wasm => Some(ObjectFormat::WebAssembly),
        BinaryFormat::Xcoff => Some(ObjectFormat::Xcoff),
        _ => None,
    }
}

const fn endianness(endianness: Endianness) -> BrayEndianness {
    match endianness {
        Endianness::Little => BrayEndianness::Little,
        Endianness::Big => BrayEndianness::Big,
    }
}

const fn byte_ordering(ordering: ByteOrdering) -> BrayEndianness {
    match ordering {
        ByteOrdering::LittleEndian => BrayEndianness::Little,
        ByteOrdering::BigEndian => BrayEndianness::Big,
    }
}

fn scalar_layout_matches(
    data: &inkwell::targets::TargetData,
    context: &Context,
    layout: TargetScalarLayout,
) -> bool {
    let ty: BasicTypeEnum<'_> = match layout.kind() {
        TargetScalarKind::Boolean => context.bool_type().into(),
        TargetScalarKind::Integer(width) => {
            let width = NonZeroU32::new(u32::from(width.get())).unwrap_or(NonZeroU32::MIN);

            let Ok(ty) = context.custom_width_int_type(width) else {
                return false;
            };

            ty.into()
        }
        TargetScalarKind::Float(width) => match width.get() {
            16 => context.f16_type().into(),
            32 => context.f32_type().into(),
            64 => context.f64_type().into(),
            128 => context.f128_type().into(),
            _ => return false,
        },
    };

    data.get_store_size(&ty) == u64::from(layout.size_bytes().get())
        && data.get_abi_alignment(&ty) == u32::from(layout.alignment_bytes().get())
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
