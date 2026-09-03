use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;
use std::process::{Command, ExitStatus};

use bray_runtime_interface::{PlatformServiceRole, RuntimeAbiRole};
use bray_target::ObjectFormat;

pub(crate) fn validate_role_exports(
    symbols: &[String],
    required: impl IntoIterator<Item = &'static str>,
) -> Result<(), ExportMismatch> {
    let required = required.into_iter().collect::<BTreeSet<_>>();

    let known = RuntimeAbiRole::ALL
        .into_iter()
        .filter_map(RuntimeAbiRole::native_symbol)
        .chain(
            PlatformServiceRole::ALL
                .iter()
                .copied()
                .map(PlatformServiceRole::native_symbol),
        )
        .collect::<BTreeSet<_>>();

    let mut definitions = BTreeMap::<&str, usize>::new();

    for symbol in symbols {
        *definitions.entry(symbol).or_default() += 1;
    }

    let missing = required
        .iter()
        .filter(|symbol| !definitions.contains_key(**symbol))
        .map(|symbol| (*symbol).to_owned())
        .collect::<Vec<_>>();

    let unexpected = definitions
        .keys()
        .filter(|symbol| {
            (known.contains(**symbol)
                || symbol.starts_with("bray_runtime_")
                || symbol.starts_with("bray_platform_"))
                && !required.contains(**symbol)
        })
        .map(|symbol| (*symbol).to_owned())
        .collect::<Vec<_>>();

    let duplicates = definitions
        .iter()
        .filter(|(symbol, count)| {
            **count > 1 && (known.contains(**symbol) || required.contains(**symbol))
        })
        .map(|(symbol, _)| (*symbol).to_owned())
        .collect::<Vec<_>>();

    if !missing.is_empty() || !unexpected.is_empty() || !duplicates.is_empty() {
        return Err(ExportMismatch {
            missing,
            unexpected,
            duplicates,
        });
    }

    Ok(())
}

#[derive(Debug, Eq, PartialEq)]
pub(crate) struct ExportMismatch {
    pub(crate) missing: Vec<String>,
    pub(crate) unexpected: Vec<String>,
    pub(crate) duplicates: Vec<String>,
}

impl fmt::Display for ExportMismatch {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "missing [{}], unexpected [{}], duplicate [{}]",
            self.missing.join(", "),
            self.unexpected.join(", "),
            self.duplicates.join(", ")
        )
    }
}

pub(crate) fn defined_exports(
    root: &Path,
    archive: &Path,
    object_format: ObjectFormat,
) -> Result<Vec<String>, InspectionError> {
    let output = Command::new(bray_llvm_toolchain::tool_path(root, "llvm-nm"))
        .args(["--defined-only", "--extern-only"])
        .arg(archive)
        .output()
        .map_err(InspectionError::Invocation)?;

    if !output.status.success() {
        return Err(InspectionError::Failed {
            status: output.status,
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    let symbols = String::from_utf8(output.stdout).map_err(InspectionError::Encoding)?;

    Ok(parse_exports(&symbols, object_format))
}

fn parse_exports(symbols: &str, object_format: ObjectFormat) -> Vec<String> {
    symbols
        .lines()
        .filter_map(|line| {
            if line.trim_end().ends_with(':') {
                return None;
            }

            let mut fields = line.split_whitespace();
            fields.next()?;
            let name = fields.next_back()?;

            let name = if object_format == ObjectFormat::MachO {
                name.strip_prefix('_').unwrap_or(name)
            } else {
                name
            };

            Some(name.to_owned())
        })
        .collect()
}

#[derive(Debug)]
pub(crate) enum InspectionError {
    Invocation(std::io::Error),
    Failed { status: ExitStatus, detail: String },
    Encoding(std::string::FromUtf8Error),
}

impl fmt::Display for InspectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invocation(error) => write!(formatter, "could not run llvm-nm: {error}"),
            Self::Failed { status, detail } => {
                write!(formatter, "llvm-nm failed with {status}: {}", detail.trim())
            }
            Self::Encoding(error) => {
                write!(formatter, "native symbol inventory is not UTF-8: {error}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::parse_exports;
    use bray_target::ObjectFormat;

    #[test]
    fn normalizes_macho_exports_without_losing_duplicate_definitions() {
        assert_eq!(
            parse_exports(
                "archive with spaces/first.o:\n0000 T _bray_role\nsecond.o:\n0000 T _bray_role\n",
                ObjectFormat::MachO
            ),
            ["bray_role", "bray_role"],
        );
    }
}
