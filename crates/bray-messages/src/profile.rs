use crate::DiagnosticLocale;
use crate::catalog::MessageCatalog;

/// Locale-aware renderer for concise compiler profile summaries.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CompilerProfileMessageRenderer {
    catalog: MessageCatalog,
}

impl CompilerProfileMessageRenderer {
    /// Creates a renderer for the selected locale.
    pub const fn new(locale: DiagnosticLocale) -> Self {
        Self {
            catalog: MessageCatalog::new(locale),
        }
    }

    /// Creates an English renderer.
    pub const fn english() -> Self {
        Self::new(DiagnosticLocale::English)
    }

    /// Renders the summary heading and total duration.
    pub fn heading(self, elapsed_nanoseconds: u64) -> String {
        self.catalog.compiler_profile_heading(elapsed_nanoseconds)
    }

    /// Renders aggregate query request, evaluation, and cache-hit counts.
    pub fn queries(self, requests: u64, evaluations: u64, cache_hits: u64) -> String {
        self.catalog
            .compiler_profile_queries(requests, evaluations, cache_hits)
    }

    /// Renders trace event and dropped-event counts.
    pub fn trace(self, events: usize, dropped_events: u64) -> String {
        self.catalog.compiler_profile_trace(events, dropped_events)
    }
}

#[cfg(test)]
mod tests {
    use super::CompilerProfileMessageRenderer;

    #[test]
    fn english_profile_summary_is_rendered_from_structured_values() {
        let renderer = CompilerProfileMessageRenderer::english();

        assert_eq!(renderer.heading(1_250_000), "Compiler profile: 1.250 ms");

        assert_eq!(
            renderer.queries(12, 10, 2),
            "Queries: 12 requested, 10 evaluated, 2 cache hits"
        );

        assert_eq!(renderer.trace(8, 1), "Trace: 8 events, 1 dropped");
    }
}
