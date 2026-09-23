use bray_bound_tree::{BoundBlockItem, CheckedExpressionTypes};
use bray_compiler_known::RepresentationRole;
use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind, DiagnosticType,
    SeverityKind,
};
use bray_symbols::TypeData;

use crate::diagnostic::{diagnostic_id, expression_span};
use crate::representation::type_representation;
use crate::{CheckerQueryError, CheckerRequestContext, CheckerUnitView};

use super::diagnostic_type;

pub(super) fn inferred_bytes_diagnostics<C>(
    request: CheckerUnitView<'_, C>,
    types: &CheckedExpressionTypes,
    first_diagnostic: usize,
) -> Result<Vec<Diagnostic>, CheckerQueryError<C::UpstreamError>>
where
    C: CheckerRequestContext + ?Sized,
{
    let mut diagnostics = Vec::new();

    for (_, block) in request.unit().tree().blocks() {
        for item in block.items() {
            let BoundBlockItem::LocalBinding(binding) = item else {
                continue;
            };

            if !binding.infers_byte_extent() {
                continue;
            }

            let Some(result) = types.expression(binding.initializer()) else {
                continue;
            };

            if result.is_recovered() {
                continue;
            }

            let data = request.semantic_values().type_data(result.ty());

            let (expected, actual) = match data.as_ref() {
                TypeData::Array { element, .. }
                    if type_representation(request, *element) == Some(RepresentationRole::ScalarU8) => continue,
                TypeData::Array { element, .. } => {
                    (DiagnosticType::U8, diagnostic_type(request, *element)?)
                }
                _ => (DiagnosticType::Array, diagnostic_type(request, result.ty())?),
            };

            let span = expression_span(request, binding.initializer())?;

            diagnostics.push(
                Diagnostic::new(
                    diagnostic_id(first_diagnostic + diagnostics.len()),
                    DiagnosticKind::CheckingIncompatibleExpressionType,
                    SeverityKind::Error,
                )
                .with_primary_span(span)
                .with_label(DiagnosticLabel::primary(
                    DiagnosticLabelKind::IncompatibleExpressionType,
                    span,
                ))
                .with_arg(DiagnosticArg::expected_type(expected))
                .with_arg(DiagnosticArg::actual_type(actual)),
            );
        }
    }

    Ok(diagnostics)
}
