use bray_checker::{TargetAbiValue, TargetAggregateAbi};
use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    DeclaredLayoutMode, GenericSubstitutionId, NamedTypeSymbolId, TypeData, TypeExpressionTemplate,
    TypeId,
};
use bray_target::TargetLayoutContract;

use super::super::super::Compilation;
use crate::fact::{CancellationToken, FactQueryError};

pub(in crate::compilation) fn compiler_known_representation(
    compilation: &Compilation,
    definition: NamedTypeSymbolId,
) -> Option<RepresentationRole> {
    let available = compilation.available_compiler_known_symbols();

    match definition {
        NamedTypeSymbolId::Struct(definition) => available.symbol_representation(definition),
        NamedTypeSymbolId::Union(definition) => available.symbol_representation(definition),
    }
}

pub(super) fn target_abi_value(
    compilation: &Compilation,
    template: &TypeExpressionTemplate,
    cancellation: &CancellationToken,
) -> Result<Option<TargetAbiValue>, FactQueryError> {
    let checked = compilation
        .checked_constant_terms_for_templates_with_cancellation([template], cancellation)?;

    let Some(ty) = bray_checker::resolve_type_expression_template(
        compilation.semantic_value_store()?,
        template,
        checked.value(),
    )
    .map_err(FactQueryError::CheckerInfrastructure)?
    else {
        return Ok(None);
    };

    target_abi_value_from_type(compilation, ty, cancellation)
}

pub(in crate::compilation) fn target_abi_value_from_type(
    compilation: &Compilation,
    ty: TypeId,
    cancellation: &CancellationToken,
) -> Result<Option<TargetAbiValue>, FactQueryError> {
    let data = compilation
        .semantic_value_store()?
        .type_data(ty)
        .map_err(|_| FactQueryError::InfrastructureFailure)?;

    target_abi_value_from_data(compilation, data.as_ref(), cancellation)
}

fn target_abi_value_from_data(
    compilation: &Compilation,
    data: &TypeData,
    cancellation: &CancellationToken,
) -> Result<Option<TargetAbiValue>, FactQueryError> {
    Ok(match data {
        TypeData::Named {
            definition,
            substitution,
        } => {
            return target_abi_value_from_named(
                compilation,
                *definition,
                *substitution,
                cancellation,
            );
        }
        TypeData::Callable(callable) => Some(TargetAbiValue::Callable(callable.abi())),
        TypeData::Error
        | TypeData::Tuple(_)
        | TypeData::Array { .. }
        | TypeData::FlexibleArray(_)
        | TypeData::Slice(_)
        | TypeData::Generator(_)
        | TypeData::Nullable(_)
        | TypeData::Borrow { .. }
        | TypeData::TraitView(_)
        | TypeData::OwnedIndirection { .. }
        | TypeData::TypeParameter(_)
        | TypeData::ContextualSelf(_)
        | TypeData::TypeValuedMemberProjection { .. } => None,
    })
}

fn target_abi_value_from_named(
    compilation: &Compilation,
    definition: NamedTypeSymbolId,
    substitution: GenericSubstitutionId,
    cancellation: &CancellationToken,
) -> Result<Option<TargetAbiValue>, FactQueryError> {
    if let Some(role) = compiler_known_representation(compilation, definition) {
        if role == RepresentationRole::RawPointer {
            return Ok(Some(TargetAbiValue::RawPointer));
        }

        return Ok(
            super::super::super::representation::target_scalar(role).map(TargetAbiValue::Scalar)
        );
    }

    let representation =
        compilation.declared_type_representation_with_cancellation(definition, cancellation)?;

    let contract = match representation.value().layout() {
        DeclaredLayoutMode::Default => TargetLayoutContract::Default,
        DeclaredLayoutMode::Stable => TargetLayoutContract::Stable,
        DeclaredLayoutMode::C => TargetLayoutContract::C,
        DeclaredLayoutMode::Transparent => TargetLayoutContract::Transparent,
    };

    let Some(alignment) = super::super::layout::aggregate_alignment(
        compilation,
        definition,
        substitution,
        cancellation,
    )?
    else {
        return Ok(None);
    };

    Ok(Some(TargetAbiValue::Aggregate(TargetAggregateAbi::new(
        contract, alignment,
    ))))
}
