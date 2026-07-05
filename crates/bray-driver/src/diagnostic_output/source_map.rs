use bray_source::{LineIndex, SourceLocation, SourceSnapshot, SourceSpan, SourceStore};

use crate::source_origin_output::SourceOriginOutput;

pub(crate) struct DiagnosticSourceMap<'source> {
    sources: Option<&'source SourceStore>,
    line_indexes: Vec<Option<LineIndex>>,
}

impl<'source> DiagnosticSourceMap<'source> {
    pub(crate) fn new(sources: Option<&'source SourceStore>) -> Self {
        let line_indexes = match sources {
            Some(sources) => sources
                .iter()
                .map(|snapshot| LineIndex::new(snapshot.text()).ok())
                .collect(),
            None => Vec::new(),
        };

        Self {
            sources,
            line_indexes,
        }
    }

    pub(crate) fn resolve(&self, span: SourceSpan) -> Option<SourceLocation<'_>> {
        let resolved = self.resolve_source_span(span)?;

        Some(resolved.location)
    }

    pub(crate) fn resolve_source_span(&self, span: SourceSpan) -> Option<ResolvedSourceSpan<'_>> {
        let sources = self.sources?;
        let index = span.source_id().to_index()?;
        let snapshot = sources.get(span.source_id())?;
        let line_index = self.line_indexes.get(index)?.as_ref()?;
        let location = SourceLocation::resolve(snapshot, line_index, span)?;

        Some(ResolvedSourceSpan {
            snapshot,
            line_index,
            location,
        })
    }

    pub(crate) fn source_origin(&self, span: SourceSpan) -> Option<SourceOriginOutput> {
        self.sources?
            .get(span.source_id())
            .map(|snapshot| SourceOriginOutput::from_origin(snapshot.origin()))
    }
}

pub(crate) struct ResolvedSourceSpan<'source> {
    pub(crate) snapshot: &'source SourceSnapshot,
    pub(crate) line_index: &'source LineIndex,
    pub(crate) location: SourceLocation<'source>,
}
