use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use bray_binder::BindingQueryContext;
use bray_codegen::{
    CodegenInstanceTypeMapping, CodegenParameterMapping, CodegenResultMapping, CodegenTarget,
    CodegenTypeKind, CodegenTypeMapping, TargetAddressSpaceKind,
};
use bray_diagnostics::DiagnosticBag;
use bray_ir::{MirPatternPredicate, MirTerminatorKind, MirUnit};
use bray_symbols::{GenericSubstitutionId, NamedTypeSymbolId, SelfTypeContext, TypeData, TypeId};
use bray_target::{TargetLayoutContract, TargetValueLayout};

use super::super::super::CodegenPreparationError;
use super::super::super::Compilation;
use super::super::super::checker::CompilationCheckerContext;
use super::super::super::substitution::substitution_for_owner;
use super::super::specialization::ConcreteCodegenInstance;
use super::contextual_self::{
    codegen_instance_contextual_self, implementation_subject, substitute_contextual_self,
};
use super::support::{
    callable_type_signature, closed_array_length, codegen_checker_error, pointer_layout,
    pointer_mapping, target_layout_contract,
};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    pub(super) fn active_union_referent_types(
        &self,
        unit: &MirUnit,
        instance: &ConcreteCodegenInstance,
        cancellation: &CancellationToken,
    ) -> Result<BTreeSet<TypeId>, CodegenPreparationError> {
        let values = self.semantic_value_store()?;
        let mut demanded = BTreeSet::new();

        for block in unit.blocks() {
            let MirTerminatorKind::PatternBranch {
                subject,
                predicate: MirPatternPredicate::ActiveUnionVariant(_),
                ..
            } = block.terminator().kind()
            else {
                continue;
            };

            let subject = unit
                .operand_type(subject)
                .expect("validated MIR pattern subject must have a type");

            let mut ty = self.concrete_codegen_type(
                subject,
                instance.substitution(),
                Some(instance),
                cancellation,
            )?;

            while let TypeData::Borrow { target, .. } = values.type_data(ty).as_ref() {
                demanded.insert(*target);
                ty = *target;
            }
        }

        Ok(demanded)
    }

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
                    let mapping = mapping
                        .representation_for(template)
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

        let contextual_self = instance
            .map(|instance| codegen_instance_contextual_self(&binding_context, instance))
            .transpose()?
            .flatten();

        let ty =
            substitute_contextual_self(binding_context.semantic_values(), ty, contextual_self)?;

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
            .map_err(FactQueryError::SemanticValueStore)?;

        let data = values.type_data(ty);

        match data.as_ref() {
            TypeData::ContextualSelf(SelfTypeContext::NamedType(definition)) => {
                let substitution =
                    substitution_for_owner(values, definition.into_any(), [substitution])?;

                values
                    .intern_type(TypeData::Named {
                        definition: *definition,
                        substitution,
                    })
                    .map_err(FactQueryError::SemanticValueStore)
            }
            TypeData::Borrow { kind, target } => {
                let target =
                    self.substitute_codegen_type(*target, Some(substitution), cancellation)?;

                values
                    .intern_type(TypeData::Borrow {
                        kind: *kind,
                        target,
                    })
                    .map_err(FactQueryError::SemanticValueStore)
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

        let data = values.type_data(ty);

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
                .map_err(FactQueryError::SemanticValueStore)?
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

        let data = values.type_data(ty);

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
            TypeData::FlexibleArray(_) => {
                return Err(CodegenPreparationError::UnsizedTypeByValue(ty));
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
                storage,
                target: pointee,
            } => {
                let pointee_data = values.type_data(*pointee);

                if matches!(
                    pointee_data.as_ref(),
                    TypeData::Slice(_) | TypeData::TraitView(_)
                ) {
                    self.codegen_indirection_type(
                        ty,
                        *pointee,
                        None,
                        target,
                        cancellation,
                        mappings,
                        pending,
                    )?
                } else {
                    self.codegen_type(*storage, target, cancellation, mappings, pending)?;

                    let policy = mappings
                        .get(storage)
                        .ok_or(CodegenPreparationError::UnresolvedType(*storage))?;

                    if policy.layout().is_none() {
                        return Err(CodegenPreparationError::UnsizedTypeByValue(*storage));
                    }

                    policy.representation_for(ty)
                }
            }
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
                let mut candidates = Vec::new();

                for mapping in mappings.values() {
                    let data = values.type_data(mapping.ty());

                    if matches!(
                        data.as_ref(),
                        TypeData::Named {
                            definition: candidate,
                            ..
                        } if candidate == definition
                    ) {
                        candidates.push(mapping);
                    }
                }

                let mut candidates = candidates.into_iter();

                let mapping = candidates
                    .next()
                    .cloned()
                    .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

                if candidates.any(|candidate| {
                    candidate.layout() != mapping.layout() || candidate.kind() != mapping.kind()
                }) {
                    return Err(CodegenPreparationError::UnresolvedType(ty));
                }

                mapping.representation_for(ty).with_backend_type(ty)
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

        if self
            .symbol_graph()?
            .compiler_known_provider()
            .heap_storage_policy()
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

                if representation.value().is_incomplete() {
                    return Ok(CodegenTypeMapping::new_unsized(ty, CodegenTypeKind::Opaque));
                }

                if let Some(size) = representation.value().opaque_size() {
                    let alignment = representation
                        .value()
                        .alignment()
                        .and_then(NonZeroU64::new)
                        .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

                    return Ok(CodegenTypeMapping::new(
                        ty,
                        TargetValueLayout::new(
                            size,
                            alignment,
                            target_layout_contract(representation.value().layout()),
                        ),
                        CodegenTypeKind::aggregate([]),
                    ));
                }

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

                if let Some((_, flexible)) = fields.last()
                    && let TypeData::FlexibleArray(element) =
                        self.semantic_value_store()?.type_data(*flexible).as_ref()
                {
                    return self.codegen_flexible_aggregate_type(
                        ty,
                        &fields[..fields.len().saturating_sub(1)],
                        *element,
                        target_layout_contract(representation.value().layout()),
                        representation.value().alignment(),
                        target,
                        cancellation,
                        mappings,
                        pending,
                    );
                }

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
}
