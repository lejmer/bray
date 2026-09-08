use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_ir::{MirCallableReference, MirStandardLibraryHelper};
use bray_lowering::SyntheticLoweringContext;
use bray_symbols::{
    AvailableCompilerKnownSymbols, CallableExecution, CallableInstanceData, CallableSignature,
    ConstantTermId, DeclaredTypeRepresentation, GenericSubstitutionId, NamedTypeSymbolId,
    SemanticValueStore, TypeAssociatedLifecycleSlot, TypeData, TypeExpressionTemplate, TypeId,
};

use crate::compilation::{CodegenPreparationError, Compilation};
use crate::fact::CancellationToken;

impl Compilation {
    pub(in crate::compilation::product) fn specialize_codegen_lifecycle_mir(
        &self,
        instance: &super::super::specialization::ConcreteCodegenInstance,
        mir: bray_ir::MirUnit,
        cancellation: &CancellationToken,
    ) -> Result<bray_ir::MirUnit, CodegenPreparationError> {
        let context = CompilationSyntheticLoweringContext::new(self, cancellation)?;

        bray_lowering::specialize_lifecycle_execution(&context, mir, |ty| {
            self.concrete_codegen_type(ty, instance.substitution(), Some(instance), cancellation)
        })
    }
}

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

    fn destructor_actions(
        &self,
        ty: TypeId,
    ) -> Result<Option<Vec<(TypeId, bray_checker::CleanupExecutionStep)>>, CodegenPreparationError>
    {
        if let TypeData::Named {
            definition,
            substitution,
        } = self
            .values
            .type_data(ty)
            .map_err(crate::fact::FactQueryError::SemanticValueStore)?
            .as_ref()
            && let Some(element) = self.compilation.raw_buffer_element(
                *definition,
                *substitution,
                self.cancellation,
            )?
        {
            return Ok(Some(vec![
                (element, bray_checker::CleanupExecutionStep::Finalization),
                (element, bray_checker::CleanupExecutionStep::Destruction),
            ]));
        }

        if !matches!(
            self.values
                .type_data(ty)
                .map_err(crate::fact::FactQueryError::SemanticValueStore)?
                .as_ref(),
            TypeData::Named { .. }
        ) {
            return Ok(None);
        }

        let Some((callable, _, _, _)) = self.compilation.lifecycle_callable(
            ty,
            TypeAssociatedLifecycleSlot::Destructor,
            self.cancellation,
        )?
        else {
            return Ok(None);
        };

        let Ok(target) = self.compilation.selected_target().target().codegen_target() else {
            return Ok(None);
        };

        let source = self.compilation.concrete_codegen_callable(
            callable.instance(),
            [],
            &target,
            self.cancellation,
        )?;

        if matches!(
            source.key().template(),
            bray_ir::MirUnitKey::ExternalCallable(_)
                | bray_ir::MirUnitKey::ExternalRuntimeDefault(_)
                | bray_ir::MirUnitKey::CompilerProvidedCallable(_)
        ) {
            return Ok(None);
        }

        let mir = self.compilation.codegen_mir_for_plan(
            source.key(),
            bray_ir::MirUnitId::new(0),
            None,
            self.cancellation,
        )?;

        let mut actions = std::collections::BTreeSet::new();

        for operation in mir.operations() {
            for helper in operation.kind().helper_references() {
                let (Some(role), Some(ty)) = (
                    bray_ir::MirGeneratedLifecycleRole::from_reference(&helper),
                    helper.lifecycle_type(),
                ) else {
                    continue;
                };

                let ty = self.compilation.concrete_codegen_type(
                    ty,
                    source.substitution(),
                    Some(&source),
                    self.cancellation,
                )?;

                actions.extend(cleanup_execution_steps(role).iter().map(|step| (ty, *step)));
            }
        }

        Ok(Some(actions.into_iter().collect()))
    }
}

fn cleanup_execution_steps(
    role: bray_ir::MirGeneratedLifecycleRole,
) -> &'static [bray_checker::CleanupExecutionStep] {
    use bray_checker::CleanupExecutionStep as Step;
    use bray_ir::{MirAbandonmentAction, MirCleanupPhase, MirGeneratedLifecycleRole};

    match role {
        MirGeneratedLifecycleRole::Finalize => &[Step::Finalization],
        MirGeneratedLifecycleRole::Destroy => &[Step::Destruction],
        MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::LifecycleResolution) => {
            &[Step::Finalization, Step::Destruction]
        }
        MirGeneratedLifecycleRole::Abandon(MirAbandonmentAction::Quiesce) => &[Step::Quiescence],
        MirGeneratedLifecycleRole::StaticFinalize
        | MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::TaskCancellation)
        | MirGeneratedLifecycleRole::Abandon(
            MirAbandonmentAction::Destroy | MirAbandonmentAction::Destructor,
        ) => &[],
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

    fn cleanup_type_execution(
        &self,
        ty: TypeId,
    ) -> Result<bray_bound_tree::StorageCleanupType, Self::Error> {
        let context = crate::compilation::checker::CompilationCheckerContext::new(
            self.compilation.binding_context(self.cancellation)?,
        );

        let checked = bray_checker::cleanup_type_execution_with(
            &context,
            ty,
            |ty| self.destructor_actions(ty),
            |ty| self.finalization_complete(ty),
        )?;

        let (cleanup, diagnostics) = checked.into_parts();

        if diagnostics.has_errors() {
            return Err(CodegenPreparationError::Diagnostics(diagnostics));
        }

        Ok(cleanup)
    }

    fn finalization_complete(&self, ty: TypeId) -> Result<bool, Self::Error> {
        let checked = self
            .compilation
            .type_finalizer_completion(ty, self.cancellation)?;

        let (complete, diagnostics) = checked.into_parts();

        if diagnostics.has_errors() {
            return Err(CodegenPreparationError::Diagnostics(diagnostics));
        }

        Ok(complete)
    }

    fn lifecycle_callable(
        &self,
        ty: TypeId,
        slot: TypeAssociatedLifecycleSlot,
    ) -> Result<Option<(MirCallableReference, TypeId, TypeId, CallableExecution)>, Self::Error>
    {
        self.compilation
            .lifecycle_callable(ty, slot, self.cancellation)
    }

    fn storage_callable(
        &self,
        storage: TypeId,
        target: TypeId,
        member: &CompilerKnownDeclarationKey,
    ) -> Result<(MirCallableReference, CallableSignature), Self::Error> {
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

        // Lowering retains the immutable Arc-backed representation after releasing the query result.
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
