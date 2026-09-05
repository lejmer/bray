use std::collections::BTreeSet;

use bray_bound_tree::{BoundPattern, BoundPatternId};
use bray_diagnostics::{
    Diagnostic, DiagnosticKind, DiagnosticLabel, DiagnosticLabelKind, SeverityKind,
};
use bray_symbols::{LocalBindingSymbolId, StructFieldTypeQuery, UnionPayloadFieldTypeQuery};

use super::state::PatternChecker;
use crate::diagnostic::{diagnostic_id, pattern_span};
use crate::{CheckerQueryError, CheckerRequestContext, CheckerSemanticQueryProvider};

impl<C> PatternChecker<'_, '_, C>
where
    C: CheckerRequestContext
        + CheckerSemanticQueryProvider<StructFieldTypeQuery>
        + CheckerSemanticQueryProvider<UnionPayloadFieldTypeQuery>
        + ?Sized,
{
    pub(super) fn check_alternative_bindings(
        &mut self,
        id: BoundPatternId,
        pattern: &BoundPattern,
    ) -> Result<bool, CheckerQueryError<C::UpstreamError>> {
        let mut alternatives = pattern.children().iter().copied();

        let Some(first) = alternatives.next() else {
            return Ok(true);
        };

        let bindings = self.effective_bindings(first);

        if alternatives.all(|alternative| self.effective_bindings(alternative) == bindings) {
            return Ok(true);
        }

        let span = pattern_span(self.request, id)?;

        self.diagnostics.push(
            Diagnostic::new(
                diagnostic_id(self.diagnostics.len()),
                DiagnosticKind::BindingIncoherentAlternativePattern,
                SeverityKind::Error,
            )
            .with_primary_span(span)
            .with_label(DiagnosticLabel::primary(
                DiagnosticLabelKind::AlternativePattern,
                span,
            )),
        );

        Ok(false)
    }

    fn effective_bindings(&self, root: BoundPatternId) -> BTreeSet<LocalBindingSymbolId> {
        let mut pending = vec![root];
        let mut bindings = BTreeSet::new();

        while let Some(id) = pending.pop() {
            let (Some(pattern), Some(checked)) =
                (self.request.view().pattern(id), self.patterns.get(&id))
            else {
                continue;
            };

            bindings.extend(checked.bindings(pattern));
            pending.extend(pattern.children().iter().copied());
        }

        bindings
    }
}
