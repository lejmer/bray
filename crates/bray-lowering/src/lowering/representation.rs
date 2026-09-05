use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    GenericArgument, NamedTypeSymbolId, StructSymbolId, TypeData, TypeId,
    UnionPayloadFieldSymbolId, UnionVariantSymbolId,
};

use super::LoweringError;
use super::lowerer::Lowerer;

#[derive(Clone, Copy)]
pub(super) struct ResultRepresentation {
    pub(super) success_variant: UnionVariantSymbolId,
    pub(super) success_field: UnionPayloadFieldSymbolId,
    pub(super) error_variant: UnionVariantSymbolId,
    pub(super) error_field: UnionPayloadFieldSymbolId,
}

#[derive(Clone, Copy)]
pub(super) struct RunResultRepresentation {
    pub(super) completed_variant: UnionVariantSymbolId,
    pub(super) completed_field: UnionPayloadFieldSymbolId,
    pub(super) panicked_variant: UnionVariantSymbolId,
    pub(super) panicked_field: UnionPayloadFieldSymbolId,
    pub(super) cancelled_variant: UnionVariantSymbolId,
}

#[derive(Clone, Copy)]
pub(super) struct OrderingRepresentation {
    pub(super) ty: TypeId,
    pub(super) less_variant: UnionVariantSymbolId,
    pub(super) greater_variant: UnionVariantSymbolId,
}

impl Lowerer<'_> {
    pub(super) fn integer_operand(
        &self,
        ty: TypeId,
        value: u64,
    ) -> Result<bray_ir::MirOperand, LoweringError> {
        let value = self.input.semantic_values().intern_constant_value(
            bray_symbols::ConstantValueData::new(
                ty,
                bray_symbols::ConstantValueKind::Integer(bray_symbols::IntegerConstant::from_u64(
                    value,
                )),
            ),
        )?;

        Ok(bray_ir::MirOperand::Constant { value, ty })
    }

    pub(super) fn type_representation(
        &self,
        ty: TypeId,
    ) -> Result<Option<RepresentationRole>, LoweringError> {
        let data = self.input.semantic_values().type_data(ty)?;

        let role = match data.as_ref() {
            TypeData::Named { definition, .. } => match definition {
                NamedTypeSymbolId::Struct(symbol) => self
                    .input
                    .available_compiler_known_symbols()
                    .symbol_representation(*symbol),
                NamedTypeSymbolId::Union(symbol) => self
                    .input
                    .available_compiler_known_symbols()
                    .symbol_representation(*symbol),
            },
            _ => None,
        };

        Ok(role)
    }

    pub(super) fn named_type_arguments(&self, ty: TypeId) -> Result<Vec<TypeId>, LoweringError> {
        let data = self.input.semantic_values().type_data(ty)?;

        let TypeData::Named { substitution, .. } = data.as_ref() else {
            return Err(LoweringError::SemanticValueUnavailable);
        };

        let substitution = self
            .input
            .semantic_values()
            .generic_substitution_data(*substitution)?;

        Ok(substitution
            .bindings()
            .iter()
            .filter_map(|binding| match binding.argument() {
                GenericArgument::Type(ty) => Some(ty),
                GenericArgument::Constant(_) => None,
            })
            .collect())
    }

    pub(super) fn representation_type(
        &self,
        role: RepresentationRole,
    ) -> Result<TypeId, LoweringError> {
        let symbol = self
            .input
            .available_compiler_known_symbols()
            .representation_symbol::<StructSymbolId>(role)
            .ok_or(LoweringError::MissingRepresentation(role))?;

        self.input
            .semantic_values()
            .intern_non_generic_named_type(NamedTypeSymbolId::Struct(symbol))
            .map_err(LoweringError::from)
    }

    pub(super) fn result_representation(&self) -> Result<ResultRepresentation, LoweringError> {
        let representation = self
            .input
            .available_compiler_known_symbols()
            .result_representation()
            .ok_or(LoweringError::SemanticValueUnavailable)?;

        Ok(ResultRepresentation {
            success_variant: representation.success_variant(),
            success_field: representation.success_field(),
            error_variant: representation.error_variant(),
            error_field: representation.error_field(),
        })
    }

    pub(super) fn ordering_representation(&self) -> Result<OrderingRepresentation, LoweringError> {
        let representation = self
            .input
            .available_compiler_known_symbols()
            .ordering_representation()
            .ok_or(LoweringError::SemanticValueUnavailable)?;

        let ty = self
            .input
            .semantic_values()
            .intern_non_generic_named_type(NamedTypeSymbolId::Union(representation.definition()))?;

        Ok(OrderingRepresentation {
            ty,
            less_variant: representation.less_variant(),
            greater_variant: representation.greater_variant(),
        })
    }

    pub(super) fn run_result_representation(
        &self,
    ) -> Result<RunResultRepresentation, LoweringError> {
        let representation = self
            .input
            .available_compiler_known_symbols()
            .run_result_representation()
            .ok_or(LoweringError::SemanticValueUnavailable)?;

        Ok(RunResultRepresentation {
            completed_variant: representation.completed_variant(),
            completed_field: representation.completed_field(),
            panicked_variant: representation.panicked_variant(),
            panicked_field: representation.panicked_field(),
            cancelled_variant: representation.cancelled_variant(),
        })
    }
}
