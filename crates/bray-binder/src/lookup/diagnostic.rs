use bray_diagnostics::{
    Diagnostic, DiagnosticArg, DiagnosticId, DiagnosticKind, DiagnosticNameKind, SeverityKind,
};
use bray_source::{SourceSpan, TextRange};
use bray_symbols::MemberLookupResult;

use super::binding::NameLookupResult;
use super::category::ResolvedName;
use crate::{BinderFactContext, binder::Binder};

#[derive(Debug, Eq, PartialEq)]
pub(super) struct NameReference {
    text: String,
    span: SourceSpan,
}

impl NameReference {
    pub(super) fn new(
        text: impl Into<String>,
        source: bray_source::SourceId,
        range: TextRange,
    ) -> Self {
        Self {
            text: text.into(),
            span: SourceSpan::new(source, range),
        }
    }

    pub(super) fn text(&self) -> &str {
        &self.text
    }

    pub(super) const fn span(&self) -> SourceSpan {
        self.span
    }
}

pub(super) fn report_lookup_result<C, T>(
    binder: &mut Binder<'_, C>,
    reference: &NameReference,
    expected: DiagnosticNameKind,
    result: &NameLookupResult<T>,
) where
    C: BinderFactContext + ?Sized,
{
    let kind = match result {
        MemberLookupResult::Found(_) => return,
        MemberLookupResult::NotFound => DiagnosticKind::BindingUnresolvedName,
        MemberLookupResult::WrongKind(_) => DiagnosticKind::BindingWrongNameKind,
        MemberLookupResult::Ambiguous(_) => DiagnosticKind::BindingAmbiguousName,
        MemberLookupResult::Inaccessible(_) => DiagnosticKind::BindingInaccessibleName,
        MemberLookupResult::Malformed(candidates) if candidates.is_empty() => return,
        MemberLookupResult::Malformed(_) => DiagnosticKind::BindingMalformedName,
    };

    let mut diagnostic = Diagnostic::new(
        DiagnosticId::new(reference.span().range().start().bytes()),
        kind,
        SeverityKind::Error,
    )
    .with_primary_span(reference.span())
    .with_arg(DiagnosticArg::referenced_name(reference.text()));

    if kind == DiagnosticKind::BindingWrongNameKind {
        diagnostic = diagnostic.with_arg(DiagnosticArg::expected_name_kind(expected));
    }

    binder.add_diagnostic(diagnostic);
}

pub(super) fn malformed_lookup<T>() -> NameLookupResult<T> {
    MemberLookupResult::Malformed(Box::<[ResolvedName]>::default())
}
