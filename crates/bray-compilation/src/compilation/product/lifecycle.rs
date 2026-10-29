use std::collections::{BTreeMap, BTreeSet};

use bray_compiler_known::RepresentationRole;
use bray_ir::{MirCleanupPhase, MirGeneratedLifecycleRole, MirHelperReference};
use bray_symbols::{
    DeclaredStorageShape, GenericSubstitutionId, NamedTypeSymbolId, TypeAssociatedLifecycleSlot,
    TypeData, TypeId,
};

use super::super::{CodegenPreparationError, Compilation};
use super::ProductSynchronizationComponent;
use crate::fact::{CancellationToken, FactQueryError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::compilation) struct CodegenLifecycleNeeds(u8);

impl CodegenLifecycleNeeds {
    const NONE: Self = Self(0);
    const FINALIZE: Self = Self(1 << 0);
    const DESTROY: Self = Self(1 << 1);
    const TASK_CANCELLATION: Self = Self(1 << 2);
    const LIFECYCLE_RESOLUTION: Self = Self(1 << 3);
    const QUIESCE: Self = Self(1 << 4);
    const SUSPEND: Self = Self(1 << 5);
    const ALL: Self = Self(
        Self::FINALIZE.0
            | Self::DESTROY.0
            | Self::TASK_CANCELLATION.0
            | Self::LIFECYCLE_RESOLUTION.0
            | Self::QUIESCE.0
            | Self::SUSPEND.0,
    );

    const fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    const fn requires(self, phase: bray_bound_tree::LifecyclePhase) -> bool {
        let required = match phase {
            bray_bound_tree::LifecyclePhase::Finalize => Self::FINALIZE,
            bray_bound_tree::LifecyclePhase::Destroy => Self::DESTROY,
            bray_bound_tree::LifecyclePhase::Quiesce => Self::QUIESCE,
            bray_bound_tree::LifecyclePhase::Cancel => Self::TASK_CANCELLATION,
            bray_bound_tree::LifecyclePhase::Resolve => Self::LIFECYCLE_RESOLUTION,
        };

        self.contains(required)
    }

    const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    const fn represented(children: Self) -> Self {
        let mut represented = Self::NONE;

        if children.contains(Self::SUSPEND) {
            represented = represented.with(Self::SUSPEND);
        }

        if children.contains(Self::FINALIZE) || children.contains(Self::DESTROY) {
            represented = represented.with(Self::DESTROY);
        }

        if children.contains(Self::TASK_CANCELLATION) {
            represented = represented.with(Self::TASK_CANCELLATION);
        }

        if children.contains(Self::QUIESCE) {
            represented = represented.with(Self::QUIESCE);
        }

        if represented.contains(Self::DESTROY) {
            represented = represented.with(Self::LIFECYCLE_RESOLUTION);
        }

        represented
    }
}

// Share suspension needs through represented dependencies before facts are cached.
// The existing lifecycle traversal discovers these edges without selecting types again.
#[derive(Default)]
struct LifecycleNeedsGraph {
    active: Vec<TypeId>,
    computed: BTreeMap<TypeId, CodegenLifecycleNeeds>,
    dependents: BTreeMap<TypeId, BTreeSet<TypeId>>,
}

impl LifecycleNeedsGraph {
    fn close(self) -> BTreeMap<TypeId, CodegenLifecycleNeeds> {
        let suspending = bray_base::transitive_dependencies(
            self.computed.iter().filter_map(|(ty, needs)| {
                needs
                    .contains(CodegenLifecycleNeeds::SUSPEND)
                    .then_some(*ty)
            }),
            |ty| self.dependents.get(ty).into_iter().flatten().copied(),
        );

        self.computed
            .into_iter()
            .map(|(ty, needs)| {
                (
                    ty,
                    if suspending.contains(&ty) {
                        needs.with(CodegenLifecycleNeeds::SUSPEND)
                    } else {
                        needs
                    },
                )
            })
            .collect()
    }
}

impl Compilation {
    pub(in crate::compilation) fn lifecycle_requires_frame(
        &self,
        ty: TypeId,
        role: MirGeneratedLifecycleRole,
        cancellation: &CancellationToken,
    ) -> Result<bool, CodegenPreparationError> {
        if !matches!(
            role,
            MirGeneratedLifecycleRole::Quiesce
                | MirGeneratedLifecycleRole::ReleaseBuffer
                | MirGeneratedLifecycleRole::Destroy(
                    bray_ir::MirDestructionDisposition::Ordinary
                        | bray_ir::MirDestructionDisposition::Dynamic
                )
        ) {
            return Ok(false);
        }

        let needs = self.codegen_lifecycle_needs(ty, cancellation)?;

        Ok(needs.requires(role.phase()) && needs.contains(CodegenLifecycleNeeds::SUSPEND))
    }
    pub(super) fn codegen_cleanup_is_trivial(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<bool, CodegenPreparationError> {
        self.codegen_lifecycle_is_trivial(
            &MirHelperReference::Cleanup {
                phase: MirCleanupPhase::LifecycleResolution,
                ty,
            },
            cancellation,
        )
    }

    pub(super) fn codegen_lifecycle_is_trivial(
        &self,
        reference: &MirHelperReference,
        cancellation: &CancellationToken,
    ) -> Result<bool, CodegenPreparationError> {
        let role = MirGeneratedLifecycleRole::from_reference(reference).unwrap_or_else(|| {
            panic!("lifecycle query requires a lifecycle helper, got {reference:?}")
        });

        let ty = reference
            .lifecycle_type()
            .expect("selected lifecycle helper retains its owner type");

        self.codegen_lifecycle_phase_is_trivial(ty, role.phase(), cancellation)
    }

    pub(super) fn codegen_lifecycle_phase_is_trivial(
        &self,
        ty: TypeId,
        phase: bray_bound_tree::LifecyclePhase,
        cancellation: &CancellationToken,
    ) -> Result<bool, CodegenPreparationError> {
        Ok(!self
            .codegen_lifecycle_needs(ty, cancellation)?
            .requires(phase))
    }

    fn codegen_lifecycle_needs(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<CodegenLifecycleNeeds, CodegenPreparationError> {
        if let Some(needs) = self.cached_lifecycle_needs(ty)? {
            return Ok(needs);
        }

        let mut graph = LifecycleNeedsGraph::default();

        let root = self.compute_codegen_lifecycle_needs(ty, cancellation, &mut graph)?;

        // A concurrent query may already have published this root's complete fact.
        graph.computed.insert(ty, root);

        let computed = graph.close();
        let needs = computed[&ty];

        self.state
            .codegen_lifecycle_needs
            .lock()
            .unwrap_or_else(|_| {
                panic!(
                    "product SynchronizationPoisoned construction failed: component={:?}",
                    ProductSynchronizationComponent::LifecycleNeeds
                )
            })
            .extend(computed);

        Ok(needs)
    }

    fn compute_codegen_lifecycle_needs(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
        graph: &mut LifecycleNeedsGraph,
    ) -> Result<CodegenLifecycleNeeds, CodegenPreparationError> {
        cancellation.check()?;

        if let Some(parent) = graph.active.last() {
            graph.dependents.entry(ty).or_default().insert(*parent);
        }

        if let Some(needs) = graph
            .computed
            .get(&ty)
            .copied()
            .or(self.cached_lifecycle_needs(ty)?)
        {
            return Ok(needs);
        }

        if graph.active.contains(&ty) {
            // Preserve the existing conservative structural phases. Suspension is
            // propagated from selected work across the cycle before publishing facts.
            return Ok(CodegenLifecycleNeeds(
                CodegenLifecycleNeeds::ALL.0 & !CodegenLifecycleNeeds::SUSPEND.0,
            ));
        }

        graph.active.push(ty);

        let values = self.semantic_value_store()?;

        let data = values.type_data(ty);

        let needs = match data.as_ref() {
            TypeData::ExactInteger { .. } => CodegenLifecycleNeeds::NONE,
            TypeData::Named {
                definition,
                substitution,
            } => {
                self.named_codegen_lifecycle_needs(*definition, *substitution, cancellation, graph)?
            }
            TypeData::Tuple(elements) => {
                let children = self.children_codegen_lifecycle_needs(
                    elements.iter().copied(),
                    cancellation,
                    graph,
                )?;

                CodegenLifecycleNeeds::represented(children)
            }
            TypeData::Array { element, length } => {
                if super::realization::closed_array_length(values, *length)? == 0 {
                    CodegenLifecycleNeeds::NONE
                } else {
                    let child =
                        self.compute_codegen_lifecycle_needs(*element, cancellation, graph)?;

                    CodegenLifecycleNeeds::represented(child)
                }
            }
            TypeData::Nullable(target) => {
                let child = self.compute_codegen_lifecycle_needs(*target, cancellation, graph)?;

                CodegenLifecycleNeeds::represented(child)
            }
            TypeData::OwnedIndirection { target, .. } | TypeData::Generator(target) => {
                let children =
                    self.compute_codegen_lifecycle_needs(*target, cancellation, graph)?;

                CodegenLifecycleNeeds::represented(children)
                    .with(CodegenLifecycleNeeds::DESTROY)
                    .with(CodegenLifecycleNeeds::LIFECYCLE_RESOLUTION)
            }
            TypeData::FlexibleArray(_) | TypeData::Borrow { .. } | TypeData::Callable(_) => {
                CodegenLifecycleNeeds::NONE
            }
            TypeData::Error
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(_)
            | TypeData::TypeValuedMemberProjection { .. }
            | TypeData::Slice(_)
            | TypeData::TraitView(_) => CodegenLifecycleNeeds::ALL,
        };

        assert_eq!(graph.active.pop(), Some(ty));
        graph.computed.insert(ty, needs);

        Ok(needs)
    }

    fn named_codegen_lifecycle_needs(
        &self,
        definition: NamedTypeSymbolId,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
        graph: &mut LifecycleNeedsGraph,
    ) -> Result<CodegenLifecycleNeeds, CodegenPreparationError> {
        if let Some(needs) = self.compiler_known_lifecycle_needs(definition) {
            return Ok(needs);
        }

        let raw_buffer = self.raw_buffer_element(definition, substitution, cancellation)?;

        let surface =
            self.type_associated_surface_result_with_cancellation(definition, cancellation)?;

        let representation =
            self.declared_type_representation_with_cancellation(definition, cancellation)?;

        if surface.diagnostics().has_errors()
            || representation.diagnostics().has_errors()
            || representation.value().is_recovered()
        {
            return Ok(CodegenLifecycleNeeds::ALL);
        }

        let mut needs = CodegenLifecycleNeeds::NONE;

        for member in surface.value().lifecycle_members() {
            needs = match member.slot() {
                TypeAssociatedLifecycleSlot::Finalizer => needs
                    .with(CodegenLifecycleNeeds::FINALIZE)
                    .with(CodegenLifecycleNeeds::DESTROY),
                TypeAssociatedLifecycleSlot::Destructor => {
                    needs.with(CodegenLifecycleNeeds::DESTROY)
                }
                TypeAssociatedLifecycleSlot::PrimaryConstructor
                | TypeAssociatedLifecycleSlot::ScopeEnter
                | TypeAssociatedLifecycleSlot::ScopeExit => needs,
            };
        }

        if needs.contains(CodegenLifecycleNeeds::FINALIZE) {
            let ty = self
                .semantic_value_store()?
                .intern_type(TypeData::Named {
                    definition,
                    substitution,
                })
                .map_err(FactQueryError::SemanticValueStore)?;

            let finalizer = self
                .lifecycle_callable(ty, TypeAssociatedLifecycleSlot::Finalizer, cancellation)?
                .expect("selected finalization retains its concrete callable");

            if finalizer.result.execution() == bray_symbols::CallableExecution::Asynchronous
                && !self.finalizer_is_complete(
                    finalizer.callable,
                    finalizer.result.completion_type(),
                    None,
                    cancellation,
                )?
            {
                needs = needs.with(CodegenLifecycleNeeds::SUSPEND);
            }
        }

        let children = match representation.value().storage() {
            DeclaredStorageShape::Structure(members) => self
                .template_children_codegen_lifecycle_needs(
                    members.iter().map(|member| member.ty()),
                    substitution,
                    cancellation,
                    graph,
                )?,
            DeclaredStorageShape::Union(variants) => self
                .template_children_codegen_lifecycle_needs(
                    variants
                        .iter()
                        .flat_map(|variant| variant.members())
                        .map(|member| member.ty()),
                    substitution,
                    cancellation,
                    graph,
                )?,
        };

        needs = needs.with(CodegenLifecycleNeeds::represented(children));

        if let Some(element) = raw_buffer {
            let children = self.compute_codegen_lifecycle_needs(element, cancellation, graph)?;

            needs = needs
                .with(CodegenLifecycleNeeds::represented(children))
                .with(CodegenLifecycleNeeds::DESTROY)
                .with(CodegenLifecycleNeeds::LIFECYCLE_RESOLUTION);
        }

        if needs.contains(CodegenLifecycleNeeds::FINALIZE)
            || needs.contains(CodegenLifecycleNeeds::DESTROY)
        {
            needs = needs.with(CodegenLifecycleNeeds::LIFECYCLE_RESOLUTION);
        }

        Ok(needs)
    }

    fn children_codegen_lifecycle_needs(
        &self,
        children: impl IntoIterator<Item = TypeId>,
        cancellation: &CancellationToken,
        graph: &mut LifecycleNeedsGraph,
    ) -> Result<CodegenLifecycleNeeds, CodegenPreparationError> {
        let mut needs = CodegenLifecycleNeeds::NONE;

        for child in children {
            needs = needs.with(self.compute_codegen_lifecycle_needs(child, cancellation, graph)?);
        }

        Ok(needs)
    }

    fn template_children_codegen_lifecycle_needs<'template>(
        &self,
        children: impl IntoIterator<Item = &'template bray_symbols::TypeExpressionTemplate>,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
        graph: &mut LifecycleNeedsGraph,
    ) -> Result<CodegenLifecycleNeeds, CodegenPreparationError> {
        let mut needs = CodegenLifecycleNeeds::NONE;

        for template in children {
            let child = self.resolve_codegen_type(template, substitution, cancellation)?;

            needs = needs.with(self.compute_codegen_lifecycle_needs(child, cancellation, graph)?);
        }

        Ok(needs)
    }

    fn compiler_known_lifecycle_needs(
        &self,
        definition: NamedTypeSymbolId,
    ) -> Option<CodegenLifecycleNeeds> {
        let role = super::super::foreign::compiler_known_representation(self, definition)?;

        match role {
            RepresentationRole::String | RepresentationRole::PanicReport => Some(
                CodegenLifecycleNeeds::DESTROY.with(CodegenLifecycleNeeds::LIFECYCLE_RESOLUTION),
            ),
            RepresentationRole::Task => Some(CodegenLifecycleNeeds::ALL),
            RepresentationRole::Future => Some(CodegenLifecycleNeeds::ALL),
            _ if super::super::representation::target_scalar(role).is_some()
                || matches!(
                    role,
                    RepresentationRole::Atomic
                        | RepresentationRole::Unit
                        | RepresentationRole::Never
                        | RepresentationRole::RawPointer
                        | RepresentationRole::FunctionPointer
                        | RepresentationRole::DevicePointer
                        | RepresentationRole::Uninit
                ) =>
            {
                Some(CodegenLifecycleNeeds::NONE)
            }
            _ => None,
        }
    }

    fn cached_lifecycle_needs(
        &self,
        ty: TypeId,
    ) -> Result<Option<CodegenLifecycleNeeds>, FactQueryError> {
        let needs = self
            .state
            .codegen_lifecycle_needs
            .lock()
            .unwrap_or_else(|_| {
                panic!(
                    "product SynchronizationPoisoned construction failed: component={:?}",
                    ProductSynchronizationComponent::LifecycleNeeds
                )
            })
            .get(&ty)
            .copied();

        Ok(needs)
    }
}

#[cfg(test)]
mod tests {
    use bray_compiler_known::RepresentationRole;
    use bray_ir::{MirCleanupPhase, MirHelperReference};
    use bray_symbols::{NamedTypeSymbolId, StructSymbolId, SymbolOrigin};

    use super::CodegenLifecycleNeeds;
    use crate::CancellationToken;
    use crate::compilation::substitution::named_type;
    use crate::test_support::compilation;

    #[test]
    fn represented_lifecycle_combines_child_finalization_and_destruction() {
        let represented = CodegenLifecycleNeeds::represented(CodegenLifecycleNeeds::FINALIZE);

        assert_eq!(
            represented,
            CodegenLifecycleNeeds::DESTROY.with(CodegenLifecycleNeeds::LIFECYCLE_RESOLUTION)
        );
    }

    #[test]
    fn represented_lifecycle_retains_cancellation_independently() {
        let represented =
            CodegenLifecycleNeeds::represented(CodegenLifecycleNeeds::TASK_CANCELLATION);

        assert_eq!(represented, CodegenLifecycleNeeds::TASK_CANCELLATION);
    }

    #[test]
    fn panic_reports_require_destruction_and_lifecycle_resolution() {
        let compilation = compilation("module app; func main() {}");

        let definition = compilation
            .available_compiler_known_symbols()
            .representation_symbol::<StructSymbolId>(RepresentationRole::PanicReport)
            .expect("panic report definition must resolve");

        let values = compilation
            .semantic_value_store()
            .expect("semantic values must resolve");

        let report = named_type(values, NamedTypeSymbolId::Struct(definition))
            .expect("panic report representation must resolve");

        let cancellation = CancellationToken::new();

        for reference in [
            MirHelperReference::Destroy(report, bray_ir::MirDestructionDisposition::Dynamic),
            MirHelperReference::Cleanup {
                phase: MirCleanupPhase::LifecycleResolution,
                ty: report,
            },
        ] {
            assert!(
                !compilation
                    .codegen_lifecycle_is_trivial(&reference, &cancellation)
                    .unwrap_or_else(|error| panic!(
                        "panic report lifecycle must resolve: {error:?}"
                    ))
            );
        }

        for reference in [
            MirHelperReference::StaticFinalize(report),
            MirHelperReference::Cleanup {
                phase: MirCleanupPhase::TaskCancellation,
                ty: report,
            },
        ] {
            assert!(
                compilation
                    .codegen_lifecycle_is_trivial(&reference, &cancellation)
                    .unwrap_or_else(|error| panic!(
                        "panic report lifecycle must resolve: {error:?}"
                    ))
            );
        }
    }

    #[test]
    fn nested_trivial_storage_does_not_require_a_codegen_lifecycle() {
        let compilation = compilation(
            "module app;\n\
             struct Leaf\n\
             {\n\
                 value: i32;\n\
             }\n\
             struct Container\n\
             {\n\
                 text: string;\n\
                 leaf: Leaf;\n\
             }\n",
        );

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let source_structures = symbols
            .structures()
            .iter()
            .filter(|structure| structure.origin() == SymbolOrigin::Source)
            .collect::<Vec<_>>();

        let [leaf, container] = source_structures.as_slice() else {
            panic!("test source must declare Leaf and Container");
        };

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must resolve: {error:?}"));

        let leaf = named_type(values, NamedTypeSymbolId::Struct(leaf.id()))
            .unwrap_or_else(|error| panic!("leaf type must resolve: {error:?}"));

        let container = named_type(values, NamedTypeSymbolId::Struct(container.id()))
            .unwrap_or_else(|error| panic!("container type must resolve: {error:?}"));

        let cancellation = CancellationToken::new();

        assert!(
            compilation
                .codegen_lifecycle_is_trivial(
                    &MirHelperReference::Destroy(leaf, bray_ir::MirDestructionDisposition::Dynamic),
                    &cancellation,
                )
                .unwrap_or_else(|error| panic!("leaf lifecycle must resolve: {error:?}"))
        );

        assert!(
            !compilation
                .codegen_lifecycle_is_trivial(
                    &MirHelperReference::Destroy(
                        container,
                        bray_ir::MirDestructionDisposition::Dynamic
                    ),
                    &cancellation,
                )
                .unwrap_or_else(|error| panic!("container lifecycle must resolve: {error:?}"))
        );

        assert!(
            compilation
                .codegen_lifecycle_is_trivial(
                    &MirHelperReference::Cleanup {
                        phase: MirCleanupPhase::TaskCancellation,
                        ty: container,
                    },
                    &cancellation,
                )
                .unwrap_or_else(|error| panic!("container cleanup must resolve: {error:?}"))
        );
    }

    #[test]
    fn destructor_only_storage_has_no_finalization_helper() {
        let compilation = compilation(concat!(
            "module app;\n",
            "struct Resource\n",
            "{\n",
            "    value: usize;\n",
            "    destruct() {}\n",
            "}\n",
        ));

        let symbols = compilation
            .symbol_graph()
            .unwrap_or_else(|error| panic!("test symbol graph must build: {error:?}"));

        let resource = symbols
            .structures()
            .iter()
            .find(|structure| structure.origin() == SymbolOrigin::Source)
            .unwrap_or_else(|| panic!("test source must declare Resource"));

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must resolve: {error:?}"));

        let resource = named_type(values, NamedTypeSymbolId::Struct(resource.id()))
            .unwrap_or_else(|error| panic!("resource type must resolve: {error:?}"));

        let cancellation = CancellationToken::new();

        assert!(
            compilation
                .codegen_lifecycle_is_trivial(
                    &MirHelperReference::StaticFinalize(resource),
                    &cancellation,
                )
                .unwrap_or_else(|error| panic!("resource finalization must resolve: {error:?}"))
        );

        assert!(
            !compilation
                .codegen_lifecycle_is_trivial(
                    &MirHelperReference::Destroy(
                        resource,
                        bray_ir::MirDestructionDisposition::Dynamic
                    ),
                    &cancellation,
                )
                .unwrap_or_else(|error| panic!("resource destruction must resolve: {error:?}"))
        );
    }
    #[test]
    #[should_panic(expected = "lifecycle query requires a lifecycle helper, got PanicReport")]
    fn lifecycle_query_rejects_an_internal_non_lifecycle_reference() {
        let compilation = compilation("module app;");

        let _ = compilation.codegen_lifecycle_is_trivial(
            &MirHelperReference::PanicReport,
            &CancellationToken::new(),
        );
    }
}
