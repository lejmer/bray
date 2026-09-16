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
    pub(super) fn type_representation(&self, ty: TypeId) -> Option<RepresentationRole> {
        let data = self.input.semantic_values().type_data(ty);

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

        role
    }

    pub(super) fn named_type_arguments(&self, ty: TypeId) -> Vec<TypeId> {
        let data = self.input.semantic_values().type_data(ty);

        let TypeData::Named { substitution, .. } = data.as_ref() else {
            panic!("lowering contract violation: type {ty:?} must be a named type");
        };

        let substitution = self
            .input
            .semantic_values()
            .generic_substitution_data(*substitution);

        substitution
            .bindings()
            .iter()
            .filter_map(|binding| match binding.argument() {
                GenericArgument::Type(ty) => Some(ty),
                GenericArgument::Constant(_) => None,
            })
            .collect()
    }

    pub(super) fn representation_type(
        &self,
        role: RepresentationRole,
    ) -> Result<TypeId, LoweringError> {
        let symbol = self
            .input
            .available_compiler_known_symbols()
            .representation_symbol::<StructSymbolId>(role)
            .unwrap_or_else(|| panic!("lowering contract violation: MissingRepresentation {value:?}", value = role));

        self.input
            .semantic_values()
            .intern_non_generic_named_type(NamedTypeSymbolId::Struct(symbol))
            .map_err(LoweringError::from)
    }

    pub(super) fn result_representation(&self) -> ResultRepresentation {
        let representation = self
            .input
            .available_compiler_known_symbols()
            .result_representation()
            .unwrap_or_else(|| {
                panic!("lowering contract violation: compiler-known Result representation is unavailable")
            });

        ResultRepresentation {
            success_variant: representation.success_variant(),
            success_field: representation.success_field(),
            error_variant: representation.error_variant(),
            error_field: representation.error_field(),
        }
    }

    pub(super) fn ordering_representation(&self) -> Result<OrderingRepresentation, LoweringError> {
        let representation = self
            .input
            .available_compiler_known_symbols()
            .ordering_representation()
            .unwrap_or_else(|| {
                panic!("lowering contract violation: compiler-known Ordering representation is unavailable")
            });

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

    pub(super) fn run_result_representation(&self) -> RunResultRepresentation {
        let representation = self
            .input
            .available_compiler_known_symbols()
            .run_result_representation()
            .unwrap_or_else(|| {
                panic!("lowering contract violation: compiler-known RunResult representation is unavailable")
            });

        RunResultRepresentation {
            completed_variant: representation.completed_variant(),
            completed_field: representation.completed_field(),
            panicked_variant: representation.panicked_variant(),
            panicked_field: representation.panicked_field(),
            cancelled_variant: representation.cancelled_variant(),
        }
    }
}
