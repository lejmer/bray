use bray_lowering::SyntheticLoweringError;

use crate::compilation::CodegenPreparationError;
use crate::fact::FactQueryError;

impl From<SyntheticLoweringError> for CodegenPreparationError {
    fn from(error: SyntheticLoweringError) -> Self {
        match error {
            SyntheticLoweringError::SemanticValue(cause) => {
                FactQueryError::SemanticValueStore(cause).into()
            }
            SyntheticLoweringError::ExactIntegerCapacity { minimum_bits } => {
                Self::ExactIntegerCapacity { minimum_bits }
            }
            SyntheticLoweringError::Capacity(cause) => Self::MirCapacity(cause),
            SyntheticLoweringError::LayoutOverflow(ty) => Self::LayoutOverflow(ty),
        }
    }
}

#[cfg(test)]
mod tests {
    use bray_ir::MirCapacityError;
    use bray_lowering::SyntheticLoweringError;
    use bray_messages::DiagnosticRenderer;
    use bray_symbols::{PackageIdentity, ProductIdentity};

    use super::super::{codegen_preparation_failure_kind, native_product_preparation_diagnostic};
    use super::CodegenPreparationError;

    #[test]
    fn synthetic_capacity_failures_render_product_context() {
        let error = CodegenPreparationError::from(SyntheticLoweringError::Capacity(
            MirCapacityError::IdentityCapacityExceeded,
        ));

        let failure = codegen_preparation_failure_kind(&error).unwrap();

        let product =
            ProductIdentity::try_new(PackageIdentity::try_new("example").unwrap(), "application")
                .unwrap();

        let diagnostic =
            native_product_preparation_diagnostic(failure, &product, "x86_64-pc-windows-msvc");

        let rendered = DiagnosticRenderer::english().render(&diagnostic);

        assert!(rendered.message().contains("application"));
        assert!(!rendered.message().contains("internal compiler error"));

        assert!(rendered.message().contains("implementation capacity"));
    }
}
