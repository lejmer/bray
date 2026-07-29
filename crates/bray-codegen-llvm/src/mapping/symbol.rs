use bray_codegen::{
    CodegenFailure, CodegenLinkage, CodegenMappings, CodegenSymbolMapping, CodegenTarget,
};
use inkwell::DLLStorageClass;
use inkwell::module::{Linkage, Module};
use inkwell::values::FunctionValue;

use super::LlvmTypeMappings;

pub(crate) fn declare_symbols<'context>(
    module: &Module<'context>,
    mappings: &CodegenMappings,
    target: &CodegenTarget,
    types: &mut LlvmTypeMappings<'context, '_>,
) -> Result<(), CodegenFailure> {
    for mapping in mappings.symbols() {
        let function_type = types.function_type(mapping.signature())?;
        let function = module.add_function(mapping.name().as_str(), function_type, None);

        apply_linkage(function, mapping, target)?;
        function.set_call_conventions(call_convention(mapping, target)?);
    }

    Ok(())
}

fn apply_linkage(
    function: FunctionValue<'_>,
    mapping: &CodegenSymbolMapping,
    target: &CodegenTarget,
) -> Result<(), CodegenFailure> {
    let linkage = match mapping.linkage() {
        CodegenLinkage::Private => Linkage::Private,
        CodegenLinkage::Internal => Linkage::Internal,
        CodegenLinkage::External | CodegenLinkage::Import | CodegenLinkage::Export => {
            Linkage::External
        }
        CodegenLinkage::Weak => Linkage::WeakODR,
        CodegenLinkage::LinkOnce => Linkage::LinkOnceODR,
        CodegenLinkage::Common => return Err(CodegenFailure::UnsupportedTarget),
    };

    function.set_linkage(linkage);

    if target.machine().object_format() == bray_target::ObjectFormat::Coff {
        match mapping.linkage() {
            CodegenLinkage::Import => function
                .as_global_value()
                .set_dll_storage_class(DLLStorageClass::Import),
            CodegenLinkage::Export => function
                .as_global_value()
                .set_dll_storage_class(DLLStorageClass::Export),
            CodegenLinkage::Private
            | CodegenLinkage::Internal
            | CodegenLinkage::External
            | CodegenLinkage::Weak
            | CodegenLinkage::LinkOnce
            | CodegenLinkage::Common => {}
        }
    }

    Ok(())
}

fn call_convention(
    mapping: &CodegenSymbolMapping,
    target: &CodegenTarget,
) -> Result<u32, CodegenFailure> {
    let Some(convention) = target.abi().convention(mapping.signature().abi()) else {
        return Err(CodegenFailure::UnsupportedTarget);
    };

    match convention.as_str() {
        "bray-x86_64" | "c" | "ccc" => Ok(0),
        "sysv64" => Ok(78),
        "win64" => Ok(79),
        _ => Err(CodegenFailure::UnsupportedTarget),
    }
}
