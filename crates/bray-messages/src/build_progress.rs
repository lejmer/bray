use std::time::Duration;

use crate::DiagnosticLocale;
use crate::catalog::MessageCatalog;

/// Structured build operation rendered in workflow progress.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BuildProgressOperation {
    /// A product build is underway.
    Building,
    /// A package compilation is underway.
    Compiling,
    /// A package compilation completed successfully.
    Compiled,
    /// A product build completed successfully.
    Finished,
    /// Work failed.
    Failed,
}

/// Build configuration rendered in workflow progress.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BuildProgressConfiguration {
    /// A debug build.
    Debug,
    /// An optimized release build.
    Release,
}

/// Detailed compiler action rendered by verbose workflow progress.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BuildProgressAction {
    /// Check a dependency package interface.
    CheckInterface,
    /// Produce the selected product artifacts.
    ProduceArtifacts,
}

/// Stable visual field in a localized workflow-progress line.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ProgressField {
    /// Localized operation text.
    Operation,
    /// Package or artifact identity.
    Subject,
    /// Package or output path.
    Path,
    /// Animated progress bar.
    Bar,
    /// Localized completion percentage.
    Percentage,
    /// Localized completed and total work count.
    Count,
    /// Localized line detail such as aggregate result counts.
    Detail,
    /// Localized elapsed duration.
    Duration,
}

/// Stable category of one build-progress line.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BuildProgressLineKind {
    /// A package compiler operation.
    Package,
    /// The active aggregate product build.
    ActiveProduct,
    /// The completed aggregate product build.
    FinishedProduct,
}

/// Locale-aware renderer for build progress.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BuildProgressMessageRenderer {
    catalog: MessageCatalog,
}

impl BuildProgressMessageRenderer {
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

    /// Returns the locale-defined field order for one progress line.
    pub fn fields(self, kind: BuildProgressLineKind) -> &'static [ProgressField] {
        self.catalog.build_progress_fields(kind)
    }

    /// Renders a complete product heading.
    pub fn heading(self, product: &str, configuration: BuildProgressConfiguration) -> String {
        self.catalog.build_progress_heading(product, configuration)
    }

    /// Renders one build operation.
    pub fn operation(self, operation: BuildProgressOperation) -> &'static str {
        self.catalog.build_progress_operation(operation)
    }

    /// Renders one detailed compiler action.
    pub fn action(self, action: BuildProgressAction) -> &'static str {
        self.catalog.build_progress_action(action)
    }

    /// Renders a completed and total work-unit count.
    pub fn unit_count(self, completed: u64, total: u64) -> String {
        self.catalog.build_progress_unit_count(completed, total)
    }

    /// Renders an elapsed duration with compact units selected from its magnitude.
    /// Uses nanoseconds below one microsecond, microseconds below one millisecond,
    /// milliseconds below one second, seconds below one minute, and minutes thereafter.
    /// Truncates to three decimal places and omits trailing zeros. The catalog supplies
    /// the unit labels and numeric punctuation.
    pub fn duration(self, duration: Duration) -> String {
        self.catalog.duration(duration)
    }

    /// Renders a completion percentage.
    pub fn percentage(self, percentage: u64) -> String {
        self.catalog.build_progress_percentage(percentage)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crate::TestReportMessageRenderer;

    use super::{
        BuildProgressConfiguration, BuildProgressLineKind, BuildProgressMessageRenderer,
        BuildProgressOperation,
    };

    #[test]
    fn duration_units_share_exact_boundaries_and_compact_precision() {
        let build = BuildProgressMessageRenderer::english();
        let test = TestReportMessageRenderer::english();

        for (nanoseconds, expected) in [
            (0, "0 ns"),
            (1, "1 ns"),
            (999, "999 ns"),
            (1_000, "1 us"),
            (1_001, "1.001 us"),
            (125_125, "125.125 us"),
            (999_999, "999.999 us"),
            (1_000_000, "1 ms"),
            (1_000_001, "1 ms"),
            (1_001_000, "1.001 ms"),
            (1_250_000, "1.25 ms"),
            (999_999_999, "999.999 ms"),
            (1_000_000_000, "1 s"),
            (1_000_000_001, "1 s"),
            (1_001_000_000, "1.001 s"),
            (12_340_000_000, "12.34 s"),
            (59_999_999_999, "59.999 s"),
            (60_000_000_000, "1 min"),
            (60_000_000_001, "1 min"),
            (60_060_000_000, "1.001 min"),
            (90_000_000_000, "1.5 min"),
            (3_600_000_000_000, "60 min"),
        ] {
            let duration = Duration::from_nanos(nanoseconds);

            assert_eq!(build.duration(duration), expected, "{nanoseconds} ns");
            assert_eq!(test.duration(duration), expected, "{nanoseconds} ns");
        }

        assert_eq!(build.duration(Duration::MAX), "307445734561825860.266 min");
    }

    #[test]
    fn build_progress_text_and_layout_render_through_the_selected_catalog() {
        let renderer = BuildProgressMessageRenderer::english();

        assert_eq!(
            renderer.heading("hello_world/application", BuildProgressConfiguration::Debug),
            "Building hello_world/application [debug]"
        );

        assert_eq!(
            renderer.operation(BuildProgressOperation::Compiled),
            "Compiled"
        );

        assert_eq!(renderer.unit_count(2, 3), "2/3 units");
        assert_eq!(renderer.duration(Duration::from_millis(64)), "64 ms");
        assert!(!renderer.fields(BuildProgressLineKind::Package).is_empty());
    }
}
