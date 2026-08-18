use bray_codegen::CodegenStaticInstanceKey;
use bray_symbols::{StaticReferenceSelection, TypeId};

pub(in crate::compilation::product) struct ProductStaticHostEntry {
    key: CodegenStaticInstanceKey,
    reference: StaticReferenceSelection,
    ty: TypeId,
    dependencies: Vec<CodegenStaticInstanceKey>,
    transfers_cleanup_incident: bool,
}

impl ProductStaticHostEntry {
    pub(super) fn new(
        key: CodegenStaticInstanceKey,
        reference: StaticReferenceSelection,
        ty: TypeId,
        dependencies: Vec<CodegenStaticInstanceKey>,
        transfers_cleanup_incident: bool,
    ) -> Self {
        Self {
            key,
            reference,
            ty,
            dependencies,
            transfers_cleanup_incident,
        }
    }

    pub(in crate::compilation::product) const fn key(&self) -> &CodegenStaticInstanceKey {
        &self.key
    }

    pub(in crate::compilation::product) fn dependencies(&self) -> &[CodegenStaticInstanceKey] {
        &self.dependencies
    }

    pub(in crate::compilation::product) const fn transfers_cleanup_incident(&self) -> bool {
        self.transfers_cleanup_incident
    }

    pub(in crate::compilation::product) fn lowering_entry(
        &self,
    ) -> bray_lowering::ExecutableHostStatic {
        // Lowered host MIR owns the Arc-backed static reference after planning returns.
        bray_lowering::ExecutableHostStatic::new(self.reference.clone(), self.ty)
    }
}
