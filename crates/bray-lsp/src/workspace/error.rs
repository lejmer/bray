use std::path::PathBuf;

use bray_messages::LanguageServerMessage;
use bray_source::{SourceEditError, SourceOrigin, SourceUriError, TextSizeOverflow};
use bray_symbols::ProductIdentity;
#[derive(Debug)]
pub(crate) enum WorkspaceError {
    SourceRead {
        path: PathBuf,
        cause: std::io::Error,
    },
    CompilationRequest(bray_diagnostics::DiagnosticBag),
    CompilationLoad(bray_compilation::CompilationLoadError),
    CompilationOrder {
        pending: Vec<ProductIdentity>,
    },
    GenerationExhausted {
        current: u64,
    },
    DocumentNotFound,
    InvalidDocumentUri {
        uri: String,
        cause: SourceUriError,
    },
    InvalidSourceOrigin {
        origin: SourceOrigin,
        cause: SourceUriError,
    },
    InvalidDocumentPath {
        path: PathBuf,
        cause: std::io::Error,
    },
    MissingDocumentUri,
    InvalidEdit,
    SourceEdit(SourceEditError),
    InvalidVersion {
        actual: i64,
    },
    NonIncreasingVersion {
        current: u64,
        actual: u64,
    },
    VersionExhausted {
        current: u64,
    },
    SourceIdentityExhausted {
        current: u32,
    },
    ProductNotFound,
    SourceTooLarge(TextSizeOverflow),
    UnsupportedTarget,
    InvalidDependencyIdentity(ProductIdentity),
    MissingDependencyPath(ProductIdentity),
    DependencyNotLoaded(ProductIdentity),
    DependencyExportUnavailable(ProductIdentity),
    DependencyExport {
        product: ProductIdentity,
        cause: Box<bray_compilation::PackageInterfaceExportError>,
    },
    DependencyEncoding {
        product: ProductIdentity,
        cause: bray_package_interface::InterfaceValidationError,
    },
}

impl WorkspaceError {
    pub(crate) const fn message(&self) -> LanguageServerMessage {
        match self {
            Self::SourceRead { .. }
            | Self::CompilationRequest(_)
            | Self::CompilationLoad(_)
            | Self::CompilationOrder { .. }
            | Self::GenerationExhausted { .. } => LanguageServerMessage::CompilationFailed,
            Self::DocumentNotFound => LanguageServerMessage::DocumentNotFound,
            Self::InvalidDocumentUri { .. }
            | Self::InvalidSourceOrigin { .. }
            | Self::InvalidDocumentPath { .. }
            | Self::MissingDocumentUri => LanguageServerMessage::InvalidDocumentUri,
            Self::InvalidEdit | Self::SourceEdit(_) => LanguageServerMessage::InvalidDocumentEdit,
            Self::InvalidVersion { .. }
            | Self::NonIncreasingVersion { .. }
            | Self::VersionExhausted { .. } => LanguageServerMessage::InvalidDocumentVersion,
            Self::SourceIdentityExhausted { .. } => LanguageServerMessage::CompilationFailed,
            Self::ProductNotFound => LanguageServerMessage::ProductNotFound,
            Self::SourceTooLarge(_) => LanguageServerMessage::SourceTooLarge,
            Self::UnsupportedTarget => LanguageServerMessage::UnsupportedTarget,
            Self::InvalidDependencyIdentity(_)
            | Self::MissingDependencyPath(_)
            | Self::DependencyNotLoaded(_)
            | Self::DependencyExportUnavailable(_)
            | Self::DependencyExport { .. }
            | Self::DependencyEncoding { .. } => LanguageServerMessage::DependencyUnavailable,
        }
    }

    pub(crate) fn data(&self) -> serde_json::Value {
        use serde_json::json;

        match self {
            Self::SourceRead { path, cause } => json!({
                "reason": "source_read",
                "path": format!("{path:?}"),
                "cause": cause.to_string(),
            }),
            Self::CompilationRequest(diagnostics) => json!({
                "reason": "compilation_request",
                "cause": format!("{diagnostics:?}"),
            }),
            Self::CompilationLoad(cause) => json!({
                "reason": "compilation_load",
                "cause": format!("{cause:?}"),
            }),
            Self::CompilationOrder { pending } => json!({
                "reason": "compilation_order",
                "pending": pending.iter().map(|value| format!("{value:?}")).collect::<Vec<_>>(),
            }),
            Self::GenerationExhausted { current } => json!({
                "reason": "generation_exhausted",
                "current": current,
            }),
            Self::DocumentNotFound => json!({ "reason": "document_not_found" }),
            Self::InvalidDocumentUri { uri, cause } => json!({
                "reason": "invalid_document_uri",
                "uri": uri,
                "cause": format!("{cause:?}"),
            }),
            Self::InvalidSourceOrigin { origin, cause } => json!({
                "reason": "invalid_source_origin",
                "origin": format!("{origin:?}"),
                "cause": format!("{cause:?}"),
            }),
            Self::InvalidDocumentPath { path, cause } => json!({
                "reason": "invalid_document_path",
                "path": format!("{path:?}"),
                "cause": cause.to_string(),
            }),
            Self::MissingDocumentUri => json!({ "reason": "missing_document_uri" }),
            Self::InvalidEdit => json!({ "reason": "invalid_edit" }),
            Self::SourceEdit(cause) => json!({
                "reason": "source_edit",
                "cause": format!("{cause:?}"),
            }),
            Self::InvalidVersion { actual } => json!({
                "reason": "invalid_version",
                "actual": actual,
            }),
            Self::NonIncreasingVersion { current, actual } => json!({
                "reason": "non_increasing_version",
                "current": current,
                "actual": actual,
            }),
            Self::VersionExhausted { current } => json!({
                "reason": "version_exhausted",
                "current": current,
            }),
            Self::SourceIdentityExhausted { current } => json!({
                "reason": "source_identity_exhausted",
                "current": current,
            }),
            Self::ProductNotFound => json!({ "reason": "product_not_found" }),
            Self::SourceTooLarge(cause) => json!({
                "reason": "source_too_large",
                "actual_bytes": cause.bytes(),
            }),
            Self::UnsupportedTarget => json!({ "reason": "unsupported_target" }),
            Self::InvalidDependencyIdentity(product) => json!({
                "reason": "invalid_dependency_identity",
                "product": format!("{product:?}"),
            }),
            Self::MissingDependencyPath(product) => json!({
                "reason": "missing_dependency_path",
                "product": format!("{product:?}"),
            }),
            Self::DependencyNotLoaded(product) => json!({
                "reason": "dependency_not_loaded",
                "product": format!("{product:?}"),
            }),
            Self::DependencyExportUnavailable(product) => json!({
                "reason": "dependency_export_unavailable",
                "product": format!("{product:?}"),
            }),
            Self::DependencyExport { product, cause } => json!({
                "reason": "dependency_export",
                "product": format!("{product:?}"),
                "cause": format!("{cause:?}"),
            }),
            Self::DependencyEncoding { product, cause } => json!({
                "reason": "dependency_encoding",
                "product": format!("{product:?}"),
                "cause": format!("{cause:?}"),
            }),
        }
    }
}
