use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use bray_codegen::{
    CodegenCallableSignature, CodegenFieldLayout, CodegenParameterMapping, CodegenResultMapping,
    CodegenTarget, CodegenTypeKind, CodegenTypeMapping, CodegenUnionVariantLayout,
    CodegenValueAttribute, TargetAddressSpaceKind,
};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    BorrowKind, CallableAbi, GenericSubstitutionId, NamedTypeSymbolId, StructSymbolId, TypeData,
    TypeId,
};
use bray_target::{TargetAtomicRepresentation, TargetLayoutContract, TargetValueLayout};

use super::super::super::CodegenPreparationError;
use super::super::super::Compilation;
use super::super::super::substitution::named_type;
use super::support::{
    align_to, atomic_representation_for_type, atomic_storage_is_padding_free,
    ensure_target_alignment, indirect_abi_value, indirect_parameter_kind, packed_alignment,
    pointer_mapping, signature_types, sized_layout, target_layout_contract,
};
use crate::compilation::{ProductDataKind, ProductQueryContext, ProductQueryFailure};
use crate::fact::{CancellationToken, FactQueryError};

impl Compilation {
    #[expect(
        clippy::too_many_arguments,
        reason = "union realization keeps checked representation and recursive mapping state explicit"
    )]
    pub(super) fn codegen_union_type(
        &self,
        ty: TypeId,
        union: bray_symbols::UnionSymbolId,
        substitution: GenericSubstitutionId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        let representation = self.declared_type_representation_with_cancellation(
            NamedTypeSymbolId::Union(union),
            cancellation,
        )?;

        let bray_symbols::DeclaredStorageShape::Union(storage) = representation.value().storage()
        else {
            return Err(CodegenPreparationError::UnresolvedType(ty));
        };

        let tag = representation.value().union_tag_type();

        if let Some(tag) = tag {
            self.codegen_type(tag, target, cancellation, mappings, pending)?;
        }

        let tag_layout = tag.map(|tag| sized_layout(mappings, tag)).transpose()?;

        let packing = representation.value().packing().and_then(NonZeroU64::new);

        let tag_alignment = tag_layout
            .map(|layout| packed_alignment(layout.alignment(), packing))
            .unwrap_or(NonZeroU64::MIN);

        let mut payload_alignment = NonZeroU64::MIN;
        let mut payload_size = 0_u64;
        let mut variants = Vec::with_capacity(storage.len());

        for variant in storage.iter() {
            let mut offset = 0_u64;
            let mut alignment = NonZeroU64::MIN;
            let mut fields = Vec::with_capacity(variant.members().len());

            for field in variant.members() {
                let field_ty = self.resolve_codegen_type(field.ty(), substitution, cancellation)?;

                self.codegen_type(field_ty, target, cancellation, mappings, pending)?;

                let field_layout = sized_layout(mappings, field_ty)?;

                let field_alignment = packed_alignment(field_layout.alignment(), packing);

                alignment = alignment.max(field_alignment);

                offset = align_to(offset, field_alignment)
                    .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

                fields.push(CodegenFieldLayout::new(
                    field.field().map(bray_ir::MirFieldReference::UnionPayload),
                    field_ty,
                    offset,
                ));

                offset = offset
                    .checked_add(field_layout.size())
                    .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;
            }

            let size =
                align_to(offset, alignment).ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

            payload_alignment = payload_alignment.max(alignment);
            payload_size = payload_size.max(size);
            variants.push((variant.variant(), fields));
        }

        let payload_offset = align_to(
            tag_layout.map_or(0, TargetValueLayout::size),
            payload_alignment,
        )
        .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

        let variants = variants
            .into_iter()
            .map(|(variant, fields)| {
                let fields = fields
                    .into_iter()
                    .map(|field| {
                        let offset = payload_offset
                            .checked_add(field.offset_bytes())
                            .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

                        Ok(CodegenFieldLayout::new(
                            field.reference(),
                            field.ty(),
                            offset,
                        ))
                    })
                    .collect::<Result<Vec<_>, CodegenPreparationError>>()?;

                // Representation binding_context are shared. Codegen mappings own exact tag magnitudes.
                if representation.value().is_tagless_union() {
                    Ok(CodegenUnionVariantLayout::untagged(variant, fields))
                } else {
                    let tag = representation
                        .value()
                        .union_tags()
                        .iter()
                        .find(|tag| tag.variant() == variant)
                        .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

                    Ok(CodegenUnionVariantLayout::new(
                        variant,
                        tag.value().clone(),
                        fields,
                    ))
                }
            })
            .collect::<Result<Vec<_>, CodegenPreparationError>>()?;

        let mut alignment = tag_alignment.max(payload_alignment);

        if let Some(requested) = representation.value().alignment().and_then(NonZeroU64::new) {
            alignment = alignment.max(requested);
        }

        ensure_target_alignment(ty, alignment, target)?;

        let size = payload_offset
            .checked_add(payload_size)
            .and_then(|size| align_to(size, alignment))
            .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

        Ok(CodegenTypeMapping::new(
            ty,
            TargetValueLayout::new(
                size,
                alignment,
                target_layout_contract(representation.value().layout()),
            ),
            match tag {
                Some(tag) => CodegenTypeKind::union(tag, variants),
                None => CodegenTypeKind::untagged_union(variants),
            },
        ))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "indirection realization keeps boundary metadata and recursive mapping state explicit"
    )]
    pub(super) fn codegen_indirection_type(
        &self,
        ty: TypeId,
        pointee: TypeId,
        borrow: Option<BorrowKind>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        let values = self.semantic_value_store()?;

        let pointee_data = values.type_data(pointee);

        let fields = match pointee_data.as_ref() {
            TypeData::Named { definition, .. }
                if super::super::super::foreign::compiler_known_representation(
                    self,
                    *definition,
                ) == Some(RepresentationRole::String) =>
            {
                self.codegen_type(pointee, target, cancellation, mappings, pending)?;

                return Ok(pointer_mapping(
                    ty,
                    pointee,
                    target,
                    TargetAddressSpaceKind::Default,
                ));
            }
            TypeData::Slice(element) => {
                self.codegen_type(pointee, target, cancellation, mappings, pending)?;

                let data = self.indirection_metadata_pointer(
                    *element,
                    borrow.unwrap_or(BorrowKind::Mutable),
                )?;

                let length = self.compiler_known_type(RepresentationRole::ScalarUsize)?;

                vec![data, length]
            }
            TypeData::TraitView(_) => {
                self.codegen_type(pointee, target, cancellation, mappings, pending)?;

                let metadata = self.compiler_known_type(RepresentationRole::ScalarUsize)?;

                let pointer = self.indirection_metadata_pointer(metadata, BorrowKind::Shared)?;

                vec![pointer, pointer]
            }
            _ => {
                return Ok(pointer_mapping(
                    ty,
                    pointee,
                    target,
                    TargetAddressSpaceKind::Default,
                ));
            }
        };

        self.codegen_aggregate_type(
            ty,
            fields.into_iter().map(|field| (None, field)),
            TargetLayoutContract::Default,
            None,
            None,
            target,
            cancellation,
            mappings,
            pending,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "generator realization shares recursive mapping state with element and metadata types"
    )]
    pub(super) fn codegen_generator_type(
        &self,
        ty: TypeId,
        element: TypeId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        self.codegen_type(element, target, cancellation, mappings, pending)?;

        if mappings
            .get(&element)
            .is_none_or(|mapping| mapping.layout().is_none())
        {
            return Err(CodegenPreparationError::UnsizedTypeByValue(element));
        }

        let data = self.indirection_metadata_pointer(element, BorrowKind::Mutable)?;
        let length = self.compiler_known_type(RepresentationRole::ScalarUsize)?;

        self.codegen_aggregate_type(
            ty,
            [data, length, length]
                .into_iter()
                .map(|field| (None, field)),
            TargetLayoutContract::Default,
            None,
            None,
            target,
            cancellation,
            mappings,
            pending,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "nullable realization shares recursive mapping state with tag and payload types"
    )]
    pub(super) fn codegen_nullable_type(
        &self,
        ty: TypeId,
        element: TypeId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        let present = self.compiler_known_type(RepresentationRole::ScalarBool)?;

        self.codegen_aggregate_type(
            ty,
            [(None, present), (None, element)],
            TargetLayoutContract::Default,
            None,
            None,
            target,
            cancellation,
            mappings,
            pending,
        )
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "string realization shares recursive mapping state with data and length types"
    )]
    pub(super) fn codegen_string_type(
        &self,
        ty: TypeId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        let byte = self.compiler_known_type(RepresentationRole::ScalarU8)?;
        let data = self.indirection_metadata_pointer(byte, BorrowKind::Shared)?;
        let owner = data;
        let length = self.compiler_known_type(RepresentationRole::ScalarUsize)?;

        self.codegen_aggregate_type(
            ty,
            [(None, data), (None, owner), (None, length)],
            TargetLayoutContract::Default,
            None,
            None,
            target,
            cancellation,
            mappings,
            pending,
        )
        .map(|mapping| mapping.with_behavior(Some(bray_codegen::CodegenTypeBehavior::String)))
    }

    pub(super) fn compiler_known_type(
        &self,
        role: RepresentationRole,
    ) -> Result<TypeId, FactQueryError> {
        let definition = self
            .available_compiler_known_symbols()
            .representation_symbol::<StructSymbolId>(role)
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::CompilerKnownRepresentation(role),
                    ProductDataKind::CompilerKnownRepresentation,
                )
            })?;

        named_type(
            self.semantic_value_store()?,
            NamedTypeSymbolId::Struct(definition),
        )
    }

    pub(super) fn indirection_metadata_pointer(
        &self,
        target: TypeId,
        kind: BorrowKind,
    ) -> Result<TypeId, FactQueryError> {
        self.semantic_value_store()?
            .intern_type(TypeData::Borrow { kind, target })
            .map_err(FactQueryError::SemanticValueStore)
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "recursive aggregate realization keeps layout and cycle state explicit"
    )]
    pub(super) fn codegen_aggregate_type(
        &self,
        ty: TypeId,
        fields: impl IntoIterator<Item = (Option<bray_ir::MirFieldReference>, TypeId)>,
        contract: TargetLayoutContract,
        requested_alignment: Option<u64>,
        packing: Option<u64>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        let mut offset = 0_u64;
        let mut alignment = NonZeroU64::MIN;
        let mut layouts = Vec::new();
        let packing = packing.and_then(NonZeroU64::new);

        for (reference, field) in fields {
            self.codegen_type(field, target, cancellation, mappings, pending)?;

            let field_layout = mappings
                .get(&field)
                .and_then(CodegenTypeMapping::layout)
                .ok_or(CodegenPreparationError::UnsizedTypeByValue(field))?;

            let field_alignment = packed_alignment(field_layout.alignment(), packing);

            alignment = alignment.max(field_alignment);

            offset = align_to(offset, field_alignment)
                .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

            layouts.push(CodegenFieldLayout::new(reference, field, offset));

            offset = offset
                .checked_add(field_layout.size())
                .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;
        }

        if let Some(requested) = requested_alignment.and_then(NonZeroU64::new) {
            alignment = alignment.max(requested);
        }

        ensure_target_alignment(ty, alignment, target)?;

        let size =
            align_to(offset, alignment).ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

        Ok(CodegenTypeMapping::new(
            ty,
            TargetValueLayout::new(size, alignment, contract),
            CodegenTypeKind::aggregate(layouts),
        ))
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "flexible aggregate realization keeps layout and recursive mapping state explicit"
    )]
    pub(super) fn codegen_flexible_aggregate_type(
        &self,
        ty: TypeId,
        fields: &[(Option<bray_ir::MirFieldReference>, TypeId)],
        element: TypeId,
        contract: TargetLayoutContract,
        requested_alignment: Option<u64>,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenTypeMapping, CodegenPreparationError> {
        let mut offset = 0_u64;
        let mut alignment = NonZeroU64::MIN;
        let mut layouts = Vec::with_capacity(fields.len());

        for (reference, field) in fields {
            self.codegen_type(*field, target, cancellation, mappings, pending)?;

            let field_layout = sized_layout(mappings, *field)?;

            alignment = alignment.max(field_layout.alignment());

            offset = align_to(offset, field_layout.alignment())
                .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

            layouts.push(CodegenFieldLayout::new(*reference, *field, offset));

            offset = offset
                .checked_add(field_layout.size())
                .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;
        }

        self.codegen_type(element, target, cancellation, mappings, pending)?;

        let element_layout = sized_layout(mappings, element)?;
        alignment = alignment.max(element_layout.alignment());

        if let Some(requested) = requested_alignment.and_then(NonZeroU64::new) {
            alignment = alignment.max(requested);
        }

        ensure_target_alignment(ty, alignment, target)?;

        let offset = align_to(offset, element_layout.alignment())
            .ok_or(CodegenPreparationError::LayoutOverflow(ty))?;

        Ok(CodegenTypeMapping::new(
            ty,
            TargetValueLayout::new(offset, alignment, contract),
            CodegenTypeKind::aggregate(layouts),
        )
        .with_behavior(Some(bray_codegen::CodegenTypeBehavior::FlexibleAggregate {
            element,
            offset,
        })))
    }

    pub(in crate::compilation::product) fn resolve_codegen_type(
        &self,
        template: &bray_symbols::TypeExpressionTemplate,
        substitution: GenericSubstitutionId,
        cancellation: &CancellationToken,
    ) -> Result<TypeId, FactQueryError> {
        let constants =
            self.checked_constant_terms_for_templates_with_cancellation([template], cancellation)?;

        let ty = bray_checker::resolve_type_expression_template(
            self.semantic_value_store()?,
            template,
            constants.value(),
        )
        .map_err(FactQueryError::from)?;

        let Some(ty) = ty else {
            return Err(ProductQueryFailure::missing(
                ProductQueryContext::Substitution(substitution),
                ProductDataKind::ResolvedType,
            )
            .into());
        };

        self.semantic_value_store()?
            .substitute_type(ty, substitution)
            .map_err(FactQueryError::SemanticValueStore)
    }

    pub(in crate::compilation) fn plain_storage_atomic_representation(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<Option<TargetAtomicRepresentation>, FactQueryError> {
        let Ok(target) = self.selected_target().target().codegen_target() else {
            return Ok(None);
        };

        let mut mappings = BTreeMap::new();
        let mut pending = BTreeSet::new();

        match self.codegen_type(ty, &target, cancellation, &mut mappings, &mut pending) {
            Ok(()) => {}
            Err(CodegenPreparationError::Query(error)) => return Err(error),
            Err(_) => return Ok(None),
        }

        let size = mappings
            .get(&ty)
            .and_then(CodegenTypeMapping::layout)
            .filter(|_| atomic_storage_is_padding_free(ty, &mappings))
            .map(TargetValueLayout::size);

        Ok(size.and_then(TargetAtomicRepresentation::for_storage_size))
    }

    pub(in crate::compilation) fn atomic_representation_for_type(
        &self,
        ty: TypeId,
        cancellation: &CancellationToken,
    ) -> Result<Option<TargetAtomicRepresentation>, FactQueryError> {
        let Ok(target) = self.selected_target().target().codegen_target() else {
            return Ok(None);
        };

        match atomic_representation_for_type(self, ty, &target, cancellation) {
            Ok(Some(representation))
                if target
                    .profile()
                    .properties()
                    .atomics()
                    .representation(representation)
                    .operations()
                    .any() =>
            {
                Ok(Some(representation))
            }
            Ok(_) => Ok(None),
            Err(CodegenPreparationError::Query(error)) => Err(error),
            Err(_) => Ok(None),
        }
    }

    pub(super) fn codegen_signature_types(
        &self,
        signature: &CodegenCallableSignature,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<(), CodegenPreparationError> {
        for ty in signature_types(signature) {
            self.codegen_type(ty, target, cancellation, mappings, pending)?;
        }

        Ok(())
    }

    pub(super) fn classify_codegen_signature(
        &self,
        signature: CodegenCallableSignature,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenCallableSignature, CodegenPreparationError> {
        let mut parameters = Vec::with_capacity(signature.parameters().len());

        for parameter in signature.parameters() {
            let ty = match parameter {
                CodegenParameterMapping::Direct { ty, .. } => *ty,
                CodegenParameterMapping::Ignore | CodegenParameterMapping::Indirect { .. } => {
                    return Err(CodegenPreparationError::InvalidAbiMapping);
                }
            };

            parameters.push(self.classify_codegen_parameter(
                ty,
                signature.abi(),
                target,
                cancellation,
                mappings,
                pending,
            )?);
        }

        let result = match signature.result() {
            CodegenResultMapping::Void => CodegenResultMapping::Void,
            CodegenResultMapping::Direct { ty, .. } => self.classify_codegen_result(
                *ty,
                signature.abi(),
                target,
                cancellation,
                mappings,
                pending,
            )?,
            CodegenResultMapping::Indirect { .. } => {
                return Err(CodegenPreparationError::InvalidAbiMapping);
            }
        };

        let classified = CodegenCallableSignature::new(
            parameters,
            result,
            signature.abi(),
            signature.is_variadic(),
        );

        if signature.has_panic_report_context() {
            Ok(classified.with_panic_report_context())
        } else {
            Ok(classified)
        }
    }

    pub(super) fn classify_codegen_parameter(
        &self,
        ty: TypeId,
        abi: CallableAbi,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenParameterMapping, CodegenPreparationError> {
        let mapping = mappings
            .get(&ty)
            .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

        let layout = mapping
            .layout()
            .ok_or(CodegenPreparationError::UnsizedTypeByValue(ty))?;

        if layout.size() == 0 {
            return Ok(CodegenParameterMapping::Ignore);
        }

        if !indirect_abi_value(abi, mapping.kind(), layout, target, mappings) {
            return Ok(CodegenParameterMapping::direct(ty, None, []));
        }

        let pointer = self.indirection_metadata_pointer(ty, BorrowKind::Shared)?;

        self.codegen_type(pointer, target, cancellation, mappings, pending)?;

        Ok(CodegenParameterMapping::indirect(
            pointer,
            ty,
            indirect_parameter_kind(abi, target),
            layout.alignment(),
            [CodegenValueAttribute::NonNull],
        ))
    }

    pub(super) fn classify_codegen_result(
        &self,
        ty: TypeId,
        abi: CallableAbi,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<CodegenResultMapping, CodegenPreparationError> {
        let mapping = mappings
            .get(&ty)
            .ok_or(CodegenPreparationError::UnresolvedType(ty))?;

        let layout = mapping
            .layout()
            .ok_or(CodegenPreparationError::UnsizedTypeByValue(ty))?;

        if layout.size() == 0 {
            return Ok(CodegenResultMapping::Void);
        }

        if !indirect_abi_value(abi, mapping.kind(), layout, target, mappings) {
            return Ok(CodegenResultMapping::direct(ty, None, []));
        }

        let pointer = self.indirection_metadata_pointer(ty, BorrowKind::Mutable)?;

        self.codegen_type(pointer, target, cancellation, mappings, pending)?;

        Ok(CodegenResultMapping::indirect(
            pointer,
            ty,
            layout.alignment(),
            [
                CodegenValueAttribute::NoAlias,
                CodegenValueAttribute::NonNull,
            ],
        ))
    }
}
