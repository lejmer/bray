use bray_lowering::SyntheticLoweringError;

use crate::compilation::{
    CodegenPreparationError, ProductDataKind, ProductQueryContext, ProductQueryFailure,
    ProductValueKind,
};
use crate::fact::FactQueryError;

impl From<SyntheticLoweringError> for CodegenPreparationError {
    fn from(error: SyntheticLoweringError) -> Self {
        match error {
            SyntheticLoweringError::SemanticValue(cause) => {
                FactQueryError::SemanticValueStore(cause).into()
            }
            SyntheticLoweringError::LifecycleMir(cause) => {
                Self::InvalidGeneratedLifecycleMir(cause)
            }
            SyntheticLoweringError::CompilerProvidedMir { definition, cause } => {
                Self::InvalidCompilerProvidedMir { definition, cause }
            }
            SyntheticLoweringError::MissingCallableResult(definition) => {
                ProductQueryFailure::missing(
                    ProductQueryContext::CallableDefinition(definition),
                    ProductDataKind::OperationResultType,
                )
                .into()
            }
            SyntheticLoweringError::MissingTypeResult(ty) => ProductQueryFailure::missing(
                ProductQueryContext::Type(ty),
                ProductDataKind::OperationResultType,
            )
            .into(),
            SyntheticLoweringError::InvalidStorageMemberKey(key) => {
                ProductQueryFailure::InvalidCompilerKnownDeclarationKey { key }.into()
            }
            SyntheticLoweringError::MissingHelper(reference) => {
                Self::MissingHelperInstance(reference)
            }
            SyntheticLoweringError::UnsupportedType(ty) => Self::UnsupportedType(ty),
            SyntheticLoweringError::UnresolvedType(ty) => Self::UnresolvedType(ty),
            SyntheticLoweringError::UnsupportedLifecycleRole(role) => {
                ProductQueryFailure::UnsupportedLifecycleRole { role }.into()
            }
            SyntheticLoweringError::UnexpectedLifecycleType { ty, actual } => {
                ProductQueryFailure::UnexpectedSemanticType {
                    ty,
                    expected: ProductValueKind::LifecycleRepresentableType,
                    actual,
                }
                .into()
            }
            SyntheticLoweringError::MissingOperationResult { source, operation } => {
                ProductQueryFailure::missing(
                    ProductQueryContext::MirOperation { source, operation },
                    ProductDataKind::OperationResultType,
                )
                .into()
            }
            SyntheticLoweringError::MissingRepresentation { role, argument } => {
                ProductQueryFailure::missing(
                    match argument {
                        Some(argument) => {
                            ProductQueryContext::UnaryRepresentation { role, argument }
                        }
                        None => ProductQueryContext::CompilerKnownRepresentation(role),
                    },
                    ProductDataKind::CompilerKnownRepresentation,
                )
                .into()
            }
            SyntheticLoweringError::StorageParameterCount { member, actual } => {
                ProductQueryFailure::count_mismatch(
                    ProductQueryContext::CompilerKnownDeclaration(member),
                    ProductDataKind::CallableParameters,
                    1,
                    actual,
                )
                .into()
            }
            SyntheticLoweringError::LayoutOverflow(ty) => Self::LayoutOverflow(ty),
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_diagnostics::{
        DiagnosticInterfaceDeclarationIdentity, DiagnosticInterfaceSymbolIdentity,
        DiagnosticInterfaceSymbolKind, DiagnosticNoteKind,
    };
    use bray_ir::MirUnitBuildError;
    use bray_lowering::SyntheticLoweringError;
    use bray_messages::DiagnosticRenderer;
    use bray_source::{SourceId, SourceSpan, TextRange, TextSize};
    use bray_symbols::{
        CallableDefinitionId, FunctionSymbolId, PackageIdentity, ProductIdentity, SymbolId,
    };

    use super::super::{codegen_preparation_failure_kind, native_product_preparation_diagnostic};
    use super::CodegenPreparationError;

    #[test]
    fn synthetic_failures_preserve_exact_builder_causes_and_render_product_context() {
        let definition = CallableDefinitionId::try_new(
            FunctionSymbolId::from_symbol_id(SymbolId::new(4)).into(),
        )
        .unwrap();

        let cause = MirUnitBuildError::SourceOriginMismatch;

        let error = CodegenPreparationError::from(SyntheticLoweringError::CompilerProvidedMir {
            definition,
            cause,
        });

        assert_eq!(
            error,
            CodegenPreparationError::InvalidCompilerProvidedMir { definition, cause }
        );

        assert_eq!(
            CodegenPreparationError::from(SyntheticLoweringError::LifecycleMir(cause)),
            CodegenPreparationError::InvalidGeneratedLifecycleMir(cause)
        );

        let failure = codegen_preparation_failure_kind(&error).unwrap();

        let product =
            ProductIdentity::try_new(PackageIdentity::try_new("example").unwrap(), "application")
                .unwrap();

        let diagnostic =
            native_product_preparation_diagnostic(failure, &product, "x86_64-pc-windows-msvc");

        let rendered = DiagnosticRenderer::english().render(&diagnostic);
        assert!(rendered.message().contains("application"));
        assert!(rendered.message().contains("internal compiler error"));

        for forbidden in ["MIR", "SourceOriginMismatch", "lowering", "terminator"] {
            assert!(!rendered.message().contains(forbidden));
        }
    }

    #[test]
    fn missing_implementation_diagnostics_name_the_callable_and_retain_its_source() {
        let source = SourceSpan::new(
            SourceId::new(1),
            TextRange::new(TextSize::new(4), TextSize::new(18)),
        );

        let definition = CallableDefinitionId::try_new(
            FunctionSymbolId::from_symbol_id(SymbolId::new(4)).into(),
        )
        .unwrap();

        let callable = DiagnosticInterfaceSymbolIdentity::Declaration {
            owner: Box::new(DiagnosticInterfaceSymbolIdentity::Package("example".into())),
            kind: DiagnosticInterfaceSymbolKind::Function,
            identity: DiagnosticInterfaceDeclarationIdentity::Name("missing".into()),
        };

        let error = CodegenPreparationError::MissingCallableImplementation {
            definition,
            callable: Box::new(callable),
            source: Some(source),
        };

        let product =
            ProductIdentity::try_new(PackageIdentity::try_new("example").unwrap(), "application")
                .unwrap();

        let diagnostic = native_product_preparation_diagnostic(
            codegen_preparation_failure_kind(&error).unwrap(),
            &product,
            "x86_64-pc-windows-msvc",
        );

        assert_eq!(diagnostic.primary_span(), Some(source));

        assert!(
            diagnostic
                .notes()
                .iter()
                .any(|note| note.kind() == DiagnosticNoteKind::ReportCompilerDefect)
        );

        let rendered = DiagnosticRenderer::english().render(&diagnostic);
        assert!(rendered.message().contains("missing"));
        assert!(rendered.message().contains("application"));

        assert!(
            rendered
                .message()
                .contains("no executable implementation or native import")
        );

        for forbidden in ["MIR", "SymbolId", "lowering", "terminator"] {
            assert!(!rendered.message().contains(forbidden));
        }
    }
}
