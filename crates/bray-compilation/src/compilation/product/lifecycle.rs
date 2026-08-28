use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use bray_compiler_known::{RecognizedStandardLibraryDeclarationKey, RepresentationRole};
use bray_ir::{MirCleanupPhase, MirGeneratedLifecycleRole, MirHelperReference};
use bray_symbols::{
    DeclaredStorageShape, GenericArgument, GenericSubstitutionId, NamedTypeSymbolId,
    PackageIdentity, StructSymbolId, TypeAssociatedLifecycleSlot, TypeData, TypeId,
};

use super::super::{CodegenPreparationError, Compilation};
use crate::fact::{CancellationToken, FactQueryError};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(in crate::compilation) struct CodegenLifecycleNeeds(u8);

impl CodegenLifecycleNeeds {
    const NONE: Self = Self(0);
    const FINALIZE: Self = Self(1 << 0);
    const DESTROY: Self = Self(1 << 1);
    const TASK_CANCELLATION: Self = Self(1 << 2);
    const LIFECYCLE_RESOLUTION: Self = Self(1 << 3);
    const ALL: Self = Self(
        Self::FINALIZE.0
            | Self::DESTROY.0
            | Self::TASK_CANCELLATION.0
            | Self::LIFECYCLE_RESOLUTION.0,
    );

    const fn contains(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    const fn requires(self, role: MirGeneratedLifecycleRole) -> bool {
        let required = match role {
            MirGeneratedLifecycleRole::Finalize | MirGeneratedLifecycleRole::StaticFinalize => {
                Self::FINALIZE
            }
            MirGeneratedLifecycleRole::Destroy => Self::DESTROY,
            MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::TaskCancellation) => {
                Self::TASK_CANCELLATION
            }
            MirGeneratedLifecycleRole::Cleanup(MirCleanupPhase::LifecycleResolution) => {
                Self::LIFECYCLE_RESOLUTION
            }
        };

        self.contains(required)
    }

    const fn with(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    const fn represented(children: Self) -> Self {
        let mut represented = Self::NONE;

        if children.contains(Self::FINALIZE) || children.contains(Self::DESTROY) {
            represented = represented.with(Self::DESTROY);
        }

        if children.contains(Self::TASK_CANCELLATION) {
            represented = represented.with(Self::TASK_CANCELLATION);
        }

        if represented.contains(Self::DESTROY) {
            represented = represented.with(Self::LIFECYCLE_RESOLUTION);
        }

        represented
    }
}

impl Compilation {
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
        let role = MirGeneratedLifecycleRole::from_reference(reference)
            .ok_or(FactQueryError::InfrastructureFailure)?;

        let ty = reference
            .lifecycle_type()
            .ok_or(FactQueryError::InfrastructureFailure)?;

        Ok(!self
            .codegen_lifecycle_needs(ty, cancellation)?
            .requires(role))
    }

    fn codegen_lifecycle_needs(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<CodegenLifecycleNeeds, CodegenPreparationError> {
        if let Some(needs) = self.cached_lifecycle_needs(ty)? {
            return Ok(needs);
        }

        let mut active = BTreeSet::new();
        let mut computed = BTreeMap::new();

        let needs =
            self.compute_codegen_lifecycle_needs(ty, cancellation, &mut active, &mut computed)?;

        self.state
            .codegen_lifecycle_needs
            .lock()
            .map_err(|_| FactQueryError::InfrastructureFailure)?
            .extend(computed);

        Ok(needs)
    }

    fn compute_codegen_lifecycle_needs(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
        active: &mut BTreeSet<TypeId>,
        computed: &mut BTreeMap<TypeId, CodegenLifecycleNeeds>,
    ) -> Result<CodegenLifecycleNeeds, CodegenPreparationError> {
        cancellation.check()?;

        if let Some(needs) = computed
            .get(&ty)
            .copied()
            .or(self.cached_lifecycle_needs(ty)?)
        {
            return Ok(needs);
        }

        if !active.insert(ty) {
            // Legal recursion crosses an owned indirection. Preserve all lifecycle work for
            // recovered direct cycles rather than suppressing code generation speculatively.
            return Ok(CodegenLifecycleNeeds::ALL);
        }

        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let needs = match data.as_ref() {
            TypeData::Named {
                definition,
                substitution,
            } => self.named_codegen_lifecycle_needs(
                *definition,
                *substitution,
                cancellation,
                active,
                computed,
            )?,
            TypeData::Tuple(elements) => {
                let children = self.children_codegen_lifecycle_needs(
                    elements.iter().copied(),
                    cancellation,
                    active,
                    computed,
                )?;

                CodegenLifecycleNeeds::represented(children)
            }
            TypeData::Array { element, length } => {
                if super::realization::closed_array_length(values, *length)? == 0 {
                    CodegenLifecycleNeeds::NONE
                } else {
                    let child = self.compute_codegen_lifecycle_needs(
                        *element,
                        cancellation,
                        active,
                        computed,
                    )?;

                    CodegenLifecycleNeeds::represented(child)
                }
            }
            TypeData::Nullable(target) => {
                let child =
                    self.compute_codegen_lifecycle_needs(*target, cancellation, active, computed)?;

                CodegenLifecycleNeeds::represented(child)
            }
            TypeData::Generator(_) => CodegenLifecycleNeeds::DESTROY
                .with(CodegenLifecycleNeeds::TASK_CANCELLATION)
                .with(CodegenLifecycleNeeds::LIFECYCLE_RESOLUTION),
            TypeData::OwnedIndirection { .. } => CodegenLifecycleNeeds::DESTROY
                .with(CodegenLifecycleNeeds::TASK_CANCELLATION)
                .with(CodegenLifecycleNeeds::LIFECYCLE_RESOLUTION),
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

        active.remove(&ty);
        computed.insert(ty, needs);

        Ok(needs)
    }

    fn named_codegen_lifecycle_needs(
        &self,
        definition: NamedTypeSymbolId,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
        active: &mut BTreeSet<TypeId>,
        computed: &mut BTreeMap<TypeId, CodegenLifecycleNeeds>,
    ) -> Result<CodegenLifecycleNeeds, CodegenPreparationError> {
        if let Some(needs) = self.compiler_known_lifecycle_needs(definition) {
            return Ok(needs);
        }

        let raw_buffer = self
            .imported_raw_buffer_element(definition, substitution, cancellation)?
            .is_some();

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
                TypeAssociatedLifecycleSlot::Finalizer => {
                    needs.with(CodegenLifecycleNeeds::FINALIZE)
                }
                TypeAssociatedLifecycleSlot::Destructor => {
                    needs.with(CodegenLifecycleNeeds::DESTROY)
                }
                TypeAssociatedLifecycleSlot::PrimaryConstructor
                | TypeAssociatedLifecycleSlot::ScopeEnter
                | TypeAssociatedLifecycleSlot::ScopeExit => needs,
            };
        }

        let children = match representation.value().storage() {
            DeclaredStorageShape::Structure(members) => self
                .template_children_codegen_lifecycle_needs(
                    members.iter().map(|member| member.ty()),
                    substitution,
                    cancellation,
                    active,
                    computed,
                )?,
            DeclaredStorageShape::Union(variants) => self
                .template_children_codegen_lifecycle_needs(
                    variants
                        .iter()
                        .flat_map(|variant| variant.members())
                        .map(|member| member.ty()),
                    substitution,
                    cancellation,
                    active,
                    computed,
                )?,
        };

        needs = needs.with(CodegenLifecycleNeeds::represented(children));

        if raw_buffer {
            needs = needs
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
        active: &mut BTreeSet<TypeId>,
        computed: &mut BTreeMap<TypeId, CodegenLifecycleNeeds>,
    ) -> Result<CodegenLifecycleNeeds, CodegenPreparationError> {
        let mut needs = CodegenLifecycleNeeds::NONE;

        for child in children {
            needs = needs.with(self.compute_codegen_lifecycle_needs(
                child,
                cancellation,
                active,
                computed,
            )?);
        }

        Ok(needs)
    }

    fn template_children_codegen_lifecycle_needs<'template>(
        &self,
        children: impl IntoIterator<Item = &'template bray_symbols::TypeExpressionTemplate>,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
        active: &mut BTreeSet<TypeId>,
        computed: &mut BTreeMap<TypeId, CodegenLifecycleNeeds>,
    ) -> Result<CodegenLifecycleNeeds, CodegenPreparationError> {
        let mut needs = CodegenLifecycleNeeds::NONE;

        for template in children {
            let child = self.resolve_codegen_type(template, substitution, cancellation)?;

            needs = needs.with(self.compute_codegen_lifecycle_needs(
                child,
                cancellation,
                active,
                computed,
            )?);
        }

        Ok(needs)
    }

    fn compiler_known_lifecycle_needs(
        &self,
        definition: NamedTypeSymbolId,
    ) -> Option<CodegenLifecycleNeeds> {
        let role = super::super::foreign::compiler_known_representation(self, definition)?;

        if super::super::representation::target_scalar(role).is_some()
            || matches!(
                role,
                RepresentationRole::Atomic
                    | RepresentationRole::Unit
                    | RepresentationRole::Never
                    | RepresentationRole::RawPointer
                    | RepresentationRole::DevicePointer
                    | RepresentationRole::Uninit
                    | RepresentationRole::Future
            )
        {
            return Some(CodegenLifecycleNeeds::NONE);
        }

        match role {
            RepresentationRole::String => Some(
                CodegenLifecycleNeeds::DESTROY.with(CodegenLifecycleNeeds::LIFECYCLE_RESOLUTION),
            ),
            RepresentationRole::Task => Some(CodegenLifecycleNeeds::ALL),
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
            .map_err(|_| FactQueryError::InfrastructureFailure)?
            .get(&ty)
            .copied();

        Ok(needs)
    }

    pub(super) fn imported_raw_buffer_element(
        &self,
        definition: NamedTypeSymbolId,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
    ) -> Result<Option<TypeId>, CodegenPreparationError> {
        let Some(key) = RecognizedStandardLibraryDeclarationKey::try_new("StandardRawBuffer")
        else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        let Some(package) = PackageIdentity::try_new(
            bray_standard_library::PUBLIC_STANDARD_LIBRARY_PACKAGE_IDENTITY,
        ) else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        let imported = self.imported_symbol_skeleton_result_with_cancellation(cancellation)?;

        let Some(imported) = imported.value() else {
            return Ok(None);
        };

        let target = self.selected_target().target();

        let recognized =
            Arc::clone(imported).recognize_standard_library(&package, |rule| target.supports(rule));

        let Some(raw_buffer) = recognized.declaration_symbol::<StructSymbolId>(&key) else {
            return Ok(None);
        };

        if definition != NamedTypeSymbolId::Struct(raw_buffer) {
            return Ok(None);
        }

        let substitution = self
            .semantic_value_store()?
            .generic_substitution_data(substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let [binding] = substitution.bindings() else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        let GenericArgument::Type(element) = binding.argument() else {
            return Err(FactQueryError::InfrastructureFailure.into());
        };

        Ok(Some(element))
    }
}

#[cfg(test)]
mod tests {
    use bray_ir::{MirCleanupPhase, MirHelperReference};
    use bray_symbols::{NamedTypeSymbolId, SymbolOrigin};

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
                .codegen_lifecycle_is_trivial(&MirHelperReference::Destroy(leaf), &cancellation,)
                .unwrap_or_else(|error| panic!("leaf lifecycle must resolve: {error:?}"))
        );

        assert!(
            !compilation
                .codegen_lifecycle_is_trivial(
                    &MirHelperReference::Destroy(container),
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
                    &MirHelperReference::Finalize(resource),
                    &cancellation,
                )
                .unwrap_or_else(|error| panic!("resource finalization must resolve: {error:?}"))
        );

        assert!(
            !compilation
                .codegen_lifecycle_is_trivial(
                    &MirHelperReference::Destroy(resource),
                    &cancellation,
                )
                .unwrap_or_else(|error| panic!("resource destruction must resolve: {error:?}"))
        );
    }
}
