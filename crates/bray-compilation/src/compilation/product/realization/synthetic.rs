use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_ir::MirStandardLibraryHelper;
use bray_lowering::SyntheticLoweringContext;
use bray_symbols::{
    AvailableCompilerKnownSymbols, CallableInstanceData, ConstantTermId,
    DeclaredTypeRepresentation, GenericSubstitutionId, NamedTypeSymbolId, SemanticValueStore,
    TypeAssociatedLifecycleSlot, TypeExpressionTemplate, TypeId,
};

use crate::compilation::{CodegenPreparationError, Compilation};
use crate::fact::CancellationToken;

pub(super) struct CompilationSyntheticLoweringContext<'compilation> {
    compilation: &'compilation Compilation,
    cancellation: &'compilation CancellationToken,
    values: &'compilation SemanticValueStore,
}

impl<'compilation> CompilationSyntheticLoweringContext<'compilation> {
    pub(super) fn new(
        compilation: &'compilation Compilation,
        cancellation: &'compilation CancellationToken,
    ) -> Result<Self, CodegenPreparationError> {
        Ok(Self {
            compilation,
            cancellation,
            values: compilation.semantic_value_store()?,
        })
    }
}

impl SyntheticLoweringContext for CompilationSyntheticLoweringContext<'_> {
    type Error = CodegenPreparationError;

    fn semantic_values(&self) -> &SemanticValueStore {
        self.values
    }

    fn compiler_known_symbols(&self) -> &AvailableCompilerKnownSymbols {
        self.compilation.available_compiler_known_symbols()
    }

    fn lifecycle_action(
        &self,
        ty: TypeId,
        phase: bray_bound_tree::LifecyclePhase,
    ) -> Result<bray_bound_tree::LifecycleAction, Self::Error> {
        bray_checker::select_lifecycle_action(self, ty, phase)
    }

    fn representation_role(&self, definition: NamedTypeSymbolId) -> Option<RepresentationRole> {
        crate::compilation::foreign::compiler_known_representation(self.compilation, definition)
    }

    fn representation_type(&self, role: RepresentationRole) -> Result<TypeId, Self::Error> {
        Ok(self.compilation.codegen_representation_type(role)?)
    }

    fn array_length(&self, length: ConstantTermId) -> Result<u64, Self::Error> {
        super::support::closed_array_length(self.values, length)
    }

    fn standard_library_callable(
        &self,
        helper: MirStandardLibraryHelper,
    ) -> Result<CallableInstanceData, Self::Error> {
        self.compilation
            .standard_library_helper_callable(helper, self.cancellation)
    }
}

impl bray_checker::LifecycleSelectionContext for CompilationSyntheticLoweringContext<'_> {
    type Error = CodegenPreparationError;

    fn semantic_values(&self) -> &SemanticValueStore {
        self.values
    }

    fn lifecycle_callable(
        &self,
        ty: TypeId,
        slot: TypeAssociatedLifecycleSlot,
    ) -> Result<Option<bray_bound_tree::LifecycleCallable>, Self::Error> {
        self.compilation
            .lifecycle_callable(ty, slot, self.cancellation)
    }

    fn storage_callable(
        &self,
        storage: TypeId,
        target: TypeId,
        member: &CompilerKnownDeclarationKey,
    ) -> Result<bray_bound_tree::LifecycleCallable, Self::Error> {
        let (callable, signature) = self.compilation.storage_lifecycle_callable(
            storage,
            target,
            member,
            self.cancellation,
        )?;

        let [parameter] = signature.parameters() else {
            return Err(crate::compilation::ProductQueryFailure::count_mismatch(
                // The failure owns the interned protocol identity after this query returns.
                crate::compilation::ProductQueryContext::CompilerKnownDeclaration(member.clone()),
                crate::compilation::ProductDataKind::CallableParameters,
                1,
                signature.parameters().len(),
            )
            .into());
        };

        Ok(bray_bound_tree::LifecycleCallable {
            callable: callable.instance(),
            abi: callable.abi(),
            receiver: parameter.ty(),
            result: signature.result(),
            execution: bray_symbols::CallableExecution::Synchronous,
        })
    }

    fn declared_representation(
        &self,
        definition: NamedTypeSymbolId,
    ) -> Result<DeclaredTypeRepresentation, Self::Error> {
        let representation = self
            .compilation
            .declared_type_representation_with_cancellation(definition, self.cancellation)?;

        // Selection retains the immutable Arc-backed representation after releasing the query result.
        Ok(representation.value().clone())
    }

    fn resolve_type(
        &self,
        template: &TypeExpressionTemplate,
        substitution: GenericSubstitutionId,
    ) -> Result<TypeId, Self::Error> {
        Ok(self
            .compilation
            .resolve_codegen_type(template, substitution, self.cancellation)?)
    }

    fn imported_raw_buffer_element(
        &self,
        definition: NamedTypeSymbolId,
        substitution: GenericSubstitutionId,
    ) -> Result<Option<TypeId>, Self::Error> {
        self.compilation
            .imported_raw_buffer_element(definition, substitution, self.cancellation)
    }

    fn representation_role(&self, definition: NamedTypeSymbolId) -> Option<RepresentationRole> {
        crate::compilation::foreign::compiler_known_representation(self.compilation, definition)
    }
}
