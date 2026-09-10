use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_ir::{MirCallableReference, MirStandardLibraryHelper};
use bray_symbols::{
    AvailableCompilerKnownSymbols, CallableExecution, CallableInstanceData, CallableSignature,
    ConstantTermId, DeclaredTypeRepresentation, GenericSubstitutionId, NamedTypeSymbolId,
    SemanticValueStore, TypeAssociatedLifecycleSlot, TypeExpressionTemplate, TypeId,
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

    /// Resolves checked finalization and represented destruction execution for a closed type.
    fn cleanup_type_execution(
        &self,
        ty: TypeId,
    ) -> Result<bray_bound_tree::StorageCleanupType, Self::Error>;

    /// Returns whether certified contracts prove no-work whole-value completion for every receiver.
    /// Receiver acquisition, destruction, represented owners, and release retain their obligations.
    fn finalization_complete(&self, ty: TypeId) -> Result<bool, Self::Error>;

    /// Resolves the callable, receiver argument type, completion type, and execution mode.
    fn lifecycle_callable(
        &self,
        ty: TypeId,
        slot: TypeAssociatedLifecycleSlot,
    ) -> Result<Option<(MirCallableReference, TypeId, TypeId, CallableExecution)>, Self::Error>;

    /// Resolves one exact storage-policy method and its closed signature.
    fn storage_callable(
        &self,
        storage: TypeId,
        target: TypeId,
        member: &CompilerKnownDeclarationKey,
    ) -> Result<(MirCallableReference, CallableSignature), Self::Error>;

    /// Resolves the checked stored representation without constructing MIR projections.
    fn declared_representation(
        &self,
        definition: NamedTypeSymbolId,
    ) -> Result<DeclaredTypeRepresentation, Self::Error>;

    /// Resolves a member's type in the enclosing declaration specialization.
    fn resolve_type(
        &self,
        template: &TypeExpressionTemplate,
        substitution: GenericSubstitutionId,
    ) -> Result<TypeId, Self::Error>;

    /// Resolves the element of the recognized standard-library raw buffer, if applicable.
    fn raw_buffer_element(
        &self,
        definition: NamedTypeSymbolId,
        substitution: GenericSubstitutionId,
    ) -> Result<Option<TypeId>, Self::Error>;

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

impl<C: SyntheticLoweringContext + ?Sized> SyntheticLowerer<'_, C> {
    pub(crate) fn run_result(
        &self,
        completion: TypeId,
    ) -> Result<(TypeId, bray_ir::MirRunResultVariants), C::Error> {
        let symbols = self.context.compiler_known_symbols();

        let result = symbols
            .unary_representation_type(
                self.context.semantic_values(),
                RepresentationRole::RunResult,
                completion,
            )
            .map_err(SyntheticLoweringError::SemanticValue)?
            .ok_or(SyntheticLoweringError::MissingRepresentation {
                role: RepresentationRole::RunResult,
                argument: Some(completion),
            })?;

        let representation = symbols.run_result_representation().ok_or(
            SyntheticLoweringError::MissingRepresentation {
                role: RepresentationRole::RunResult,
                argument: None,
            },
        )?;

        let variants = bray_ir::MirRunResultVariants::new(
            representation.completed_variant(),
            representation.panicked_variant(),
            representation.cancelled_variant(),
        );

        Ok((result, variants))
    }
}
