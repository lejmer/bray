use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind,
    DiagnosticRelatedLocation, DiagnosticRelatedLocationKind, SeverityKind,
};
use bray_source::SourceSpan;
use bray_symbols::{MemberLookupResult, SymbolName};
use bray_syntax::{PathSyntax, SourceSyntaxNode, SyntaxToken};

use crate::BindingQueryContext;
use crate::binder::Binder;
use crate::lookup::{PathBindingContext, ResolvedName, lookup_unqualified_name};

pub(super) fn symbol_name(
    source: &bray_source::SourceSnapshot,
    token: &SyntaxToken,
) -> Option<SymbolName> {
    if token.is_missing() {
        return None;
    }

    SymbolName::try_new(token.text(source.text())?)
}

pub(super) fn is_bytes_type_path(path: &PathSyntax) -> bool {
    let mut tokens = path.identifier_tokens();

    let Some(token) = tokens.next() else {
        return false;
    };

    tokens.next().is_none() && token.text(path.source().text()) == Some("bytes")
}

pub(super) fn name_is_available<C>(
    binder: &mut Binder<'_, C>,
    context: PathBindingContext,
    source: &bray_source::SourceSnapshot,
    token: &SyntaxToken,
) -> bool
where
    C: BindingQueryContext + ?Sized,
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
    C: BindingQueryContext + ?Sized,
{
    let lookup = lookup_unqualified_name(
        binder.unit(),
        binder.binding_context().symbols(),
        context.scope(),
        context.module(),
        text,
        context.access(),
    );

    if matches!(lookup, MemberLookupResult::NotFound) {
        return true;
    }

    let prior_spans = occupied_name_spans(binder, &lookup);

    report_name_already_defined(binder, text, span, prior_spans);

    false
}

pub(super) fn report_name_already_defined<C>(
    binder: &mut Binder<'_, C>,
    text: &str,
    span: SourceSpan,
    prior_spans: impl IntoIterator<Item = SourceSpan>,
) where
    C: BindingQueryContext + ?Sized,
{
    let mut diagnostic = Diagnostic::new(
        DiagnosticId::new(span.range().start().bytes()),
        DiagnosticKind::BindingNameAlreadyDefined,
        SeverityKind::Error,
    )
    .with_primary_span(span)
    .with_label(DiagnosticLabel::primary(
        DiagnosticLabelKind::NameDefinition,
        span,
    ))
    .with_arg(DiagnosticArg::referenced_name(text));

    let mut prior_spans = prior_spans
        .into_iter()
        .filter(|prior| *prior != span)
        .collect::<Vec<_>>();

    prior_spans.sort_unstable();
    prior_spans.dedup();

    for prior in prior_spans {
        diagnostic = diagnostic.with_related_location(DiagnosticRelatedLocation::new(
            DiagnosticRelatedLocationKind::FirstDeclaration,
            prior,
        ));
    }

    binder.add_diagnostic(diagnostic);
}

fn occupied_name_spans<C>(
    binder: &Binder<'_, C>,
    result: &MemberLookupResult<ResolvedName>,
) -> Vec<SourceSpan>
where
    C: BindingQueryContext + ?Sized,
{
    let candidates: &[ResolvedName] = match result {
        MemberLookupResult::Found(candidate) => std::slice::from_ref(candidate),
        MemberLookupResult::NotFound => &[],
        MemberLookupResult::WrongKind(candidates)
        | MemberLookupResult::Ambiguous(candidates)
        | MemberLookupResult::Inaccessible(candidates)
        | MemberLookupResult::Malformed(candidates) => candidates,
    };

    let mut spans = candidates
        .iter()
        .filter_map(|candidate| resolved_name_span(binder, *candidate))
        .collect::<Vec<_>>();

    spans.sort_unstable();
    spans.dedup();

    spans
}

fn resolved_name_span<C>(binder: &Binder<'_, C>, name: ResolvedName) -> Option<SourceSpan>
where
    C: BindingQueryContext + ?Sized,
{
    let anchor = match name {
        ResolvedName::Local(symbol) => binder.unit().local_symbol_syntax_anchor(symbol),
        ResolvedName::Surface(symbol) => binder
            .binding_context()
            .symbols()
            .declaration_syntax_anchor(symbol)?,
    };

    Some(SourceSpan::new(anchor.source_id(), anchor.full_range()))
}
