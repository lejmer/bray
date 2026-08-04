use crate::DiagnosticLocale;
use crate::catalog::MessageCatalog;

/// Structured user-facing vocabulary used by build progress renderers.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum BuildProgressMessage {
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
    /// The selected configuration is a debug build.
    Debug,
    /// The selected configuration favors optimized release artifacts.
    Release,
    /// The count describes compilation units.
    Units,
    /// The compiler is checking a dependency interface.
    CheckingInterface,
    /// The compiler is producing the selected product artifacts.
    ProducingArtifacts,
}

/// Locale-aware renderer for build progress vocabulary.
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

    /// Renders one structured build progress message.
    pub fn render(self, message: BuildProgressMessage) -> &'static str {
        self.catalog.build_progress_message(message)
    }
}

#[cfg(test)]
mod tests {
    use super::{BuildProgressMessage, BuildProgressMessageRenderer};

    #[test]
    fn build_progress_vocabulary_renders_through_the_selected_catalog() {
        let renderer = BuildProgressMessageRenderer::english();

        assert_eq!(renderer.render(BuildProgressMessage::Building), "Building");
        assert_eq!(renderer.render(BuildProgressMessage::Units), "units");
    }
}
