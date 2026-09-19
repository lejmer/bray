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
    Ok(defined_external_symbols(root, archive, object_format)?
        .into_iter()
        .map(DefinedExternalSymbol::into_name)
        .collect())
}

pub(crate) fn defined_external_symbols(
    root: &Path,
    archive: &Path,
    object_format: ObjectFormat,
) -> Result<Vec<DefinedExternalSymbol>, InspectionError> {
    let output = Command::new(bray_llvm_toolchain::tool_path(root, "llvm-nm"))
        .args(["--defined-only", "--extern-only"])
        .arg(archive)
        .output()
        .map_err(|source| InspectionError::Invocation {
            tool: "llvm-nm",
            source,
        })?;

    if !output.status.success() {
        return Err(InspectionError::Failed {
            tool: "llvm-nm",
            status: output.status,
            detail: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    let symbols = String::from_utf8(output.stdout).map_err(InspectionError::Encoding)?;

    let mut symbols = parse_external_symbols(&symbols, object_format);

    if object_format == ObjectFormat::Coff
        && symbols
            .iter()
            .any(|symbol| !symbol.is_weak() && symbol.name().starts_with("bray_platform_"))
    {
        let output = Command::new(bray_llvm_toolchain::tool_path(root, "llvm-readobj"))
            .arg("--symbols")
            .arg(archive)
            .output()
            .map_err(|source| InspectionError::Invocation {
                tool: "llvm-readobj",
                source,
            })?;

        if !output.status.success() {
            return Err(InspectionError::Failed {
                tool: "llvm-readobj",
                status: output.status,
                detail: String::from_utf8_lossy(&output.stderr).into_owned(),
            });
        }

        let inventory = String::from_utf8(output.stdout).map_err(InspectionError::Encoding)?;
        let fallbacks = parse_coff_comdat_external_symbols(&inventory);

        for symbol in &mut symbols {
            if symbol.name().starts_with("bray_platform_") && fallbacks.contains(symbol.name()) {
                symbol.coff_comdat = true;
            }
        }
    }

    Ok(symbols)
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct DefinedExternalSymbol {
    name: String,
    weak: bool,
    coff_comdat: bool,
}

impl DefinedExternalSymbol {
    pub(crate) fn name(&self) -> &str {
        &self.name
    }

    pub(crate) const fn is_weak(&self) -> bool {
        self.weak
    }

    pub(crate) const fn is_coff_comdat(&self) -> bool {
        self.coff_comdat
    }

    fn into_name(self) -> String {
        self.name
    }
}

fn parse_external_symbols(
    symbols: &str,
    object_format: ObjectFormat,
) -> Vec<DefinedExternalSymbol> {
    symbols
        .lines()
        .filter_map(|line| {
            if line.trim_end().ends_with(':') {
                return None;
            }

            let mut fields = line.split_whitespace();
            fields.next()?;
            let kind = fields.next()?;
            let name = fields.next_back()?;

            let name = if object_format == ObjectFormat::MachO {
                name.strip_prefix('_').unwrap_or(name)
            } else {
                name
            };

            Some(DefinedExternalSymbol {
                name: name.to_owned(),
                weak: matches!(kind, "V" | "W" | "v" | "w"),
                coff_comdat: false,
            })
        })
        .collect()
}

fn parse_coff_comdat_external_symbols(inventory: &str) -> BTreeSet<String> {
    let mut file = 0usize;
    let mut block = None::<CoffSymbolBlock>;
    let mut records = Vec::new();

    for line in inventory.lines() {
        let trimmed = line.trim();

        if block.is_none() && trimmed.starts_with("File: ") {
            file = file.saturating_add(1);
            continue;
        }

        if trimmed == "Symbol {" {
            block = Some(CoffSymbolBlock::new(
                file,
                line.len() - line.trim_start().len(),
            ));
            continue;
        }

        let Some(current) = block.as_mut() else {
            continue;
        };

        let indentation = line.len() - line.trim_start().len();

        if trimmed == "}" && indentation == current.indentation {
            records.push(block.take().unwrap_or_else(|| unreachable!()));
            continue;
        }

        if let Some(value) = trimmed.strip_prefix("Name: ") {
            current.name = Some(value.to_owned());
        } else if current.section.is_none()
            && let Some(value) = trimmed.strip_prefix("Section: ")
        {
            current.section = Some(value.to_owned());
        } else if let Some(value) = trimmed.strip_prefix("StorageClass: ") {
            current.external = value.starts_with("External ");
        } else if let Some(value) = trimmed.strip_prefix("Selection: ") {
            current.comdat = !value.ends_with("(0x0)");
        }
    }

    let comdat_sections = records
        .iter()
        .filter(|record| record.comdat)
        .filter_map(CoffSymbolBlock::section_key)
        .collect::<BTreeSet<_>>();

    let mut definitions = BTreeMap::<String, (bool, bool)>::new();

    for record in records.iter().filter(|record| record.external) {
        let (Some(name), Some(section)) = (&record.name, record.section_key()) else {
            continue;
        };

        if section.1.starts_with("IMAGE_SYM_") {
            continue;
        }

        let binding = definitions.entry(name.clone()).or_default();

        if comdat_sections.contains(&section) {
            binding.1 = true;
        } else {
            binding.0 = true;
        }
    }

    definitions
        .into_iter()
        .filter_map(|(name, (regular, comdat))| (!regular && comdat).then_some(name))
        .collect()
}

#[derive(Debug)]
struct CoffSymbolBlock {
    file: usize,
    indentation: usize,
    name: Option<String>,
    section: Option<String>,
    external: bool,
    comdat: bool,
}

impl CoffSymbolBlock {
    const fn new(file: usize, indentation: usize) -> Self {
        Self {
            file,
            indentation,
            name: None,
            section: None,
            external: false,
            comdat: false,
        }
    }

    fn section_key(&self) -> Option<(usize, String)> {
        self.section
            .as_ref()
            .map(|section| (self.file, section.clone()))
    }
}

#[derive(Debug)]
pub(crate) enum InspectionError {
    Invocation {
        tool: &'static str,
        source: std::io::Error,
    },
    Failed {
        tool: &'static str,
        status: ExitStatus,
        detail: String,
    },
    Encoding(std::string::FromUtf8Error),
}

impl fmt::Display for InspectionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Invocation { tool, source } => {
                write!(formatter, "could not run {tool}: {source}")
            }
            Self::Failed {
                tool,
                status,
                detail,
            } => {
                write!(formatter, "{tool} failed with {status}: {}", detail.trim())
            }
            Self::Encoding(error) => {
                write!(formatter, "native symbol inventory is not UTF-8: {error}")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DefinedExternalSymbol, parse_coff_comdat_external_symbols, parse_external_symbols,
    };
    use bray_target::ObjectFormat;

    #[test]
    fn normalizes_macho_symbols_without_losing_bindings_or_duplicates() {
        assert_eq!(
            parse_external_symbols(
                "archive with spaces/first.o:\n0000 T _bray_role\nsecond.o:\n0000 W _bray_role\n",
                ObjectFormat::MachO
            ),
            [
                DefinedExternalSymbol {
                    name: "bray_role".to_owned(),
                    weak: false,
                    coff_comdat: false,
                },
                DefinedExternalSymbol {
                    name: "bray_role".to_owned(),
                    weak: true,
                    coff_comdat: false,
                },
            ],
        );
    }

    #[test]
    fn excludes_addressless_coff_weak_alias_references() {
        assert_eq!(
            parse_external_symbols(
                "component.o:\n00000000 T bray_runtime_entry\n         U bray_platform_file_open\n",
                ObjectFormat::Coff,
            ),
            [DefinedExternalSymbol {
                name: "bray_runtime_entry".to_owned(),
                weak: false,
                coff_comdat: false,
            }],
        );
    }

    #[test]
    fn classifies_only_exclusively_comdat_coff_definitions_as_fallbacks() {
        let inventory = r#"
File: component.lib(fallback.o)
Format: COFF-x86-64
  Symbols [
    Symbol {
      Name: .text
      Section: .text (4)
      StorageClass: Static (0x3)
      AuxSymbolCount: 1
      AuxSectionDef {
        Selection: ExactMatch (0x4)
      }
    }
    Symbol {
      Name: bray_platform_file_open
      Section: .text (4)
      StorageClass: External (0x2)
      AuxSymbolCount: 0
    }
    Symbol {
      Name: bray_runtime_entry
      Section: .text (5)
      StorageClass: External (0x2)
      AuxSymbolCount: 0
    }
  ]
File: component.lib(owner.o)
Format: COFF-x86-64
  Symbols [
    Symbol {
      Name: bray_platform_promoted
      Section: .text (1)
      StorageClass: External (0x2)
      AuxSymbolCount: 0
    }
    Symbol {
      Name: bray_platform_promoted
      Section: .text (2)
      StorageClass: External (0x2)
      AuxSymbolCount: 0
    }
  ]
"#;

        assert_eq!(
            parse_coff_comdat_external_symbols(inventory),
            ["bray_platform_file_open".to_owned()].into_iter().collect()
        );
    }
}
