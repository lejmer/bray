use bray_compiler_known::RepresentationRole;
use bray_symbols::{
    ConstantTermData, ConstantValueKind, NamedTypeSymbolId, SemanticValueStore, StructSymbolId,
    TypeId,
};

use crate::compilation::substitution::named_type;
use crate::compilation::{
    CodegenPreparationError, Compilation, ProductDataKind, ProductQueryContext, ProductQueryFailure,
};
use crate::fact::FactQueryError;

impl Compilation {
    pub(in crate::compilation::product::realization) fn codegen_representation_type(
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

    pub(in crate::compilation::product::realization) fn codegen_opaque_pointer_type(
        &self,
    ) -> Result<TypeId, FactQueryError> {
        let element = self.codegen_representation_type(RepresentationRole::ScalarU8)?;

        self.available_compiler_known_symbols()
            .unary_representation_type(
                self.semantic_value_store()?,
                RepresentationRole::RawPointer,
                element,
            )
            .map_err(FactQueryError::SemanticValueStore)?
            .ok_or_else(|| {
                ProductQueryFailure::missing(
                    ProductQueryContext::UnaryRepresentation {
                        role: RepresentationRole::RawPointer,
                        argument: element,
                    },
                    ProductDataKind::CompilerKnownRepresentation,
                )
                .into()
            })
    }
}

pub(in crate::compilation::product) fn closed_array_length(
    values: &SemanticValueStore,
    term_id: bray_symbols::ConstantTermId,
) -> Result<u64, CodegenPreparationError> {
    let term = values.constant_term_data(term_id);

    match term.as_ref() {
        ConstantTermData::Typed { term, .. } => closed_array_length(values, *term),
        ConstantTermData::Value(value) => {
            let data = values.constant_value_data(*value);

            let ConstantValueKind::Integer(value) = data.kind() else {
                return Err(CodegenPreparationError::InvalidArrayLength(term_id));
            };

            value
                .to_u64()
                .ok_or(CodegenPreparationError::InvalidArrayLength(term_id))
        }
        ConstantTermData::IntegerLiteral { value, .. } => value
            .to_u64()
            .ok_or(CodegenPreparationError::InvalidArrayLength(term_id)),
        _ => Err(CodegenPreparationError::OpenConstantTerm(term_id)),
    }
}

pub(in crate::compilation::product::realization) fn codegen_checker_error(
    error: bray_checker::CheckerQueryError<FactQueryError>,
) -> CodegenPreparationError {
    match error {
        bray_checker::CheckerQueryError::Cancelled => FactQueryError::Cancelled.into(),
        bray_checker::CheckerQueryError::Upstream(error) => error.into(),
    }
}
