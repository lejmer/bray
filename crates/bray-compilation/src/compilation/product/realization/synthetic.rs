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
    pub(super) compilation: &'compilation Compilation,
    pub(super) cancellation: &'compilation CancellationToken,
    pub(super) values: &'compilation SemanticValueStore,
    pub(super) realization:
        Option<&'compilation super::super::specialization::ConcreteCodegenInstance>,
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
            && let bray_bound_tree::LifecycleAction::Call(ref callable) = action
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

    fn callable_completion_reports(
        &self,
        callable: CallableInstanceData,
    ) -> Result<bool, Self::Error> {
        let binding = self.compilation.binding_context(self.cancellation)?;

        let summary = bray_binder::SymbolQueryProvider::resolve_symbol_query(
            &binding,
            bray_symbols::SymbolQueryRequest::<bray_symbols::CallableResultDependenciesQuery>::new(
                callable.definition().callable_symbol(),
            ),
        )
        .map_err(crate::compilation::binder::binding_query_error)?;

        Ok(self
            .values
            .dependency_contract_template_data(*summary.value())
            .completion_reports()
            .is_some())
    }

    fn lifecycle_requires_frame(
        &self,
        ty: TypeId,
        role: bray_ir::MirGeneratedLifecycleRole,
    ) -> Result<bool, Self::Error> {
        let ty = match self.realization {
            Some(realization) => self.compilation.concrete_codegen_type(
                ty,
                realization.substitution(),
                Some(realization),
                self.cancellation,
            )?,
            None => ty,
        };

        self.compilation
            .lifecycle_requires_frame(ty, role, self.cancellation)
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

    fn destruction_body(
        &self,
        callable: CallableInstanceData,
    ) -> Result<std::sync::Arc<bray_lowering::DestructionBody>, Self::Error> {
        let target = self
            .compilation
            .selected_target()
            .target()
            .codegen_target()
            .expect("selected cleanup target supports native realization");

        let instance = match self.realization {
            Some(owner) => self.compilation.concrete_codegen_callable_data(
                owner,
                &callable,
                &target,
                self.cancellation,
            )?,
            None => self.compilation.concrete_codegen_callable(
                callable,
                [],
                &target,
                self.cancellation,
            )?,
        };

        let key = instance.key().clone();

        let cell = self
            .compilation
            .state
            .destruction_bodies
            .cell(key.clone())?;

        let result = cell.get_or_compute_requested(
            &self.compilation.state.fact_runtime,
            crate::fact::CompilationFactKey::DestructionBody(key),
            self.cancellation,
            |cancellation| {
                let prepared = (|| {
                    let unit = self.compilation.codegen_mir_for_plan(
                        instance.key(),
                        bray_ir::MirUnitId::new(0),
                        None,
                        cancellation,
                    )?;

                    let types = unit
                        .referenced_types()
                        .into_iter()
                        .map(|ty| {
                            self.compilation
                                .concrete_codegen_type(
                                    ty,
                                    instance.substitution(),
                                    Some(&instance),
                                    cancellation,
                                )
                                .map(|concrete| (ty, concrete))
                        })
                        .collect::<Result<_, _>>()?;

                    let binding = self.compilation.binding_context(cancellation)?;

                    let contextual_self =
                        super::codegen_instance_contextual_self(&binding, &instance)?;

                    let callables = bray_codegen::demanded_callable_instances_for_mir(&unit)
                        .into_iter()
                        .map(|demand| {
                            let reference = demand.reference();

                            super::substitute_callable_instance(
                                self.values,
                                reference.instance(),
                                instance.substitution(),
                                contextual_self,
                            )
                            .map(|callable| {
                                (
                                    reference,
                                    bray_ir::MirCallableReference::new(callable, reference.abi()),
                                )
                            })
                        })
                        .collect::<Result<_, _>>()?;

                    Ok::<_, CodegenPreparationError>(bray_lowering::DestructionBody {
                        unit,
                        types,
                        callables,
                    })
                })();

                Ok(prepared.map(std::sync::Arc::new))
            },
        )?;

        result.as_ref().cloned().map_err(Clone::clone)
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
