use crate::catalog::MessageCatalog;
use crate::DiagnosticLocale;

/// Structured user-facing messages emitted by the language-server protocol boundary.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum LanguageServerMessage {
    /// Request parameters do not match the method contract.
    InvalidParams,
    /// The requested protocol method is not supported.
    MethodNotFound,
    /// The request was cancelled before completion.
    RequestCancelled,
    /// The compilation changed before the request completed.
    ContentModified,
    /// The semantic query could not produce a result.
    QueryFailed,
    /// The workspace compilation could not be created.
    CompilationFailed,
    /// The requested document is not open.
    DocumentNotFound,
    /// The document URI is invalid.
    InvalidDocumentUri,
    /// A document edit does not describe a valid source range.
    InvalidDocumentEdit,
    /// The document version is invalid or stale.
    InvalidDocumentVersion,
    /// The project does not contain the requested product.
    ProductNotFound,
    /// The source is too large to represent.
    SourceTooLarge,
    /// The selected target is not supported by the product or compiler.
    UnsupportedTarget,
    /// A live workspace dependency could not provide its interface.
    DependencyUnavailable,
}

/// Locale-aware renderer for language-server protocol messages.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct LanguageServerMessageRenderer {
    catalog: MessageCatalog,
}

impl LanguageServerMessageRenderer {
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

    /// Renders one structured language-server message.
    pub fn render(self, message: LanguageServerMessage) -> &'static str {
        self.catalog.language_server_message(message)
    }
}

#[cfg(test)]
mod tests {
    use super::{LanguageServerMessage, LanguageServerMessageRenderer};

    #[test]
    fn protocol_messages_render_through_the_selected_catalog() {
        assert_eq!(
            LanguageServerMessageRenderer::english()
                .render(LanguageServerMessage::ContentModified),
            "The document changed before the request completed."
        );
    }
}
