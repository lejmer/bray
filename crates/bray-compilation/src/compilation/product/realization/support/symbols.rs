use bray_codegen::{CodegenInstance, CodegenLinkage, CodegenSymbolKey};
use bray_ir::{MirHelperReference, MirRuntimeReference};
use bray_runtime_interface::{BinarySymbolName, RuntimeAbiRole};
use bray_symbols::{ForeignCallableDirection, NativeSymbolBinding, SymbolKey, SymbolKeyData};

use crate::compilation::CodegenPreparationError;
use crate::compilation::product::realization::symbols::NativeBoundaryMapping;

pub(in crate::compilation::product::realization) fn source_backed_symbol_key(
    key: &SymbolKey,
) -> bool {
    match key.data() {
        SymbolKeyData::SourceDeclaration { .. } => true,
        SymbolKeyData::Synthesized(synthesized) => source_backed_symbol_key(synthesized.subject()),
        SymbolKeyData::Root(_)
        | SymbolKeyData::Module { .. }
        | SymbolKeyData::CompilerKnownDeclaration { .. }
        | SymbolKeyData::External(_) => false,
    }
}

pub(in crate::compilation::product::realization) fn native_boundary_mapping(
    symbol: &str,
    direction: ForeignCallableDirection,
    binding: NativeSymbolBinding,
) -> Result<NativeBoundaryMapping, CodegenPreparationError> {
    let name =
        BinarySymbolName::try_new(symbol).ok_or(CodegenPreparationError::InvalidSymbolName)?;

    let linkage = match (direction, binding) {
        (ForeignCallableDirection::Import, NativeSymbolBinding::Strong) => CodegenLinkage::Import,
        (ForeignCallableDirection::Export, NativeSymbolBinding::Strong) => CodegenLinkage::Export,
        (ForeignCallableDirection::Export, NativeSymbolBinding::Weak) => CodegenLinkage::Weak,
        (ForeignCallableDirection::Import, NativeSymbolBinding::Weak) => {
            panic!("checked native import {symbol:?} cannot have weak binding");
        }
    };

    Ok(match direction {
        ForeignCallableDirection::Import => NativeBoundaryMapping::Direct { name, linkage },
        ForeignCallableDirection::Export => NativeBoundaryMapping::Callback {
            name: Some(name),
            linkage,
            kind: bray_runtime_interface::NativeEntryKind::ForeignCallback,
        },
    })
}

pub(in crate::compilation::product::realization) fn helper_runtime_symbol(
    owner: &CodegenInstance,
    role: RuntimeAbiRole,
) -> CodegenSymbolKey {
    CodegenSymbolKey::Runtime(MirRuntimeReference::new(
        role,
        owner.key().target().runtime_abi(),
    ))
}

pub(in crate::compilation::product::realization) fn direct_helper_symbol(
    owner: &CodegenInstance,
    reference: &MirHelperReference,
) -> Option<CodegenSymbolKey> {
    reference
        .runtime_role()
        .map(|role| helper_runtime_symbol(owner, role))
}

pub(in crate::compilation::product::realization) fn dependency_symbol(
    owner: &CodegenInstance,
    instance: &bray_codegen::CodegenInstanceKey,
    reference: &MirHelperReference,
) -> Result<CodegenSymbolKey, CodegenPreparationError> {
    owner
        .dependencies()
        .iter()
        .find(|dependency| dependency.instance() == instance)
        .map(|dependency| CodegenSymbolKey::Instance(dependency.instance().clone()))
        .ok_or_else(|| CodegenPreparationError::MissingHelperInstance(reference.clone()))
}
