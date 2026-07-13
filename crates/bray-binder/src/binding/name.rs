use bray_diagnostics::{Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, SeverityKind};
use bray_source::SourceSpan;
use bray_symbols::{MemberLookupResult, SymbolName};
use bray_syntax::SyntaxToken;

use crate::BinderFactContext;
use crate::binder::Binder;
use crate::lookup::{PathBindingContext, lookup_unqualified_name};

pub(super) fn symbol_name(
    source: &bray_source::SourceSnapshot,
    token: &SyntaxToken,
) -> Option<SymbolName> {
    if token.is_missing() {
        return None;
    }

    SymbolName::try_new(token.text(source.text())?)
}

pub(super) fn name_is_available<C>(
    binder: &mut Binder<'_, C>,
    context: PathBindingContext,
    source: &bray_source::SourceSnapshot,
    token: &SyntaxToken,
) -> bool
where
    C: BinderFactContext + ?Sized,
{
    let Some(text) = token.text(source.text()) else {
        return false;
    };

    name_text_is_available(
        binder,
        context,
        text,
        SourceSpan::new(source.source_id(), token.range()),
    )
}

pub(super) fn name_text_is_available<C>(
    binder: &mut Binder<'_, C>,
    context: PathBindingContext,
    text: &str,
    span: SourceSpan,
) -> bool
where
    C: BinderFactContext + ?Sized,
{
    if matches!(
        lookup_unqualified_name(
            binder.unit(),
            binder.facts().symbols(),
            context.scope(),
            context.module(),
            text,
            context.access(),
        ),
        MemberLookupResult::NotFound
    ) {
        return true;
    }

    report_name_already_defined(binder, text, span);

    false
}

pub(super) fn report_name_already_defined<C>(
    binder: &mut Binder<'_, C>,
    text: &str,
    span: SourceSpan,
) where
    C: BinderFactContext + ?Sized,
{
    let diagnostic = Diagnostic::new(
        DiagnosticId::new(span.range().start().bytes()),
        DiagnosticKind::BindingNameAlreadyDefined,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_arg(DiagnosticArg::referenced_name(text));

    binder.add_diagnostic(diagnostic);
}
