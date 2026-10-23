use bray_bound_tree::{BoundUnitKey, CheckedTemplateKind};
use bray_declarations::SyntaxAnchor;
use bray_package_interface::InterfaceCheckedTemplate;

use super::super::PackageInterfaceExportError;
use super::super::semantic::SemanticExporter;
use crate::compilation::Compilation;
use crate::compilation::template::SourceTemplateInput;

pub(in crate::compilation::export) fn export_checked_source_template(
    compilation: &Compilation,
    export: &mut SemanticExporter<'_>,
    key: BoundUnitKey,
    kind: CheckedTemplateKind,
    expression: SyntaxAnchor,
    inputs: Vec<SourceTemplateInput>,
    dependency: bray_symbols::DependencyContractTemplateId,
) -> Result<InterfaceCheckedTemplate, PackageInterfaceExportError> {
    let template = compilation
        .checked_source_template(
            key,
            kind,
            expression,
            inputs,
            dependency,
            &compilation.state.cancellation,
        )
        .map_err(super::super::fact_query_export_error)?;

    let Some(template) = template.value() else {
        return Err(super::super::error::source_diagnostic_export_error(template.diagnostics())
            .unwrap_or_else(|| panic!("checked source template {kind:?} at {expression:?} is absent without a source error")));
    };

    super::encoding::encode_template(export, template)
}
