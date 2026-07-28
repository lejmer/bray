use bray_compiler_known::{CompilerKnownDeclarationKey, RepresentationRole};
use bray_symbols::{
    GenericArgument, NamedTypeSymbolId, StructSymbolId, TypeData, TypeId, UnionPayloadFieldSymbolId,
    UnionVariantSymbolId,
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

impl Lowerer<'_> {
    pub(super) fn type_representation(
        &self,
        ty: TypeId,
    ) -> Result<Option<RepresentationRole>, LoweringError> {
        let data = self
            .input
            .semantic_values()
            .type_data(ty)
            .map_err(|_| LoweringError::SemanticValueUnavailable)?;

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
        let data = self
            .input
            .semantic_values()
            .type_data(ty)
            .map_err(|_| LoweringError::SemanticValueUnavailable)?;

        let TypeData::Named { substitution, .. } = data.as_ref() else {
            return Err(LoweringError::SemanticValueUnavailable);
        };

        let substitution = self
            .input
            .semantic_values()
            .generic_substitution_data(*substitution)
            .map_err(|_| LoweringError::SemanticValueUnavailable)?;

        Ok(substitution
            .bindings()
            .iter()
            .filter_map(|binding| match binding.argument() {
                GenericArgument::Type(ty) => Some(ty),
                GenericArgument::Constant(_) => None,
            })
            .collect())
    }

    pub(super) fn panic_report_type(&self) -> Result<TypeId, LoweringError> {
        let symbol = self
            .input
            .available_compiler_known_symbols()
            .representation_symbol::<StructSymbolId>(RepresentationRole::PanicReport)
            .ok_or(LoweringError::MissingRepresentation(
                RepresentationRole::PanicReport,
            ))?;

        self.input
            .semantic_values()
            .intern_non_generic_named_type(NamedTypeSymbolId::Struct(symbol))
            .map_err(|_| LoweringError::SemanticValueUnavailable)
    }

    pub(super) fn result_representation(&self) -> Result<ResultRepresentation, LoweringError> {
        Ok(ResultRepresentation {
            success_variant: self.compiler_known_declaration("ResultVariant0Ok")?,
            success_field: self.compiler_known_declaration("ResultVariant0OkValue")?,
            error_variant: self.compiler_known_declaration("ResultVariant1Error")?,
            error_field: self.compiler_known_declaration("ResultVariant1ErrorError")?,
        })
    }

    pub(super) fn run_result_representation(
        &self,
    ) -> Result<RunResultRepresentation, LoweringError> {
        Ok(RunResultRepresentation {
            completed_variant: self.compiler_known_declaration("RunResultVariant0Completed")?,
            completed_field: self
                .compiler_known_declaration("RunResultVariant0CompletedValue")?,
            panicked_variant: self.compiler_known_declaration("RunResultVariant1Panicked")?,
            panicked_field: self
                .compiler_known_declaration("RunResultVariant1PanickedReport")?,
            cancelled_variant: self.compiler_known_declaration("RunResultVariant2Cancelled")?,
        })
    }

    fn compiler_known_declaration<I>(
        &self,
        key: &'static str,
    ) -> Result<I, LoweringError>
    where
        I: bray_symbols::ExactSymbolId,
    {
        let Some(key) = CompilerKnownDeclarationKey::try_new(key) else {
            unreachable!("lowering uses validated compiler-known declaration keys");
        };

        self.input
            .available_compiler_known_symbols()
            .declaration_symbol(&key)
            .ok_or(LoweringError::SemanticValueUnavailable)
    }
}
