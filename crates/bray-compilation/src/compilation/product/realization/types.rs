use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use bray_codegen::{
    CodegenInstanceTypeMapping, CodegenParameterMapping, CodegenResultMapping, CodegenTarget,
    CodegenTypeKind, CodegenTypeMapping, TargetAddressSpaceKind,
};
use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_diagnostics::DiagnosticBag;
use bray_symbols::{
    GenericArgument, GenericSubstitutionId, NamedTypeSymbolId, SelfTypeContext, StructSymbolId,
    TypeData, TypeId,
};
use bray_target::{TargetLayoutContract, TargetScalarKind, TargetValueLayout};

use super::super::super::CodegenPreparationError;
use super::super::super::Compilation;
use super::super::super::checker::CompilationCheckerContext;
use super::super::super::substitution::substitution_for_owner;
use super::super::specialization::ConcreteCodegenInstance;
use super::support::{
    atomic_representation_for_type, atomic_storage_role, callable_type_signature,
    closed_array_length, codegen_checker_error, implementation_subject, pointer_layout,
    pointer_mapping, scalar_mapping, target_layout_contract,
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    #[cfg(test)]
    pub(super) fn codegen_types(
        &self,
        demanded: BTreeSet<TypeId>,
        substitution: Option<GenericSubstitutionId>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
    ) -> Result<Vec<CodegenTypeMapping>, CodegenPreparationError> {
        let mut mappings = BTreeMap::new();
        let mut instance_mappings = Vec::new();

        self.extend_codegen_types(
            demanded,
            substitution,
            target,
            cancellation,
            &mut mappings,
            None,
            &mut instance_mappings,
        )?;

        Ok(mappings.into_values().collect())
    }

    pub(super) fn extend_codegen_types(
        &self,
        demanded: BTreeSet<TypeId>,
        substitution: Option<GenericSubstitutionId>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        instance: Option<&ConcreteCodegenInstance>,
        instance_mappings: &mut Vec<CodegenInstanceTypeMapping>,
    ) -> Result<(), CodegenPreparationError> {
        let mut pending = BTreeSet::new();

        for template in demanded {
            let ty = self.concrete_codegen_type(template, substitution, instance, cancellation)?;

            self.codegen_type(ty, target, cancellation, mappings, &mut pending)?;

            if template != ty {
                let mapping = mappings
                    .get(&ty)
                    .cloned()
                    .ok_or(CodegenPreparationError::UnsupportedType(ty))?;

                if let Some(instance) = instance {
                    // Published mappings own instance identities independently of realization.
                    instance_mappings.push(CodegenInstanceTypeMapping::new(
                        instance.key().clone(),
                        template,
                        ty,
                    ));
                } else {
                    let mapping = match mapping.layout() {
                        Some(layout) => {
                            CodegenTypeMapping::new(template, layout, mapping.kind().clone())
                        }
                        None => CodegenTypeMapping::new_unsized(template, mapping.kind().clone()),
                    }
                    .with_backend_type(ty)
                    .with_behavior(mapping.behavior());

                    mappings.insert(template, mapping);
                }
            }
        }

        self.classify_codegen_callable_types(target, cancellation, mappings, &mut pending)?;

        Ok(())
    }

    pub(super) fn concrete_codegen_type(
        &self,
        ty: TypeId,
        substitution: Option<GenericSubstitutionId>,
        instance: Option<&ConcreteCodegenInstance>,
        cancellation: &CancellationToken,
    ) -> Result<TypeId, CodegenPreparationError> {
        let ty = self.substitute_codegen_type(ty, substitution, cancellation)?;
        let binding_context = self.binding_context(cancellation)?;

        let checker = CompilationCheckerContext::new(binding_context)
            .with_implementation_witnesses(
                instance
                    .into_iter()
                    .flat_map(ConcreteCodegenInstance::implementation_witnesses)
                    .copied(),
            );

        let mut diagnostics = DiagnosticBag::new();

        let ty = bray_checker::normalize_type_valued_members(&checker, ty, &mut diagnostics)
            .map_err(codegen_checker_error)?;

        if diagnostics.has_errors() {
            return Err(CodegenPreparationError::Diagnostics(diagnostics));
        }

        Ok(ty)
    }

    pub(super) fn substitute_codegen_type(
        &self,
        ty: TypeId,
        substitution: Option<GenericSubstitutionId>,
        cancellation: &CancellationToken,
    ) -> Result<TypeId, FactQueryError> {
        let ty = self.resolve_codegen_contextual_self(ty, cancellation)?;

        let Some(substitution) = substitution else {
            return Ok(ty);
        };

        let values = self.semantic_value_store()?;

        let ty = values
            .substitute_type(ty, substitution)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        match data.as_ref() {
            TypeData::ContextualSelf(SelfTypeContext::NamedType(definition)) => {
                let substitution =
                    substitution_for_owner(values, definition.into_any(), [substitution])?;

                values
                    .intern_type(TypeData::Named {
                        definition: *definition,
                        substitution,
                    })
                    .map_err(|_| FactQueryError::InfrastructureFailure)
            }
            TypeData::Borrow { kind, target } => {
                let target =
                    self.substitute_codegen_type(*target, Some(substitution), cancellation)?;

                values
                    .intern_type(TypeData::Borrow {
                        kind: *kind,
                        target,
                    })
                    .map_err(|_| FactQueryError::InfrastructureFailure)
            }
            _ => Ok(ty),
        }
    }

    pub(super) fn resolve_codegen_contextual_self(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<TypeId, FactQueryError> {
        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let TypeData::ContextualSelf(SelfTypeContext::Implementation(implementation)) =
            data.as_ref()
        else {
            return Ok(ty);
        };

        let binding_context = self.binding_context(cancellation)?;

        implementation_subject(&binding_context, *implementation)
    }

    pub(super) fn substitute_codegen_constant_term(
        &self,
        term: bray_symbols::ConstantTermId,
        substitution: Option<GenericSubstitutionId>,
    ) -> Result<bray_symbols::ConstantTermId, CodegenPreparationError> {
        let term = if let Some(substitution) = substitution {
            self.semantic_value_store()?
                .substitute_constant_term(term, substitution)
                .map_err(|_| FactQueryError::InfrastructureFailure)?
        } else {
            term
        };

        self.realize_codegen_constant_argument(term)
    }

    pub(super) fn classify_codegen_callable_types(
        &self,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<(), CodegenPreparationError> {
        let mut classified = BTreeSet::new();

        loop {
            let callable =
                mappings.values().find_map(|mapping| {
                    if classified.contains(&mapping.ty()) {
                        return None;
                    }

                    let CodegenTypeKind::Callable(signature) = mapping.kind() else {
                        return None;
                    };

                    if signature.parameters().iter().any(|parameter| {
                        !matches!(parameter, CodegenParameterMapping::Direct { .. })
                    }) || matches!(signature.result(), CodegenResultMapping::Indirect { .. })
                    {
                        return None;
                    }

                    // Signatures use shared slices. This clone releases the mapping borrow before recursion.
                    Some((mapping.ty(), signature.as_ref().clone()))
                });

            let Some((ty, signature)) = callable else {
                break;
            };

            classified.insert(ty);

            self.codegen_signature_types(&signature, target, cancellation, mappings, pending)?;

            let signature = self.classify_codegen_signature(
                signature,
                target,
                cancellation,
                mappings,
                pending,
            )?;

            mappings.insert(
                ty,
                CodegenTypeMapping::new(
                    ty,
                    pointer_layout(target),
                    CodegenTypeKind::Callable(signature.into()),
                ),
            );
        }

        Ok(())
    }

    pub(super) fn codegen_type(
        &self,
        ty: TypeId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<(), CodegenPreparationError> {
        if mappings.contains_key(&ty) {
            return Ok(());
        }

        cancellation.check()?;

        if !pending.insert(ty) {
            return Err(CodegenPreparationError::RecursiveValueType(ty));
        }

        let values = self.semantic_value_store()?;

        let data = values
            .type_data(ty)
            .map_err(|_| FactQueryError::InfrastructureFailure)?;

        let mapping = match data.as_ref() {
            TypeData::Named {
                definition,
                substitution,
            } => self.codegen_named_type(
                ty,
                *definition,
                *substitution,
                target,
                cancellation,
                mappings,
                pending,
            )?,
            TypeData::Tuple(elements) => self.codegen_aggregate_type(
                ty,
                elements.iter().copied().map(|element| (None, element)),
                TargetLayoutContract::Default,
                None,
                None,
                target,
                cancellation,
                mappings,
                pending,
            )?,
            TypeData::Array { element, length } => {
                self.codegen_type(*element, target, cancellation, mappings, pending)?;

                let length = closed_array_length(values, *length)?;

                let element_layout = mappings
                    .get(element)
                    .and_then(CodegenTypeMapping::layout)
                    .ok_or(CodegenPreparationError::UnsizedTypeByValue(*element))?;

                CodegenTypeMapping::new(
                    ty,
                    TargetValueLayout::new(
                        element_layout
                            .size()
                            .checked_mul(length)
                            .ok_or(CodegenPreparationError::LayoutOverflow(ty))?,
                        element_layout.alignment(),
                        TargetLayoutContract::Default,
                    ),
                    CodegenTypeKind::Array {
                        element: *element,
                        length,
                    },
                )
            }
            TypeData::Borrow {
                kind,
                target: pointee,
            } => self.codegen_indirection_type(
                ty,
                *pointee,
                Some(*kind),
                target,
                cancellation,
                mappings,
                pending,
            )?,
            TypeData::OwnedIndirection {
                target: pointee, ..
            } => self.codegen_indirection_type(
                ty,
                *pointee,
                None,
                target,
                cancellation,
                mappings,
                pending,
            )?,
            TypeData::Callable(callable) => {
                let signature = callable_type_signature(self, callable)?;

                CodegenTypeMapping::new(
                    ty,
                    pointer_layout(target),
                    CodegenTypeKind::Callable(signature.into()),
                )
            }
            TypeData::Slice(element) => {
                self.codegen_type(*element, target, cancellation, mappings, pending)?;

                CodegenTypeMapping::new_unsized(
                    ty,
                    CodegenTypeKind::UnsizedSlice { element: *element },
                )
            }
            TypeData::Generator(element) => {
                self.codegen_generator_type(ty, *element, target, cancellation, mappings, pending)?
            }
            TypeData::Nullable(element) => {
                self.codegen_nullable_type(ty, *element, target, cancellation, mappings, pending)?
            }
            TypeData::TraitView(_) => {
                CodegenTypeMapping::new_unsized(ty, CodegenTypeKind::UnsizedTraitView)
            }
            TypeData::ContextualSelf(SelfTypeContext::NamedType(definition)) => {
                let mut candidates = mappings.values().filter(|mapping| {
                    values.type_data(mapping.ty()).is_ok_and(|data| {
                        matches!(
                            data.as_ref(),
                            TypeData::Named {
                                definition: candidate,
                                ..
                            } if candidate == definition
                        )
                    })
                });

                let mapping = candidates
                    .next()
                    .cloned()
                    .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

                if candidates.any(|candidate| {
                    candidate.layout() != mapping.layout() || candidate.kind() != mapping.kind()
                }) {
                    return Err(CodegenPreparationError::UnresolvedType(ty));
                }

                match mapping.layout() {
                    Some(layout) => CodegenTypeMapping::new(ty, layout, mapping.kind().clone()),
                    None => CodegenTypeMapping::new_unsized(ty, mapping.kind().clone()),
                }
            }
            TypeData::Error
            | TypeData::TypeParameter(_)
            | TypeData::ContextualSelf(
                SelfTypeContext::Trait(_) | SelfTypeContext::Implementation(_),
            )
            | TypeData::TypeValuedMemberProjection { .. } => {
                return Err(CodegenPreparationError::UnresolvedType(ty));
            }
        };

        pending.remove(&ty);
        mappings.insert(ty, mapping);

        Ok(())
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "recursive type realization keeps the selected target and cycle state explicit"
    )]
    pub(super) fn codegen_named_type(
        &self,
        ty: TypeId,
        definition: NamedTypeSymbolId,
        substitution: GenericSubstitutionId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        let role = super::super::super::foreign::compiler_known_representation(self, definition);

        if let Some(role) = role
            && let Some(mapping) = self.codegen_compiler_known_type(
                ty,
                role,
                substitution,
                target,
                cancellation,
                mappings,
                pending,
            )?
        {
            return Ok(mapping);
        }

        let heap_key = CompilerKnownDeclarationKey::try_new("Heap")
            .ok_or(FactQueryError::InfrastructureFailure)?;

        if self
            .available_compiler_known_symbols()
            .declaration_symbol::<StructSymbolId>(&heap_key)
            .is_some_and(|heap| definition == NamedTypeSymbolId::Struct(heap))
        {
            return Ok(pointer_mapping(
                ty,
                ty,
                target,
                TargetAddressSpaceKind::Default,
            ));
        }

        match definition {
            NamedTypeSymbolId::Struct(structure) => {
                let representation = self.declared_type_representation_with_cancellation(
                    NamedTypeSymbolId::Struct(structure),
                    cancellation,
                )?;

                let bray_symbols::DeclaredStorageShape::Structure(storage) =
                    representation.value().storage()
                else {
                    return Err(CodegenPreparationError::UnresolvedType(ty));
                };

                let fields = storage
                    .iter()
                    .map(|field| {
                        let field_ty =
                            self.resolve_codegen_type(field.ty(), substitution, cancellation)?;

                        Ok((
                            field.field().map(bray_ir::MirFieldReference::Struct),
                            field_ty,
                        ))
                    })
                    .collect::<Result<Vec<_>, FactQueryError>>()?;

                self.codegen_aggregate_type(
                    ty,
                    fields,
                    target_layout_contract(representation.value().layout()),
                    representation.value().alignment(),
                    representation.value().packing(),
                    target,
                    cancellation,
                    mappings,
                    pending,
                )
            }
            NamedTypeSymbolId::Union(union) => self.codegen_union_type(
                ty,
                union,
                substitution,
                target,
                cancellation,
                mappings,
                pending,
            ),
        }
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "compiler-known type realization shares recursive mapping state with structural types"
    )]
    pub(super) fn codegen_compiler_known_type(
        &self,
        ty: TypeId,
        role: RepresentationRole,
        substitution: GenericSubstitutionId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<Option<CodegenTypeMapping>, CodegenPreparationError> {
        if matches!(role, RepresentationRole::Unit | RepresentationRole::Never) {
            return Ok(Some(CodegenTypeMapping::new(
                ty,
                TargetValueLayout::new(0, NonZeroU64::MIN, TargetLayoutContract::Default),
                CodegenTypeKind::Unit,
            )));
        }

        if matches!(
            role,
            RepresentationRole::RawPointer | RepresentationRole::DevicePointer
        ) {
            let substitution = self
                .semantic_value_store()?
                .generic_substitution_data(substitution)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let [binding] = substitution.bindings() else {
                return Err(CodegenPreparationError::UnresolvedType(ty));
            };

            let GenericArgument::Type(pointee) = binding.argument() else {
                return Err(CodegenPreparationError::UnresolvedType(ty));
            };

            let address_space = match role {
                RepresentationRole::RawPointer => TargetAddressSpaceKind::Default,
                RepresentationRole::DevicePointer => TargetAddressSpaceKind::Device,
                _ => return Err(CodegenPreparationError::UnresolvedType(ty)),
            };

            self.codegen_type(pointee, target, cancellation, mappings, pending)?;

            return Ok(Some(pointer_mapping(ty, pointee, target, address_space)));
        }

        if role == RepresentationRole::Atomic {
            let values = self.semantic_value_store()?;

            let substitution = values
                .generic_substitution_data(substitution)
                .map_err(|_| FactQueryError::InfrastructureFailure)?;

            let [binding] = substitution.bindings() else {
                return Err(CodegenPreparationError::UnresolvedType(ty));
            };

            let GenericArgument::Type(value) = binding.argument() else {
                return Err(CodegenPreparationError::UnresolvedType(ty));
            };

            self.codegen_type(value, target, cancellation, mappings, pending)?;

            let representation = atomic_representation_for_type(self, value, target, cancellation)?
                .ok_or(CodegenPreparationError::UnsupportedType(value))?;

            let storage_type = match atomic_storage_role(representation) {
                Some(role) => self.codegen_representation_type(role)?,
                None => value,
            };

            self.codegen_type(storage_type, target, cancellation, mappings, pending)?;

            let storage_mapping = mappings
                .get(&storage_type)
                .ok_or(CodegenPreparationError::UnresolvedType(storage_type))?;

            let storage_layout = storage_mapping
                .layout()
                .ok_or(CodegenPreparationError::UnsizedTypeByValue(storage_type))?;

            let binding_context = target
                .profile()
                .properties()
                .atomics()
                .representation(representation);

            if !binding_context.operations().any() {
                return Err(CodegenPreparationError::UnsupportedType(value));
            }

            return Ok(Some(
                CodegenTypeMapping::new(
                    ty,
                    TargetValueLayout::new(
                        storage_layout.size(),
                        binding_context.required_alignment(),
                        storage_layout.contract(),
                    ),
                    // The wrapper and backing storage deliberately share one backend kind.
                    storage_mapping.kind().clone(),
                )
                .with_backend_type(storage_type),
            ));
        }

        if let Some(scalar) = super::super::super::representation::target_scalar(role) {
            return scalar_mapping(
                self,
                ty,
                role,
                scalar,
                target,
                cancellation,
                mappings,
                pending,
            )
            .map(Some);
        }

        match role {
            RepresentationRole::Uninit => {
                let values = self.semantic_value_store()?;

                let element = self
                    .available_compiler_known_symbols()
                    .unary_representation_argument(values, role, ty)
                    .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

                self.codegen_type(element, target, cancellation, mappings, pending)?;

                let element_mapping = mappings
                    .get(&element)
                    .ok_or(CodegenPreparationError::UnresolvedType(element))?;

                // The wrapper shares immutable physical kind metadata but intentionally omits the element lifecycle behavior.
                let kind = element_mapping.kind().clone();

                let mapping = match element_mapping.layout() {
                    Some(layout) => CodegenTypeMapping::new(ty, layout, kind),
                    None => CodegenTypeMapping::new_unsized(ty, kind),
                }
                .with_backend_type(element_mapping.backend_type());

                Ok(Some(mapping))
            }
            RepresentationRole::String => self
                .codegen_string_type(ty, target, cancellation, mappings, pending)
                .map(Some),
            RepresentationRole::Range => {
                let element = self
                    .available_compiler_known_symbols()
                    .unary_representation_argument(self.semantic_value_store()?, role, ty)
                    .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

                self.codegen_aggregate_type(
                    ty,
                    [(None, element), (None, element)],
                    TargetLayoutContract::Default,
                    None,
                    None,
                    target,
                    cancellation,
                    mappings,
                    pending,
                )
                .map(Some)
            }
            RepresentationRole::PanicReport => scalar_mapping(
                self,
                ty,
                RepresentationRole::ScalarUsize,
                TargetScalarKind::Usize,
                target,
                cancellation,
                mappings,
                pending,
            )
            .map(Some),
            RepresentationRole::Future => {
                let pointer = self.codegen_opaque_pointer_type()?;

                self.codegen_aggregate_type(
                    ty,
                    [(None, pointer), (None, pointer)],
                    TargetLayoutContract::C,
                    None,
                    None,
                    target,
                    cancellation,
                    mappings,
                    pending,
                )
                .map(Some)
            }
            RepresentationRole::Task => scalar_mapping(
                self,
                ty,
                RepresentationRole::ScalarU64,
                TargetScalarKind::U64,
                target,
                cancellation,
                mappings,
                pending,
            )
            .map(Some),
            RepresentationRole::Result
            | RepresentationRole::RunResult
            | RepresentationRole::ConversionError => Ok(None),
            RepresentationRole::BooleanTrue
            | RepresentationRole::BooleanFalse
            | RepresentationRole::UnitValue
            | RepresentationRole::NoneValue => Err(CodegenPreparationError::UnresolvedType(ty)),
            RepresentationRole::Unit
            | RepresentationRole::Never
            | RepresentationRole::Atomic
            | RepresentationRole::RawPointer
            | RepresentationRole::DevicePointer
            | RepresentationRole::ScalarBool
            | RepresentationRole::ScalarChar
            | RepresentationRole::ScalarI8
            | RepresentationRole::ScalarI16
            | RepresentationRole::ScalarI32
            | RepresentationRole::ScalarI64
            | RepresentationRole::ScalarI128
            | RepresentationRole::ScalarU8
            | RepresentationRole::ScalarU16
            | RepresentationRole::ScalarU32
            | RepresentationRole::ScalarU64
            | RepresentationRole::ScalarU128
            | RepresentationRole::ScalarIsize
            | RepresentationRole::ScalarUsize
            | RepresentationRole::ScalarR16
            | RepresentationRole::ScalarR32
            | RepresentationRole::ScalarR64
            | RepresentationRole::ScalarR128
            | RepresentationRole::ScalarC32
            | RepresentationRole::ScalarC64
            | RepresentationRole::ScalarC128
            | RepresentationRole::ScalarC256 => Err(CodegenPreparationError::UnresolvedType(ty)),
        }
    }
}
