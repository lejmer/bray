use bray_compiler_known::RepresentationRole;
use bray_ir::MirStandardLibraryHelper;
use bray_symbols::{
    AvailableCompilerKnownSymbols, CallableInstanceData, ConstantTermId, NamedTypeSymbolId,
    SemanticValueStore, TypeId,
};

use super::SyntheticLoweringError;

/// Demanded semantic inputs for compiler-generated bodies. Implementations resolve queries only.
pub trait SyntheticLoweringContext {
    /// Query failures retain their original cause alongside synthetic lowering failures.
    type Error: From<SyntheticLoweringError>;

    /// Returns the semantic values shared with the selected compilation.
    fn semantic_values(&self) -> &SemanticValueStore;

    /// Returns exact compiler-known identities and representation contracts.
    fn compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols;

    /// Returns the checker-selected local lifecycle action after substitution.
    fn lifecycle_action(
        &self,
        ty: TypeId,
        phase: bray_bound_tree::LifecyclePhase,
    ) -> Result<bray_bound_tree::LifecycleAction, Self::Error>;

    /// Returns the exact representation role of a source or imported type.
    fn representation_role(&self, definition: NamedTypeSymbolId) -> Option<RepresentationRole>;

    /// Resolves the semantic type assigned to a compiler-known representation role.
    fn representation_type(&self, role: RepresentationRole) -> Result<TypeId, Self::Error>;

    /// Resolves a checked array extent before constructing element projections.
    fn array_length(&self, length: ConstantTermId) -> Result<u64, Self::Error>;

    /// Resolves one recognized standard-library helper under its existing authority checks.
    fn standard_library_callable(
        &self,
        helper: MirStandardLibraryHelper,
    ) -> Result<CallableInstanceData, Self::Error>;
}

pub(crate) struct SyntheticLowerer<'context, C: SyntheticLoweringContext + ?Sized> {
    pub(crate) context: &'context C,
}
