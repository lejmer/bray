use crate::DiagnosticLocale;
use crate::catalog::MessageCatalog;

/// Localized label or status in a build-storage report.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum StorageReportMessage {
    /// Column headings for a storage report.
    Heading,
    /// No managed entries match the selection.
    Empty,
    /// Stable public product artifacts.
    CurrentOutputs,
    /// Current immutable files retained for execution.
    RetainedRerun,
    /// The previous distinct product generation.
    RetainedHistory,
    /// Files protected by a live reader or writer.
    ActiveWork,
    /// Optional cache files within their retention lifetime.
    ReusableCache,
    /// Unpinned files eligible for automatic cleanup.
    Reclaimable,
    /// An active writer prevents stable accounting.
    ChangingBytes,
    /// Cleanup removed the selected files.
    Removed,
    /// Active ownership prevented cleanup.
    KeptActive,
    /// A dry run selected these files for removal.
    WouldRemove,
    /// The report left these files in place.
    Retained,
}

/// Locale-aware renderer for build-storage report labels and statuses.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct StorageReportMessageRenderer {
    catalog: MessageCatalog,
}

impl StorageReportMessageRenderer {
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

    /// Renders a report label or status.
    pub const fn render(self, message: StorageReportMessage) -> &'static str {
        self.catalog.storage_report_message(message)
    }
}
