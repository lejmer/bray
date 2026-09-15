use std::collections::{BTreeMap, BTreeSet};
use std::num::NonZeroU64;

use bray_codegen::{CodegenTarget, CodegenTypeKind, CodegenTypeMapping, TargetAddressSpaceKind};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{GenericArgument, GenericSubstitutionId, TypeId};
use bray_target::{TargetLayoutContract, TargetScalarKind, TargetValueLayout};

use super::super::super::{CodegenPreparationError, Compilation};
use super::support::{
    atomic_representation_for_type, atomic_storage_role, pointer_mapping, scalar_mapping,
};
use crate::fact::CancellationToken;

impl Compilation {
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
            return self.codegen_indirect_pointer_type(
                ty,
                role,
                substitution,
                target,
                cancellation,
                mappings,
                pending,
            );
        }

        if role == RepresentationRole::Atomic {
            let values = self.semantic_value_store()?;

            let substitution = values.generic_substitution_data(substitution);

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

                Ok(Some(element_mapping.representation_for(ty)))
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
            RepresentationRole::PanicReport => {
                let u32 = self.codegen_representation_type(RepresentationRole::ScalarU32)?;
                let u64 = self.codegen_representation_type(RepresentationRole::ScalarU64)?;
                let usize = self.codegen_representation_type(RepresentationRole::ScalarUsize)?;
                let pointer = self.codegen_opaque_pointer_type()?;

                self.codegen_aggregate_type(
                    ty,
                    [
                        u32, u32, u32, u32, u64, u32, usize, usize, pointer, pointer, usize, usize,
                        usize, usize, pointer,
                    ]
                    .map(|field| (None, field)),
                    TargetLayoutContract::C,
                    None,
                    None,
                    target,
                    cancellation,
                    mappings,
                    pending,
                )
                .map(|mapping| {
                    Some(
                        mapping.with_behavior(Some(bray_codegen::CodegenTypeBehavior::PanicReport)),
                    )
                })
            }
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

    #[expect(
        clippy::too_many_arguments,
        reason = "pointer realization shares recursive mapping state with its caller"
    )]
    fn codegen_indirect_pointer_type(
        &self,
        ty: TypeId,
        role: RepresentationRole,
        substitution: GenericSubstitutionId,
        target: &CodegenTarget,
        cancellation: &CancellationToken,
        mappings: &mut BTreeMap<TypeId, CodegenTypeMapping>,
        pending: &mut BTreeSet<TypeId>,
    ) -> Result<Option<CodegenTypeMapping>, CodegenPreparationError> {
        let substitution = self
            .semantic_value_store()?
            .generic_substitution_data(substitution);

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

        let mapping = pointer_mapping(ty, pointee, target, address_space);

        // Publish the indirection before following its pointee so recursive pointer graphs
        // terminate here while non-recursive pointees still receive complete mappings.
        mappings.insert(ty, mapping.clone());

        if !pending.contains(&pointee)
            && let Err(error) = self.codegen_type(pointee, target, cancellation, mappings, pending)
        {
            mappings.remove(&ty);

            return Err(error);
        }

        Ok(Some(mapping))
    }
}
