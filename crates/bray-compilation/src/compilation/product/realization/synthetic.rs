use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_ir::{MirHelperReference, MirStandardLibraryHelper};
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
    realization: Option<&'compilation super::super::specialization::ConcreteCodegenInstance>,
}

impl<'compilation> CompilationSyntheticLoweringContext<'compilation> {
    pub(super) fn new(
        compilation: &'compilation Compilation,
        realization: Option<&'compilation super::super::specialization::ConcreteCodegenInstance>,
        cancellation: &'compilation CancellationToken,
    ) -> Result<Self, CodegenPreparationError> {
        Ok(Self {
            compilation,
            realization,
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
        let ty = match self.realization {
            Some(realization) => self.compilation.concrete_codegen_type(
                ty,
                realization.substitution(),
                Some(realization),
                self.cancellation,
            )?,
            None => ty,
        };

        if self
            .compilation
            .codegen_lifecycle_phase_is_trivial(ty, phase, self.cancellation)?
        {
            return Ok(bray_bound_tree::LifecycleAction::None);
        }

        let action = bray_checker::select_lifecycle_action(self, ty, phase)?;

        if phase == bray_bound_tree::LifecyclePhase::Finalize
            && let bray_bound_tree::LifecycleAction::Call(callable) = action
            && self.compilation.finalizer_is_complete(
                callable.callable,
                callable.result.completion_type(),
                None,
                self.cancellation,
            )?
        {
            return Ok(bray_bound_tree::LifecycleAction::None);
        }

        Ok(action)
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
        self.compilation
            .storage_lifecycle_callable(storage, target, member, self.cancellation)
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

    fn raw_buffer_element(
        &self,
        definition: NamedTypeSymbolId,
        substitution: GenericSubstitutionId,
    ) -> Result<Option<TypeId>, Self::Error> {
        self.compilation
            .raw_buffer_element(definition, substitution, self.cancellation)
            .map_err(Into::into)
    }

    fn representation_role(&self, definition: NamedTypeSymbolId) -> Option<RepresentationRole> {
        crate::compilation::foreign::compiler_known_representation(self.compilation, definition)
    }
}

impl Compilation {
    pub(super) fn codegen_lifecycle_action(
        &self,
        reference: &MirHelperReference,
        cancellation: &CancellationToken,
    ) -> Result<bray_bound_tree::LifecycleAction, CodegenPreparationError> {
        let role = bray_ir::MirGeneratedLifecycleRole::from_reference(reference)
            .expect("selected lifecycle helper retains its generated role");

        let ty = reference
            .lifecycle_type()
            .expect("selected lifecycle helper retains its concrete owner type");

        CompilationSyntheticLoweringContext::new(self, None, cancellation)?
            .lifecycle_action(ty, role.phase())
    }
}

impl Compilation {
    pub(in crate::compilation::product) fn expand_lifecycle_mir(
        &self,
        realization: &super::super::specialization::ConcreteCodegenInstance,
        unit: &bray_ir::MirUnit,
        cancellation: &CancellationToken,
    ) -> Result<Option<bray_ir::MirUnit>, CodegenPreparationError> {
        let context =
            CompilationSyntheticLoweringContext::new(self, Some(realization), cancellation)?;

        bray_lowering::expand_lifecycle_continuations(&context, unit)
    }
}
