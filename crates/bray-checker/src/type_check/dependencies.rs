use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    GenericArgument, GenericOwnerId, GenericParameterSymbolId, GenericSubstitutionData,
    NamedTypeSymbolId, StructSymbolId, TypeData, TypeId,
};

use crate::{CheckerInfrastructureError, CheckerRequestContext, UnitCheckRequest};

pub(super) struct ExpressionTypeDependencies {
    pub(super) error: TypeId,
    pub(super) unit: TypeId,
    pub(super) never: TypeId,
    pub(super) boolean: TypeId,
    // TODO(checker): Remove this expectation when character literal checking uses this dependency.
    #[expect(dead_code, reason = "used by character literal checking")]
    pub(super) character: TypeId,
    pub(super) string: TypeId,
    // TODO(checker): Remove this expectation when integer literal adaptation uses this dependency.
    #[expect(dead_code, reason = "used by integer literal adaptation")]
    pub(super) i32: TypeId,
    // TODO(checker): Remove this expectation when real literal adaptation uses this dependency.
    #[expect(dead_code, reason = "used by real literal adaptation")]
    pub(super) r64: TypeId,
    // TODO(checker): Remove this expectation when complex literal adaptation uses this dependency.
    #[expect(dead_code, reason = "used by complex literal adaptation")]
    pub(super) c128: TypeId,
    pub(super) usize: TypeId,
}

impl ExpressionTypeDependencies {
    pub(super) fn new<C>(
        request: UnitCheckRequest<'_, C>,
    ) -> Result<Self, CheckerInfrastructureError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        let error = request
            .semantic_values()
            .intern_type(TypeData::Error)
            .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;
        let unit = representation_type(request, RepresentationRole::Unit)?;
        let never = representation_type(request, RepresentationRole::Never)?;
        let boolean = representation_type(request, RepresentationRole::ScalarBool)?;
        let character = representation_type(request, RepresentationRole::ScalarChar)?;
        let string = representation_type(request, RepresentationRole::String)?;
        let i32 = representation_type(request, RepresentationRole::ScalarI32)?;
        let r64 = representation_type(request, RepresentationRole::ScalarR64)?;
        let c128 = representation_type(request, RepresentationRole::ScalarC128)?;
        let usize = representation_type(request, RepresentationRole::ScalarUsize)?;

        Ok(Self {
            error,
            unit,
            never,
            boolean,
            character,
            string,
            i32,
            r64,
            c128,
            usize,
        })
    }
}

fn representation_type<C>(
    request: UnitCheckRequest<'_, C>,
    role: RepresentationRole,
) -> Result<TypeId, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    let Some(definition) = request
        .available_compiler_known_symbols()
        .representation_symbol::<StructSymbolId>(role)
    else {
        return Err(CheckerInfrastructureError::CompilerKnownRepresentationUnavailable { role });
    };

    let definition = NamedTypeSymbolId::Struct(definition);
    let Some(owner) = GenericOwnerId::try_new(definition.into_any()) else {
        return Err(CheckerInfrastructureError::SemanticValueUnavailable);
    };

    let substitution = GenericSubstitutionData::try_new(
        owner,
        std::iter::empty::<GenericParameterSymbolId>(),
        std::iter::empty::<GenericArgument>(),
    )
    .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    let substitution = request
        .semantic_values()
        .intern_generic_substitution(substitution)
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)?;

    request
        .semantic_values()
        .intern_type(TypeData::Named {
            definition,
            substitution,
        })
        .map_err(|_| CheckerInfrastructureError::SemanticValueUnavailable)
}
