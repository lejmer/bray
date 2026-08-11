use std::collections::BTreeMap;
use std::fs;
use std::io;
use std::num::NonZeroU16;
use std::path::{Path, PathBuf};

use bray_diagnostics::DiagnosticDocumentParseKind;
use bray_formatter::{FormatterConfiguration, FormatterRule};
use serde::Deserialize;

#[derive(Debug)]
pub(crate) enum ConfigurationError {
    Read {
        path: PathBuf,
        io_error_kind: io::ErrorKind,
    },
    Malformed {
        path: PathBuf,
        parse_kind: DiagnosticDocumentParseKind,
        line: u64,
        column: u64,
    },
    UnknownRule {
        path: PathBuf,
        rule_name: String,
    },
    InvalidMaximumLineWidth {
        path: PathBuf,
        maximum_line_width: u64,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ConfigurationFile {
    maximum_line_width: Option<u64>,
    #[serde(default)]
    rules: BTreeMap<String, bool>,
}

pub(crate) fn load_configuration(
    path: &Path,
) -> Result<FormatterConfiguration, ConfigurationError> {
    let bytes = fs::read(path).map_err(|error| ConfigurationError::Read {
        path: path.to_path_buf(),
        io_error_kind: error.kind(),
    })?;

    let file = serde_json::from_slice::<ConfigurationFile>(&bytes).map_err(|error| {
        ConfigurationError::Malformed {
            path: path.to_path_buf(),
            parse_kind: match error.classify() {
                serde_json::error::Category::Io => DiagnosticDocumentParseKind::Input,
                serde_json::error::Category::Syntax => DiagnosticDocumentParseKind::Syntax,
                serde_json::error::Category::Data => DiagnosticDocumentParseKind::Schema,
                serde_json::error::Category::Eof => DiagnosticDocumentParseKind::UnexpectedEnd,
            },
            line: u64::try_from(error.line()).unwrap_or(u64::MAX),
            column: u64::try_from(error.column()).unwrap_or(u64::MAX),
        }
    })?;

    let mut configuration = FormatterConfiguration::default();

    if let Some(maximum_line_width) = file.maximum_line_width {
        let maximum_line_width = u16::try_from(maximum_line_width)
            .ok()
            .and_then(NonZeroU16::new)
            .ok_or_else(|| ConfigurationError::InvalidMaximumLineWidth {
                path: path.to_path_buf(),
                maximum_line_width,
            })?;

        configuration = configuration.with_maximum_line_width(maximum_line_width);
    }

    for (rule_name, enabled) in file.rules {
        let Some(rule) = FormatterRule::from_name(&rule_name) else {
            return Err(ConfigurationError::UnknownRule {
                path: path.to_path_buf(),
                rule_name,
            });
        };

        configuration = configuration.with_rule(rule, enabled);
    }

    Ok(configuration)
}

#[cfg(test)]
mod tests {
    use bray_formatter::FormatterRule;
    use bray_testing::TemporaryFile;

    use super::{ConfigurationError, load_configuration};

    #[test]
    fn configuration_files_resolve_width_and_rule_overrides() {
        let file = TemporaryFile::write(
            "brayfmt.json",
            br#"{
                "maximum_line_width": 88,
                "rules": {
                    "indentation": false,
                    "simplify-nested-if": true
                }
            }"#,
        );

        let configuration = load_configuration(file.path())
            .unwrap_or_else(|error| panic!("configuration should load: {error:?}"));

        assert_eq!(configuration.maximum_line_width(), 88);
        assert!(!configuration.is_enabled(FormatterRule::Indentation));
        assert!(configuration.is_enabled(FormatterRule::SimplifyNestedIf));
    }

    #[test]
    fn configuration_files_reject_unknown_rules_and_invalid_widths() {
        let unknown = TemporaryFile::write(
            "unknown-brayfmt.json",
            br#"{"rules":{"unknown-rule":true}}"#,
        );

        let invalid_width =
            TemporaryFile::write("invalid-width-brayfmt.json", br#"{"maximum_line_width":0}"#);

        let unknown = load_configuration(unknown.path()).unwrap_err();
        let invalid_width = load_configuration(invalid_width.path()).unwrap_err();

        assert!(matches!(
            unknown,
            ConfigurationError::UnknownRule { rule_name, .. } if rule_name == "unknown-rule"
        ));

        assert!(matches!(
            invalid_width,
            ConfigurationError::InvalidMaximumLineWidth {
                maximum_line_width: 0,
                ..
            }
        ));
    }
}
