use crate::LanguageServerMessage;

pub(crate) const fn message(message: LanguageServerMessage) -> &'static str {
    match message {
        LanguageServerMessage::InvalidParams => "Invalid request parameters.",
        LanguageServerMessage::MethodNotFound => "The requested method is not supported.",
        LanguageServerMessage::RequestCancelled => "The request was cancelled.",
        LanguageServerMessage::ContentModified => {
            "The document changed before the request completed."
        }
        LanguageServerMessage::QueryFailed => "The language-server request could not be completed.",
        LanguageServerMessage::CompilationFailed => {
            "The workspace compilation could not be created."
        }
        LanguageServerMessage::DocumentNotFound => "The requested document is not open.",
        LanguageServerMessage::InvalidDocumentUri => "The document URI is invalid.",
        LanguageServerMessage::InvalidDocumentEdit => "The document edit range is invalid.",
        LanguageServerMessage::InvalidDocumentVersion => {
            "The document version is invalid or stale."
        }
        LanguageServerMessage::ProductNotFound => {
            "The project does not contain the requested product."
        }
        LanguageServerMessage::SourceTooLarge => "The source is too large to represent.",
        LanguageServerMessage::UnsupportedTarget => {
            "The selected target is not supported by this product or compiler."
        }
        LanguageServerMessage::DependencyUnavailable => {
            "A workspace dependency could not provide its compiled interface."
        }
    }
}
