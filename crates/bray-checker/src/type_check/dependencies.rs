use bray_compiler_known::{NumericRepresentationKind, RepresentationRole};
use bray_symbols::{TypeData, TypeId};

use crate::representation::{representation_type, type_representation};
use crate::{CheckerInfrastructureError, CheckerRequestContext, CheckerUnitView};

pub(super) struct ExpressionTypeDependencies {
    pub(super) box_storage_policies:
        std::collections::BTreeMap<bray_bound_tree::BoundExpressionId, TypeId>,
    pub(super) error: TypeId,
    pub(super) unit: TypeId,
    pub(super) never: TypeId,
    pub(super) boolean: TypeId,
    pub(super) character: TypeId,
    pub(super) string: TypeId,
    pub(super) i32: TypeId,
    pub(super) r64: TypeId,
    pub(super) c128: TypeId,
}

impl ExpressionTypeDependencies {
    pub(super) fn new<C>(
        request: CheckerUnitView<'_, C>,
    ) -> Result<Self, CheckerInfrastructureError>
    where
        C: CheckerRequestContext + ?Sized,
    {
        let error = request
            .semantic_values()
            .intern_type(TypeData::Error)
            .map_err(CheckerInfrastructureError::SemanticValueStore)?;

        let unit = representation_type(request, RepresentationRole::Unit)?;
        let never = representation_type(request, RepresentationRole::Never)?;
        let boolean = representation_type(request, RepresentationRole::ScalarBool)?;
        let character = representation_type(request, RepresentationRole::ScalarChar)?;
        let string = representation_type(request, RepresentationRole::String)?;
        let i32 = representation_type(request, RepresentationRole::ScalarI32)?;
        let r64 = representation_type(request, RepresentationRole::ScalarR64)?;
        let c128 = representation_type(request, RepresentationRole::ScalarC128)?;

        Ok(Self {
            box_storage_policies: std::collections::BTreeMap::new(),
            error,
            unit,
            never,
            boolean,
            character,
            string,
            i32,
            r64,
            c128,
        })
    }
}

pub(super) fn numeric_kind<C>(
    request: CheckerUnitView<'_, C>,
    ty: TypeId,
) -> Result<Option<NumericRepresentationKind>, CheckerInfrastructureError>
where
    C: CheckerRequestContext + ?Sized,
{
    Ok(type_representation(request, ty).and_then(RepresentationRole::numeric_kind))
}
