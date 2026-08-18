use std::fmt::Write;
use std::hash::{Hash, Hasher};

use bray_base::StableDigestHasher;
use bray_codegen::{CodegenLinkage, CodegenTarget};
use bray_ir::MirUnitKey;
use bray_runtime_interface::{BinarySymbolName, ProtectedFrameOperation};

use super::super::super::CodegenPreparationError;

pub(in crate::compilation) fn generated_symbol_name(
    target: &CodegenTarget,
    linkage: CodegenLinkage,
    category: &str,
    identity: &impl Hash,
) -> Result<BinarySymbolName, CodegenPreparationError> {
    binary_symbol_name(
        target,
        linkage,
        category,
        generated_identity(category, identity),
    )
}

pub(in crate::compilation) fn generated_identity(category: &str, identity: &impl Hash) -> [u8; 32] {
    let mut hasher = StableDigestHasher::new();

    hasher.write(b"bray.codegen-symbol");
    category.hash(&mut hasher);
    identity.hash(&mut hasher);

    hasher.finalize()
}

pub(super) fn generated_instance_symbol_name(
    target: &CodegenTarget,
    linkage: CodegenLinkage,
    instance: &bray_codegen::CodegenInstanceKey,
) -> Result<BinarySymbolName, CodegenPreparationError> {
    let category = if matches!(instance.template(), MirUnitKey::GeneratedLifecycle(_)) {
        "lifecycle"
    } else {
        "instance"
    };

    generated_symbol_name(target, linkage, category, instance)
}

pub(in crate::compilation) fn generated_frame_symbol_name(
    target: &CodegenTarget,
    frame: bray_runtime_interface::ProtectedAsyncFrameId,
    operation: ProtectedFrameOperation,
) -> Result<BinarySymbolName, CodegenPreparationError> {
    let mut hasher = StableDigestHasher::new();

    hasher.write(b"bray.protected-frame-symbol");
    frame.hash(&mut hasher);
    operation.hash(&mut hasher);

    binary_symbol_name(
        target,
        CodegenLinkage::Internal,
        operation.as_str(),
        hasher.finalize(),
    )
}

pub(super) fn binary_symbol_name(
    target: &CodegenTarget,
    linkage: CodegenLinkage,
    category: &str,
    digest: [u8; 32],
) -> Result<BinarySymbolName, CodegenPreparationError> {
    let prefix = if linkage == CodegenLinkage::Private {
        target.symbols().private_prefix()
    } else {
        target.symbols().global_prefix()
    };

    let mut name = format!("{prefix}bray_{category}_");

    for byte in digest {
        let _ = write!(name, "{byte:02x}");
    }

    BinarySymbolName::try_new(name).ok_or(CodegenPreparationError::InvalidSymbolName)
}
