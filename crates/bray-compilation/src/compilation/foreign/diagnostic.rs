use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticCallableAbi, DiagnosticKind, DiagnosticLabelKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, DiagnosticType,
};
use bray_source::SourceSpan;
use bray_symbols::{CallableAbi, TypeExpressionTemplate};
use bray_syntax::SyntaxKind;

use super::super::Compilation;
use crate::fact::FactQueryError;

pub(super) fn source_diagnostic(
    anchor: bray_declarations::SyntaxAnchor,
    kind: DiagnosticKind,
) -> Diagnostic {
    crate::compilation::diagnostics::labeled_source_diagnostic(
        anchor,
        kind,
        DiagnosticLabelKind::InvalidForeignBoundary,
    )
}

pub(super) fn missing_directive(
    anchor: bray_declarations::SyntaxAnchor,
    expected: SyntaxKind,
) -> Diagnostic {
    source_diagnostic(
        anchor,
        DiagnosticKind::CheckingMissingForeignCallableDirective,
    )
    .with_arg(DiagnosticArg::expected_syntax_kind(expected))
}

pub(super) fn duplicate_native_symbol(
    anchor: bray_declarations::SyntaxAnchor,
    symbol: &str,
    previous: &[bray_declarations::SyntaxAnchor],
) -> Diagnostic {
    let mut diagnostic = source_diagnostic(anchor, DiagnosticKind::CheckingDuplicateNativeSymbol)
        .with_arg(DiagnosticArg::declaration_name(symbol));

    for previous in previous {
        diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::FirstDeclaration,
            SourceSpan::new(previous.source_id(), previous.full_range()),
        ));
    }

    diagnostic
}

pub(super) fn diagnostic_abi(abi: CallableAbi) -> DiagnosticCallableAbi {
    match abi {
        CallableAbi::C => DiagnosticCallableAbi::C,
        CallableAbi::System => DiagnosticCallableAbi::System,
        CallableAbi::Bray => unreachable!("Bray ABI is not a foreign boundary"),
    }
}

pub(in crate::compilation) fn template_diagnostic_type(
    compilation: &Compilation,
    template: &TypeExpressionTemplate,
    cancellation: &crate::fact::CancellationToken,
) -> Result<DiagnosticType, FactQueryError> {
    let diagnostic = match template {
        TypeExpressionTemplate::Resolved(ty) => {
            let context = compilation.checker_context(cancellation)?;

            bray_checker::diagnostic_type(&context, *ty)
                .map_err(FactQueryError::CheckerInfrastructure)?
        }
        TypeExpressionTemplate::Named { .. } => DiagnosticType::Unknown,
        TypeExpressionTemplate::TypeValuedMemberProjection { .. } => {
            DiagnosticType::TypeValuedMember
        }
        TypeExpressionTemplate::Tuple(elements) => {
            DiagnosticType::Tuple(u64::try_from(elements.len()).unwrap_or(u64::MAX))
        }
        TypeExpressionTemplate::Array { .. } => DiagnosticType::Array,
        TypeExpressionTemplate::Slice(_) => DiagnosticType::Slice,
        TypeExpressionTemplate::Nullable(_) => DiagnosticType::Nullable,
        TypeExpressionTemplate::Borrow { .. } => DiagnosticType::Borrow,
        TypeExpressionTemplate::TraitView(_) => DiagnosticType::TraitView,
        TypeExpressionTemplate::OwnedIndirection { .. } => DiagnosticType::OwnedIndirection,
        TypeExpressionTemplate::Callable(_) => DiagnosticType::Callable,
    };

    Ok(diagnostic)
}
