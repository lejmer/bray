use std::collections::BTreeSet;
use std::num::NonZeroU64;

use bray_binder::SymbolFactProvider;
use bray_symbols::{
    GenericSubstitutionId, NamedTypeSymbolId, StructFieldTypeFact, SymbolFactRequest, TypeData,
    TypeExpressionTemplate, TypeId, UnionPayloadFieldTypeFact,
};

use super::super::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

pub(super) fn aggregate_alignment(
    compilation: &Compilation,
    definition: NamedTypeSymbolId,
    substitution: GenericSubstitutionId,
    cancellation: &CancellationToken,
) -> Result<NonZeroU64, FactQueryError> {
    let values = compilation.semantic_value_store()?;

    let ty = values
        .intern_type(TypeData::Named {
            definition,
            substitution,
        })
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    alignment_of_type(compilation, ty, cancellation, &mut BTreeSet::new())
}

fn alignment_of_type(
    compilation: &Compilation,
    ty: TypeId,
    cancellation: &CancellationToken,
    pending: &mut BTreeSet<TypeId>,
) -> Result<NonZeroU64, FactQueryError> {
    cancellation.check()?;

    if !pending.insert(ty) {
        return Ok(NonZeroU64::MIN);
    }

    let values = compilation.semantic_value_store()?;

    let data = values
        .type_data(ty)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    let alignment = match data.as_ref() {
        TypeData::Named {
            definition,
            substitution,
        } => named_alignment(
            compilation,
            *definition,
            *substitution,
            cancellation,
            pending,
        )?,
        TypeData::Tuple(elements) => {
            maximum_alignment(compilation, elements, cancellation, pending)?
        }
        TypeData::Array { element, .. }
        | TypeData::Nullable(element)
        | TypeData::Generator(element) => {
            alignment_of_type(compilation, *element, cancellation, pending)?
        }
        TypeData::Slice(_)
        | TypeData::Borrow { .. }
        | TypeData::TraitView(_)
        | TypeData::OwnedIndirection { .. }
        | TypeData::Callable(_) => pointer_alignment(compilation),
        TypeData::Error
        | TypeData::TypeParameter(_)
        | TypeData::ContextualSelf(_)
        | TypeData::TypeValuedMemberProjection { .. } => NonZeroU64::MIN,
    };

    pending.remove(&ty);

    Ok(alignment)
}

fn named_alignment(
    compilation: &Compilation,
    definition: NamedTypeSymbolId,
    substitution: GenericSubstitutionId,
    cancellation: &CancellationToken,
    pending: &mut BTreeSet<TypeId>,
) -> Result<NonZeroU64, FactQueryError> {
    if let Some(role) = super::validation::compiler_known_representation(compilation, definition) {
        return Ok(representation_alignment(compilation, role));
    }

    let facts = compilation.binder_facts(cancellation)?;

    let representation =
        compilation.declared_type_representation_with_cancellation(definition, cancellation)?;

    let mut alignment = match definition {
        NamedTypeSymbolId::Struct(structure) => {
            let structure = facts
                .structure(structure)
                .map_err(super::super::binder::binder_fact_error)?
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let mut alignments = Vec::with_capacity(structure.fields().len());

            for field in structure.fields() {
                let field = facts
                    .symbol_fact(SymbolFactRequest::<StructFieldTypeFact>::new(*field))
                    .map_err(super::super::binder::binder_fact_error)?;

                alignments.push(resolve_member_alignment(
                    compilation,
                    field.value(),
                    substitution,
                    cancellation,
                    pending,
                )?);
            }

            alignments.into_iter().max().unwrap_or(NonZeroU64::MIN)
        }
        NamedTypeSymbolId::Union(union) => {
            let union = facts
                .union(union)
                .map_err(super::super::binder::binder_fact_error)?
                .ok_or(FactQueryError::InfrastructureFailure)?;

            let mut alignment = representation
                .value()
                .union_tag_type()
                .map(|tag| alignment_of_type(compilation, tag, cancellation, pending))
                .transpose()?
                .unwrap_or(NonZeroU64::MIN);

            for variant in union.variants() {
                let variant = facts
                    .union_variant(*variant)
                    .map_err(super::super::binder::binder_fact_error)?
                    .ok_or(FactQueryError::InfrastructureFailure)?;

                for field in variant.payload_fields() {
                    let field = facts
                        .symbol_fact(SymbolFactRequest::<UnionPayloadFieldTypeFact>::new(*field))
                        .map_err(super::super::binder::binder_fact_error)?;

                    alignment = alignment.max(resolve_member_alignment(
                        compilation,
                        field.value(),
                        substitution,
                        cancellation,
                        pending,
                    )?);
                }
            }

            alignment
        }
    };

    if let Some(packing) = representation.value().packing().and_then(NonZeroU64::new) {
        alignment = alignment.min(packing);
    }

    if let Some(requested) = representation.value().alignment().and_then(NonZeroU64::new) {
        alignment = alignment.max(requested);
    }

    Ok(alignment)
}

fn resolve_member_alignment(
    compilation: &Compilation,
    template: &TypeExpressionTemplate,
    substitution: GenericSubstitutionId,
    cancellation: &CancellationToken,
    pending: &mut BTreeSet<TypeId>,
) -> Result<NonZeroU64, FactQueryError> {
    let checked = compilation
        .checked_constant_terms_for_templates_with_cancellation([template], cancellation)?;

    let Some(ty) = bray_checker::resolve_type_expression_template(
        compilation.semantic_value_store()?,
        template,
        checked.value(),
    )
    .map_err(FactQueryError::CheckerInfrastructure)?
    else {
        return Ok(NonZeroU64::MIN);
    };

    let ty = compilation
        .semantic_value_store()?
        .substitute_type(ty, substitution)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    alignment_of_type(compilation, ty, cancellation, pending)
}

fn maximum_alignment(
    compilation: &Compilation,
    types: &[TypeId],
    cancellation: &CancellationToken,
    pending: &mut BTreeSet<TypeId>,
) -> Result<NonZeroU64, FactQueryError> {
    let mut maximum = NonZeroU64::MIN;

    for ty in types {
        maximum = maximum.max(alignment_of_type(compilation, *ty, cancellation, pending)?);
    }

    Ok(maximum)
}

fn representation_alignment(
    compilation: &Compilation,
    role: bray_compiler_known::RepresentationRole,
) -> NonZeroU64 {
    use bray_compiler_known::RepresentationRole;

    if let Some(scalar) = super::super::representation::target_scalar(role) {
        return compilation
            .selected_target()
            .target()
            .profile()
            .facts()
            .scalars()
            .alignment(scalar);
    }

    match role {
        RepresentationRole::RawPointer
        | RepresentationRole::DevicePointer
        | RepresentationRole::String
        | RepresentationRole::Result
        | RepresentationRole::RunResult
        | RepresentationRole::PanicReport
        | RepresentationRole::ConversionError
        | RepresentationRole::Future
        | RepresentationRole::Task => pointer_alignment(compilation),
        RepresentationRole::Unit
        | RepresentationRole::Never
        | RepresentationRole::BooleanTrue
        | RepresentationRole::BooleanFalse
        | RepresentationRole::UnitValue
        | RepresentationRole::NoneValue => NonZeroU64::MIN,
        RepresentationRole::ScalarBool
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
        | RepresentationRole::ScalarC256 => unreachable!("scalar roles return above"),
    }
}

fn pointer_alignment(compilation: &Compilation) -> NonZeroU64 {
    NonZeroU64::from(
        compilation
            .selected_target()
            .target()
            .profile()
            .machine()
            .pointer_alignment_bytes(),
    )
}

#[cfg(test)]
mod tests {
    use bray_symbols::NamedTypeSymbolId;

    use super::aggregate_alignment;
    use crate::CancellationToken;
    use crate::compilation::substitution::empty_substitution;
    use crate::test_support::{compilation_with_dependencies, encoded_semantic_dependency};

    #[test]
    fn imported_aggregate_alignment_uses_dependency_symbols() {
        let interface = bray_package_interface::test_support::encoded_semantic_test_interface();

        let compilation =
            compilation_with_dependencies("module app;", [encoded_semantic_dependency(&interface)]);

        let imported = compilation
            .imported_symbol_skeleton_result()
            .unwrap_or_else(|error| panic!("imported symbols must load: {error:?}"));

        let structure = imported
            .value()
            .as_ref()
            .and_then(|symbols| symbols.structures().first())
            .unwrap_or_else(|| panic!("test interface must contain a structure"));

        let definition = NamedTypeSymbolId::from(structure.id());

        let values = compilation
            .semantic_value_store()
            .unwrap_or_else(|error| panic!("semantic values must load: {error:?}"));

        let substitution = empty_substitution(values, definition.into_any())
            .unwrap_or_else(|error| panic!("empty substitution must intern: {error:?}"));

        let alignment = aggregate_alignment(
            &compilation,
            definition,
            substitution,
            &CancellationToken::new(),
        )
        .unwrap_or_else(|error| panic!("imported aggregate alignment must resolve: {error:?}"));

        assert_eq!(alignment, std::num::NonZeroU64::MIN);
    }
}
