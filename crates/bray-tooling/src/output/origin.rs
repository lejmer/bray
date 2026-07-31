use bray_source::SourceOrigin;
use serde::Serialize;

use crate::output::path_to_output_string;

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub(crate) struct SourceOriginOutput {
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    file_path: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    virtual_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    generated_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    lsp_uri: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    test_fixture_name: Option<String>,
}

impl SourceOriginOutput {
    pub(crate) fn from_origin(origin: &SourceOrigin) -> Self {
        Self {
            kind: origin.kind().as_str(),
            file_path: origin.file_path().map(path_to_output_string),
            virtual_name: origin.virtual_name().map(str::to_owned),
            generated_name: origin.generated_name().map(str::to_owned),
            lsp_uri: origin.lsp_uri().map(str::to_owned),
            test_fixture_name: origin.test_fixture_name().map(str::to_owned),
        }
    }

    #[cfg(feature = "analysis")]
    pub(crate) const fn kind(&self) -> &'static str {
        self.kind
    }

    #[cfg(feature = "analysis")]
    pub(crate) fn display_name(&self) -> &str {
        self.file_path()
            .or_else(|| self.virtual_name())
            .or_else(|| self.generated_name())
            .or_else(|| self.lsp_uri())
            .or_else(|| self.test_fixture_name())
            .unwrap_or(self.kind)
    }

    #[cfg(feature = "analysis")]
    pub(crate) fn file_path(&self) -> Option<&str> {
        self.file_path.as_deref()
    }

    #[cfg(feature = "analysis")]
    pub(crate) fn virtual_name(&self) -> Option<&str> {
        self.virtual_name.as_deref()
    }

    #[cfg(feature = "analysis")]
    pub(crate) fn generated_name(&self) -> Option<&str> {
        self.generated_name.as_deref()
    }

    #[cfg(feature = "analysis")]
    pub(crate) fn lsp_uri(&self) -> Option<&str> {
        self.lsp_uri.as_deref()
    }

    #[cfg(feature = "analysis")]
    pub(crate) fn test_fixture_name(&self) -> Option<&str> {
        self.test_fixture_name.as_deref()
    }
}
